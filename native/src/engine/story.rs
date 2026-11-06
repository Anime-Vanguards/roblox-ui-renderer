//! Mounting what `runtime/main.luau` mounts, shared by the one-shot CLI (cli.rs) and live
//! sessions (session.rs): the project (the emulator's mount: engine start, Studio snapshots, Rojo
//! roots, data roots, package stand-ins, the local player, the Studio hierarchy; with rur's story
//! host, `--plugin` and rur.json `clientGui` in between), then a story
//! (UI Labs formats: `story` tables with controls and vide, `render` tables, functions), a module
//! or fixture (`function(target)` / an Instance) or a model into the story gui; requires that
//! yield mount when they finish (pending mounts), `--controls`, `--actions` timelines, the
//! RUR_ORDER sibling order replay and the scene diagnostics.

use std::{path::PathBuf, rc::Rc, time::Instant};

use mlua::prelude::*;
use serde_json::{Map, Value as Json, json};

use super::{
    Engine, GuiEngine, InstanceId, K, NameId,
    export,
    lua::{
        self as engine_lua,
        globals::{CallOutcome, call, randomseed},
        instance_value,
    },
    mount, project, scheduler, services,
    value::{Str, Value},
};

const HOST: &str = include_str!("story/host.luau");
const PLUGIN: &str = include_str!("story/plugin.luau");

/// What to mount (main.luau --story / --module / --fixture / --model).
#[derive(Clone, Debug, PartialEq)]
pub enum SourceKind {
    Story,
    Module,
    Fixture,
    Model,
}

#[derive(Clone, Debug)]
pub struct Source {
    pub kind: SourceKind,
    /// Absolute path (the CLI passes resolved paths)
    pub path: String,
}

/// The mounted project.
pub struct Project {
    /// The story-mounting half of main.luau (story/host.luau), run with the shared globals
    pub host: LuaTable,
    /// Studio hierarchy snapshot in use (diagnostics.studioOnly.hierarchy)
    pub hierarchy: Option<String>,
    /// Seconds from the start to the end of the project mount
    pub mount_time: f64,
    pub started: Instant,
    /// `--plugin`: the dock widgets the plugin stand-in created (exported after the story gui)
    pub plugin_widgets: Option<LuaTable>,
}

/// Where the project comes from.
#[derive(Clone, Debug, Default)]
pub struct ProjectSpec {
    pub roots: Vec<String>,
    /// Studio exports (<Service>.rbxm) loaded into their services
    pub snapshots: Vec<String>,
    pub snapshot_dir: Option<String>,
    /// Explicit mounts after the roots (`--mount` / `--instance`, Rojo project files)
    pub mounts: Vec<project::Mount>,
    /// `--plugin`: a Studio `plugin` global (story/plugin.luau) whose dock widgets are rendered;
    /// the value seeds its settings (a JSON object, else empty)
    pub plugin: Option<Json>,
    /// Where and which manifest bundles mount instead of rur.json `modelAssets`' choice
    pub model_assets: project::ModelAssetsOverride,
}

fn lua_error(error: LuaError) -> String {
    error.to_string()
}

