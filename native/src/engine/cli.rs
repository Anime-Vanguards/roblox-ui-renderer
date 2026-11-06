//! Headless runner for the engine: `rur-native engine ...` (the golden check, engine-oracle tests, story
//! controls and ad-hoc runs spawn it), plus the in-process forms `rur-native render` / the render
//! service (story_render.rs, serve.rs: [`run_once`], [`Preloaded`]) and the desktop app ([`Host`]) use:
//!
//!   rur-native engine --fixture <file.luau> --out <scene.json>
//!       [--module <file>] [--story <file>] [--model <file.rbxm>] [--roots "<dir>;<dir>"]
//!       [--project <dir>] [--place <name>] (the engine's launch options)
//!       [--viewport WxH] [--scale S] [--time T] [--mode edit|play] [--player Name] [--place-id N]
//!       [--camera WxH] [--margin M] [--background R,G,B|transparent] [--rotation-clipping enabled|disabled]
//!       [--controls <json|file>] [--actions <json|file>] [--snapshot <file.rbxm>]...
//!       [--snapshot-dir <dir>] [--no-snapshots] [--session 1] [--verbose] [--repo <dir>]
//!       [--mount Service/Name=<dir|file>]... [--instance Service/Name=<ClassName>]...
//!       [--plugin 1|<settings JSON|file>] [--all-layers 1] [--load Service/Name=<file.rbxm>]...
//!       [--topbar-inset px] [--device desktop|touch|gamepad]
//!
//! `--mount` / `--instance` (in order, after the roots) place a directory, file or instance at an
//! instance path (cli/rojo.mjs translates Rojo project files into them); with mounts and no
//! `--roots` there is no implicit project. `--plugin` defines Studio's `plugin` global
//! (lua/plugin.luau) and exports its dock widgets. `--all-layers 1` exports every enabled ScreenGui
//! the run leaves under the local player's PlayerGui or CoreGui, in Roblox's draw order (layers.rs).
//! `--load` puts a saved model's top-level instances in a Folder at that path before the story runs.
//! `--topbar-inset px` is GuiService's top inset (default 0, as Studio's Edit mode): ScreenGuis with
//! IgnoreGuiInset false start that far down, and GetGuiInset reports it (a running client reserves
//! the topbar's height, 58). `--device` is the input hardware UserInputService reports
//! (TouchEnabled, MouseEnabled, GetLastInputType...; default desktop).
//!
//! `--out` (alias `--dump-scene`) writes the scene JSON (else stdout) and a
//! `rur: <status>; <n> nodes; <n> output lines; <s>s -> <out>` line on stderr.
//!
//!   rur-native engine-batch
//!
//! runs such engine runs one after another in one process ([`batch`]: tools/engine/golden.mjs
//! sends its scene entries so), each exactly as a process of its own would.
//!
//! Mounts the project, mounts the story, runs virtual time, lays out and exports (story.rs holds
//! the mounting both paths share; this was the Luau runtime's main.luau until the engine replaced
//! it). `--session 1` keeps the story running and answers session commands on stdin (session.rs).
//! [`Host`] is the same mount for the native app's in-process session (runtime.rs drives it and
//! applies its replies). Environment: RUR_CAMERA / RUR_PLACE_ID / RUR_PLAYER* /
//! RUR_ROTATION_CLIPPING stand in for the flags, RUR_CLOCK (recorded Studio clock, scheduler.rs)
//! and RUR_ORDER (services/order.rs) come from `rur compare`, RUR_LUAU_PROFILE samples the Luau
//! stacks (luau_profile.rs).

use std::{
    ops::{Deref, DerefMut},
    rc::Rc,
    time::Instant,
};

use mlua::prelude::*;
use serde_json::{Map, Value as Json, json};

use super::{Engine, Options, export, launch, project, story};

/// The engine CLI's arguments: the emulator's engine options (launch.rs; read through `Deref`, so
/// `args.roots`, `args.mode`, ... are theirs) and what a story render adds.
#[derive(Default, Debug, Clone)]
pub struct Args {
    pub engine: launch::Args,
    pub fixture: Option<String>,
    pub module: Option<String>,
    pub story: Option<String>,
    pub model: Option<String>,
    pub time: Option<f64>,
    pub margin: f64,
    pub controls: Option<String>,
    pub actions: Option<String>,
    pub session: bool,
    pub background: Option<[f64; 3]>,
    /// `--background transparent`: the scene has no background, so render.png keeps the UI's own
    /// coverage as alpha (ror draws SurfaceGuis and BillboardGuis from such renders)
    pub transparent: bool,
    pub dump_scene: Option<String>,
    /// Writes the source files the run required ({"files": [...]}; `rur compare` keys Studio captures on them)
    pub dependencies: Option<String>,
    pub test_hooks: bool,
    pub plugin: Option<String>,
    /// `--all-layers 1`: the export roots are every enabled ScreenGui under the local player's
    /// PlayerGui and CoreGui after the run, the story gui among them (layers.rs)
    pub all_layers: bool,
    /// `--load Service/Name=<file.rbxm>` (repeatable): models loaded into Folders before the story
    /// (story.rs load_models)
    pub loads: Vec<String>,
    /// `--topbar-inset px`: the engine's topbar inset (Options::topbar_inset)
    pub topbar_inset: f64,
    /// `--device desktop|touch|gamepad`: the input hardware UserInputService reports (Options::device)
    pub device: Option<String>,
}

impl Deref for Args {
    type Target = launch::Args;

    fn deref(&self) -> &launch::Args {
        &self.engine
    }
}

impl DerefMut for Args {
    fn deref_mut(&mut self) -> &mut launch::Args {
        &mut self.engine
    }
}

/// Runs the engine CLI for `rur-native engine ...`; returns the exit code. Other command lines are
/// the native app's (lib.rs NativeOptions).
pub fn dispatch() -> Option<i32> {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("engine-batch") {
        return Some(batch());
    }
    if args.get(1).map(String::as_str) != Some("engine") {
        return None;
    }
    let parsed = match parse(&args[2..]) {
        Ok(parsed) => parsed,
        Err(error) => {
            eprintln!("rur-native engine: {error}");
            return Some(2);
        }
    };
    match run(parsed) {
        Ok(code) => Some(code),
        Err(error) => {
            eprintln!("rur-native engine: {error}");
            Some(1)
        }
    }
}

/// `--background r,g,b` (0-255) as 0-1 channels
fn parse_background(text: &str) -> Option<[f64; 3]> {
    let parts: Vec<f64> = text.split(',').filter_map(|p| p.trim().parse().ok()).collect();
    (parts.len() == 3).then(|| [parts[0] / 255.0, parts[1] / 255.0, parts[2] / 255.0])
}

pub fn parse(args: &[String]) -> Result<Args, String> {
    let mut result = Args::default();
    let mut index = 0;
    while index < args.len() {
        let key = args[index].as_str();
        let value = args.get(index + 1).cloned();
        if let Some(used) = result.engine.accept(key, value.as_ref())? {
            index += used;
            continue;
        }
        let need = || value.clone().ok_or_else(|| format!("{key} needs a value"));
        let number = |text: String| text.parse::<f64>().map_err(|_| format!("{key}: not a number"));
        match key {
            "--headless" | "--no-png" => {}
            "--fixture" => result.fixture = Some(need()?),
            "--module" => result.module = Some(need()?),
            "--story" => result.story = Some(need()?),
            "--model" => result.model = Some(need()?),
            "--controls" => result.controls = Some(need()?),
            "--actions" => result.actions = Some(need()?),
            "--session" => result.session = need()? != "0",
            "--time" => result.time = Some(number(need()?)?),
            "--margin" => result.margin = number(need()?)?.floor().max(0.0),
            "--background" => {
                let text = need()?;
                if text == "transparent" || text == "none" {
                    result.transparent = true;
                    result.background = None;
                } else if let Some(background) = parse_background(&text) {
                    result.background = Some(background);
                    result.transparent = false;
                }
            }
            "--test-hooks" => result.test_hooks = need()? != "0",
            "--dump-scene" | "--out" => result.dump_scene = Some(need()?),
            "--dependencies" => result.dependencies = Some(need()?),
            "--plugin" => result.plugin = Some(need()?),
            "--all-layers" => result.all_layers = need()? != "0",
            "--load" => result.loads.push(need()?),
            "--topbar-inset" => result.topbar_inset = number(need()?)?.max(0.0),
            "--device" => {
                let device = need()?;
                if !matches!(device.as_str(), "desktop" | "touch" | "gamepad") {
                    return Err(format!("--device: expected desktop, touch or gamepad, got {device}"));
                }
                result.device = Some(device);
            }
            _ => {
                index += 1;
                continue;
            }
        }
        index += if matches!(key, "--headless" | "--no-png") { 1 } else { 2 };
    }
    Ok(result)
}

use launch::absolute;

/// What a run needs from its arguments: the engine options, project roots, snapshot directory and
/// the story target size.
pub struct Prepared {
    pub options: Options,
    pub roots: Vec<String>,
    pub snapshot_dir: Option<String>,
    pub target: (f64, f64),
    /// --mount / --instance, relative paths resolved
    pub mounts: Vec<project::Mount>,
}

/// `--plugin`: "1" (empty settings), inline JSON or a JSON file with the plugin's settings.
fn plugin_settings(args: &Args) -> Result<Option<Json>, String> {
    let Some(spec) = &args.plugin else { return Ok(None) };
    let trimmed = spec.trim();
    if trimmed == "0" {
        return Ok(None);
    }
    if trimmed == "1" || trimmed == "true" || trimmed.is_empty() {
        return Ok(Some(Json::Object(Map::new())));
    }
    let text = if trimmed.starts_with('{') { spec.clone() } else { std::fs::read_to_string(spec).map_err(|e| format!("--plugin {spec}: {e}"))? };
    let settings = super::lune_json::decode(&text).map_err(|e| format!("--plugin: {e}"))?;
    if !settings.is_object() {
        return Err("--plugin: settings must be a JSON object".into());
    }
    Ok(Some(settings))
}

