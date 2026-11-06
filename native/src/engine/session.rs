//! Live sessions (port of `runtime/session.luau`): the mounted story keeps running and answers
//! frame / control / scene commands. The native app calls [`Session`] in process (replies are
//! `serde_json::Value`s its SceneStore applies, changed nodes as patches of the ones it holds);
//! `rur-native engine ... --session 1` speaks the stdio session protocol (the browser storybook)
//! ([`run_stdio`]: one JSON command per stdin line, one `@@RUR ` reply line each), which the
//! golden live corpus drives (tools/engine/golden.mjs).
//!
//! Commands:
//!   {"cmd":"frame","dt":0.0167,"events":[...],"cadence":"display"?}  input events, then whole
//!        frames of virtual time (display cadence: the elapsed Heartbeats, one render step), layout,
//!        export; the reply carries the scene delta, or `unchanged` when nothing could have changed
//!   {"cmd":"control","path":["Group","Name"],"value":...}   sets a UI Labs control source
//!   {"cmd":"scene"}   the full one-shot export of the current state
//!   {"cmd":"quit"}
//! The first reply ("ready") also carries the controls and the mount diagnostics.
//!
//! Input events go to [`Session::input`] (runtime/input's Dispatch: engine::input, installed by
//! Engine.Start); without one they are ignored, as the Luau session ignores them without
//! runtime/input. Presentation (predicted scrolling: a frame command's `presentation` packet,
//! events' `presentation` poses, the replies' `presentation` snapshot) and the clipboard of a
//! copy / cut (`clipboard`) go through engine::input as in runtime/session.luau.
//!
//! Frame replies come from the session's retained export (export::retained, Scene.NewCache): only
//! nodes whose inputs changed are rebuilt and sent (patches of their moving keys when the
//! appearance was reused, else whole nodes; in process the delta carries `retained`, which lets
//! SceneStore apply them without comparing), nodes the client holds
//! that the frame did not export are `removed` (outside presentation mode that includes nodes their
//! clip hides entirely, as in the Luau session), draw lists / fonts / header only when they changed.

use std::{
    cell::Cell,
    io::{BufRead, Write},
    rc::Rc,
    time::Instant,
};

use mlua::prelude::*;
use serde_json::{Map, Value as Json, json};

use crate::engine::GuiEngine;
use super::{
    Engine, profile, scheduler,
    story::{self, Project, Story},
};

/// Session.NewStepper: scripted callers accumulate whole fixed intervals; interactive displays
/// (cadence "display") consume the actual elapsed time, so a high-refresh display can present
/// more often than the fixed replay clock.
pub struct Stepper {
    frame: f64,
    pending: f64,
}

impl Stepper {
    pub fn new(engine: &Engine) -> Self {
        Stepper { frame: engine.state.borrow().sched.frame_duration, pending: 0.0 }
    }

    /// Advances by `dt` seconds: (render steps run, elapsed fixed intervals).
    pub fn advance(&mut self, lua: &Lua, engine: &Rc<Engine>, dt: f64, cadence: Option<&str>) -> LuaResult<(u32, f64)> {
        self.pending += dt.clamp(0.0, 0.25);
        let display = cadence == Some("display");
        if display && !engine.state.borrow().sched.has_recorded_clock() {
            let duration = self.pending;
            self.pending = 0.0;
            return Ok((scheduler::step_display_duration(lua, engine, duration)?, duration / self.frame));
        }
        let frames = (self.pending / self.frame + 1e-6).floor().max(0.0);
        self.pending -= frames * self.frame;
        let frames = frames as u32;
        if display && frames > 0 {
            return Ok((scheduler::step_display(lua, engine, frames)?, frames as f64));
        }
        for _ in 0..frames {
            scheduler::step(lua, engine, Some(self.frame))?;
        }
        Ok((frames, frames as f64))
    }
}

/// An input event handler (runtime/input Dispatch).
pub type InputHandler = Box<dyn FnMut(&Lua, &Rc<Engine>, &Json) -> LuaResult<()>>;