/// Engine.Start plus the project mount of main.luau (the emulator's mount::begin_project /
/// Mounting::finish: snapshots, roots, data roots, stand-ins, JoinLocalPlayer, UseStudioHierarchy,
/// manifest bundles) with rur's part between the player joining and the Studio hierarchy: the story
/// host (story/host.luau), `--plugin` and rur.json clientGui. RUR_ENGINE_TIMINGS=1 prints phase
/// timings; RUR_ENGINE_DUMP=<file> writes every instance after the mount.
pub fn mount_project(lua: &Lua, engine: &Rc<Engine>, spec: &ProjectSpec, started: Instant) -> Result<Project, String> {
    let core = mount::ProjectSpec {
        roots: spec.roots.clone(),
        snapshots: spec.snapshots.clone(),
        snapshot_dir: spec.snapshot_dir.clone(),
        mounts: spec.mounts.clone(),
        model_assets: spec.model_assets.clone(),
    };
    let mounting = mount::begin_project(lua, engine, &core, started)?;

    let host_env = engine.lua.borrow().shared.clone().ok_or("no shared globals")?;
    let host: LuaTable = lua.load(HOST).set_name("=rur.host").set_environment(host_env.clone()).call(()).map_err(lua_error)?;
    let plugin_widgets = match &spec.plugin {
        Some(settings) => Some(install_plugin(lua, engine, &host_env, settings).map_err(lua_error)?),
        None => None,
    };

    // The running client's own gui: rur.json clientGui may name a ModuleScript whose Mount() builds
    // what a play-testing client has in its PlayerGui. It runs once, when a story in edit mode first
    // reaches the PlayerGui (the emulator's lua/globals.rs missing_child), so stories that wait for
    // the HUD mount as while playing.
    if let Some(Json::String(module_path)) = project::project_setting(&spec.roots, "clientGui") {
        let mount: LuaFunction = host.get("ClientGuiMount").map_err(lua_error)?;
        let require: LuaFunction = engine.prelude().exports.get("Require").map_err(lua_error)?;
        let e = engine.clone();
        let warn = lua
            .create_function(move |_, message: String| {
                e.warn(message);
                Ok(())
            })
            .map_err(lua_error)?;
        let function = lua
            .create_function(move |_, ()| mount.call::<()>((module_path.clone(), require.clone(), warn.clone())))
            .map_err(lua_error)?;
        engine.state.borrow_mut().project.client_gui_mount = Some(function);
    }

    let core = mounting.finish(engine, &core)?;
    Ok(Project { host, hierarchy: core.hierarchy, mount_time: core.mount_time, started: core.started, plugin_widgets })
}

/// `--plugin`: the `plugin` global (story/plugin.luau) in the shared environment. Its dock widgets are
/// DockWidgetPluginGuis in CoreGui (not creatable by scripts, so made here). Returns the widget list.
fn install_plugin(lua: &Lua, engine: &Rc<Engine>, shared: &LuaTable, settings: &Json) -> LuaResult<LuaTable> {
    let e = engine.clone();
    let new_widget = lua.create_function(move |lua, name: String| {
        let game = e.world().game;
        let core_gui = services::get_service(&e, game, "CoreGui").map_err(|_| LuaError::runtime("CoreGui"))?;
        let class = e.reflection.class_id("DockWidgetPluginGui").ok_or_else(|| LuaError::runtime("no DockWidgetPluginGui class"))?;
        let widget = {
            let mut state = e.state.borrow_mut();
            let widget = super::instance::create(&mut state, &e.reflection, class);
            super::instance::raw_set_silent(&mut state, &e.reflection, widget, K::Name as NameId, Value::String(Str::from(name)));
            widget
        };
        engine_lua::instance::set_parent(&e, widget, Some(core_gui)).map_err(|_| LuaError::runtime("widget parent"))?;
        instance_value(lua, &e, widget)
    })?;
    let settings = match settings {
        Json::Object(_) => lua.to_value(settings)?,
        _ => LuaValue::Table(lua.create_table()?),
    };
    let (plugin, widgets): (LuaTable, LuaTable) =
        lua.load(PLUGIN).set_name("=rur.plugin").set_environment(shared.clone()).call((new_widget, settings))?;
    shared.raw_set("plugin", plugin)?;
    Ok(widgets)
}

/// `--plugin`: the dock widgets created so far become exported roots (after the story gui), with
/// the size of the viewport and Enabled unless the plugin turned them off.
pub fn add_plugin_widgets(project: &Project, story: &mut Story) {
    let Some(widgets) = &project.plugin_widgets else { return };
    for widget in widgets.sequence_values::<LuaValue>().flatten() {
        if let Some(id) = engine_lua::instance_id(&widget) {
            if !story.roots.contains(&id) {
                story.roots.push(id);
            }
        }
    }
}

/// A mount call that yielded: it may still finish while virtual time runs.
struct Pending {
    call: engine_lua::globals::Call,
    reported: bool,
}

/// The mounted story and its outcome so far.
pub struct Story {
    /// Exported roots: the story gui (RURStory), plus a model's own LayerCollectors
    pub roots: Vec<InstanceId>,
    pub status: String,
    pub failure: Option<String>,
    pending: Vec<Pending>,
    /// main.luau's mount state: RequireYielded, and StoryControls / StoryControlSpecs (the UI
    /// Labs control sources and definitions, for live sessions)
    pub state: LuaTable,
    /// Seconds spent in the story require + mount
    pub story_time: f64,
    /// Engine.Order.Summary of the RUR_ORDER replay
    pub order: Option<Json>,
}