/// The answers of [`batch`] start with this on a line of stdout
pub const BATCH_ANSWER: &str = "@@RUR-BATCH ";

/// `rur-native engine-batch`: engine runs one after another in this process, each exactly as a
/// `rur-native engine <arguments>` process of its own would make it (the golden check sends its scene
/// entries so: a process start, its exit and the reflection load were most of a small run). A job is a
/// line on stdin, `{"argv": [engine arguments], "env": {"RUR_…": value}}`; its variables replace every
/// RUR_* variable of the job before. Each run has an engine and a VM of its own, freed after it; what
/// the next run reads unchanged is kept, as in the render service: module sources and their bytecode,
/// mounted model files and the project's listings. Its answer is a line on stdout, `@@RUR-BATCH {"code": exit code, "error":
/// message}`, after whatever the run itself printed; a run that panics answers code 101 and the next
/// job runs. Live sessions (`--session`) are not batched.
pub fn batch() -> i32 {
    use std::io::{BufRead, Write};
    super::lua::modules::reuse_compiled_modules();
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let answer = match serde_json::from_str::<Json>(&line) {
            Ok(job) => batch_job(&job),
            Err(error) => json!({"code": 2, "error": format!("unreadable job: {error}")}),
        };
        // What the run freed goes back to the system before the next (a process of its own would end)
        #[cfg(not(target_os = "emscripten"))]
        unsafe {
            libmimalloc_sys::mi_collect(true)
        };
        let _ = std::io::stderr().flush();
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "{BATCH_ANSWER}{answer}");
        let _ = out.flush();
    }
    0
}

fn batch_job(job: &Json) -> Json {
    let mut argv: Vec<String> = job["argv"].as_array().into_iter().flatten().filter_map(|arg| arg.as_str().map(str::to_string)).collect();
    if argv.first().is_some_and(|arg| arg == "engine") {
        argv.remove(0);
    }
    // The job's environment as a process of its own would have it: the runs read RUR_CLOCK, RUR_ORDER,
    // RUR_PLAYER*, RUR_EPOCH, RUR_FONT_ROOT... when they start
    let names: Vec<std::ffi::OsString> = std::env::vars_os().map(|(name, _)| name).filter(|name| name.to_string_lossy().starts_with("RUR_")).collect();
    // SAFETY: jobs run one at a time on this thread, and nothing else in the process reads or
    // writes the environment while it changes (no run is in progress)
    unsafe {
        for name in names {
            std::env::remove_var(name);
        }
        for (name, value) in job["env"].as_object().into_iter().flatten() {
            if let Some(value) = value.as_str().filter(|_| name.starts_with("RUR_")) {
                std::env::set_var(name, value);
            }
        }
    }
    let args = match parse(&argv) {
        Ok(args) if !args.session => args,
        Ok(_) => return json!({"code": 2, "error": "live sessions are not batched"}),
        Err(error) => {
            eprintln!("rur-native engine: {error}");
            return json!({"code": 2, "error": error});
        }
    };
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_with(args, true))) {
        Ok(Ok(code)) => json!({"code": code}),
        Ok(Err(error)) => {
            eprintln!("rur-native engine: {error}");
            json!({"code": 1, "error": error})
        }
        Err(_) => json!({"code": 101, "error": "the run panicked"}),
    }
}

/// One render (or live session): returns the process exit code.
pub fn run(args: Args) -> Result<i32, String> {
    run_with(args, false)
}

/// [`run`], tearing the engine down after it when `free` (a batch's next run follows in this
/// process; a process of its own just exits)
fn run_with(args: Args, free: bool) -> Result<i32, String> {
    let _memory = crate::memtrace::scope(crate::memtrace::Tag::Script);
    let started = Instant::now();
    // A bad --actions list stops the run before anything is loaded (input/actions.rs names the action)
    if let Some(spec) = &args.actions {
        story::read_actions(spec)?;
    }
    let prepared = prepare(&args)?;
    let engine = super::new_engine(prepared.options)?;
    // RUR_LUAU_PROFILE=<file>: folded Luau stacks (startup, then frames after the first reply)
    let _profile = crate::luau_profile::begin();
    let lua = Lua::new();
    crate::luau_profile::install(&lua);
    let result = (|| {
        let spec = story::ProjectSpec {
            roots: prepared.roots.clone(),
            snapshots: args.snapshots.clone(),
            snapshot_dir: prepared.snapshot_dir.clone(),
            mounts: prepared.mounts.clone(),
            plugin: plugin_settings(&args)?,
            model_assets: Default::default(),
        };
        let project = story::mount_project(&lua, &engine, &spec, started)?;
        if args.test_hooks {
            install_test_hooks(&lua, &engine).map_err(|e| e.to_string())?;
        }
        render(&lua, &engine, &args, project, prepared.target, started)
    })();
    // Scripts may hold references into the engine; the VM goes first
    drop(lua);
    if free {
        engine.teardown();
    }
    result
}

/// `--test-hooks 1`: a global `rur` table for engine-oracle test scripts (tools/tests/*.luau run as
/// fixtures by their .mjs drivers): `rur.layout()` (a layout pass), `rur.scene(roots, presentation)`
/// (the scene a fresh retained export cache builds, as Scene.Build with a new cache: clipped-node
/// culling, or presentation fields when `presentation`), `rur.id(instance)` (the node id),
/// `rur.flush()` (deferred events), `rur.input.dispatch(event)` / `snapshot()` /
/// `applyPresentation(packet, phase)` / `enablePresentation(on)`, and `rur.emit(prefix, value)`
/// (prints `<prefix> <json>` on stdout).
fn install_test_hooks(lua: &Lua, engine: &Rc<Engine>) -> LuaResult<()> {
    use super::{input, layout, scheduler};
    let shared = engine.lua.borrow().shared_globals().ok_or_else(|| LuaError::runtime("engine not started"))?;
    let rur = lua.create_table()?;
    let e = engine.clone();
    rur.set("layout", lua.create_function(move |_, ()| {
        layout::update_all(&e);
        Ok(())
    })?)?;
    let e = engine.clone();
    rur.set("scene", lua.create_function(move |lua, (roots, presentation): (LuaTable, Option<bool>)| {
        let roots: Vec<_> = roots.sequence_values::<LuaValue>().filter_map(|v| v.ok()).filter_map(|v| super::lua::instance_id(&v)).collect();
        let mut cache = export::retained::Retained::new(&e);
        cache.set_presentation(presentation.unwrap_or(false));
        let update = cache.build(&e, &roots);
        let scene = json!({
            "version": 1,
            "viewport": export::viewport_json(&e),
            "nodes": update.nodes,
            "layers": update.layers.unwrap_or(Json::Array(Vec::new())),
            "fonts": update.fonts.unwrap_or(Json::Array(Vec::new())),
            "images": update.images,
        });
        lua.to_value(&scene)
    })?)?;
    let e = engine.clone();
    rur.set("id", lua.create_function(move |_, value: LuaValue| {
        let id = super::lua::instance_id(&value).ok_or_else(|| LuaError::runtime("rur.id: not an Instance"))?;
        Ok(e.state.borrow().dom.at(id).serial)
    })?)?;
    let e = engine.clone();
    rur.set("flush", lua.create_function(move |lua, ()| scheduler::flush_deferred(lua, &e))?)?;
    let hooks = lua.create_table()?;
    let e = engine.clone();
    hooks.set("dispatch", lua.create_function(move |lua, event: LuaValue| {
        let event: Json = lua.from_value(event)?;
        match input::get(&e) {
            Some(handler) => handler.dispatch(lua, &e, &event),
            None => Ok(()),
        }
    })?)?;
    let e = engine.clone();
    hooks.set("snapshot", lua.create_function(move |lua, ()| {
        let snapshot = input::get(&e).and_then(|handler| handler.presentation_snapshot(lua, &e));
        match snapshot {
            Some(snapshot) => lua.to_value(&snapshot),
            None => Ok(LuaValue::Nil),
        }
    })?)?;
    let e = engine.clone();
    hooks.set("applyPresentation", lua.create_function(move |lua, (packet, phase): (LuaValue, String)| {
        let packet: Json = lua.from_value(packet)?;
        match input::get(&e) {
            Some(handler) => handler.apply_presentation(lua, &e, &packet, &phase),
            None => Ok(false),
        }
    })?)?;
    let e = engine.clone();
    hooks.set("enablePresentation", lua.create_function(move |_, enabled: bool| {
        if let Some(handler) = input::get(&e) {
            handler.enable_presentation(&e, enabled);
        }
        Ok(())
    })?)?;
    rur.set("input", hooks)?;
    rur.set("emit", lua.create_function(|lua, (prefix, value): (String, LuaValue)| {
        let value: Json = lua.from_value(value)?;
        println!("{prefix} {}", serde_json::to_string(&value).map_err(LuaError::external)?);
        Ok(())
    })?)?;
    shared.raw_set("rur", rur)
}

/// main.luau's option handling: engine options, roots, snapshots and target size of `args` (the
/// emulator's launch::prepare, with the story's --margin around the viewport and this checkout as the
/// default repo).
pub fn prepare(args: &Args) -> Result<Prepared, String> {
    let mut engine = args.engine.clone();
    let (width, height) = engine.viewport.unwrap_or((1920.0, 1080.0));
    engine.viewport = Some((width + 2.0 * args.margin, height + 2.0 * args.margin));
    let prepared = launch::prepare(&engine, &super::default_repo())?;
    let device = args.device.clone().unwrap_or(prepared.options.device.clone());
    let options = Options { topbar_inset: args.topbar_inset, device, ..prepared.options };
    Ok(Prepared { options, roots: prepared.roots, snapshot_dir: prepared.snapshot_dir, target: (width, height), mounts: prepared.mounts })
}

/// The story source of `args`
fn source_of(args: &Args) -> Option<story::Source> {
    if let Some(story) = &args.story {
        Some(story::Source { kind: story::SourceKind::Story, path: absolute(story) })
    } else if let Some(module) = &args.module {
        Some(story::Source { kind: story::SourceKind::Module, path: absolute(module) })
    } else if let Some(fixture) = &args.fixture {
        Some(story::Source { kind: story::SourceKind::Fixture, path: absolute(fixture) })
    } else {
        args.model.as_ref().map(|model| story::Source { kind: story::SourceKind::Model, path: absolute(model) })
    }
}