/// Timings / counters of the frame being answered (runtime/session.luau Frame profile): input
/// and simulation work excluding layout, and a frame command's input / simulation profiles
/// (engine::profile: `timings.inputParts` / `simulationParts` / `*Layout` / `*Callbacks`,
/// `counters.inputDetail` / `simulationDetail`).
#[derive(Default)]
struct Profile {
    started: Option<Instant>,
    /// Layout.ProfileTime when the command started (timings.layout is the layout since)
    layout_before: Option<f64>,
    input: f64,
    simulation: f64,
    events: usize,
    intervals: f64,
    input_detail: Option<profile::Report>,
    detail: Option<profile::Report>,
}

/// Context.Layout.ProfileTime
fn layout_time(engine: &Engine) -> f64 {
    super::layout::profile_time(engine).unwrap_or(0.0)
}

/// SimulationProfile.Begin(function() return Context.Layout.ProfileTime end); while a layout pass
/// holds the layout state the last read stands.
fn begin_profile(engine: &Rc<Engine>) {
    let engine = Rc::downgrade(engine);
    let last = Cell::new(0.0);
    profile::begin(
        Box::new(move || {
            if let Some(time) = engine.upgrade().and_then(|engine| super::layout::profile_time(&engine)) {
                last.set(time);
            }
            last.get()
        }),
        None,
    );
}

/// The session's `other` fill-up: work the profile's buckets did not see (outside its begin /
/// finish) goes to `other`, so the parts sum to the phase's work.
fn fill_other(report: &mut profile::Report, work: f64) {
    let partitioned: f64 = report.parts.iter().map(|(_, seconds)| seconds).sum();
    report.add_part("other", (work - partitioned).max(0.0));
}

pub struct Session {
    engine: Rc<Engine>,
    project: Project,
    pub story: Story,
    background: [f64; 3],
    /// `--background transparent`: the scene has no background, so frames keep the UI's own alpha
    pub transparent: bool,
    stepper: Stepper,
    /// Anything that can change the picture marks the scene dirty; clean frames skip layout + export
    dirty: Rc<Cell<bool>>,
    /// The retained export (Scene.NewCache) and the header the client holds
    cache: super::export::retained::Retained,
    sent_header: Option<Json>,
    last_nodes: usize,
    /// In-process clients (SceneStore): replies mark their nodes `delta.retained` (rebuilt nodes,
    /// applied without comparing); the stdio protocol leaves the flag out
    pub patches: bool,
    /// runtime/input Dispatch; None ignores input events
    pub input: Option<InputHandler>,
    /// Presentation enabled by the client (a frame command's `presentation.version == 1`)
    presentation: bool,
    /// Layout seconds during the last frame reply's build (subtracted from timings.export)
    layout_time: f64,
    /// Seconds of the one-shot run before the session (diagnostics timings.run)
    pub run_time: f64,
}

impl Session {
    /// Node entries the retained export holds (diagnostics and tests)
    pub fn export_entries(&self) -> usize {
        self.cache.entry_count()
    }

    /// Session.Run's setup: dirty tracking through the property / hierarchy hooks.
    pub fn new(engine: &Rc<Engine>, project: Project, story: Story, background: [f64; 3], patches: bool) -> Session {
        let dirty = Rc::new(Cell::new(true));
        {
            let mut hooks = engine.hooks.borrow_mut();
            let flag = dirty.clone();
            hooks.property_changed.push(Box::new(move |_, _, _| flag.set(true)));
            let flag = dirty.clone();
            hooks.hierarchy_changed.push(Box::new(move |_, _, _, _| flag.set(true)));
        }
        Session {
            engine: engine.clone(),
            stepper: Stepper::new(engine),
            project,
            story,
            background,
            transparent: false,
            dirty,
            cache: super::export::retained::Retained::new(engine),
            sent_header: None,
            last_nodes: 0,
            patches,
            input: super::input::get(engine).map(|input| {
                Box::new(move |lua: &Lua, engine: &Rc<Engine>, event: &Json| input.dispatch(lua, engine, event)) as InputHandler
            }),
            presentation: false,
            layout_time: 0.0,
            run_time: 0.0,
        }
    }

    pub fn engine(&self) -> &Rc<Engine> {
        &self.engine
    }

    /// Context.Status: pending mounts settle first.
    fn status(&mut self, lua: &Lua) -> LuaResult<(String, Option<String>)> {
        self.story.settle(lua, &self.engine)?;
        Ok((self.story.status.clone(), self.story.failure.clone()))
    }