impl Story {
    /// A story with nothing mounted (sessions over scripted content, tests).
    pub fn empty(lua: &Lua) -> LuaResult<Story> {
        Ok(Story {
            roots: Vec::new(),
            status: "ok".into(),
            failure: None,
            pending: Vec::new(),
            state: lua.create_table()?,
            story_time: 0.0,
            order: None,
        })
    }

    /// The UI Labs control sources (StoryControls) and definitions (StoryControlSpecs).
    pub fn controls(&self) -> (LuaValue, LuaValue) {
        (
            self.state.get("StoryControls").unwrap_or(LuaValue::Nil),
            self.state.get("StoryControlSpecs").unwrap_or(LuaValue::Nil),
        )
    }

    /// main.luau Report: a failed (not merely yielding) call is the scene's failure.
    fn report(&mut self, engine: &Rc<Engine>, call: engine_lua::globals::Call) -> LuaResult<()> {
        match call.outcome()? {
            CallOutcome::Finished(_) => {}
            CallOutcome::Failed(message, trace) => {
                self.status = "error".into();
                self.failure = Some(message.clone());
                engine.output("error", message, trace);
            }
            CallOutcome::Pending => self.pending.push(Pending { call, reported: false }),
        }
        Ok(())
    }

    /// SettlePendingMounts: mounts that finished or failed meanwhile, and a warning for ones that
    /// still yield. Updates the status (live sessions ask again every frame).
    pub fn settle(&mut self, lua: &Lua, engine: &Rc<Engine>) -> LuaResult<()> {
        let mut still = Vec::new();
        for mut entry in std::mem::take(&mut self.pending) {
            match entry.call.outcome()? {
                CallOutcome::Failed(message, trace) => {
                    self.status = "error".into();
                    self.failure.get_or_insert_with(|| message.clone());
                    engine.output("error", message, trace);
                }
                CallOutcome::Pending => {
                    if !entry.reported {
                        entry.reported = true;
                        let trace = engine_lua::traceback(lua, &entry.call.thread);
                        engine.output("warn", "story mount is still yielding (waiting on something that never arrives offline)", trace);
                    }
                    still.push(entry);
                }
                CallOutcome::Finished(_) => {}
            }
        }
        if !still.is_empty() && self.status == "ok" {
            self.status = "yielded".into();
        } else if still.is_empty() && self.status == "yielded" {
            self.status = "ok".into();
        }
        self.pending = still;
        Ok(())
    }

    /// Renderer-side phases must not take the run down (main.luau Protected): a failure is the
    /// scene's failure, reported with its message.
    pub fn protected(&mut self, engine: &Rc<Engine>, phase: &str, result: LuaResult<()>) -> bool {
        match result {
            Ok(()) => true,
            Err(error) => {
                let message = format!("{phase}: {}", scheduler::error_message(&error));
                self.status = "error".into();
                self.failure.get_or_insert_with(|| message.clone());
                engine.output("error", message, None);
                false
            }
        }
    }
}

/// MakeStoryGui: the ScreenGui (RURStory in CoreGui) and target frame a story mounts into.
fn make_story_gui(project: &Project, margin: f64, target: (f64, f64)) -> Result<(LuaValue, LuaValue, InstanceId, InstanceId), String> {
    let make_gui: LuaFunction = project.host.get("MakeStoryGui").map_err(lua_error)?;
    let (gui, target_frame): (LuaValue, LuaValue) = make_gui.call((margin, target.0, target.1)).map_err(lua_error)?;
    let id = |value: &LuaValue| -> Result<InstanceId, String> {
        match value {
            LuaValue::UserData(ud) => engine_lua::raw::instance_of_ud(ud).ok_or_else(|| "MakeStoryGui: instance expected".to_string()),
            _ => Err("MakeStoryGui: instance expected".into()),
        }
    };
    let (gui_id, target_id) = (id(&gui)?, id(&target_frame)?);
    Ok((gui, target_frame, gui_id, target_id))
}