/// The moment a run captures for `--time`: the engine frame nearest the requested time (its frames are
/// 1/60 s apart). A frame is never shortened ([`super::scheduler::run_until`] runs whole frames until
/// it reaches its time), so `--time 0.0167` (1/60 to four places, a hair past frame 1) ran to frame 2
/// before, and every third frame of a 1/60 s series written to a few decimals matched its neighbour. A
/// recorded Studio clock (`rur compare`) keeps its own frames.
pub fn capture_time(engine: &Engine, requested: f64) -> f64 {
    let state = engine.state.borrow();
    let frame = state.sched.frame_duration;
    if state.sched.has_recorded_clock() || !(frame > 0.0) || !requested.is_finite() || requested <= 0.0 {
        return requested;
    }
    (requested / frame).round() * frame
}

/// The story of `args` mounted (its --controls applied first, --load models placed), no time run yet
fn mount_story(
    lua: &Lua,
    engine: &Rc<Engine>,
    project: &story::Project,
    args: &Args,
    source: Option<&story::Source>,
    target: (f64, f64),
) -> Result<story::Story, String> {
    // --controls (inline JSON or a JSON file): applied to the story's control sources before it mounts
    let controls = match &args.controls {
        Some(spec) => {
            let text = if spec.trim_start().starts_with('{') {
                spec.clone()
            } else {
                std::fs::read_to_string(spec).map_err(|e| format!("{spec}: {e}"))?
            };
            Some(super::lune_json::decode(&text).map_err(|e| format!("--controls: {e}"))?)
        }
        None => None,
    };
    story::load_models(engine, &args.loads)?;
    let mut mounted = story::mount(lua, engine, project, source, args.margin, target, controls.as_ref())?;
    story::add_plugin_widgets(project, &mut mounted);
    Ok(mounted)
}

/// main.luau after the project mount: the story (with --controls), virtual time to --time (or the
/// --actions timeline), pending mounts, the RUR_ORDER replay. Returns the story and the run time.
fn mount_and_run(
    lua: &Lua,
    engine: &Rc<Engine>,
    project: &story::Project,
    args: &Args,
    source: Option<&story::Source>,
    target: (f64, f64),
) -> Result<(story::Story, Instant), String> {
    let lua_error = |error: LuaError| error.to_string();
    let mut mounted = mount_story(lua, engine, project, args, source, target)?;

    let run_time = capture_time(engine, args.time.unwrap_or(1.0));
    let exec_start = Instant::now();
    // Renderer-side phases (frame loop, layout, export) must not take the whole run down
    let frame_loop = match &args.actions {
        Some(spec) => story::load_actions(lua, spec)
            .map_err(LuaError::runtime)
            .and_then(|actions| story::run_timeline(lua, engine, &actions, run_time, &mut story::input_dispatch)),
        None => super::scheduler::run_until(lua, engine, run_time),
    };
    mounted.protected(engine, "frame loop", frame_loop);
    mounted.settle(lua, engine).map_err(lua_error)?;
    // Widgets a plugin creates while it runs (after a yield) are exported too
    story::add_plugin_widgets(project, &mut mounted);
    story::apply_order(lua, engine, &mut mounted);
    // (after the order replay, which finds the story target in the first root)
    if args.all_layers {
        mounted.roots = super::layers::screen_layers(engine, &mounted.roots);
    }
    Ok((mounted, exec_start))
}

/// A live session hosted in process (the native app): the engine with its
/// project mounted (a standby can wait for its story here), then the story's [`Session`], whose
/// replies are serde_json values for SceneStore::apply_owned (changed nodes as patches).
/// Drop order: the session and project (Lua references), the VM, then the engine's own cycles
/// ([`Engine::teardown`]).
pub struct Host {
    session: Option<super::session::Session>,
    project: Option<story::Project>,
    pub engine: Rc<Engine>,
    target: (f64, f64),
    background: [f64; 3],
    transparent: bool,
    pub lua: std::mem::ManuallyDrop<Lua>,
}

impl Drop for Host {
    fn drop(&mut self) {
        drop(self.session.take());
        drop(self.project.take());
        // SAFETY: the field is never used again (this is the Host's drop)
        unsafe { std::mem::ManuallyDrop::drop(&mut self.lua) };
        self.engine.teardown();
    }
}

impl Host {
    /// Engine.Start and the project mount of `args` (no story yet).
    pub fn start(args: &Args) -> Result<Host, String> {
        let _memory = crate::memtrace::scope(crate::memtrace::Tag::Project);
        let started = Instant::now();
        let prepared = prepare(args)?;
        let engine = super::new_engine(prepared.options)?;
        let lua = Lua::new();
        let spec = story::ProjectSpec {
            roots: prepared.roots.clone(),
            snapshots: args.snapshots.clone(),
            snapshot_dir: prepared.snapshot_dir.clone(),
            mounts: prepared.mounts.clone(),
            plugin: plugin_settings(args)?,
            model_assets: Default::default(),
        };
        let project = story::mount_project(&lua, &engine, &spec, started)?;
        Ok(Host {
            session: None,
            project: Some(project),
            engine,
            target: prepared.target,
            background: args.background.unwrap_or([0.18, 0.18, 0.18]),
            transparent: args.transparent,
            lua: std::mem::ManuallyDrop::new(lua),
        })
    }

    /// Mounts the story of `args` (--story / --time / --controls; the engine options must be the
    /// ones the host started with) and opens its session; returns the ready reply.
    pub fn mount(&mut self, args: &Args) -> Result<Json, String> {
        let project = self.project.take().ok_or("the host already mounted a story")?;
        let source = source_of(args);
        let (mounted, exec_start) = mount_and_run(&self.lua, &self.engine, &project, args, source.as_ref(), self.target)?;
        let mut session = super::session::Session::new(&self.engine, project, mounted, self.background, true);
        session.transparent = self.transparent;
        session.run_time = exec_start.elapsed().as_secs_f64();
        let ready = session.ready(&self.lua).map_err(|e| e.to_string())?;
        self.session = Some(session);
        Ok(ready)
    }

    /// Idle work between commands: compiles one deferred chunk to native code
    /// (`modules::defer_codegen`); whether more is waiting.
    pub fn idle_step(&self) -> bool {
        super::lua::modules::compile_deferred(&self.lua, &self.engine)
    }

    /// One session command (None for "quit").
    pub fn handle(&mut self, command: &Json) -> Option<Json> {
        let lua = &self.lua;
        match self.session.as_mut() {
            Some(session) => session.handle(lua, command),
            None => Some(json!({ "type": "error", "message": "no mounted story" })),
        }
    }
}

fn render(
    lua: &Lua,
    engine: &Rc<Engine>,
    args: &Args,
    project: story::Project,
    target: (f64, f64),
    started: Instant,
) -> Result<i32, String> {
    if args.session {
        let source = source_of(args);
        let (mounted, exec_start) = mount_and_run(lua, engine, &project, args, source.as_ref(), target)?;
        let background = args.background.unwrap_or([0.18, 0.18, 0.18]);
        // Live preview: keep running and answer frame / control / scene commands (session.rs)
        let mut session = super::session::Session::new(engine, project, mounted, background, false);
        session.transparent = args.transparent;
        session.run_time = exec_start.elapsed().as_secs_f64();
        crate::luau_profile::mark_frames();
        super::session::run_stdio(lua, &mut session)?;
        return Ok(0);
    }
    let output = run_scene(lua, engine, args, &project, target)?;
    output.write(args, started)?;
    Ok(0)
}

/// The scene of a one-shot run, as `--out` writes it: sorted keys, empty arrays as objects (the Luau
/// runtime's encoder), plus what the run reports.
pub struct SceneOutput {
    /// The scene JSON as written (sorted keys, `[]` as `{}`)
    pub text: String,
    /// The same scene as a value
    pub scene: Json,
    pub status: String,
    pub nodes: usize,
    /// Output lines (print / warn / error) of the run
    pub lines: usize,
    /// The source files the run required (`--dependencies`)
    pub dependencies: Vec<String>,
}

impl SceneOutput {
    /// Writes `--dependencies` and `--out` (else prints the scene) with the CLI's status line.
    pub fn write(&self, args: &Args, started: Instant) -> Result<(), String> {
        if let Some(path) = &args.dependencies {
            std::fs::write(path, serde_json::to_string(&json!({ "files": self.dependencies })).map_err(|e| e.to_string())?)
                .map_err(|e| format!("{path}: {e}"))?;
        }
        match &args.dump_scene {
            Some(path) => {
                std::fs::write(path, &self.text).map_err(|e| format!("{path}: {e}"))?;
                eprintln!("rur: {}; {} nodes; {} output lines; {}s -> {path}", self.status, self.nodes, self.lines, started.elapsed().as_secs_f64());
            }
            None => println!("{}", self.text),
        }
        Ok(())
    }
}

/// A one-shot run in a VM of its own: the engine and the project of `args`, the story, virtual time,
/// layout and export (what `rur-native engine --out` does, without writing anything).
pub fn run_once(args: &Args) -> Result<SceneOutput, String> {
    let started = Instant::now();
    if let Some(spec) = &args.actions {
        story::read_actions(spec)?;
    }
    let prepared = prepare(args)?;
    let spec = project_spec(args, &prepared)?;
    let target = prepared.target;
    let engine = super::new_engine(prepared.options)?;
    let lua = Lua::new();
    interpret_in_service(&lua, &engine);
    let result = (|| {
        let project = story::mount_project(&lua, &engine, &spec, started)?;
        run_scene(&lua, &engine, args, &project, target)
    })();
    // The VM, then the engine's own cycles (a process may run many of these: the render service)
    drop(lua);
    engine.teardown();
    result
}