    /// The session's scene header fields (background, status, failure).
    fn header_fields(&mut self, lua: &Lua) -> LuaResult<Map<String, Json>> {
        let (status, failure) = self.status(lua)?;
        let mut extra = Map::new();
        let background = if self.transparent { Json::Null } else { Json::Array(self.background.iter().map(|v| super::export::number(*v)).collect()) };
        extra.insert("background".into(), background);
        extra.insert("status".into(), Json::String(status));
        if let Some(failure) = failure {
            extra.insert("failure".into(), Json::String(failure));
        }
        Ok(extra)
    }

    /// The current state's one-shot scene (layout + export) with the session header fields.
    fn build(&mut self, lua: &Lua) -> LuaResult<Json> {
        let extra = self.header_fields(lua)?;
        Ok(story::build_scene(&self.engine, &self.story.roots, extra))
    }

    fn message(&self, kind: &str, frames: u32, profile: &Profile, layout: f64) -> Map<String, Json> {
        let mut message = Map::new();
        message.insert("type".into(), json!(kind));
        message.insert("time".into(), super::export::number(self.engine.state.borrow().sched.now));
        message.insert("frames".into(), json!(frames));
        let mut timings = json!({ "input": profile.input, "simulation": profile.simulation, "layout": layout, "export": 0,
            "build": 0, "serialize": 0, "total": 0, "exported": 0, "nodes": self.last_nodes });
        let mut counters = json!({ "inputEvents": profile.events, "simulationFrames": frames, "elapsedIntervals": profile.intervals,
            "nodes": self.last_nodes, "exported": 0, "changed": 0, "removed": 0 });
        for (detail, prefix, key) in [(&profile.input_detail, "input", "inputDetail"), (&profile.detail, "simulation", "simulationDetail")] {
            let Some(detail) = detail else { continue };
            timings[format!("{prefix}Parts")] = detail.parts_json();
            timings[format!("{prefix}Layout")] = json!(detail.layout);
            if let Some(callbacks) = detail.callbacks_json() {
                timings[format!("{prefix}Callbacks")] = callbacks;
            }
            counters[key] = detail.counts_json();
        }
        message.insert("timings".into(), timings);
        message.insert("counters".into(), counters);
        message
    }

    /// Session Frame: a reply for `kind` after `frames` render steps, with the scene delta (or
    /// `unchanged` for a clean "frame").
    fn frame_reply(&mut self, lua: &Lua, kind: &str, frames: u32, extra: Map<String, Json>, profile: Profile) -> LuaResult<Json> {
        let started = Instant::now();
        let _reply_scope = crate::microprofile::scope("Build reply");
        let layout_before = profile.layout_before.unwrap_or_else(|| layout_time(&self.engine));
        let mut message = self.message(kind, frames, &profile, layout_time(&self.engine) - layout_before);
        if kind == "frame" && !self.dirty.get() {
            message.insert("unchanged".into(), Json::Bool(true));
            self.insert_presentation(lua, &mut message);
            message.extend(extra);
            self.finish_timings(lua, &mut message, &profile, started, 0.0);
            return Ok(Json::Object(message));
        }
        // Frame replies: the session cache's export (runtime/session.luau Scene.Build with Cache)
        let fields = self.header_fields(lua)?;
        let build_layout_before = layout_time(&self.engine);
        {
            let _scope = crate::microprofile::scope("Layout");
            story::layout(&self.engine, &self.story.roots);
        }
        let mut export_scope = crate::microprofile::scope("Export scene");
        let update = self.cache.build(&self.engine, &self.story.roots);
        export_scope.set_detail(update.misses as u64);
        drop(export_scope);
        let build = started.elapsed().as_secs_f64();
        // Layout.ProfileTime during the build (the pass, and lazy layout during export)
        self.layout_time = layout_time(&self.engine) - build_layout_before;
        // Cleared after layout + export: their own writes (Absolute*, TextBounds) are not changes
        self.dirty.set(false);
        self.insert_presentation(lua, &mut message);
        message.extend(extra);
        let delta_started = Instant::now();
        let misses = update.misses;
        let delta_scope = crate::microprofile::scope("Serialize delta");
        self.delta(&mut message, update, fields);
        drop(delta_scope);
        let delta = delta_started.elapsed().as_secs_f64();
        self.finish_timings(lua, &mut message, &profile, started, build);
        if let Some(Json::Object(timings)) = message.get_mut("timings") {
            timings.insert("layout".into(), json!(layout_time(&self.engine) - layout_before));
            timings.insert("export".into(), json!((build - self.layout_time).max(0.0)));
            timings.insert("serialize".into(), json!(delta));
            timings.insert("exported".into(), json!(misses));
        }
        Ok(Json::Object(message))
    }