/// Fixtures: standalone modules (outside the project roots) returning function(target), mounted
/// as a ModuleScript under ReplicatedStorage.RURFixtures. Stories and modules outside the mounted
/// roots are mounted there too (their requires must not depend on where the file sits on disk).
fn mount_fixture(lua: &Lua, engine: &Rc<Engine>, fixture: &str) -> Result<(), String> {
    let source = std::fs::read(fixture).map_err(|e| format!("{fixture}: {e}"))?;
    // An existing folder (a second mount) is looked up without FindFirstChild, which would
    // materialize ReplicatedStorage's Studio-only children
    let game = engine.world().game;
    let existing = {
        let state = engine.state.borrow();
        state.dom.find_child(game, b"ReplicatedStorage").and_then(|storage| state.dom.find_child(storage, b"RURFixtures"))
    };
    let folder = match existing {
        Some(folder) => folder,
        None => {
            let new_folder: LuaFunction = lua
                .load("local Folder = Instance.new('Folder') Folder.Name = 'RURFixtures' Folder.Parent = game:GetService('ReplicatedStorage') return Folder")
                .set_name("=rur.fixture")
                .set_environment(engine.lua.borrow().shared.clone().unwrap())
                .into_function()
                .map_err(lua_error)?;
            let folder: LuaValue = new_folder.call(()).map_err(lua_error)?;
            let LuaValue::UserData(folder) = folder else { return Err("fixture folder".into()) };
            engine_lua::raw::instance_of_ud(&folder).ok_or("fixture folder")?
        }
    };
    let name = PathBuf::from(fixture)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .map(|n| n.trim_end_matches(".luau").trim_end_matches(".lua").to_string())
        .unwrap_or_else(|| "Fixture".into());
    let hooks = engine.hooks.borrow();
    let mut state = engine.state.borrow_mut();
    let class = engine.reflection.class_id("ModuleScript").unwrap();
    let module = super::instance::create(&mut state, &engine.reflection, class);
    super::instance::raw_set_silent(&mut state, &engine.reflection, module, K::Name as NameId, Value::String(Str::from(name)));
    state.dom.at_mut(module).set_source(Some(Rc::from(source.as_slice())));
    super::instance::set_parent(&mut state, &engine.reflection, &hooks, module, Some(folder)).map_err(|e| e.message)?;
    state.dom.at_mut(module).set_source_path(Some(Rc::from(fixture)));
    Ok(())
}

/// `--load Service/Name=<file.rbxm>`: a saved model (a SerializationService blob, a Studio export) whose
/// top-level instances go, with their own names, into the Folder at that path (made, with any missing
/// Folders on the way, when absent) before the story mounts, for it to clone. A run's own option: a
/// standby VM loads nothing.
pub fn load_models(engine: &Rc<Engine>, specs: &[String]) -> Result<(), String> {
    let folder_class = engine.reflection.class_id("Folder").ok_or("no Folder class")?;
    for spec in specs {
        let (at, file) = spec.split_once('=').ok_or_else(|| format!("--load {spec}: expected Service/Name=<file.rbxm>"))?;
        let segments: Vec<&str> = at.split(['/', '\\']).map(str::trim).filter(|s| !s.is_empty()).collect();
        if segments.len() < 2 {
            return Err(format!("--load {spec}: expected Service/Name=<file.rbxm>"));
        }
        if !engine.reflection.class_by_name(segments[0]).is_some_and(|c| c.service) {
            return Err(format!("--load {spec}: {} is not a service", segments[0]));
        }
        let game = engine.world().game;
        let mut parent = services::get_service(engine, game, segments[0]).map_err(|_| format!("--load {spec}: {} is not a service", segments[0]))?;
        for segment in &segments[1..] {
            let existing = engine.state.borrow().dom.find_child(parent, segment.as_bytes());
            parent = match existing {
                Some(child) => child,
                None => {
                    let folder = super::instance::create_named(&mut engine.state.borrow_mut(), &engine.reflection, folder_class, segment);
                    engine_lua::instance::set_parent(engine, folder, Some(parent)).map_err(|_| format!("--load {spec}: cannot make {segment}"))?;
                    folder
                }
            };
        }
        let roots = project::load_model(engine, std::path::Path::new(file)).map_err(|e| format!("--load {spec}: {e}"))?;
        for root in roots {
            engine_lua::instance::set_parent(engine, root, Some(parent)).map_err(|_| format!("--load {spec}: cannot parent the model"))?;
        }
    }
    Ok(())
}