/// A render service's VM (the service keeps compiled modules between VMs) runs every chunk in the
/// interpreter: module chunks already did, and compiling the engine's prelude and services to native
/// code at every VM start, a VM per render, was most of the start (results are the same)
fn interpret_in_service(lua: &Lua, engine: &Engine) {
    if super::lua::modules::reusing_compiled_modules() {
        super::lua::modules::interpret_all(lua, engine);
    }
}

fn project_spec(args: &Args, prepared: &Prepared) -> Result<story::ProjectSpec, String> {
    Ok(story::ProjectSpec {
        roots: prepared.roots.clone(),
        snapshots: args.snapshots.clone(),
        snapshot_dir: prepared.snapshot_dir.clone(),
        mounts: prepared.mounts.clone(),
        plugin: plugin_settings(args)?,
        model_assets: Default::default(),
    })
}

/// The options of `args` that a VM needs before its story: everything but the source and the run's
/// own options (time, actions, controls, background, output paths). Two runs with equal startup
/// arguments can use the same preloaded project ([`Preloaded`]).
pub fn startup_key(args: &Args) -> String {
    let mut startup = args.clone();
    startup.fixture = None;
    startup.module = None;
    startup.story = None;
    startup.model = None;
    startup.time = None;
    startup.actions = None;
    startup.controls = None;
    startup.background = None;
    startup.transparent = false;
    startup.dump_scene = None;
    startup.dependencies = None;
    startup.session = false;
    startup.all_layers = false;
    startup.loads = Vec::new();
    format!("{startup:?}")
}

/// The engine and the project of some startup arguments, loaded ahead of one run: the render service
/// keeps these warm, each on a thread of its own (serve.rs Standby). Not Send: it lives on the thread
/// that loaded it. Fields drop in order: the project (Lua references), the VM, then the engine.
pub struct Preloaded {
    project: Option<story::Project>,
    lua: Option<Lua>,
    engine: Rc<Engine>,
    target: (f64, f64),
}

impl Drop for Preloaded {
    /// The project (Lua references) and the VM go first, then the engine's own cycles
    /// ([`Engine::teardown`])
    fn drop(&mut self) {
        drop(self.project.take());
        drop(self.lua.take());
        self.engine.teardown();
    }
}

impl Preloaded {
    /// Engine.Start and the project mount of `args` (their startup part).
    pub fn load(args: &Args) -> Result<Preloaded, String> {
        let started = Instant::now();
        let prepared = prepare(args)?;
        let spec = project_spec(args, &prepared)?;
        let target = prepared.target;
        let engine = super::new_engine(prepared.options)?;
        let lua = Lua::new();
        interpret_in_service(&lua, &engine);
        let project = match story::mount_project(&lua, &engine, &spec, started) {
            Ok(project) => project,
            Err(error) => {
                drop(lua);
                engine.teardown();
                return Err(error);
            }
        };
        Ok(Preloaded { project: Some(project), lua: Some(lua), engine, target })
    }

    /// The run of `args` (whose startup part must equal the loaded one's): as [`run_once`] would.
    pub fn run(mut self, args: &Args) -> Result<SceneOutput, String> {
        let (Some(project), Some(lua)) = (self.project.as_mut(), self.lua.as_ref()) else {
            return Err("the preloaded VM was released".into());
        };
        // The reported total is the run's own work: the project load plus what follows
        project.started = Instant::now().checked_sub(std::time::Duration::from_secs_f64(project.mount_time)).unwrap_or(project.started);
        if let Some(spec) = &args.actions {
            story::read_actions(spec)?;
        }
        run_scene(lua, &self.engine, args, project, self.target)
    }

    /// The sequence of `args` (whose startup part must equal the loaded one's) in this VM: its frames
    /// and end go to `sink` ([`run_sequence`])
    pub fn sequence(mut self, args: &Args, spec: &SequenceSpec, sink: &mut dyn FnMut(SequenceMessage) -> bool) {
        let (Some(project), Some(lua)) = (self.project.take(), self.lua.as_ref()) else {
            sink(SequenceMessage::End(Err("the preloaded VM was released".into())));
            return;
        };
        let result = match &args.actions {
            Some(actions) => story::read_actions(actions).map(|_| ()),
            None => Ok(()),
        }
        .and_then(|()| run_sequence(lua, &self.engine, project, args, self.target, spec, sink));
        if let Err(error) = result {
            sink(SequenceMessage::End(Err(error)));
        }
    }

    /// [`Preloaded::run`], keeping the run for a later moment of its story ([`Kept`]). Its
    /// [`Kept::refusal`] says when going on from it would not be a run from the start; the render
    /// service lets such a run go at once.
    pub fn run_kept(mut self, args: &Args) -> Result<(SceneOutput, Kept), String> {
        let (Some(project), Some(lua)) = (self.project.as_mut(), self.lua.as_ref()) else {
            return Err("the preloaded VM was released".into());
        };
        // The reported total is the run's own work: the project load plus what follows
        project.started = Instant::now().checked_sub(std::time::Duration::from_secs_f64(project.mount_time)).unwrap_or(project.started);
        if let Some(spec) = &args.actions {
            story::read_actions(spec)?;
        }
        let given = given_files(args);
        let source = source_of(args);
        super::files::start_noting();
        let ran = mount_story(lua, &self.engine, project, args, source.as_ref(), self.target).map(|mut mounted| {
            let actions = args.actions.as_ref().map(|spec| story::load_actions(lua, spec));
            let mut base_roots = mounted.roots.clone();
            let step = run_on(lua, &self.engine, args, project, &mut mounted, &mut base_roots, source.as_ref(), actions.clone());
            (mounted, base_roots, actions, step)
        });
        let noted = super::files::stop_noting();
        let (story, base_roots, actions, step) = ran?;
        let Step { output, reached, mut refusal } = step?;
        if let Some(why) = noted.changed() {
            refusal.get_or_insert(why);
        }
        let kept = Kept {
            story,
            key: run_key(args),
            reached,
            dispatched: actions.and_then(Result::ok).unwrap_or_default(),
            base_roots,
            noted: if noted.len() > 0 { vec![noted] } else { Vec::new() },
            given,
            refusal,
            checked: std::time::Duration::ZERO,
            loaded: self,
        };
        Ok((output, kept))
    }
}

/// The options two renders must share for the later one to go on from the earlier one's run
/// ([`Kept`]): every argument but the moment, the actions (compared apart) and the output paths
pub fn run_key(args: &Args) -> String {
    let mut key = args.clone();
    key.time = None;
    key.actions = None;
    key.dump_scene = None;
    key.dependencies = None;
    format!("{key:?}")
}

/// A render's run kept for a later moment of its story (the render service keeps the last one,
/// serve.rs): the VM with the story mounted and its clock at the moment the render captured. A
/// render with the same options ([`run_key`]) at that moment or a later one goes on from there
/// ([`Kept::resume`]): the clock runs on, with the actions that come after the ones dispatched,
/// and the scene is laid out and exported as a run from the start exports it at that moment. A
/// loop of `render --time t` over a story's frames then costs a frame each instead of a mount each.
///
/// A run is gone on from only while that is a run from the start ([`Kept::refusal`]): no action
/// ran at the capture instead of at its own later time, the frame loop did not fail, no mount was
/// still yielding, settling, laying out and exporting wrote no output and queued no event, there is
/// no recorded clock or sibling-order replay, and no file a run read has changed since
/// ([`Kept::changed`]; the service checks the project's structure too, serve.rs `fingerprint`).
/// Scripts that read the wall clock (tick, os.time, DateTime) read it from the run's start on, as in
/// a sequence. Fields drop in order: the story (Lua references), then the VM and engine.
pub struct Kept {
    story: story::Story,
    key: String,
    /// The engine moment the clock stands at
    reached: f64,
    /// The `--actions` dispatched so far, in the order they ran (load_actions)
    dispatched: Vec<Json>,
    /// The story's own roots (`--all-layers` exports the ScreenGuis found from them at each render)
    base_roots: Vec<super::InstanceId>,
    /// What the runs read outside the project's listing, and the listings they made, as they were then
    noted: Vec<super::files::Noted>,
    /// The files the arguments name that a run reads itself, as they were before it started
    given: Vec<(String, Option<super::files::Stamp>)>,
    refusal: Option<String>,
    /// How long the last [`Kept::resume`] took to check the files the runs read
    checked: std::time::Duration,
    loaded: Preloaded,
}

/// What [`Kept::resume`] did
pub enum Resumed {
    /// The run went on to the moment asked for (whether it can go on again: [`Kept::refusal`])
    Ran(Result<SceneOutput, String>),
    /// Nothing ran: why this run cannot go on to that render
    Refused(String),
}

impl Kept {
    /// Why going on from this run would not be a run from the start (None: it would be)
    pub fn refusal(&self) -> Option<&str> {
        self.refusal.as_deref()
    }

    /// The engine moment the run's clock stands at
    pub fn reached(&self) -> f64 {
        self.reached
    }

    /// How long the last [`Kept::resume`] took to check that no file the runs read has changed
    pub fn checked(&self) -> std::time::Duration {
        self.checked
    }