    /// PresentationSnapshot (only while the client enabled presentation)
    fn insert_presentation(&self, lua: &Lua, message: &mut Map<String, Json>) {
        if !self.presentation {
            return;
        }
        if let Some(input) = super::input::get(&self.engine) {
            if let Some(snapshot) = input.presentation_snapshot(lua, &self.engine) {
                message.insert("presentation".into(), snapshot);
            }
        }
    }

    /// Reply: every reply carries the clipboard text of a copy / cut since the previous one.
    fn with_clipboard(&self, mut reply: Json) -> Json {
        if let Some(input) = super::input::get(&self.engine) {
            if let Some(text) = input.take_clipboard() {
                if let Json::Object(map) = &mut reply {
                    map.insert("clipboard".into(), Json::String(String::from_utf8_lossy(&text).into_owned()));
                }
            }
        }
        reply
    }

    fn finish_timings(&self, lua: &Lua, message: &mut Map<String, Json>, profile: &Profile, started: Instant, build: f64) {
        if let Some(Json::Object(timings)) = message.get_mut("timings") {
            // Gauges (the profiler's `instances` / `luauHeapMB` counters): the live instances, in the
            // DataModel or not, and the Luau heap. A story that leaks what it rebuilds shows here.
            timings.insert("instances".into(), json!(self.engine.state.borrow().dom.live_count()));
            timings.insert("luauHeapBytes".into(), json!(lua.used_memory()));
            if let Some((private, _)) = crate::memtrace::process_bytes() {
                timings.insert("processBytes".into(), json!(private));
            }
            if crate::memtrace::reporting() {
                timings.insert("memory".into(), crate::memtrace::live_json());
                let (userdata, anchored, _) = super::lua::raw::stats(&self.engine);
                timings.insert("counts".into(), json!({
                    "userdata": userdata, "anchored": anchored,
                    "valueCache": super::lua::raw::value_cache_len(&self.engine),
                    "layout": self.engine.layout().borrow().as_ref().map(|layout| layout.counts()),
                    "signals": self.engine.state.borrow().signals.counts().0,
                    "connections": self.engine.state.borrow().signals.counts().1,
                    "dom": self.engine.state.borrow().dom.storage(),
                }));
            }
            timings.insert("build".into(), json!(build));
            timings.insert("export".into(), json!(build));
            timings.insert("total".into(), json!(started.elapsed().as_secs_f64()));
            timings.insert("nodes".into(), json!(self.last_nodes));
            // (Reply: since the command started, or since the reply started for "ready")
            let frame_started = profile.started.unwrap_or(started);
            timings.insert("frameTotal".into(), json!(frame_started.elapsed().as_secs_f64()));
        }
    }