/// Mounts `source` into a fresh story gui (main.luau after the project mount). `controls` are
/// the decoded `--controls` ({"Path.To.Control": value}); `margin` / `target` size the story
/// target like --margin / --viewport.
pub fn mount(
    lua: &Lua,
    engine: &Rc<Engine>,
    project: &Project,
    source: Option<&Source>,
    margin: f64,
    target: (f64, f64),
    controls: Option<&Json>,
) -> Result<Story, String> {
    let story_start = Instant::now();
    let mut story = Story {
        roots: Vec::new(),
        status: "ok".into(),
        failure: None,
        pending: Vec::new(),
        state: lua.create_table().map_err(lua_error)?,
        story_time: 0.0,
        order: None,
    };
    if let Some(controls) = controls {
        let options = LuaSerializeOptions::new().serialize_none_to_null(false).serialize_unit_to_null(false).set_array_metatable(false);
        let decoded = lua.to_value_with(controls, options).map_err(lua_error)?;
        story.state.set("Controls", decoded).map_err(lua_error)?;
    }
    let Some(source) = source else {
        return Ok(story);
    };
    let outside_roots = source.kind != SourceKind::Model
        && source.kind != SourceKind::Fixture
        && project::find_module_by_path(engine, &source.path).is_none()
        && std::path::Path::new(&source.path).is_file();
    if source.kind == SourceKind::Fixture || outside_roots {
        mount_fixture(lua, engine, &source.path)?;
    }
    let (_gui, target_frame, gui_id, target_id) = if source.kind == SourceKind::Model {
        make_story_gui(project, margin, target)?
    } else {
        // (the module must exist before the story gui is made, as in main.luau)
        let _ = project::find_module_by_path(engine, &source.path).ok_or_else(|| {
            format!("{} is not part of the mounted roots", source.path)
        })?;
        make_story_gui(project, margin, target)?
    };
    story.roots.push(gui_id);

    let e = engine.clone();
    let begin_mount = lua
        .create_function(move |_, ()| {
            e.state.borrow_mut().sched.begin_mount();
            randomseed(&e)
        })
        .map_err(lua_error)?;

    if source.kind == SourceKind::Model {
        // Scheduler.BeginMount, then the model's roots: LayerCollectors go to CoreGui (exported roots),
        // GuiObjects and Folders into the target
        begin_mount.call::<()>(()).map_err(lua_error)?;
        let roots = project::load_model(engine, std::path::Path::new(&source.path)).map_err(|e| format!("{}: {e}", source.path))?;
        let game = engine.world().game;
        let core_gui = services::get_service(engine, game, "CoreGui").map_err(|_| "CoreGui")?;
        let reflection = &engine.reflection;
        let layer_collector = reflection.class_id("LayerCollector");
        let gui_object = reflection.class_id("GuiObject");
        let folder = reflection.class_id("Folder");
        for root in roots {
            let class = engine.state.borrow().dom.at(root).class;
            let info = reflection.class(class);
            if layer_collector.is_some_and(|c| info.is_a(c)) {
                let _ = engine_lua::instance::set_parent(engine, root, Some(core_gui));
                story.roots.push(root);
            } else if gui_object.is_some_and(|c| info.is_a(c)) || Some(class) == folder {
                let _ = engine_lua::instance::set_parent(engine, root, Some(target_id));
            }
        }
        story.story_time = story_start.elapsed().as_secs_f64();
        return Ok(story);
    }

    let module = project::find_module_by_path(engine, &source.path).expect("checked above");
    let is_module = source.kind != SourceKind::Story;
    let load_and_mount: LuaFunction = project.host.get("LoadAndMount").map_err(lua_error)?;
    let mount_story: LuaFunction = project.host.get("MountStory").map_err(lua_error)?;
    let require: LuaFunction = engine.prelude().exports.get("Require").map_err(lua_error)?;
    let module_value = instance_value(lua, engine, module).map_err(lua_error)?;
    // A story module that yields while loading mounts when it finishes during the run, as UI Labs
    // mounts a story once require returns. Scheduler.BeginMount: math.random restarts from the run's
    // seed right before the mount (after the require: Studio keeps required modules loaded).
    let first = call(
        lua,
        engine,
        load_and_mount,
        LuaMultiValue::from_iter([
            LuaValue::Function(require),
            module_value,
            target_frame.clone(),
            LuaValue::Boolean(is_module),
            LuaValue::Function(begin_mount.clone()),
            LuaValue::Table(story.state.clone()),
        ]),
    )
    .map_err(lua_error)?;
    match first.outcome().map_err(lua_error)? {
        CallOutcome::Finished(results) => {
            begin_mount.call::<()>(()).map_err(lua_error)?;
            let loaded = results.into_iter().next().unwrap_or(LuaValue::Nil);
            let mounted = call(
                lua,
                engine,
                mount_story,
                LuaMultiValue::from_iter([loaded, target_frame, LuaValue::Boolean(is_module), LuaValue::Table(story.state.clone())]),
            )
            .map_err(lua_error)?;
            story.report(engine, mounted).map_err(lua_error)?;
        }
        CallOutcome::Failed(message, trace) => {
            story.status = "error".into();
            story.failure = Some(message.clone());
            engine.output("error", message, trace);
        }
        CallOutcome::Pending => {
            story.state.set("RequireYielded", true).map_err(lua_error)?;
            story.pending.push(Pending { call: first, reported: false });
        }
    }
    story.story_time = story_start.elapsed().as_secs_f64();
    Ok(story)
}