    /// The run of `args` from where this one stands, when `args` asks for its story with its
    /// options ([`run_key`]) at its moment or a later one, with the actions dispatched so far first
    /// (in their order) and the others after its moment, and no file it read has changed; else
    /// nothing runs and the answer says why. `roots_unchanged`: the caller knows no file under the
    /// project's roots changed since they were last found unchanged (watch.rs counts), so the project
    /// files the runs required are not stamped again.
    pub fn resume(&mut self, args: &Args, roots_unchanged: bool) -> Resumed {
        if let Some(why) = &self.refusal {
            return Resumed::Refused(why.clone());
        }
        if run_key(args) != self.key {
            return Resumed::Refused("other options".into());
        }
        let engine = self.loaded.engine.clone();
        let until = capture_time(&engine, args.time.unwrap_or(1.0));
        if until + 1e-9 < self.reached {
            return Resumed::Refused(format!("{until:.4} s is before the {:.4} s it stands at", self.reached));
        }
        let checking = Instant::now();
        let changed = self.changed(roots_unchanged);
        self.checked = checking.elapsed();
        if let Some(why) = changed {
            return Resumed::Refused(why);
        }
        let (Some(project), Some(lua)) = (self.loaded.project.as_mut(), self.loaded.lua.as_ref()) else {
            return Resumed::Refused("the VM was released".into());
        };
        let actions = match args.actions.as_ref().map(|spec| story::load_actions(lua, spec)) {
            None => None,
            Some(Ok(actions)) => Some(actions),
            Some(Err(error)) => return Resumed::Refused(error),
        };
        let done = self.dispatched.len();
        let all = actions.clone().unwrap_or_default();
        if all.len() < done || all[..done] != self.dispatched[..] {
            return Resumed::Refused("other actions".into());
        }
        if all[done..].iter().any(|action| action.get("t").and_then(Json::as_f64).unwrap_or(0.0) <= self.reached + 1e-9) {
            return Resumed::Refused("an action at or before the moment it stands at".into());
        }
        project.started = Instant::now().checked_sub(std::time::Duration::from_secs_f64(project.mount_time)).unwrap_or(project.started);
        let source = source_of(args);
        super::files::start_noting();
        let step = run_on(lua, &engine, args, project, &mut self.story, &mut self.base_roots, source.as_ref(), actions.map(|actions| Ok(actions[done..].to_vec())));
        let noted = super::files::stop_noting();
        match step {
            Ok(Step { output, reached, refusal }) => {
                self.reached = reached;
                self.dispatched = all;
                self.refusal = refusal.or_else(|| noted.changed());
                if noted.len() > 0 {
                    self.noted.push(noted);
                }
                Resumed::Ran(Ok(output))
            }
            Err(error) => {
                self.refusal = Some(error.clone());
                Resumed::Ran(Err(error))
            }
        }
    }

    /// The first file a run read that is not as it was then, as a reason (None: none changed): the
    /// files the arguments name, what it read outside the project's listing, and (unless
    /// `roots_unchanged`) the project files it required, against the listing (taken before the VM read
    /// any of them)
    fn changed(&self, roots_unchanged: bool) -> Option<String> {
        if let Some((path, _)) = self.given.iter().find(|(path, was)| super::files::stamp(std::path::Path::new(path)) != *was) {
            return Some(format!("{path} changed"));
        }
        if let Some(why) = self.noted.iter().find_map(super::files::Noted::changed) {
            return Some(why);
        }
        if roots_unchanged {
            return None;
        }
        let engine = &self.loaded.engine;
        let read = super::lua::modules::read_files(engine);
        let state = engine.state.borrow();
        read.iter()
            .find(|path| state.project.stamps.get(path.as_str()).is_some_and(|listed| super::files::stamp(std::path::Path::new(path.as_str())) != Some(*listed)))
            .map(|path| format!("{path} changed"))
    }
}

/// The files `args` names that a run reads itself rather than through the project's listing (the
/// source: a fixture's, or a story's outside the roots; a --controls file; --load models), with their stamps
fn given_files(args: &Args) -> Vec<(String, Option<super::files::Stamp>)> {
    let mut paths: Vec<String> = [&args.story, &args.module, &args.fixture, &args.model].into_iter().flatten().map(|path| absolute(path)).collect();
    if let Some(controls) = args.controls.as_ref().filter(|spec| !spec.trim_start().starts_with('{')) {
        paths.push(absolute(controls));
    }
    paths.extend(args.loads.iter().filter_map(|spec| spec.split_once('=')).map(|(_, file)| absolute(file)));
    paths
        .into_iter()
        .map(|path| {
            let stamp = super::files::stamp(std::path::Path::new(&path));
            (path, stamp)
        })
        .collect()
}

/// A step of a kept run besides its output
struct Step {
    output: SceneOutput,
    reached: f64,
    refusal: Option<String>,
}

/// One render of a kept run from where its clock stands: the clock to `args`' moment (`actions`:
/// the ones to dispatch on the way, loaded; None without `--actions`), then what mount_and_run does
/// after its frame loop and the export ([`scene_output`]), noting what would make going on from
/// here differ from a run from the start ([`Kept`]).
#[allow(clippy::too_many_arguments)]
fn run_on(
    lua: &Lua,
    engine: &Rc<Engine>,
    args: &Args,
    project: &story::Project,
    mounted: &mut story::Story,
    base_roots: &mut Vec<super::InstanceId>,
    source: Option<&story::Source>,
    actions: Option<Result<Vec<Json>, String>>,
) -> Result<Step, String> {
    let lua_error = |error: LuaError| error.to_string();
    let mut refusal: Option<String> = None;
    let reached = capture_time(engine, args.time.unwrap_or(1.0));
    let exec_start = Instant::now();
    let frame_loop = match actions {
        None => super::scheduler::run_until(lua, engine, reached),
        Some(Err(error)) => Err(LuaError::runtime(error)),
        Some(Ok(actions)) => {
            let mut timeline = story::Timeline::new(actions);
            let advanced = timeline.advance(lua, engine, reached, true, &mut story::input_dispatch, &mut |lua, engine, at| super::scheduler::run_until(lua, engine, at));
            if timeline.clamped() {
                refusal.get_or_insert_with(|| "an action ran at the capture instead of at its own later time".into());
            }
            advanced.and_then(|()| timeline.finish())
        }
    };
    if !mounted.protected(engine, "frame loop", frame_loop) {
        refusal.get_or_insert_with(|| "the frame loop failed".into());
    }
    // What a run from the start would not have done by this moment, and going on from here would
    // keep: output written, an unemulated member used, an event queued for the scripts
    let said = |engine: &Rc<Engine>| {
        let state = engine.state.borrow();
        (state.output.entries.len(), state.output.missing.iter().map(|(_, count)| *count).sum::<u64>(), state.sched.deferred_len())
    };
    let before = said(engine);
    mounted.settle(lua, engine).map_err(lua_error)?;
    if mounted.status == "yielded" {
        refusal.get_or_insert_with(|| "the story's mount still yields".into());
    }
    // Widgets a plugin creates while it runs (after a yield) are exported too
    mounted.roots = base_roots.clone();
    story::add_plugin_widgets(project, mounted);
    *base_roots = mounted.roots.clone();
    story::apply_order(lua, engine, mounted);
    if std::env::var("RUR_ORDER").is_ok_and(|file| !file.is_empty()) {
        refusal.get_or_insert_with(|| "RUR_ORDER replays a sibling order".into());
    }
    if args.all_layers {
        mounted.roots = super::layers::screen_layers(engine, base_roots);
    }
    let output = scene_output(engine, args, project, mounted, source, exec_start)?;
    if said(engine) != before {
        refusal.get_or_insert_with(|| "settling, laying out or exporting wrote output or queued an event".into());
    }
    if engine.state.borrow().sched.has_recorded_clock() {
        refusal.get_or_insert_with(|| "a recorded clock".into());
    }
    Ok(Step { output, reached, refusal })
}

/// What a sequence captures (`rur sequence`, sequence.rs): `frames` frames `1 / fps` s apart, the first at
/// `from` s (the engine frame nearest it, as `--time`)
#[derive(Clone, Debug)]
pub struct SequenceSpec {
    pub from: f64,
    pub frames: u32,
    pub fps: f64,
}

/// One captured moment of a sequence: the session's reply (the first, "ready", holds the whole scene and
/// the mount's diagnostics; the rest deltas, or `unchanged`) and the virtual time it shows
pub struct SequenceFrame {
    pub index: u32,
    pub time: f64,
    pub reply: Json,
}

/// What a sequence's engine half sends its draw half, in order: every frame, then the end
pub enum SequenceMessage {
    Frame(SequenceFrame),
    /// `{status, failure, diagnostics}` of the story after the last frame (its script output in full),
    /// or why the sequence could not run
    End(Result<Json, String>),
}