    /// ReplyDelta: what changed since the previous reply (the retained export's update) and the
    /// header when it changed.
    fn delta(&mut self, message: &mut Map<String, Json>, update: super::export::retained::Update, fields: Map<String, Json>) {
        self.last_nodes = update.exported;
        let mut delta = Map::new();
        let changed_count = update.nodes.len();
        let removed_count = update.removed.len();
        delta.insert("nodes".into(), Json::Array(update.nodes));
        delta.insert("removed".into(), Json::Array(update.removed.iter().map(|&id| json!(id)).collect()));
        if self.patches {
            delta.insert("retained".into(), Json::Bool(true));
        }
        if let Some(layers) = update.layers {
            delta.insert("layers".into(), layers);
        }
        if let Some(fonts) = update.fonts {
            delta.insert("fonts".into(), fonts);
        }
        if !update.images.is_empty() {
            delta.insert("images".into(), Json::Array(update.images.into_iter().map(Json::String).collect()));
        }
        let mut header = Map::new();
        header.insert("viewport".into(), super::export::viewport_json(&self.engine));
        header.extend(fields);
        if self.presentation {
            header.insert(
                "presentation".into(),
                json!({ "version": 1, "retained": true, "rotationClipping": self.engine.options.clips_descendants_supports_rotation }),
            );
        }
        let header = Json::Object(header);
        if self.sent_header.as_ref() != Some(&header) {
            self.sent_header = Some(header.clone());
            delta.insert("header".into(), header);
        }
        message.insert("delta".into(), Json::Object(delta));
        if let Some(Json::Object(counters)) = message.get_mut("counters") {
            counters.insert("changed".into(), json!(changed_count));
            counters.insert("removed".into(), json!(removed_count));
            counters.insert("nodes".into(), json!(self.last_nodes));
            counters.insert("exported".into(), json!(update.misses));
        }
    }

    /// The first reply: the controls, whether input is emulated, and the mount diagnostics.
    pub fn ready(&mut self, lua: &Lua) -> LuaResult<Json> {
        let (_, specs) = self.story.controls();
        let describe: LuaFunction = self.project.host.get("DescribeControls")?;
        let controls: LuaValue = describe.call(specs)?;
        let controls: Json = lua.from_value(controls)?;
        let diagnostics = story::diagnostics(&self.engine, &self.project, &self.story, self.run_time);
        let mut extra = Map::new();
        extra.insert("controls".into(), match controls {
            Json::Object(map) if map.is_empty() => Json::Array(Vec::new()),
            other => other,
        });
        extra.insert("input".into(), Json::Bool(self.input.is_some()));
        extra.insert("supportsPresentation".into(), Json::Bool(self.input.is_some() && super::input::get(&self.engine).is_some()));
        extra.insert("diagnostics".into(), diagnostics);
        let reply = self.frame_reply(lua, "ready", 0, extra, Profile::default())?;
        Ok(self.with_clipboard(reply))
    }

    fn set_control(&mut self, lua: &Lua, path: &Json, value: &Json, strict: bool) -> LuaResult<()> {
        let (controls, specs) = self.story.controls();
        let set: LuaFunction = self.project.host.get("SetControl")?;
        let options = LuaSerializeOptions::new().serialize_none_to_null(false).serialize_unit_to_null(false).set_array_metatable(false);
        let path = lua.to_value_with(path, options)?;
        let value = lua.to_value_with(value, options)?;
        set.call::<()>((controls, specs, path, value, strict))
    }

    /// {"cmd":"frame"}: input events (and control events, which travel with the frame stream so
    /// they stay ordered with input), then `dt` seconds of virtual time.
    pub fn frame(&mut self, lua: &Lua, dt: f64, cadence: Option<&str>, events: &[Json]) -> Json {
        self.frame_with(lua, dt, cadence, events, None)
    }