/// An input event handler (runtime/input Dispatch): the input port installs one; without it
/// events are ignored, as the Luau session does without runtime/input.
pub type Dispatch<'a> = &'a mut dyn FnMut(&Lua, &Rc<Engine>, &Json) -> LuaResult<()>;

/// runtime/input Dispatch as a timeline's dispatch callback (engine::input, phase 5).
pub fn input_dispatch(lua: &Lua, engine: &Rc<Engine>, event: &Json) -> LuaResult<()> {
    match super::input::get(engine) {
        Some(input) => input.dispatch(lua, engine, event),
        None => Ok(()),
    }
}

/// The action list of `--actions` (inline JSON or a file), checked against the schema
/// (input/actions.rs); the error names the offending action. The engine CLI calls it before
/// anything runs, so a bad list is a usage error rather than a failed render.
pub fn read_actions(spec: &str) -> Result<Vec<Json>, String> {
    let text = if spec.trim_start().starts_with('[') {
        spec.to_string()
    } else {
        std::fs::read_to_string(spec).map_err(|e| format!("--actions {spec}: {e}"))?
    };
    let actions: Json = super::lune_json::decode(&text).map_err(|e| format!("--actions: not valid JSON: {e}"))?;
    super::input::actions::validate(&actions)?;
    let Json::Array(items) = actions else { unreachable!("validated") };
    Ok(items)
}

/// Session.LoadActions: a JSON list of {"t": seconds, ...event} (inline JSON or a file), in time
/// order. The sort runs in Luau (table.sort with main.luau's comparator: equal times keep the
/// order Luau's sort leaves them in). Click / drag actions get their list index (`_index`) for
/// their error messages.
pub fn load_actions(lua: &Lua, spec: &str) -> Result<Vec<Json>, String> {
    let mut items = read_actions(spec)?;
    for (index, action) in items.iter_mut().enumerate() {
        if super::input::actions::is_compound(action) {
            action["_index"] = Json::from(index);
        }
    }
    // Sort indices in Luau by each action's t
    let keys = lua.create_table().map_err(lua_error)?;
    for (index, action) in items.iter().enumerate() {
        let entry = lua.create_table().map_err(lua_error)?;
        entry.set("i", index + 1).map_err(lua_error)?;
        if let Some(t) = action.get("t").and_then(Json::as_f64) {
            entry.set("t", t).map_err(lua_error)?;
        }
        keys.raw_push(entry).map_err(lua_error)?;
    }
    let sort: LuaFunction = lua
        .load("local List = ... table.sort(List, function(A, B) return (A.t or 0) < (B.t or 0) end) return List")
        .set_name("=rur.actions")
        .into_function()
        .map_err(lua_error)?;
    let sorted: LuaTable = sort.call(keys).map_err(lua_error)?;
    let mut result = Vec::with_capacity(items.len());
    for entry in sorted.sequence_values::<LuaTable>() {
        let index: usize = entry.and_then(|e| e.get("i")).map_err(lua_error)?;
        result.push(items[index - 1].clone());
    }
    Ok(result)
}