/// A sequence's engine half in a loaded VM: the story is mounted once, its clock run to the first
/// frame's moment as `render --time` runs it (with the `--actions` due by then), then for each further
/// frame the clock runs on to that frame's moment and the frame is captured. The capture is a live
/// session's (session.rs: layout and the retained export's delta), so a frame costs its own simulation,
/// not a run from 0. Each frame shows an exact moment: `from + index / fps` s on the engine's 60 Hz grid
/// when `fps` divides 60 (whole engine frames, as `render --time` steps them: frame `i` is `--time` at its
/// moment), else one display-cadence render step of `1 / fps` s per frame (a high-refresh display: the
/// edit-mode Heartbeats keep their own 240 Hz deadlines). Actions run when the clock reaches their time
/// (story::Timeline); later ones wait for later frames. `sink` returns false when nobody takes frames
/// any more (the draw half failed or restarts): the run stops there.
fn run_sequence(
    lua: &Lua,
    engine: &Rc<Engine>,
    project: story::Project,
    args: &Args,
    target: (f64, f64),
    spec: &SequenceSpec,
    sink: &mut dyn FnMut(SequenceMessage) -> bool,
) -> Result<(), String> {
    let lua_error = |error: LuaError| error.to_string();
    if !(spec.fps > 0.0 && spec.fps.is_finite()) || spec.frames == 0 {
        return Err(format!("a sequence needs frames (got {}) and a positive fps (got {})", spec.frames, spec.fps));
    }
    let run_started = Instant::now();
    let source = source_of(args);
    let mut mounted = mount_story(lua, engine, &project, args, source.as_ref(), target)?;
    let actions = match &args.actions {
        Some(actions) => story::load_actions(lua, actions)?,
        None => Vec::new(),
    };
    let frame = engine.state.borrow().sched.frame_duration;
    let first = (spec.from.max(0.0) / frame).round();
    // Engine frames per captured frame: whole ones when the rate divides the engine's
    let ratio = 1.0 / (spec.fps * frame);
    let whole = ratio.round() >= 1.0 && (ratio - ratio.round()).abs() < 1e-6;
    let moment = |index: u32| if whole { (first + f64::from(index) * ratio.round()) * frame } else { first * frame + f64::from(index) / spec.fps };
    let mut timeline = story::Timeline::new(actions);
    let mut whole_frames = |lua: &Lua, engine: &Rc<Engine>, at: f64| super::scheduler::run_until(lua, engine, at);
    let start = timeline.advance(lua, engine, moment(0), spec.frames == 1, &mut story::input_dispatch, &mut whole_frames);
    mounted.protected(engine, "frame loop", start);
    mounted.settle(lua, engine).map_err(lua_error)?;
    story::add_plugin_widgets(&project, &mut mounted);
    story::apply_order(lua, engine, &mut mounted);
    let base_roots = mounted.roots.clone();
    if args.all_layers {
        mounted.roots = super::layers::screen_layers(engine, &base_roots);
    }
    let background = args.background.unwrap_or([0.18, 0.18, 0.18]);
    let mut session = super::session::Session::new(engine, project, mounted, background, true);
    session.transparent = args.transparent;
    session.run_time = run_started.elapsed().as_secs_f64();
    let ready = session.ready(lua).map_err(lua_error)?;
    let now = |engine: &Rc<Engine>| engine.state.borrow().sched.now;
    if !sink(SequenceMessage::Frame(SequenceFrame { index: 0, time: now(engine), reply: ready })) {
        return Ok(());
    }
    // The frames' own work (stepping and capturing), apart from the time the drawing kept them waiting
    let frames_started = Instant::now();
    let mut blocked = std::time::Duration::ZERO;
    for index in 1..spec.frames {
        let until = moment(index);
        let last = index + 1 == spec.frames;
        let steps_before = engine.state.borrow().sched.frame;
        let result = if whole {
            timeline.advance(lua, engine, until, last, &mut story::input_dispatch, &mut whole_frames)
        } else {
            // One render step from the previous frame's moment to this one's, however many actions fall
            // between them (they run at this frame)
            let mut display = |lua: &Lua, engine: &Rc<Engine>, at: f64| {
                let current = engine.state.borrow().sched.now;
                if at > current + 1e-9 {
                    super::scheduler::step_display_duration(lua, engine, until - current)?;
                }
                Ok(())
            };
            timeline.advance(lua, engine, until, last, &mut story::input_dispatch, &mut display)
        };
        session.story.protected(engine, "frame loop", result);
        if args.all_layers {
            session.story.roots = super::layers::screen_layers(engine, &base_roots);
        }
        let steps = engine.state.borrow().sched.frame.saturating_sub(steps_before) as u32;
        let reply = session.capture(lua, steps);
        let sent = Instant::now();
        if !sink(SequenceMessage::Frame(SequenceFrame { index, time: now(engine), reply })) {
            return Ok(());
        }
        blocked += sent.elapsed();
    }
    let failed = timeline.finish();
    session.story.protected(engine, "frame loop", failed);
    let mut end = session.end_report(lua);
    end["engine"] = json!({
        "mount": session.run_time,
        "frames": frames_started.elapsed().saturating_sub(blocked).as_secs_f64(),
        "blocked": blocked.as_secs_f64(),
    });
    sink(SequenceMessage::End(Ok(end)));
    Ok(())
}

/// The run after the project mount: the story, virtual time (or the --actions timeline), layout and
/// the export.
fn run_scene(
    lua: &Lua,
    engine: &Rc<Engine>,
    args: &Args,
    project: &story::Project,
    target: (f64, f64),
) -> Result<SceneOutput, String> {
    let source = source_of(args);
    let (mounted, exec_start) = mount_and_run(lua, engine, project, args, source.as_ref(), target)?;
    scene_output(engine, args, project, &mounted, source.as_ref(), exec_start)
}

/// The scene of a run's story as it stands after its frame loop: a full layout pass and the export
/// (`exec_start`: when the frame loop began)
fn scene_output(
    engine: &Rc<Engine>,
    args: &Args,
    project: &story::Project,
    mounted: &story::Story,
    source: Option<&story::Source>,
    exec_start: Instant,
) -> Result<SceneOutput, String> {
    let run_time = args.time.unwrap_or(1.0);
    let background = args.background.unwrap_or([0.18, 0.18, 0.18]);

    // main.luau: Layout.Invalidate() then Layout.UpdateAll(Game) (a full pass) before the export
    super::layout::invalidate(engine);
    let records = story::layout(engine, &mounted.roots);
    let exec_time = exec_start.elapsed().as_secs_f64();

    let mut extra = Map::new();
    let mut source_map = Map::new();
    if let Some(source) = source {
        let path = json!(source.path);
        match source.kind {
            story::SourceKind::Story => source_map.insert("story".into(), path),
            story::SourceKind::Model => source_map.insert("model".into(), path),
            story::SourceKind::Module => source_map.insert("module".into(), path),
            story::SourceKind::Fixture => {
                source_map.insert("module".into(), path.clone());
                source_map.insert("fixture".into(), path)
            }
        };
    }
    source_map.insert("time".into(), export::number(run_time));
    source_map.insert("mode".into(), json!(engine.options.mode));
    extra.insert("source".into(), Json::Object(source_map));
    // A transparent run's scene has none: the frame is cleared to alpha 0 (render/scene_draw.rs)
    let background = if args.transparent { Json::Null } else { Json::Array(background.iter().map(|v| export::number(*v)).collect()) };
    extra.insert("background".into(), background);
    extra.insert("status".into(), json!(mounted.status));
    if let Some(failure) = &mounted.failure {
        extra.insert("failure".into(), json!(failure));
    }
    let export_start = Instant::now();
    extra.insert("diagnostics".into(), story::diagnostics(engine, project, mounted, exec_time));
    let mut scene = export::build(engine, &mounted.roots, &records, extra);
    if let Some(timings) = scene.pointer_mut("/diagnostics/timings").and_then(Json::as_object_mut) {
        timings.insert("export".into(), export::number(export_start.elapsed().as_secs_f64()));
    }
    empty_arrays_as_objects(&mut scene);
    let nodes = scene.get("nodes").and_then(Json::as_array).map_or(0, Vec::len);
    let lines = engine.state.borrow().output.entries.len();
    let status = mounted.status.clone();
    let scene = sort_keys(scene);
    let text = serde_json::to_string(&scene).map_err(|e| e.to_string())?;
    Ok(SceneOutput { text, scene, status, nodes, lines, dependencies: super::lua::modules::required_files(engine) })
}

/// Lune's JSON encoder writes an empty table as `{}`.
fn empty_arrays_as_objects(value: &mut Json) {
    match value {
        Json::Array(items) if items.is_empty() => *value = Json::Object(Map::new()),
        Json::Array(items) => items.iter_mut().for_each(empty_arrays_as_objects),
        Json::Object(map) => map.values_mut().for_each(empty_arrays_as_objects),
        _ => {}
    }
}

/// Keys in sorted order (the Luau runtime's encoder sorts them).
fn sort_keys(value: Json) -> Json {
    match value {
        Json::Object(map) => {
            let mut entries: Vec<(String, Json)> = map.into_iter().collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            let mut sorted = Map::new();
            for (key, value) in entries {
                sorted.insert(key, sort_keys(value));
            }
            Json::Object(sorted)
        }
        Json::Array(items) => Json::Array(items.into_iter().map(sort_keys).collect()),
        other => other,
    }
}


#[cfg(test)]
mod lifetime_tests {
    use super::*;

    /// A finished run frees its engine: nothing the Luau VM keeps (closures, userdata, registry
    /// values) holds the engine after the VM is dropped (the render service runs thousands of VMs)
    #[test]
    fn a_dropped_vm_frees_its_engine() {
        let repo = crate::engine::default_repo();
        let args = parse(&[
            "--repo".into(), repo.to_string_lossy().into_owned(),
            "--roots".into(), repo.join("tools/tests/oracle-root").to_string_lossy().into_owned(),
            "--fixture".into(), repo.join("fixtures/canvasgroup.luau").to_string_lossy().into_owned(),
            "--viewport".into(), "400x300".into(), "--time".into(), "0.1".into(), "--no-snapshots".into(),
        ]).unwrap();
        let preloaded = Preloaded::load(&args).unwrap();
        let weak = Rc::downgrade(&preloaded.engine);
        let weak_lua = preloaded.lua.as_ref().unwrap().weak();
        let output = preloaded.run(&args).unwrap();
        assert_eq!(output.status, "ok");
        assert!(weak_lua.try_upgrade().is_none(), "the Luau VM outlived its run");
        assert!(weak.upgrade().is_none(), "the engine outlived its VM ({} strong references)", weak.strong_count());
    }

    /// Closing a story and opening it again (the desktop app's story switch, Reload and Restart:
    /// a new Host each time, the old one dropped) starts from nothing and leaves nothing: every
    /// closed story's VM and engine are freed, with every instance it made. (The process's memory
    /// over reopens is measured by tools/native-app/scroll-bench.mjs --script reopen: other tests
    /// share this test process.)
    #[test]
    fn reopening_a_story_frees_the_previous_one() {
        let repo = crate::engine::default_repo();
        let args = parse(&[
            "--repo".into(), repo.to_string_lossy().into_owned(),
            "--roots".into(), repo.join("tools/tests/oracle-root").to_string_lossy().into_owned(),
            "--fixture".into(), repo.join("fixtures/live-scroll.luau").to_string_lossy().into_owned(),
            "--viewport".into(), "400x300".into(), "--time".into(), "0.1".into(), "--no-snapshots".into(),
            "--session".into(), "1".into(),
        ]).unwrap();
        let mut instances = Vec::new();
        for _ in 0..40 {
            let mut host = Host::start(&args).unwrap();
            let ready = host.mount(&args).unwrap();
            assert_eq!(ready["type"], "ready", "{ready}");
            let mut last = Json::Null;
            for _ in 0..4 {
                last = host.handle(&json!({ "cmd": "frame", "dt": 1.0 / 60.0, "events": [] })).unwrap();
            }
            instances.push(last["timings"]["instances"].as_u64().unwrap());
            let (engine, lua) = (Rc::downgrade(&host.engine), host.lua.weak());
            drop(host);
            assert!(lua.try_upgrade().is_none(), "the Luau VM outlived its story");
            assert!(engine.upgrade().is_none(), "the engine outlived its story ({} strong references)", engine.strong_count());
        }
        assert!(instances.iter().all(|&count| count == instances[0] && count > 900), "every open starts from the same instances: {instances:?}");
    }
}