    /// {"cmd":"frame"} with the command's `presentation` packet (predicted scrolling).
    pub fn frame_with(&mut self, lua: &Lua, dt: f64, cadence: Option<&str>, events: &[Json], presentation: Option<&Json>) -> Json {
        let stepped = Instant::now();
        let _frame_scope = crate::microprofile::scope_detail("Engine frame", events.len() as u64);
        let layout_before = layout_time(&self.engine);
        let input = super::input::get(&self.engine).filter(|_| self.input.is_some());
        let result = (|| -> LuaResult<Json> {
            // The input profile: presentation poses and each event's dispatch
            let input_scope = crate::microprofile::scope_detail("Input", events.len() as u64);
            begin_profile(&self.engine);
            if let (Some(input), Some(packet @ Json::Object(_))) = (&input, presentation) {
                let enabled = packet.get("version").and_then(Json::as_f64) == Some(1.0);
                if enabled != self.presentation {
                    self.presentation = enabled;
                    self.cache.set_presentation(enabled);
                    input.enable_presentation(&self.engine, enabled);
                    self.dirty.set(true);
                }
            }
            for event in events {
                let pose = event.get("presentation").filter(|p| !p.is_null() && **p != Json::Bool(false));
                if let (true, Some(input), Some(pose)) = (self.presentation, &input, pose) {
                    let phase = match event.get("type").and_then(Json::as_str) {
                        Some("wheel") => "wheel",
                        Some("move") => "frame",
                        _ => "input",
                    };
                    input.apply_presentation(lua, &self.engine, pose, phase)?;
                }
                profile::enter("dispatch");
                profile::count("events", 1);
                if event.get("type").and_then(Json::as_str) == Some("control") {
                    let path = event.get("path").cloned().unwrap_or(Json::Array(Vec::new()));
                    let value = event.get("value").cloned().unwrap_or(Json::Null);
                    self.set_control(lua, &path, &value, false)?;
                } else if let Some(input) = self.input.as_mut() {
                    input(lua, &self.engine, event)?;
                }
                profile::leave();
            }
            drop(input_scope);
            let mut input_detail = profile::finish().unwrap_or_default();
            let input_time = stepped.elapsed().as_secs_f64();
            let input_layout = layout_time(&self.engine) - layout_before;
            let input_work = (input_time - input_layout).max(0.0);
            fill_other(&mut input_detail, input_work);
            // The simulation profile: the frame's presentation packet and the stepped frames
            let simulated = Instant::now();
            let simulation_scope = crate::microprofile::scope("Simulation");
            begin_profile(&self.engine);
            if let (true, Some(input)) = (self.presentation, &input) {
                input.apply_presentation(lua, &self.engine, presentation.unwrap_or(&Json::Null), "frame")?;
            }
            let (frames, intervals) = self.stepper.advance(lua, &self.engine, dt, cadence)?;
            drop(simulation_scope);
            let mut detail = profile::finish().unwrap_or_default();
            let step_time = stepped.elapsed().as_secs_f64();
            let simulation_time = simulated.elapsed().as_secs_f64();
            let simulation = (simulation_time - (layout_time(&self.engine) - layout_before - input_layout)).max(0.0);
            fill_other(&mut detail, simulation);
            let mut extra = Map::new();
            extra.insert("stepTime".into(), json!(step_time));
            let profile = Profile {
                started: Some(stepped),
                layout_before: Some(layout_before),
                input: input_work,
                simulation,
                events: events.len(),
                intervals,
                input_detail: Some(input_detail),
                detail: Some(detail),
            };
            self.frame_reply(lua, "frame", frames, extra, profile)
        })();
        if result.is_err() {
            profile::cancel();
        }
        self.with_clipboard(result.unwrap_or_else(|error| error_reply(&error)))
    }

    /// A frame reply for a clock the caller ran itself (`frames` render steps: a sequence's timeline,
    /// cli.rs run_sequence): the scene delta, or `unchanged` when nothing could have changed.
    pub fn capture(&mut self, lua: &Lua, frames: u32) -> Json {
        let profile = Profile { started: Some(Instant::now()), ..Profile::default() };
        let reply = self.frame_reply(lua, "frame", frames, Map::new(), profile).unwrap_or_else(|error| error_reply(&error));
        self.with_clipboard(reply)
    }

    /// The story's status, failure and scene diagnostics as they stand (a sequence's end: the script
    /// output of every frame, not only the mount's)
    pub fn end_report(&mut self, lua: &Lua) -> Json {
        let (status, failure) = self.status(lua).unwrap_or_else(|error| ("error".into(), Some(scheduler::error_message(&error))));
        let diagnostics = story::diagnostics(&self.engine, &self.project, &self.story, self.run_time);
        json!({ "status": status, "failure": failure, "diagnostics": diagnostics })
    }

    /// {"cmd":"control"}: sets a UI Labs control source, runs the resumption point, answers a frame.
    pub fn control(&mut self, lua: &Lua, path: &Json, value: &Json) -> Json {
        let started = Instant::now();
        let layout_before = layout_time(&self.engine);
        let result = (|| -> LuaResult<Json> {
            self.set_control(lua, path, value, true)?;
            self.stepper.advance(lua, &self.engine, 0.0, None)?;
            // (no detail profiles: the whole command is input work, excluding layout)
            let input = (started.elapsed().as_secs_f64() - (layout_time(&self.engine) - layout_before)).max(0.0);
            let profile =
                Profile { started: Some(started), layout_before: Some(layout_before), input, events: 1, ..Profile::default() };
            self.frame_reply(lua, "frame", 0, Map::new(), profile)
        })();
        self.with_clipboard(result.unwrap_or_else(|error| error_reply(&error)))
    }