/// Session.RunTimeline: runs virtual time to `until`, dispatching each action when the clock
/// reaches its time. A click / drag (input/actions.rs) is located against the layout of that
/// moment and becomes timed move / down / up events, dispatched as the clock reaches them (before
/// a later action of the same time); one whose node is not found is skipped, and the run fails at
/// the end with every such error.
pub fn run_timeline(lua: &Lua, engine: &Rc<Engine>, actions: &[Json], until: f64, dispatch: Dispatch) -> LuaResult<()> {
    let mut timeline = Timeline::new(actions.to_vec());
    timeline.advance(lua, engine, until, true, dispatch, &mut |lua, engine, at| scheduler::run_until(lua, engine, at))?;
    timeline.finish()
}

/// Moves virtual time to (at least) a moment: [`scheduler::run_until`], or a sequence's own cadence
pub type StepTo<'a> = &'a mut dyn FnMut(&Lua, &Rc<Engine>, f64) -> LuaResult<()>;

/// An `--actions` timeline that stops at a moment and goes on from there later: [`run_timeline`] runs it
/// to the capture in one call, a sequence (cli.rs run_sequence) to each frame it captures
pub struct Timeline {
    /// In time order (load_actions)
    actions: Vec<Json>,
    next: usize,
    /// Expanded events not dispatched yet, in time order (equal times in insertion order)
    queue: std::collections::VecDeque<(f64, Json)>,
    failures: Vec<String>,
    /// An action or event ran at the end rather than at its own later time ([`Timeline::clamped`])
    clamped: bool,
}

impl Timeline {
    pub fn new(actions: Vec<Json>) -> Timeline {
        Timeline { actions, next: 0, queue: Default::default(), failures: Vec::new(), clamped: false }
    }

    /// Whether the last advance ran an action or event at its `until` instead of at its own later
    /// time (a timeline whose actions end after the capture): a run that goes on from there would
    /// have run it later (cli.rs Kept)
    pub fn clamped(&self) -> bool {
        self.clamped
    }

    fn drain(&mut self, lua: &Lua, engine: &Rc<Engine>, to: f64, until: f64, dispatch: Dispatch, step_to: StepTo) -> LuaResult<()> {
        while self.queue.front().is_some_and(|(time, _)| *time <= to) {
            let (time, event) = self.queue.pop_front().unwrap();
            self.clamped |= time > until + 1e-9;
            let at = time.min(until);
            if at > engine.state.borrow().sched.now {
                step_to(lua, engine, at)?;
            }
            dispatch(lua, engine, &event)?;
        }
        Ok(())
    }

    /// Dispatches what is due by `until` (each action and expanded event when the clock reaches it,
    /// `step_to` moving the clock there), then moves the clock to `until`. With `last` the timeline
    /// ends there: later actions and events happen at `until`, as a capture's own run clamps them.
    pub fn advance(&mut self, lua: &Lua, engine: &Rc<Engine>, until: f64, last: bool, dispatch: Dispatch, step_to: StepTo) -> LuaResult<()> {
        while let Some(action) = self.actions.get(self.next) {
            let time = action.get("t").and_then(Json::as_f64).unwrap_or(0.0);
            if !last && time > until {
                break;
            }
            let action = action.clone();
            self.next += 1;
            self.clamped |= time > until + 1e-9;
            let at = time.min(until);
            self.drain(lua, engine, at, until, &mut *dispatch, &mut *step_to)?;
            if at > engine.state.borrow().sched.now {
                step_to(lua, engine, at)?;
            }
            if !super::input::actions::is_compound(&action) {
                dispatch(lua, engine, &action)?;
                continue;
            }
            let now = engine.state.borrow().sched.now;
            match super::input::actions::expand(engine, &action, now) {
                Ok(events) => {
                    for (time, event) in events {
                        let position = self.queue.iter().position(|(queued, _)| *queued > time).unwrap_or(self.queue.len());
                        self.queue.insert(position, (time, event));
                    }
                    self.drain(lua, engine, now, until, &mut *dispatch, &mut *step_to)?;
                }
                Err(error) => {
                    let index = action.get("_index").and_then(Json::as_u64).map(|i| format!("[{i}]")).unwrap_or_default();
                    let kind = action.get("type").and_then(Json::as_str).unwrap_or("");
                    self.failures.push(format!("--actions{index} ({kind} at t={at}): {error}"));
                }
            }
        }
        self.drain(lua, engine, if last { f64::INFINITY } else { until }, until, &mut *dispatch, &mut *step_to)?;
        step_to(lua, engine, until)
    }