#[cfg(test)]
mod kept_tests {
    use super::*;

    fn args(fixture: &std::path::Path, roots: &std::path::Path, extra: &[&str]) -> Args {
        let repo = crate::engine::default_repo();
        let mut argv: Vec<String> = vec![
            "--repo".into(), repo.to_string_lossy().into_owned(),
            "--roots".into(), roots.to_string_lossy().into_owned(),
            "--fixture".into(), fixture.to_string_lossy().into_owned(),
            "--viewport".into(), "800x520".into(), "--no-snapshots".into(),
        ];
        argv.extend(extra.iter().map(|arg| arg.to_string()));
        parse(&argv).unwrap()
    }

    fn fixture(name: &str) -> std::path::PathBuf {
        crate::engine::default_repo().join("fixtures").join(name)
    }

    fn oracle_root() -> std::path::PathBuf {
        crate::engine::default_repo().join("tools/tests/oracle-root")
    }

    /// The scene as two runs of one moment must both write it: all of it but the wall-clock timings
    fn comparable(output: &SceneOutput) -> String {
        let mut scene = output.scene.clone();
        if let Some(diagnostics) = scene.get_mut("diagnostics").and_then(Json::as_object_mut) {
            diagnostics.remove("timings");
        }
        serde_json::to_string(&scene).unwrap()
    }

    /// Where two scenes first differ (a node's path or a top-level field), for the assertion message
    fn first_difference(a: &str, b: &str) -> String {
        let (a, b): (Json, Json) = (serde_json::from_str(a).unwrap(), serde_json::from_str(b).unwrap());
        let (nodes_a, nodes_b) = (a["nodes"].as_array().cloned().unwrap_or_default(), b["nodes"].as_array().cloned().unwrap_or_default());
        if let Some((x, y)) = nodes_a.iter().zip(&nodes_b).find(|(x, y)| x != y) {
            return format!("node {}: {x} vs {y}", x["path"]);
        }
        if nodes_a.len() != nodes_b.len() {
            return format!("{} nodes vs {}", nodes_a.len(), nodes_b.len());
        }
        let keys: Vec<&String> = a.as_object().unwrap().keys().filter(|key| a[key.as_str()] != b[key.as_str()]).collect();
        format!("fields {keys:?}")
    }

    fn resumed(kept: &mut Kept, args: &Args) -> SceneOutput {
        match kept.resume(args, false) {
            Resumed::Ran(output) => output.unwrap(),
            Resumed::Refused(why) => panic!("refused: {why}"),
        }
    }

    /// A kept run gone on to later moments writes the scene a run from the start writes at each of them:
    /// nodes, layers, fonts, images, status, script output and diagnostics (timings apart), with tweens,
    /// springs and Vide springs moving in between; the same moment again is the same scene, an earlier one
    /// is refused
    #[test]
    fn a_kept_run_goes_on_as_a_run_from_the_start() {
        let root = oracle_root();
        for name in ["anim-tweens.luau", "anim-mount.luau"] {
            let at = |time: &str| args(&fixture(name), &root, &["--time", time]);
            let (first, mut kept) = Preloaded::load(&at("0.1")).unwrap().run_kept(&at("0.1")).unwrap();
            assert_eq!(kept.refusal(), None, "{name}");
            let fresh = comparable(&run_once(&at("0.1")).unwrap());
            assert!(comparable(&first) == fresh, "{name} at 0.1: {}", first_difference(&comparable(&first), &fresh));
            for time in ["0.1167", "0.5", "0.5", "1.25", "2.0167"] {
                let output = comparable(&resumed(&mut kept, &at(time)));
                let fresh = comparable(&run_once(&at(time)).unwrap());
                assert!(output == fresh, "{name} at {time}: {}", first_difference(&output, &fresh));
                assert_eq!(kept.refusal(), None, "{name} at {time}");
            }
            assert!(matches!(kept.resume(&at("1.0"), false), Resumed::Refused(why) if why.contains("before")), "{name}: an earlier moment");
        }
    }

    /// Other options (controls, viewport, background) are refused; the run is left as it was
    #[test]
    fn other_options_are_refused() {
        let root = oracle_root();
        let at = |extra: &[&str]| args(&fixture("anim-tweens.luau"), &root, extra);
        let (_, mut kept) = Preloaded::load(&at(&["--time", "0.2"])).unwrap().run_kept(&at(&["--time", "0.2"])).unwrap();
        for other in [&["--time", "0.5", "--background", "10,20,30"][..], &["--time", "0.5", "--controls", "{\"A\":1}"], &["--time", "0.5", "--scale", "2"]] {
            assert!(matches!(kept.resume(&at(other), false), Resumed::Refused(why) if why == "other options"), "{other:?}");
        }
        let output = comparable(&resumed(&mut kept, &at(&["--time", "0.5"])));
        assert_eq!(output, comparable(&run_once(&at(&["--time", "0.5"])).unwrap()));
    }

    /// With --actions: a later render lists the actions dispatched so far first and adds later ones,
    /// which run as the clock reaches them, as in a run from the start; a list that changes a
    /// dispatched one, or adds one at or before the moment the run stands at, is refused; a run whose
    /// timeline ran an action at the capture instead of at its own later time is not gone on from
    #[test]
    fn a_kept_run_goes_on_with_later_actions() {
        let root = oracle_root();
        let text = std::fs::read_to_string(fixture("interaction-textbox.actions.json")).unwrap();
        let all: Vec<Json> = serde_json::from_str(&text).unwrap();
        let until = |time: f64| serde_json::to_string(&all.iter().filter(|action| action["t"].as_f64().unwrap() <= time).collect::<Vec<_>>()).unwrap();
        let at = |time: &str, actions: &str| args(&fixture("interaction-textbox.luau"), &root, &["--time", time, "--actions", actions]);
        let (first, mut kept) = Preloaded::load(&at("0.2", &until(0.2))).unwrap().run_kept(&at("0.2", &until(0.2))).unwrap();
        assert_eq!(kept.refusal(), None);
        assert_eq!(comparable(&first), comparable(&run_once(&at("0.2", &until(0.2))).unwrap()));
        let changed = until(0.2).replacen("400", "401", 1);
        assert!(matches!(kept.resume(&at("0.3", &changed), false), Resumed::Refused(why) if why == "other actions"));
        let early = serde_json::to_string(&[all[0].clone(), all[1].clone(), all[2].clone(), all[3].clone(), json!({"t": 0.2000000001, "type": "move", "x": 10, "y": 10})]).unwrap();
        assert!(matches!(kept.resume(&at("0.3", &early), false), Resumed::Refused(why) if why.contains("at or before")));
        for time in [0.3, 0.45, 0.7] {
            let actions = until(time);
            let output = comparable(&resumed(&mut kept, &at(&time.to_string(), &actions)));
            let fresh = comparable(&run_once(&at(&time.to_string(), &actions)).unwrap());
            assert!(output == fresh, "at {time}: {}", first_difference(&output, &fresh));
        }
        // 0.0167 is a hair past frame 1, where the capture runs it: a later run would run it at frame 2
        let clamped = at("0.0167", "[{\"t\":0.0167,\"type\":\"move\",\"x\":5,\"y\":5}]");
        let (_, kept) = Preloaded::load(&clamped).unwrap().run_kept(&clamped).unwrap();
        assert!(kept.refusal().is_some_and(|why| why.contains("its own later time")), "{:?}", kept.refusal());
    }