    /// {"cmd":"scene"}: the full one-shot export of the current state (no cache, no culling): a
    /// client that applied every delta must draw exactly this scene.
    pub fn scene(&mut self, lua: &Lua) -> Json {
        let reply = self.scene_reply(lua);
        self.with_clipboard(reply)
    }

    fn scene_reply(&mut self, lua: &Lua) -> Json {
        match self.build(lua) {
            Ok(scene) => {
                let time = super::export::number(self.engine.state.borrow().sched.now);
                json!({ "type": "scene", "time": time, "scene": scene })
            }
            Err(error) => error_reply(&error),
        }
    }

    /// One decoded command; None for "quit".
    pub fn handle(&mut self, lua: &Lua, command: &Json) -> Option<Json> {
        let reply = match command.get("cmd").and_then(Json::as_str) {
            Some("quit") => return None,
            Some("frame") => {
                let dt = command.get("dt").and_then(json_number).unwrap_or(0.0);
                let cadence = command.get("cadence").and_then(Json::as_str);
                let events = command.get("events").and_then(Json::as_array).map(Vec::as_slice).unwrap_or(&[]);
                self.frame_with(lua, dt, cadence, events, command.get("presentation"))
            }
            Some("control") => {
                let path = command.get("path").cloned().unwrap_or(Json::Array(Vec::new()));
                let value = command.get("value").cloned().unwrap_or(Json::Null);
                self.control(lua, &path, &value)
            }
            Some("scene") => self.scene(lua),
            // What keeps removed UI alive (lua/leaks.rs)
            Some("leaks") => super::lua::leaks::report(lua, &self.engine).unwrap_or_else(|error| error_reply(&error)),
            other => {
                let name = match other {
                    Some(name) => name.to_string(),
                    None => command.get("cmd").map_or("nil".into(), |v| v.to_string()),
                };
                self.with_clipboard(json!({ "type": "error", "message": format!("unknown command {name}") }))
            }
        };
        Some(reply)
    }
}

/// tonumber of a command field
fn json_number(value: &Json) -> Option<f64> {
    match value {
        Json::Number(n) => n.as_f64(),
        Json::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

fn error_reply(error: &LuaError) -> Json {
    json!({ "type": "error", "message": scheduler::error_message(error) })
}

const PREFIX: &str = "@@RUR ";

/// Luau's serializer writes an empty table as `{}`.
fn empty_arrays_as_objects(value: &mut Json) {
    match value {
        Json::Array(items) if items.is_empty() => *value = Json::Object(Map::new()),
        Json::Array(items) => items.iter_mut().for_each(empty_arrays_as_objects),
        Json::Object(map) => map.values_mut().for_each(empty_arrays_as_objects),
        _ => {}
    }
}

fn write_reply(out: &mut impl Write, mut reply: Json) -> std::io::Result<()> {
    empty_arrays_as_objects(&mut reply);
    let text = serde_json::to_string(&reply).unwrap_or_else(|e| format!("{{\"type\":\"error\",\"message\":\"{e}\"}}"));
    out.write_all(PREFIX.as_bytes())?;
    out.write_all(text.as_bytes())?;
    out.write_all(b"\n")?;
    out.flush()
}

/// The stdio session loop (runtime/session.luau without a native transport).
pub fn run_stdio(lua: &Lua, session: &mut Session) -> Result<(), String> {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let ready = session.ready(lua).unwrap_or_else(|error| error_reply(&error));
    write_reply(&mut out, ready).map_err(|e| e.to_string())?;
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let line = line.map_err(|e| e.to_string())?;
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }
        let command: Json = match super::lune_json::decode(line) {
            Ok(command @ Json::Object(_)) => command,
            _ => {
                write_reply(&mut out, json!({ "type": "error", "message": format!("bad command: {line}") })).map_err(|e| e.to_string())?;
                continue;
            }
        };
        match session.handle(lua, &command) {
            Some(reply) => write_reply(&mut out, reply).map_err(|e| e.to_string())?,
            None => break,
        }
    }
    Ok(())
}