    /// The actions that failed (a node no click found), as one error
    pub fn finish(self) -> LuaResult<()> {
        if self.failures.is_empty() {
            Ok(())
        } else {
            Err(LuaError::runtime(self.failures.join("; ")))
        }
    }
}

/// rur compare: Studio's sibling order for lists built in an address-dependent order
/// (RUR_ORDER, services/order.rs), applied to the story target after the run.
pub fn apply_order(lua: &Lua, engine: &Rc<Engine>, story: &mut Story) {
    let Ok(file) = std::env::var("RUR_ORDER") else { return };
    if file.is_empty() {
        return;
    }
    let Some(&gui) = story.roots.first() else { return };
    let target = engine.state.borrow().dom.find_child(gui, b"StoryTarget");
    let Some(target) = target else { return };
    match super::order::apply(lua, engine, target, &file) {
        Ok(summary) => story.order = Some(summary),
        Err(error) => {
            story.protected(engine, "order", Err(error));
        }
    }
}

/// The scene's `diagnostics` (main.luau SceneOptions): output, unemulated members, Studio-only
/// use, the order replay and timings (seconds: project load, story require + mount, frame loop
/// + layout, total).
pub fn diagnostics(engine: &Rc<Engine>, project: &Project, story: &Story, run_time: f64) -> Json {
    let state = engine.state.borrow();
    let output: Vec<Json> = state
        .output
        .entries
        .iter()
        .map(|entry| {
            let mut map = Map::new();
            map.insert("kind".into(), json!(entry.kind));
            map.insert("message".into(), json!(entry.message));
            map.insert("time".into(), export::number(entry.time));
            if let Some(trace) = &entry.trace {
                map.insert("trace".into(), json!(trace));
            }
            Json::Object(map)
        })
        .collect();
    let mut missing = state.output.missing.clone();
    missing.sort_by(|a, b| b.1.cmp(&a.1));
    let unemulated: Vec<Json> = missing.iter().map(|(member, count)| json!({ "member": member, "count": count })).collect();
    let project_state = &state.project;
    let mut studio = Map::new();
    if let Some(hierarchy) = &project.hierarchy {
        studio.insert("hierarchy".into(), json!(hierarchy));
    }
    studio.insert("count".into(), json!(project_state.studio_only_count));
    studio.insert("used".into(), json!(project_state.studio_only_used));
    studio.insert("gui".into(), json!(project_state.studio_only_gui));
    // Fonts.Diagnostics / Fonts.MissingAssets (the app downloads the missing font assets)
    let (fonts, missing_fonts): (Vec<Json>, Vec<Json>) = {
        let mut text = engine.text().borrow_mut();
        let text = text.get_or_insert_with(|| super::text::TextEngine::new(super::text::FontPaths::for_repo(&engine.options.repo.to_string_lossy())));
        (
            text.fonts.diagnostics.iter().map(|(key, message)| json!({ "key": key, "message": message })).collect(),
            text.fonts.missing_assets.iter().map(|id| json!(id)).collect(),
        )
    };
    let mut result = json!({
        "output": output,
        "unemulated": unemulated,
        "fonts": fonts,
        "missingFontAssets": missing_fonts,
        "studioOnly": Json::Object(studio),
        "timings": {
            "mount": project.mount_time,
            "story": story.story_time,
            "run": run_time,
            "total": project.started.elapsed().as_secs_f64(),
        },
    });
    if let Some(order) = &story.order {
        result["order"] = order.clone();
    }
    result
}

/// Layout.UpdateAll: the geometry records the export reads (kept in `engine.layout`; the returned
/// marker only sequences the calls). The one place the CLI and live sessions lay out.
pub fn layout(engine: &Rc<Engine>, roots: &[InstanceId]) -> export::Records {
    let _ = roots;
    super::layout::update_all(engine);
    export::Records::default()
}

/// Layout + one-shot export of `roots` (Layout.UpdateAll + Scene.Build): the scene JSON plus the
/// `extra` top-level fields.
pub fn build_scene(engine: &Rc<Engine>, roots: &[InstanceId], extra: Map<String, Json>) -> Json {
    let records = layout(engine, roots);
    export::build(engine, roots, &records, extra)
}