    /// A run is not gone on from once a file it read changed: the fixture itself (read by the mount)
    /// or a module it required
    #[test]
    fn a_changed_file_ends_a_kept_run() {
        let directory = std::env::temp_dir().join(format!("rur-kept-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let root = directory.join("root");
        std::fs::create_dir_all(root.join("ReplicatedStorage")).unwrap();
        let module = root.join("ReplicatedStorage").join("KeptText.luau");
        std::fs::write(&module, "return \"one\"\n").unwrap();
        let shown = directory.join("shown.luau");
        std::fs::write(&shown, "return function(Target)\n\tlocal Label = Instance.new(\"TextLabel\")\n\tLabel.Size = UDim2.fromOffset(200, 40)\n\tLabel.Text = require(game:GetService(\"ReplicatedStorage\").KeptText)\n\tLabel.Parent = Target\nend\n").unwrap();
        let at = |time: &str| args(&shown, &root, &["--time", time]);
        let (_, mut kept) = Preloaded::load(&at("0.1")).unwrap().run_kept(&at("0.1")).unwrap();
        resumed(&mut kept, &at("0.2"));
        std::fs::write(&module, "return \"two!\"\n").unwrap();
        assert!(matches!(kept.resume(&at("0.3"), false), Resumed::Refused(why) if why.contains("KeptText.luau changed")));
        let (_, mut kept) = Preloaded::load(&at("0.1")).unwrap().run_kept(&at("0.1")).unwrap();
        resumed(&mut kept, &at("0.2"));
        std::fs::write(&shown, format!("{}\n-- edited\n", std::fs::read_to_string(&shown).unwrap())).unwrap();
        // (the fixture is no project file: it is checked even when the roots are known unchanged)
        assert!(matches!(kept.resume(&at("0.3"), true), Resumed::Refused(why) if why.contains("shown.luau changed")));
        let _ = std::fs::remove_dir_all(directory);
    }
}

#[cfg(test)]
mod device_tests {
    use super::*;

    fn probe(extra: &[&str]) -> Result<String, String> {
        let repo = crate::engine::default_repo();
        let mut list: Vec<String> = vec![
            "--repo".into(), repo.to_string_lossy().into_owned(),
            "--roots".into(), repo.join("tools/tests/oracle-root").to_string_lossy().into_owned(),
            "--fixture".into(), repo.join("fixtures/device-probe.luau").to_string_lossy().into_owned(),
            "--viewport".into(), "400x300".into(), "--time".into(), "0".into(), "--no-snapshots".into(),
        ];
        list.extend(extra.iter().map(|arg| arg.to_string()));
        let args = parse(&list)?;
        Ok(run_once(&args)?.text)
    }

    /// `--device` and `--topbar-inset` reach the scripts: UserInputService reports the device's
    /// input and GuiService:GetGuiInset() the inset, and a default ScreenGui (CoreUISafeInsets)
    /// starts below the inset while the story's (IgnoreGuiInset) does not; without them a run is
    /// Studio's edit-mode desktop.
    #[test]
    fn the_device_and_inset_reach_the_scripts() {
        let desktop = probe(&[]).unwrap();
        assert!(desktop.contains("touch=false mouse=true keyboard=true gamepad=false last=Focus inset=0 story=0 gui=0,300"), "{desktop}");
        let phone = probe(&["--device", "touch", "--topbar-inset", "58"]).unwrap();
        assert!(phone.contains("touch=true mouse=false keyboard=false gamepad=false last=Touch inset=58 story=0 gui=58,242"), "{phone}");
        let pad = probe(&["--device", "gamepad"]).unwrap();
        assert!(pad.contains("gamepad=true last=Gamepad1"), "{pad}");
        assert!(probe(&["--device", "phone"]).unwrap_err().contains("--device"));
    }

    fn flow(device: &str) -> String {
        let repo = crate::engine::default_repo();
        let list: Vec<String> = vec![
            "--repo".into(), repo.to_string_lossy().into_owned(),
            "--roots".into(), repo.join("tools/tests/oracle-root").to_string_lossy().into_owned(),
            "--fixture".into(), repo.join("fixtures/input-device-flow.luau").to_string_lossy().into_owned(),
            "--viewport".into(), "400x300".into(), "--time".into(), "0.2".into(), "--no-snapshots".into(),
            "--device".into(), device.into(),
        ];
        run_once(&parse(&list).unwrap()).unwrap().text
    }

    /// PreferredInput and the glyph lookup follow the device; ContextActionService runs by priority,
    /// Pass goes on and Sink stops (also the selected button's activation and gameProcessedEvent);
    /// GuiService:Select takes a selectable descendant; a finger is one Touch InputObject.
    #[test]
    fn controller_and_touch_input_reach_the_scripts() {
        let pad = flow("gamepad");
        assert!(pad.contains("preferred=Gamepad glyph=rbxasset://textures/ui/Controls/XboxController/ButtonRT@2x.png"), "{pad}");
        assert!(pad.contains("selected=First"), "{pad}");
        assert!(pad.contains("high:Begin:Gamepad1 low:Begin down=true high:End:Gamepad1 low:End"), "{pad}");
        assert!(pad.contains("sinks processed=true sinks"), "{pad}");
        assert!(pad.contains("low:Begin low:End activated activated"), "{pad}");
        let phone = flow("touch");
        assert!(phone.contains("preferred=Touch"), "{phone}");
        assert!(phone.contains("touch=Touch same=true"), "{phone}");
        assert!(flow("desktop").contains("preferred=KeyboardAndMouse glyph= "), "desktop");
    }

    /// Runs on different devices or insets never share a standby VM.
    #[test]
    fn the_device_and_inset_are_startup_options() {
        let base = parse(&["--viewport".into(), "400x300".into()]).unwrap();
        let touch = parse(&["--viewport".into(), "400x300".into(), "--device".into(), "touch".into()]).unwrap();
        let inset = parse(&["--viewport".into(), "400x300".into(), "--topbar-inset".into(), "58".into()]).unwrap();
        assert_ne!(startup_key(&base), startup_key(&touch));
        assert_ne!(startup_key(&base), startup_key(&inset));
    }
}

#[cfg(test)]
mod background_tests {
    use super::*;

    /// `--background transparent` exports no background (render.png keeps the UI's coverage as
    /// alpha); a colour still exports its channels. The background is a run's own option: both
    /// runs share a preloaded VM.
    #[test]
    fn a_transparent_background_exports_none() {
        let repo = crate::engine::default_repo();
        let args = |background: &str| {
            parse(&[
                "--repo".into(), repo.to_string_lossy().into_owned(),
                "--roots".into(), repo.join("tools/tests/oracle-root").to_string_lossy().into_owned(),
                "--fixture".into(), repo.join("fixtures/corners.luau").to_string_lossy().into_owned(),
                "--viewport".into(), "400x300".into(), "--time".into(), "0".into(), "--no-snapshots".into(),
                "--background".into(), background.into(),
            ])
            .unwrap()
        };
        let transparent = args("transparent");
        assert!(transparent.transparent && transparent.background.is_none());
        assert_eq!(run_once(&transparent).unwrap().scene["background"], Json::Null);
        let colour = args("51,102,153");
        assert!(!colour.transparent && colour.background.is_some());
        assert_eq!(run_once(&colour).unwrap().scene["background"].as_array().map(Vec::len), Some(3));
        assert_eq!(startup_key(&transparent), startup_key(&colour));
    }
}

#[cfg(test)]
mod layer_tests {
    use super::*;

    fn args(fixture: &std::path::Path, extra: &[&str]) -> Args {
        let repo = crate::engine::default_repo();
        let mut argv: Vec<String> = vec![
            "--repo".into(), repo.to_string_lossy().into_owned(),
            "--roots".into(), repo.join("tools/tests/oracle-root").to_string_lossy().into_owned(),
            "--fixture".into(), fixture.to_string_lossy().into_owned(),
            "--viewport".into(), "300x240".into(), "--time".into(), "0".into(), "--no-snapshots".into(),
        ];
        argv.extend(extra.iter().map(|s| s.to_string()));
        parse(&argv).unwrap()
    }

    /// The exported layers in draw order (the renderer's: pass, then DisplayOrder, stable) by root name.
    fn drawn(scene: &Json) -> Vec<String> {
        let names: std::collections::HashMap<u64, &str> = scene["nodes"].as_array().into_iter().flatten()
            .filter_map(|n| Some((n["id"].as_u64()?, n["name"].as_str()?))).collect();
        let mut layers: Vec<&Json> = scene["layers"].as_array().into_iter().flatten().collect();
        layers.sort_by(|a, b| crate::scene::layer_pass(a).cmp(&crate::scene::layer_pass(b))
            .then(a["displayOrder"].as_f64().unwrap_or(0.).total_cmp(&b["displayOrder"].as_f64().unwrap_or(0.))));
        layers.iter().map(|l| names.get(&l["root"].as_u64().unwrap()).copied().unwrap_or("?").to_string()).collect()
    }

    /// `--all-layers` draws the PlayerGui pass under CoreGui's, then by DisplayOrder, ties by when a
    /// ScreenGui was last parented or enabled (fixtures/screen-layers.luau: Low was re-enabled last),
    /// looking through Folders and leaving out disabled ScreenGuis and SurfaceGuis. Without it only the
    /// story gui is exported, as before.
    #[test]
    fn all_layers_draw_in_roblox_order() {
        let fixture = crate::engine::default_repo().join("fixtures/screen-layers.luau");
        let all = run_once(&args(&fixture, &["--all-layers", "1"])).unwrap();
        assert_eq!(all.status, "ok");
        assert_eq!(drawn(&all.scene), ["Mid", "Nested", "Low", "High", "Core", "RURStory"]);
        let player_gui: Vec<bool> = all.scene["layers"].as_array().unwrap().iter().map(|l| l["playerGui"] == Json::Bool(true)).collect();
        assert_eq!(player_gui.iter().filter(|&&p| p).count(), 4);
        let story = run_once(&args(&fixture, &[])).unwrap();
        assert_eq!(drawn(&story.scene), ["RURStory"]);
        assert!(story.scene["layers"][0].get("playerGui").is_none());
        let all_args = args(&fixture, &["--all-layers", "1"]);
        assert_eq!(startup_key(&all_args), startup_key(&args(&fixture, &[])));
    }

    /// `--load Service/Name=<file.rbxm>` puts a model's top-level instances (two ScreenGuis here, as a
    /// SerializationService blob of a PlayerGui's children holds) in a Folder at that path for the
    /// fixture to clone; a path that does not start with a service is an error.
    #[test]
    fn load_puts_a_model_in_a_folder() {
        use rbx_dom_weak::{InstanceBuilder, WeakDom, types::{UDim, UDim2}};
        let directory = std::env::temp_dir().join(format!("rur-load-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(&directory).unwrap();
        let frame = |name: &str| InstanceBuilder::new("Frame").with_name(name).with_property("Size", UDim2::new(UDim::new(0., 50), UDim::new(0., 40)));
        let dom = WeakDom::new(InstanceBuilder::new("DataModel")
            .with_child(InstanceBuilder::new("ScreenGui").with_name("First").with_child(frame("A")))
            .with_child(InstanceBuilder::new("ScreenGui").with_name("Second").with_child(frame("B"))));
        let model = directory.join("blob.rbxm");
        let mut bytes = Vec::new();
        rbx_binary::to_writer(&mut bytes, &dom, dom.root().children()).unwrap();
        std::fs::write(&model, bytes).unwrap();
        let fixture = directory.join("clone.luau");
        std::fs::write(&fixture, "return function()\n\tlocal Blob = game:GetService('ServerStorage').Saved.Blob\n\tassert(Blob:IsA('Folder') and #Blob:GetChildren() == 2)\n\tfor _, Gui in Blob:GetChildren() do Gui:Clone().Parent = game:GetService('Players').LocalPlayer.PlayerGui end\nend\n").unwrap();
        let load = format!("ServerStorage/Saved/Blob={}", model.to_string_lossy());
        let output = run_once(&args(&fixture, &["--all-layers", "1", "--load", &load])).unwrap();
        assert_eq!(output.status, "ok", "{}", output.text);
        assert_eq!(drawn(&output.scene), ["First", "Second", "RURStory"]);
        let bad = format!("Saved/Blob={}", model.to_string_lossy());
        assert!(run_once(&args(&fixture, &["--load", &bad])).err().unwrap().contains("not a service"));
        std::fs::remove_dir_all(directory).unwrap();
    }
}
