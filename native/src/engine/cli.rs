//! Headless runner for the engine: `rur-native engine ...` (the golden check, engine-oracle tests, story
//! controls and ad-hoc runs spawn it), plus the in-process forms `rur-native render` / the render
//! service (story_render.rs, serve.rs: [`run_once`], [`Preloaded`]) and the desktop app ([`Host`]) use:
//!
//!   rur-native engine --fixture <file.luau> --out <scene.json>
//!       [--module <file>] [--story <file>] [--model <file.rbxm|.rbxmx|.rbx>] [--roots "<dir>;<dir>"]
//!       [--project <dir>] [--place <name>] (the engine's launch options)
//!       [--viewport WxH] [--scale S] [--time T] [--mode edit|play] [--player Name] [--place-id N]
//!       [--camera WxH] [--margin M] [--background R,G,B|transparent] [--rotation-clipping enabled|disabled]
//!       [--controls <json|file>] [--actions <json|file>] [--snapshot <file.rbxm>]...
//!       [--snapshot-dir <dir>] [--no-snapshots] [--session 1] [--verbose] [--repo <dir>]
//!       [--mount Service/Name=<dir|file>]... [--instance Service/Name=<ClassName>]...
//!       [--plugin 1|<settings JSON|file>] [--all-layers 1] [--load Service/Name=<file.rbxm|.rbxmx|.rbx>]...
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
//! applies its replies). Every VM of every host runs its Luau as a playing client and the emulator's
//! `record` do ([`interpret`]): interpreted, in safe script environments, its work charged on the work
//! clock. Environment: RUR_CAMERA / RUR_PLACE_ID / RUR_PLAYER* /
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
use crate::paths::RurVariables;

/// The engine CLI's arguments: the emulator's engine options (launch.rs; read through `Deref`, so
/// `args.roots`, `args.mode`, ... are theirs) and what a story render adds.
#[derive(Default, Debug, Clone)]
pub struct Args {
    pub engine: launch::Args,
    /// Whether `--roots` was among the arguments. The engine's `roots` list drops empty entries, so
    /// it is empty both for no `--roots` and for `--roots ""`. Standalone project selection must tell
    /// them apart: omitted roots come from the place, while an empty list means no implicit roots.
    pub roots_given: bool,
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
    /// `--topbar-inset px`: the engine's topbar inset (the client profile's device.topbarInset); unset keeps the
    /// profile's (0 without one)
    pub topbar_inset: Option<f64>,
    /// `--server-harness <file>` with `--run-context loopback`: the loopback harness whose `Server` runs on the pair's
    /// server (the emulator's "Loopback run context"); the story is the client's. None: the place's own server scripts
    pub server_harness: Option<String>,
    /// `--latency <frames>` with `--run-context loopback`: each way's delivery that many frames on (1: the next frame)
    pub latency: Option<u32>,
    /// The first argument [`parse`] did not know (a misspelt option, or a value with no option); the
    /// commands people type refuse it ([`parse_command`]), the service and batch requests the CLI
    /// builds skip it
    pub unknown: Option<String>,
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
    if crate::paths::is_standalone() && args.get(1).map(String::as_str) == Some(BATCH_JOB_COMMAND) {
        return Some(batch_job_child(&args[2..]));
    }
    if args.get(1).map(String::as_str) != Some("engine") {
        return None;
    }
    let parsed = match with_rojo_project(&args[2..]).and_then(|args| parse_command(&args)) {
        Ok(parsed) => parsed,
        Err(error) => {
            eprintln!("rur-native engine: {error}");
            return Some(2);
        }
    };
    let code = match run(parsed) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("rur-native engine: {error}");
            1
        }
    };
    #[cfg(feature = "desktop")]
    crate::viewports::release_shared_workers();
    Some(code)
}

/// `--background r,g,b` (0-255) as 0-1 channels
fn parse_background(text: &str) -> Option<[f64; 3]> {
    let parts: Vec<f64> = text.split(',').filter_map(|p| p.trim().parse().ok()).collect();
    (parts.len() == 3).then(|| [parts[0] / 255.0, parts[1] / 255.0, parts[2] / 255.0])
}

pub fn parse(args: &[String]) -> Result<Args, String> {
    crate::paths::validate_arguments(args)?;
    let mut result = Args::default();
    let mut index = 0;
    while index < args.len() {
        let key = args[index].as_str();
        let value = args.get(index + 1).cloned();
        if let Some(used) = result.engine.accept(key, value.as_ref())? {
            result.roots_given |= key == "--roots";
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
            "--server-harness" => result.server_harness = Some(need()?),
            "--latency" => {
                let text = need()?;
                let frames = text.parse::<u32>().map_err(|_| format!("--latency: expected a whole number of frames, got {text}"))?;
                result.latency = Some(frames);
            }
            "--dump-scene" | "--out" => result.dump_scene = Some(need()?),
            "--dependencies" => result.dependencies = Some(need()?),
            "--plugin" => result.plugin = Some(need()?),
            "--all-layers" => result.all_layers = need()? != "0",
            "--load" => result.loads.push(need()?),
            "--topbar-inset" => result.topbar_inset = Some(number(need()?)?.max(0.0)),
            // (the Node CLI's engine arguments start with the command's name: cli/engine.mjs engineBaseArgs)
            "engine" if index == 0 => {}
            _ => {
                result.unknown.get_or_insert_with(|| key.to_string());
                index += 1;
                continue;
            }
        }
        index += if matches!(key, "--headless" | "--no-png" | "engine") { 1 } else { 2 };
    }
    Ok(result)
}

/// [`parse`] for a command line someone typed (`rur-native engine`, `render`, `sequence`): an argument it does not
/// know is an error, not skipped
pub fn parse_command(args: &[String]) -> Result<Args, String> {
    let result = parse(args)?;
    match &result.unknown {
        Some(key) if key.starts_with('-') => Err(format!("unknown option {key}")),
        Some(key) => Err(format!("unexpected argument {key:?}: every option takes at most one value (quote a value with spaces)")),
        None => Ok(result),
    }
}

/// The command line of a native story command with its Rojo project resolved ([`crate::rojo::resolve_arguments`]):
/// a project given or found becomes mounts, and a line on stderr says which project was mounted where
pub fn with_rojo_project(args: &[String]) -> Result<Vec<String>, String> {
    with_rojo_project_noted(args).map(|(arguments, _)| arguments)
}

/// [`with_rojo_project`], also returning the lines it printed (a failure's hint that repeats one is not printed again)
pub fn with_rojo_project_noted(args: &[String]) -> Result<(Vec<String>, Vec<String>), String> {
    let resolved = crate::rojo::resolve_arguments(args)?;
    let printed: Vec<String> = resolved.warnings.iter().cloned().chain(resolved.note()).collect();
    for line in &printed {
        crate::term::project_line(line);
    }
    Ok((resolved.arguments, printed))
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

/// An inline scene of [`batch`] (`{"inline": true}`) starts with this on a line of stdout
pub const BATCH_SCENE: &str = "@@RUR-SCENE ";

/// The command a release batch runner starts a child with: `rur-native engine-batch-job <engine
/// arguments>` runs one job and answers it like [`batch`] does.
pub const BATCH_JOB_COMMAND: &str = "engine-batch-job";

/// `rur-native engine-batch`: engine runs one after another in this process, each exactly as a
/// `rur-native engine <arguments>` process of its own would make it (the golden check sends its scene
/// entries so: a process start, its exit and the reflection load were most of a small run). A job is a
/// line on stdin, `{"argv": [engine arguments], "env": {"RUR_…": value}}`; its variables replace every
/// RUR_* variable of the job before. Each run has an engine and a VM of its own, freed after it; what
/// the next run reads unchanged is kept, as in the render service: module sources and their bytecode,
/// mounted model files and the project's listings. Its answer is a line on stdout, `@@RUR-BATCH {"code": exit code, "error":
/// message}`, after whatever the run itself printed; a run that panics answers code 101 and the next
/// job runs. Live sessions (`--session`) are not batched. A release build shares this process only with
/// jobs whose environment matches its own; other jobs use `batch_job_process`.
pub fn batch() -> i32 {
    use std::io::{BufRead, Write};
    super::lua::modules::reuse_compiled_modules();
    super::lua::modules::set_deferred_store_flush(true);
    struct FlushOnExit;
    impl Drop for FlushOnExit {
        fn drop(&mut self) {
            super::lua::modules::flush_module_store();
            super::lua::modules::set_deferred_store_flush(false);
        }
    }
    let _flush_on_exit = FlushOnExit;
    #[cfg(not(target_os = "emscripten"))]
    let mut jobs_since_full_collect: u32 = 0;
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
        // Reclaim empty pages between jobs without purging the working set on every run; force a full
        // collect periodically or when resident memory exceeds 512 MiB.
        #[cfg(not(target_os = "emscripten"))]
        {
            jobs_since_full_collect += 1;
            let high_memory = crate::memtrace::process_bytes().is_some_and(|(_, ws)| ws > 512 * 1024 * 1024);
            let force = jobs_since_full_collect >= 32 || high_memory;
            if force {
                jobs_since_full_collect = 0;
            }
            unsafe {
                libmimalloc_sys::mi_collect(force);
            }
        }
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
    let inline = job["inline"].as_bool().unwrap_or(false);
    let commands = job["commands"].as_array().map(Vec::as_slice);
    let dedup_scenes = job["dedupScenes"].as_bool().unwrap_or(false);
    if crate::paths::is_standalone() {
        // Refused before the job picks this process or a child: neither may start work (or choose
        // a default location) for an unusable path.
        if let Err(error) = crate::paths::validate_arguments(&argv) {
            return json!({"code": 2, "error": error});
        }
        // A release build never changes its environment once threads exist (see the `paths` module
        // comment), and a finished run can leave threads behind (workers, GPU drivers). So a job
        // runs here only when this process already has the variables it asks for, which keeps the
        // warm caches and saves a process start. Any other job gets a child that starts with them.
        let variables = job_variables(job);
        if crate::paths::batch_environment_matches(&variables, &argv) {
            return run_batch_job(&argv, inline, commands, dedup_scenes);
        }
        if inline || commands.is_some() {
            return json!({"code": 2, "error": "inline and session batch jobs require matching process environment"});
        }
        return batch_job_process(&variables, &argv);
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
    run_batch_job(&argv, inline, commands, dedup_scenes)
}

enum BatchMode<'a> {
    Standard,
    InlineScene,
    SessionCommands(&'a [Json], bool),
}

struct CaptureGuard(bool);

impl CaptureGuard {
    fn start() -> Self {
        super::output::capture();
        Self(true)
    }

    fn finish(mut self) -> String {
        self.0 = false;
        super::output::take_captured()
    }
}

impl Drop for CaptureGuard {
    fn drop(&mut self) {
        if self.0 {
            let _ = super::output::take_captured();
        }
    }
}

/// The structured result is identical for checkout jobs, warm release jobs and child jobs.
fn run_batch_job(argv: &[String], inline: bool, commands: Option<&[Json]>, dedup_scenes: bool) -> Json {
    let args = match parse(argv) {
        Ok(args) if !args.session || commands.is_some() => args,
        Ok(_) => return json!({"code": 2, "error": "live sessions are not batched"}),
        Err(error) => {
            eprintln!("rur-native engine: {error}");
            return json!({"code": 2, "error": error});
        }
    };
    if args.session {
        let cmds = commands.expect("checked above");
        let capture = CaptureGuard::start();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_with_mode(args, true, BatchMode::SessionCommands(cmds, dedup_scenes))
        }));
        let captured = capture.finish();
        return match outcome {
            Ok(Ok(code)) => json!({"code": code, "stderr": captured}),
            Ok(Err(error)) => {
                eprintln!("rur-native engine: {error}");
                json!({"code": 1, "error": error, "stderr": captured})
            }
            Err(_) => json!({"code": 101, "error": "the run panicked", "stderr": captured}),
        };
    }
    let mode = if inline { BatchMode::InlineScene } else { BatchMode::Standard };
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_with_mode(args, true, mode))) {
        Ok(Ok(code)) => json!({"code": code}),
        Ok(Err(error)) => {
            eprintln!("rur-native engine: {error}");
            json!({"code": 1, "error": error})
        }
        Err(_) => json!({"code": 101, "error": "the run panicked"}),
    }
}

/// The `RUR_*` variables a job asks for; other names in its `env` are ignored.
fn job_variables(job: &Json) -> RurVariables {
    let requested = job["env"].as_object().into_iter().flatten();
    requested
        .filter(|(name, _)| name.starts_with("RUR_"))
        .filter_map(|(name, value)| Some((name.into(), value.as_str()?.into())))
        .collect()
}

/// A release job whose variables differ from this process's runs in a child that starts with
/// exactly those variables, so this process's environment never changes.
fn batch_job_process(variables: &RurVariables, argv: &[String]) -> Json {
    // Answer bad arguments from here, as the in-process path does, without starting a child
    match parse(argv) {
        Ok(args) if args.session => return json!({"code": 2, "error": "live sessions are not batched"}),
        Err(error) => return json!({"code": 2, "error": error}),
        Ok(_) => {}
    }
    match run_job_process(variables, argv) {
        Ok(answer) => answer,
        Err(error) => json!({"code": 1, "error": error.to_string()}),
    }
}

/// Runs the job in a child and returns its answer.
fn run_job_process(variables: &RurVariables, argv: &[String]) -> std::io::Result<Json> {
    let mut child = spawn_job_child(variables, argv)?;
    // The child ends itself when this pipe closes (see `batch_job_child`), so keep it open until
    // the child has answered. `Child::wait` would close it first.
    let _stdin_held_open = child.stdin.take();
    let answer = read_job_answer(&mut child);
    if answer.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    answer
}

/// Starts `rur-native engine-batch-job <arguments>` with the job's variables and no other `RUR_*`
/// variable of this runner. The child finds its state directory and repo itself, as any release
/// process does (`paths::initialize`).
fn spawn_job_child(variables: &RurVariables, argv: &[String]) -> std::io::Result<std::process::Child> {
    use std::process::{Command, Stdio};
    let mut command = Command::new(std::env::current_exe()?);
    command.arg(BATCH_JOB_COMMAND).args(argv).stdin(Stdio::piped()).stdout(Stdio::piped());
    for name in crate::paths::rur_variables().into_keys() {
        command.env_remove(name);
    }
    command.envs(variables);
    // It downloads: the API key this process took out of its environment at startup goes with it
    #[cfg(feature = "desktop")]
    crate::download::with_api_key(&mut command);
    let child = command.spawn()?;
    #[cfg(feature = "desktop")]
    crate::process::attach_child(&child);
    Ok(child)
}

/// Waits for the child and returns its answer line. Whatever else it printed goes on to this
/// process's stdout, as the run's own output would in a batch of one process.
fn read_job_answer(child: &mut std::process::Child) -> std::io::Result<Json> {
    use std::io::{BufRead, BufReader};
    let output = child.stdout.take().expect("piped stdout");
    let mut answer = None;
    for line in BufReader::new(output).lines() {
        let line = line?;
        match line.strip_prefix(BATCH_ANSWER) {
            Some(text) => answer = Some(serde_json::from_str::<Json>(text)?),
            None => println!("{line}"),
        }
    }
    let status = child.wait()?;
    // A child that ended without an answer failed, even with exit code 0; a killed one has no code
    let code = status.code().filter(|code| *code != 0).unwrap_or(1);
    Ok(answer.unwrap_or_else(|| json!({"code": code, "error": "batch child exited without a result"})))
}

/// `rur-native engine-batch-job <engine arguments>`: one job of a release batch runner, run here
/// as a process of its own. It answers on stdout as [`batch`] does and exits with the job's code.
fn batch_job_child(argv: &[String]) -> i32 {
    // The runner holds the write end of our stdin and never writes to it, so stdin reaches its end
    // only when the runner exits or is killed. A job that never yields (busy in virtual time, say)
    // cannot be cancelled from inside, so this thread ends the process instead.
    std::thread::spawn(|| {
        let _ = std::io::copy(&mut std::io::stdin().lock(), &mut std::io::sink());
        std::process::exit(1);
    });
    let answer = run_batch_job(argv, false, None, false);
    println!("{BATCH_ANSWER}{answer}");
    answer["code"].as_i64().unwrap_or(1) as i32
}

/// One render (or live session): returns the process exit code.
pub fn run(args: Args) -> Result<i32, String> {
    run_with_mode(args, false, BatchMode::Standard)
}

/// [`run`], tearing the engine down after it when `free` (a batch's next run follows in this
/// process; a process of its own just exits)
fn run_with_mode(args: Args, free: bool, mode: BatchMode<'_>) -> Result<i32, String> {
    let _memory = crate::memtrace::scope(crate::memtrace::Tag::Script);
    let started = Instant::now();
    // A bad --actions list stops the run before anything is loaded (input/actions.rs names the action)
    if let Some(spec) = &args.actions {
        story::read_actions(spec)?;
    }
    let prepared = prepare(&args)?;
    let spec = project_spec(&args, &prepared)?;
    let server = loopback_server(&args, &prepared, &spec)?;
    let engine = story_engine(&prepared, &server)?;
    // RUR_LUAU_PROFILE=<file>: folded Luau stacks (startup, then frames after the first reply)
    let _profile = crate::luau_profile::begin();
    let lua = Lua::new();
    interpret(&lua, &engine);
    crate::luau_profile::install(&lua);
    let result = (|| {
        let project = mount_story_project(&lua, &engine, &args, &spec, started, server)?;
        if args.test_hooks {
            install_test_hooks(&lua, &engine).map_err(|e| e.to_string())?;
        }
        render(&lua, &engine, &args, project, prepared.target, started, mode)
    })();
    // Scripts may hold references into the engine; the VM goes first
    drop(lua);
    if free {
        engine.teardown();
    }
    result
}

/// `--test-hooks 1`: a global `rur` table for engine-oracle test scripts (tools/tests/*.luau run as
/// fixtures by their .mjs drivers): `rur.processId` (release builds only: this process's id, which shows
/// whether a batch job ran in the runner or in a child), `rur.layout()` (a layout pass),
/// `rur.scene(roots, presentation)`
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
    if crate::paths::is_standalone() {
        rur.set("processId", std::process::id())?;
    }
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
        let mut scene = Map::new();
        scene.insert("version".into(), Json::from(1));
        scene.insert("viewport".into(), export::viewport_json(&e));
        scene.insert("nodes".into(), Json::Array(update.nodes));
        scene.insert("layers".into(), update.layers.unwrap_or_else(|| Json::Array(Vec::new())));
        scene.insert("fonts".into(), update.fonts.unwrap_or_else(|| Json::Array(Vec::new())));
        scene.insert("images".into(), Json::Array(update.images.into_iter().map(Json::String).collect()));
        lua.to_value(&Json::Object(scene))
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
    #[cfg(feature = "desktop")]
    select_standalone_project(args, &mut engine)?;
    let prepared = launch::prepare(&engine, &super::default_repo())?;
    let mut options = prepared.options;
    if let Some(inset) = args.topbar_inset {
        options.profile.device.topbar_inset = Some(inset);
    }
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
    // (a profile's phase from here: the frames, apart from the mount, as `ree run` marks them)
    crate::luau_profile::mark_frames();

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
        let spec = project_spec(args, &prepared)?;
        let server = loopback_server(args, &prepared, &spec)?;
        let engine = story_engine(&prepared, &server)?;
        let lua = Lua::new();
        interpret(&lua, &engine);
        let project = mount_story_project(&lua, &engine, args, &spec, started, server)?;
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
    mode: BatchMode<'_>,
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
        match mode {
            BatchMode::SessionCommands(commands, dedup_scenes) => super::session::run_commands(lua, &mut session, commands, dedup_scenes)?,
            _ => super::session::run_stdio(lua, &mut session)?,
        }
        return Ok(0);
    }
    let output = run_scene(lua, engine, args, &project, target)?;
    if let BatchMode::InlineScene = mode {
        if args.dependencies.is_some() {
            output.save(args)?;
        }
        use std::io::Write;
        let mut out = std::io::stdout().lock();
        writeln!(out, "{BATCH_SCENE}{}", output.text).map_err(|e| e.to_string())?;
        out.flush().map_err(|e| e.to_string())?;
    } else {
        output.write(args, started)?;
    }
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
    /// The engine frames the run stepped, the virtual seconds they took and the frame rate (a profile's run)
    pub frames: u64,
    pub seconds: f64,
    pub fps: f64,
}

impl SceneOutput {
    /// Writes `--dependencies` and `--out` (else prints the scene) with the CLI's status line.
    pub fn write(&self, args: &Args, started: Instant) -> Result<(), String> {
        self.save(args)?;
        match &args.dump_scene {
            Some(path) => eprintln!("rur: {}; {} nodes; {} output lines; {}s -> {path}", self.status, self.nodes, self.lines, started.elapsed().as_secs_f64()),
            None => println!("{}", self.text),
        }
        Ok(())
    }

    /// Writes `--dependencies` and `--out`, for a command that reports the run itself (`rur-native render`)
    pub fn save(&self, args: &Args) -> Result<(), String> {
        if let Some(path) = &args.dependencies {
            std::fs::write(path, serde_json::to_string(&json!({ "files": self.dependencies })).map_err(|e| e.to_string())?)
                .map_err(|e| format!("{path}: {e}"))?;
        }
        if let Some(path) = &args.dump_scene {
            std::fs::write(path, &self.text).map_err(|e| format!("{path}: {e}"))?;
        }
        Ok(())
    }
}

/// A one-shot run in a VM of its own: the engine and the project of `args`, the story, virtual time,
/// layout and export (what `rur-native engine --out` does, without writing anything).
pub fn run_once(args: &Args) -> Result<SceneOutput, String> {
    run_once_with(args, |_| {})
}

/// A one-shot run with the final layout-pass counters and live-DOM hidden-in-shown accounting (`rur cost`).
#[cfg(feature = "desktop")]
pub struct CostRunOutput {
    pub scene: SceneOutput,
    pub layout: super::layout::Stats,
    pub dom_hidden: crate::cost::DomHiddenSummary,
    pub device: String,
    pub waiting_in_loop: bool,
}

/// [`run_once`] for `rur cost`: also captures the deterministic full layout-pass counters and walks
/// the live DOM under enabled ScreenGuis to count `Visible = false` subtrees before teardown.
#[cfg(feature = "desktop")]
pub fn run_once_with_cost(args: &Args) -> Result<CostRunOutput, String> {
    let started = Instant::now();
    if let Some(spec) = &args.actions {
        story::read_actions(spec)?;
    }
    let prepared = prepare(args)?;
    let spec = project_spec(args, &prepared)?;
    let target = prepared.target;
    let server = loopback_server(args, &prepared, &spec)?;
    let engine = story_engine(&prepared, &server)?;
    let lua = Lua::new();
    interpret(&lua, &engine);
    let result = (|| {
        let project = mount_story_project(&lua, &engine, args, &spec, started, server)?;
        let source = source_of(args);
        let (mounted, exec_start) = mount_and_run(&lua, &engine, &project, args, source.as_ref(), target)?;
        let layout_before = super::layout::stats_snapshot(&engine);
        let scene = scene_output(&engine, args, &project, &mounted, source.as_ref(), exec_start)?;
        let layout = super::layout::stats_snapshot(&engine).delta_since(&layout_before);
        let dom_hidden = crate::cost::collect_dom_hidden(&engine, &mounted.roots);
        Ok(CostRunOutput {
            scene,
            layout,
            dom_hidden,
            device: engine.options.profile.device.kind().name().to_string(),
            waiting_in_loop: mounted.wait_in_loop,
        })
    })();
    drop(lua);
    engine.teardown();
    result
}

/// [`run_once`], with `ready` given the story's engine before anything mounts in it (a test gives it fonts of its own)
fn run_once_with(args: &Args, ready: impl FnOnce(&Engine)) -> Result<SceneOutput, String> {
    let started = Instant::now();
    if let Some(spec) = &args.actions {
        story::read_actions(spec)?;
    }
    let prepared = prepare(args)?;
    let spec = project_spec(args, &prepared)?;
    let target = prepared.target;
    let server = loopback_server(args, &prepared, &spec)?;
    let engine = story_engine(&prepared, &server)?;
    ready(&engine);
    let lua = Lua::new();
    interpret(&lua, &engine);
    let result = (|| {
        let project = mount_story_project(&lua, &engine, args, &spec, started, server)?;
        // A profile of the run (`rur-native render --profile`, the emulator's profiler) hooks the VM once the project
        // is mounted, as `ree record` does, and reads it after the last frame and the export; nothing without one
        super::luau_profile::install(&lua);
        run_scene(&lua, &engine, args, &project, target)
    })();
    super::profiler::collect(&lua);
    // The VM, then the engine's own cycles (a process may run many of these: the render service)
    drop(lua);
    engine.teardown();
    result
}

/// How every VM here runs Luau, whatever the host (a one-shot `engine` run, a batch job, `render`, the render
/// service, a sequence, the desktop app), as a playing client and the emulator's `record` run it: every chunk
/// interpreted, in a script environment that is safe as a client's is (a module's imports resolved when it
/// loads, builtins taken as fast calls), and the scripts' own Luau charged on the work clock as a client runs
/// it, a `--!native` module's instructions at native code's prices (the emulator's cost.rs). So `os.clock`
/// moves by the same work in every host, and code that budgets its work by it takes the same frames. Natively
/// compiled code runs unseen by the VM's work counters (the clock would miss it, a profile would read 0
/// instructions) in an unsafe environment; compiling every module cost a one-shot story more than it gained
/// (the golden corpus took 35 % less CPU interpreted). The engine's own Luau (the prelude, services) is not
/// charged either way and is interpreted too: compiling it at every VM start was most of a start. Before the
/// project mounts.
fn interpret(lua: &Lua, engine: &Engine) {
    super::lua::modules::interpret_all(lua, engine);
}

/// `--run-context loopback`'s server (the emulator's "Loopback run context"): started before its client, the story's
/// engine, with the project mounted as a server run mounts it; none for any other run. `--server-harness` needs a
/// loopback run.
fn loopback_server(args: &Args, prepared: &Prepared, spec: &story::ProjectSpec) -> Result<Option<super::Emulator>, String> {
    if prepared.options.run_context != super::RunContext::Loopback {
        if args.server_harness.is_some() || args.latency.is_some() {
            return Err("--server-harness and --latency need --run-context loopback".into());
        }
        return Ok(None);
    }
    let core = super::mount::ProjectSpec {
        roots: spec.roots.clone(),
        snapshots: spec.snapshots.clone(),
        snapshot_dir: spec.snapshot_dir.clone(),
        mounts: spec.mounts.clone(),
        model_assets: spec.model_assets.clone(),
        ..Default::default()
    };
    super::loopback::start_server(&super::new_engine, &prepared.options, &core, &|_| {}).map(Some)
}

/// The engine a run's story mounts in: [`super::new_engine`] of the run's options, or a loopback pair's client (its
/// own random sequence: the run's seed plus one)
fn story_engine(prepared: &Prepared, server: &Option<super::Emulator>) -> Result<Rc<Engine>, String> {
    if server.is_none() {
        return super::new_engine(prepared.options.clone());
    }
    let engine = super::new_engine(super::loopback::side_options(&prepared.options, super::loopback::Side::Client))?;
    super::loopback::ready_client(&engine);
    Ok(engine)
}

/// The project of `spec` mounted in the story's VM. A loopback pair's client mounts none (its DataModel comes from the
/// server at the join): it starts the engine alone, links with `server`, whose part of `--server-harness` starts (none:
/// the place's own server scripts), and its player joins before the story mounts.
fn mount_story_project(lua: &Lua, engine: &Rc<Engine>, args: &Args, spec: &story::ProjectSpec, started: Instant, server: Option<super::Emulator>) -> Result<story::Project, String> {
    let Some(server) = server else { return story::mount_project(lua, engine, spec, started) };
    let client = story::ProjectSpec { roots: Vec::new(), snapshots: Vec::new(), snapshot_dir: None, mounts: Vec::new(), ..spec.clone() };
    let project = story::mount_project(lua, engine, &client, started)?;
    super::loopback::link(server, lua, engine, super::loopback::Spec { latency: args.latency.unwrap_or(1), record_server: false })?;
    let frame = engine.state.borrow().sched.frame_duration;
    let frame = if frame > 0.0 { frame } else { 1.0 / 60.0 };
    let harness = args.server_harness.as_deref().map(std::path::Path::new);
    super::record::run::begin_server(lua, engine, harness, frame, 600)?;
    Ok(project)
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
    let mut startup = match with_selected_project(args) {
        Ok(selected) => selected,
        Err(error) => return format!("invalid startup: {error}; {args:?}"),
    };
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

/// A copy of `args` that carries the project, place and roots a run of `args` uses, so a cache key
/// built from the copy tells two runs apart by their resolved project, not by how they spell it.
/// Only standalone mode resolves anything; other builds return `args` unchanged.
///
/// The spelling is not enough because `rur.json` can change between two requests that spell the
/// same arguments, and the story's location then picks another place or another root order.
/// Without the resolved project in the key, a standby or a kept run loaded for the old choice
/// would answer the new request.
fn with_selected_project(args: &Args) -> Result<Args, String> {
    #[cfg(feature = "desktop")]
    {
        let mut selected = args.clone();
        select_standalone_project(args, &mut selected.engine)?;
        Ok(selected)
    }
    #[cfg(not(feature = "desktop"))]
    Ok(args.clone())
}

/// Standalone mode only: gives `engine` the project, place and roots that a run of `args` uses
/// ([`crate::paths::prepare_project`]; the story's own place gives the roots unless `--place` names
/// another). `prepare` runs it before the engine reads its options, and the cache keys run it
/// through [`with_selected_project`].
#[cfg(feature = "desktop")]
fn select_standalone_project(args: &Args, engine: &mut launch::Args) -> Result<(), String> {
    if !crate::paths::is_standalone() {
        return Ok(());
    }
    let source = source_of(args);
    let source_path = source.as_ref().map(|source| std::path::Path::new(&source.path));
    let repo = args.repo.as_ref().map(std::path::PathBuf::from).unwrap_or_else(super::default_repo);
    crate::paths::prepare_project(engine, &repo, source_path, args.roots_given)
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
        let server = loopback_server(args, &prepared, &spec)?;
        let engine = story_engine(&prepared, &server)?;
        let lua = Lua::new();
        interpret(&lua, &engine);
        let project = match mount_story_project(&lua, &engine, args, &spec, started, server) {
            Ok(project) => project,
            Err(error) => {
                drop(lua);
                engine.teardown();
                return Err(error);
            }
        };
        // (a profile running on this thread hooks the VM once the project is mounted: `rur-native sequence --profile`;
        // nothing without one)
        super::luau_profile::install(&lua);
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
        // A profile running on this thread reads the VM when the sequence ends, after its last frame (before the end
        // goes out: the sink may write the profile then)
        let mut ending = |message: SequenceMessage| {
            if matches!(message, SequenceMessage::End(_)) {
                super::profiler::collect(lua);
            }
            sink(message)
        };
        let result = match &args.actions {
            Some(actions) => story::read_actions(actions).map(|_| ()),
            None => Ok(()),
        }
        .and_then(|()| run_sequence(lua, &self.engine, project, args, self.target, spec, &mut ending));
        if let Err(error) = result {
            ending(SequenceMessage::End(Err(error)));
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
/// ([`Kept`]): every argument but the moment, the actions (compared apart) and the output paths.
/// Standalone mode adds the resolved project, place and ordered roots, as [`startup_key`] does.
/// The service's fingerprint of the project (serve.rs `fingerprint`) cannot stand in for this: it
/// folds the roots together with xor, so another root order gives the same value, and it never
/// sees the place name.
pub fn run_key(args: &Args) -> String {
    let mut key = match with_selected_project(args) {
        Ok(selected) => selected,
        Err(error) => return format!("invalid run: {error}; {args:?}"),
    };
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
/// `from` s (the engine frame nearest it, as `--time`).
///
/// When `client_hz` is `Some(hz)` (`rur cost`, defaulting to `60.0`), the client clock always advances
/// in whole `1 / hz` s frames and `fps` only controls how often samples are captured. When `None`
/// (`rur sequence`), legacy cadence applies (whole 60 Hz engine frames when `fps` divides 60, else
/// display-cadence steps of `1 / fps`).
#[derive(Clone, Debug)]
pub struct SequenceSpec {
    pub from: f64,
    pub frames: u32,
    pub fps: f64,
    pub client_hz: Option<f64>,
    /// Collect per-frame `Layout.Stats` deltas and live-DOM hidden-in-shown counts (`rur cost`, `rur sequence --cost`)
    pub collect_cost: bool,
    /// Capture per-frame pointer position and cursor state (`rur sequence --cursor`); skipped when false so `--profile` hit counters stay untouched.
    pub collect_cursor: bool,
}

/// One captured moment of a sequence: the session's reply (the first, "ready", holds the whole scene and
/// the mount's diagnostics; the rest deltas, or `unchanged`) and the virtual time it shows
pub struct SequenceFrame {
    pub index: u32,
    pub time: f64,
    pub reply: Json,
    pub layout: super::layout::Stats,
    /// Number of client simulation steps executed to reach this sample (`0`, `1`, `2`, ...).
    pub client_frames: u32,
    /// Effective simulation rate in Hz for this sequence (`60.0`, `--client-hz`, or `fps` in display cadence).
    pub client_hz: f64,
    /// `"client"` for fixed client frames (`1 / client_hz`) or `"display"` for display-cadence steps (`1 / fps`).
    pub cadence_kind: &'static str,
    /// Actual device the engine ran with (the client profile's device kind, e.g. `"desktop"`, `"touch"`, `"gamepad"`, `"phone"`).
    pub device: String,
    pub layout_churn: Vec<super::layout::LayoutChurnEntry>,
    /// Pointer position and cursor state at this frame (`rur sequence --cursor`).
    pub pointer: Option<super::input::PointerState>,
    #[cfg(feature = "desktop")]
    pub dom_hidden: Option<crate::cost::DomHiddenSummary>,
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
    if let Some(hz) = spec.client_hz {
        if !(hz > 0.0 && hz <= 1000.0 && hz.is_finite()) {
            return Err(format!("--client-hz must be above 0 and at most 1000 (got {hz})"));
        }
    }
    let run_started = Instant::now();
    if spec.collect_cost {
        super::layout::set_track_churn(engine, true);
    }
    let layout_start = super::layout::stats_snapshot(engine);
    let source = source_of(args);
    let mut mounted = mount_story(lua, engine, &project, args, source.as_ref(), target)?;
    // (a profile's phase from here: the frames, apart from the mount, as `ree run` marks them)
    crate::luau_profile::mark_frames();
    let actions = match &args.actions {
        Some(actions) => story::load_actions(lua, actions)?,
        None => Vec::new(),
    };
    let fixed_client_hz = spec.client_hz;
    if let Some(hz) = fixed_client_hz {
        engine.state.borrow_mut().sched.frame_duration = 1.0 / hz;
    }
    let frame = engine.state.borrow().sched.frame_duration;
    let first = (spec.from.max(0.0) / frame).round();
    // Engine frames per captured frame: whole ones when the rate divides the engine's
    let ratio = 1.0 / (spec.fps * frame);
    let whole = fixed_client_hz.is_some() || (ratio.round() >= 1.0 && (ratio - ratio.round()).abs() < 1e-6);
    let effective_hz = fixed_client_hz.unwrap_or_else(|| if whole { 1.0 / frame } else { spec.fps });
    let cadence_kind: &'static str = if fixed_client_hz.is_some() || whole { "client" } else { "display" };
    let moment = |index: u32| {
        if fixed_client_hz.is_some() {
            first * frame + f64::from(index) / spec.fps
        } else if whole {
            (first + f64::from(index) * ratio.round()) * frame
        } else {
            first * frame + f64::from(index) / spec.fps
        }
    };
    let mut timeline = story::Timeline::new(actions);
    let mut whole_frames = |lua: &Lua, engine: &Rc<Engine>, at: f64| super::scheduler::run_until(lua, engine, at);
    let mut fixed_client_frames = |lua: &Lua, engine: &Rc<Engine>, at: f64| -> LuaResult<()> {
        loop {
            let current = engine.state.borrow().sched.now;
            if current + frame <= at + 1e-9 {
                super::scheduler::step(lua, engine, Some(frame))?;
            } else {
                break;
            }
        }
        Ok(())
    };
    let start = if fixed_client_hz.is_some() {
        timeline.advance(lua, engine, moment(0), spec.frames == 1, &mut story::input_dispatch, &mut fixed_client_frames)
    } else {
        timeline.advance(lua, engine, moment(0), spec.frames == 1, &mut story::input_dispatch, &mut whole_frames)
    };
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
    let layout_0 = super::layout::stats_snapshot(engine).delta_since(&layout_start);
    let layout_churn_0 = if spec.collect_cost { super::layout::take_churn(engine) } else { Vec::new() };
    let steps_0 = engine.state.borrow().sched.frame as u32;
    #[cfg(feature = "desktop")]
    let dom_hidden_0 = spec.collect_cost.then(|| crate::cost::collect_dom_hidden(engine, &session.story.roots));
    let now = |engine: &Rc<Engine>| engine.state.borrow().sched.now;
    let pointer_0 = spec.collect_cursor.then(|| super::input::get(engine).map(|input| input.pointer_state(engine))).flatten();
    if !sink(SequenceMessage::Frame(SequenceFrame {
        index: 0,
        time: now(engine),
        reply: ready,
        layout: layout_0,
        client_frames: steps_0,
        client_hz: effective_hz,
        cadence_kind,
        device: engine.options.profile.device.kind().name().to_string(),
        layout_churn: layout_churn_0,
        pointer: pointer_0,
        #[cfg(feature = "desktop")]
        dom_hidden: dom_hidden_0,
    })) {
        return Ok(());
    }
    // The frames' own work (stepping and capturing), apart from the time the drawing kept them waiting
    let frames_started = Instant::now();
    let mut blocked = std::time::Duration::ZERO;
    for index in 1..spec.frames {
        if spec.collect_cursor {
            if let Some(input) = super::input::get(engine) {
                input.clear_frame_click();
            }
        }
        let until = moment(index);
        let last = index + 1 == spec.frames;
        let layout_before = super::layout::stats_snapshot(engine);
        let steps_before = engine.state.borrow().sched.frame;
        let result = if fixed_client_hz.is_some() {
            timeline.advance(lua, engine, until, last, &mut story::input_dispatch, &mut fixed_client_frames)
        } else if whole {
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
        let layout_delta = super::layout::stats_snapshot(engine).delta_since(&layout_before);
        let layout_churn = if spec.collect_cost { super::layout::take_churn(engine) } else { Vec::new() };
        let pointer = spec.collect_cursor.then(|| super::input::get(engine).map(|input| input.pointer_state(engine))).flatten();
        let unchanged = reply["unchanged"] == true;
        #[cfg(feature = "desktop")]
        let dom_hidden = (spec.collect_cost && !unchanged)
            .then(|| crate::cost::collect_dom_hidden(engine, &session.story.roots));
        let sent = Instant::now();
        if !sink(SequenceMessage::Frame(SequenceFrame {
            index,
            time: now(engine),
            reply,
            layout: layout_delta,
            client_frames: steps,
            client_hz: effective_hz,
            cadence_kind,
            device: engine.options.profile.device.kind().name().to_string(),
            layout_churn,
            pointer,
            #[cfg(feature = "desktop")]
            dom_hidden,
        })) {
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
    let (lines, frames, seconds, fps) = {
        let state = engine.state.borrow();
        (state.output.entries.len(), state.sched.frame, state.sched.now, 1.0 / state.sched.frame_duration)
    };
    let status = mounted.status.clone();
    // serde_json::Map is backed by BTreeMap (no preserve_order), so every object's keys are already sorted.
    let text = serde_json::to_string(&scene).map_err(|e| e.to_string())?;
    Ok(SceneOutput { text, scene, status, nodes, lines, dependencies: super::lua::modules::required_files(engine), frames, seconds, fps })
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
mod work_clock_tests {
    use super::*;

    /// The same loop in a plain module and a `--!native` one, timed with os.clock on a Heartbeat (the work before
    /// the first frame is not charged)
    const LOOP: &str = "local Data = {}\nfor Index = 1, 64 do\n\tData[Index] = { Value = Index }\nend\nreturn function()\n\tlocal Sum = 0\n\tfor _ = 1, 400 do\n\t\tfor Index = 1, 64 do\n\t\t\tSum += Data[Index].Value\n\t\tend\n\tend\n\treturn Sum\nend\n";
    const FIXTURE: &str = "return function(Target)\n\tlocal ReplicatedStorage = game:GetService(\"ReplicatedStorage\")\n\tlocal Plain = require(ReplicatedStorage.Plain)\n\tlocal Native = require(ReplicatedStorage.Native)\n\tgame:GetService(\"RunService\").Heartbeat:Once(function()\n\t\tlocal Start = os.clock()\n\t\tPlain()\n\t\tlocal Middle = os.clock()\n\t\tNative()\n\t\tlocal Finish = os.clock()\n\t\tprint(string.format(\"plain %.9f native %.9f\", Middle - Start, Finish - Middle))\n\tend)\nend\n";

    fn timed(output: &[Json]) -> String {
        let line = output.iter().filter_map(|entry| entry["message"].as_str()).find(|message| message.starts_with("plain "));
        line.unwrap_or_else(|| panic!("no timing printed: {output:?}")).to_string()
    }

    /// Every host runs a script's Luau as a client and the emulator's `record` do, charged on the work clock: a
    /// one-shot `engine` run (and so a batch job), `render` and the render service, a standby VM and the desktop
    /// app's session read the same os.clock for the same loop, and a `--!native` module's copy of it costs less
    /// (native code's prices). Natively compiled modules were charged nothing (both readings 0)
    #[test]
    fn every_host_charges_a_scripts_luau_alike() {
        let directory = std::env::temp_dir().join(format!("rur-clock-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let root = directory.join("root");
        std::fs::create_dir_all(root.join("ReplicatedStorage")).unwrap();
        std::fs::write(root.join("ReplicatedStorage").join("Plain.luau"), LOOP).unwrap();
        std::fs::write(root.join("ReplicatedStorage").join("Native.luau"), format!("--!native\n{LOOP}")).unwrap();
        let fixture = directory.join("clock.luau");
        std::fs::write(&fixture, FIXTURE).unwrap();
        let repo = crate::engine::default_repo();
        let argv = |extra: &[&str]| -> Args {
            let mut list: Vec<String> = vec![
                "--repo".into(), repo.to_string_lossy().into_owned(),
                "--roots".into(), root.to_string_lossy().into_owned(),
                "--fixture".into(), fixture.to_string_lossy().into_owned(),
                "--viewport".into(), "400x300".into(), "--time".into(), "0.1".into(), "--no-snapshots".into(),
            ];
            list.extend(extra.iter().map(|arg| arg.to_string()));
            parse(&list).unwrap()
        };
        let output = |scene: &Json| scene["diagnostics"]["output"].as_array().cloned().unwrap_or_default();

        let once = timed(&output(&run_once(&argv(&[])).unwrap().scene));
        let standby = timed(&output(&Preloaded::load(&argv(&[])).unwrap().run(&argv(&[])).unwrap().scene));
        let file = directory.join("scene.json");
        assert_eq!(run(argv(&["--out", &file.to_string_lossy()])).unwrap(), 0);
        let engine_run = timed(&output(&serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap()));
        let session = argv(&["--session", "1"]);
        let mut host = Host::start(&session).unwrap();
        host.mount(&session).unwrap();
        for _ in 0..6 {
            host.handle(&json!({ "cmd": "frame", "dt": 1.0 / 60.0, "events": [] })).unwrap();
        }
        let lines: Vec<Json> = host.engine.state.borrow().output.entries.iter().map(|entry| json!({ "message": entry.message })).collect();
        let app = timed(&lines);
        drop(host);
        let _ = std::fs::remove_dir_all(&directory);

        let numbers: Vec<f64> = once.split_whitespace().filter_map(|word| word.parse().ok()).collect();
        let (plain, native) = (numbers[0], numbers[1]);
        assert!(plain > 50e-6 && native > 0.0 && native < plain / 1.5, "plain {plain} s, native {native} s");
        assert_eq!(standby, once, "a standby VM");
        assert_eq!(engine_run, once, "rur-native engine");
        assert_eq!(app, once, "the desktop app's session");
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
    /// starts below the inset while the story's (IgnoreGuiInset) does not. Scripts read
    /// AbsolutePosition measured from below the inset, as in Roblox: the story's gui at -inset, the
    /// default one at 0 and the inset shorter. Without them a run is Studio's edit-mode desktop.
    #[test]
    fn the_device_and_inset_reach_the_scripts() {
        let desktop = probe(&[]).unwrap();
        // (the label draws the device's and the story's values; the session's are printed: fixtures/device-probe.luau)
        assert!(desktop.contains("touch=false mouse=true keyboard=true inset=0 story=0 gui=0"), "{desktop}");
        assert!(desktop.contains("gamepad=false last=Focus gui height=300"), "{desktop}");
        let phone = probe(&["--device", "touch", "--topbar-inset", "58"]).unwrap();
        assert!(phone.contains("touch=true mouse=false keyboard=false inset=58 story=-58 gui=0"), "{phone}");
        assert!(phone.contains("gamepad=false last=Touch gui height=242"), "{phone}");
        let pad = probe(&["--device", "gamepad"]).unwrap();
        assert!(pad.contains("gamepad=true last=Gamepad1"), "{pad}");
        // (a phone is the engine's touch device with a phone's work-clock prices: ree --device phone)
        let handset = probe(&["--device", "phone"]).unwrap();
        assert!(handset.contains("touch=true mouse=false keyboard=false inset="), "{handset}");
        assert!(handset.contains("gamepad=false last=Touch"), "{handset}");
        assert!(probe(&["--device", "console"]).unwrap_err().contains("--device"));
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
        let handset = flow("phone");
        assert!(handset.contains("preferred=Touch"), "{handset}");
        assert!(handset.contains("touch=Touch same=true"), "{handset}");
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

    /// `--load` and `--model` read a Roblox archive (.rbx) as the model it holds, with the values its writer kept outside
    /// the scene put back (here a label's long Text, which the stored scene alone reads as empty). The label draws with
    /// the test's own font, so it draws on a machine with no Studio install too.
    #[test]
    fn load_and_model_read_an_archive_with_its_values() {
        use rbx_dom_weak::{InstanceBuilder, WeakDom, types::{UDim, UDim2}};
        let directory = std::env::temp_dir().join(format!("rur-load-archive-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(&directory).unwrap();
        let text = "abcdefghij".repeat(30);
        let label = || InstanceBuilder::new("TextLabel").with_name("Long").with_property("Text", text.clone()).with_property("Size", UDim2::new(UDim::new(0., 300), UDim::new(0., 40)));
        let archive = |dom: WeakDom, file: &str| {
            let mut bytes = Vec::new();
            rbx_binary::to_writer(&mut bytes, &dom, dom.root().children()).unwrap();
            let file = directory.join(file);
            std::fs::write(&file, ror_archive::Builder::model(bytes).externalize(256).build().unwrap()).unwrap();
            file
        };
        let model = archive(WeakDom::new(InstanceBuilder::new("DataModel").with_child(InstanceBuilder::new("ScreenGui").with_name("Saved").with_child(label()))), "blob.rbx");
        let alone = archive(WeakDom::new(InstanceBuilder::new("DataModel").with_child(label())), "label.rbx");
        let fixture = directory.join("check.luau");
        std::fs::write(&fixture, format!("return function()
	local Long = game:GetService('ServerStorage').Saved.Blob.Saved.Long
	assert(#Long.Text == {}, 'Text read as ' .. #Long.Text .. ' characters')
end
", text.len())).unwrap();
        let load = format!("ServerStorage/Saved/Blob={}", model.to_string_lossy());
        let output = run_once(&args(&fixture, &["--load", &load])).unwrap();
        assert_eq!(output.status, "ok", "{}", output.text);
        let mut model_args = args(&fixture, &[]);
        (model_args.fixture, model_args.model) = (None, Some(alone.to_string_lossy().into_owned()));
        let output = run_once_with_fonts(&model_args, &own_fonts(&directory, &crate::engine::default_repo())).unwrap();
        assert_eq!(output.status, "ok", "{}", output.text);
        let glyphs: usize = output.scene["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|node| node["path"].as_str().is_some_and(|path| path.ends_with(".Long")))
            .flat_map(|node| node["text"]["lines"].as_array().cloned().unwrap_or_default())
            .map(|line| line["glyphs"].as_array().map_or(0, Vec::len))
            .sum();
        assert!(glyphs > 0, "the label draws its text: {}", output.text);
        std::fs::remove_dir_all(directory).unwrap();
    }
}

/// Fonts of a test's own under `directory`: a frozen font set (as RUR_FONT_ROOT names one) whose only face is the
/// test font in the place of the face every family without files of its own falls back to (Source Sans Pro Regular),
/// so a test's labels draw the same glyphs on a machine with no Studio install
#[cfg(test)]
fn own_fonts(directory: &std::path::Path, repo: &std::path::Path) -> crate::engine::text::FontPaths {
    let font = crate::assets::tests::test_font().expect("the labels need a TrueType font: set RUR_TEST_FONT");
    let fonts = directory.join("fonts");
    let faces = fonts.join("studio").join("content").join("fonts");
    std::fs::create_dir_all(&faces).unwrap();
    std::fs::copy(font, faces.join("SourceSansPro-Regular.ttf")).unwrap();
    let fonts = Some(fonts.to_string_lossy().replace('\\', "/"));
    crate::engine::text::FontPaths { fonts, ..crate::engine::text::FontPaths::for_repo(&repo.to_string_lossy()) }
}

/// [`run_once`], its text engine reading `fonts` instead of the machine's
#[cfg(test)]
fn run_once_with_fonts(args: &Args, fonts: &crate::engine::text::FontPaths) -> Result<SceneOutput, String> {
    use crate::engine::{GuiEngine, text::TextEngine};
    run_once_with(args, |engine| *engine.text().borrow_mut() = Some(TextEngine::new(fonts.clone())))
}

#[cfg(test)]
mod loopback_tests {
    use super::*;

    const SERVER: &str = "return {\n\tServer = function(Context)\n\t\tlocal Storage = game:GetService(\"ReplicatedStorage\")\n\t\tlocal Greeting = Instance.new(\"StringValue\")\n\t\tGreeting.Name = \"Greeting\"\n\t\tGreeting.Value = \"hello from the server\"\n\t\tGreeting.Parent = Storage\n\t\tlocal Count = Instance.new(\"RemoteEvent\")\n\t\tCount.Name = \"Count\"\n\t\tCount.Parent = Storage\n\t\ttask.wait(0.2)\n\t\tGreeting.Value = \"changed by the server\"\n\t\tCount:FireAllClients(42)\n\tend,\n}\n";
    // (each label says what it shows as it is set: a scene holds glyphs, not text)
    const FIXTURE: &str = "return function(Target)\n\tlocal Storage = game:GetService(\"ReplicatedStorage\")\n\tlocal RunService = game:GetService(\"RunService\")\n\tlocal function Set(Label, Text)\n\t\tLabel.Text = Text\n\t\tprint(Label.Name .. \": \" .. Text)\n\tend\n\tlocal function Label(Name, Y, Text)\n\t\tlocal Made = Instance.new(\"TextLabel\")\n\t\tMade.Name = Name\n\t\tMade.Position = UDim2.fromOffset(10, Y)\n\t\tMade.Size = UDim2.fromOffset(380, 40)\n\t\tMade.Parent = Target\n\t\tSet(Made, Text)\n\t\treturn Made\n\tend\n\tLabel(\"Side\", 10, (RunService:IsClient() and \"client\" or \"server\") .. \" \" .. tostring(Storage:FindFirstChild(\"Greeting\") ~= nil))\n\tlocal Greeting = Label(\"Greeting\", 60, Storage.Greeting.Value)\n\tlocal Count = Label(\"Count\", 110, \"nothing yet\")\n\tStorage.Greeting.Changed:Connect(function(Value)\n\t\tSet(Greeting, Value)\n\tend)\n\tStorage.Count.OnClientEvent:Connect(function(Value)\n\t\tSet(Count, \"the server sent \" .. Value)\n\tend)\nend\n";

    /// What a run printed, and each label's glyph count (its path's last part)
    fn shown(output: &SceneOutput) -> (Vec<String>, Vec<(String, usize)>) {
        let scene = &output.scene;
        let printed = scene["diagnostics"]["output"].as_array().into_iter().flatten().filter_map(|line| line["message"].as_str().map(str::to_string)).collect();
        let glyphs = scene["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|node| {
                let name = node["path"].as_str()?.rsplit('.').next()?.to_string();
                let count = node["text"]["lines"].as_array()?.iter().map(|line| line["glyphs"].as_array().map_or(0, Vec::len)).sum();
                Some((name, count))
            })
            .collect();
        (printed, glyphs)
    }

    /// The glyphs a label of `text` draws (one a character but spaces)
    fn glyphs_of(text: &str) -> usize {
        text.chars().filter(|c| !c.is_whitespace()).count()
    }

    /// `--run-context loopback --server-harness`: the story is the client's, its DataModel the server's replicated
    /// containers (the emulator's "Loopback run context"); the server's harness makes a value before the join, then
    /// changes it and fires a remote at 0.2 s, and the client's labels draw what arrived. `--server-harness` without the
    /// loopback context is refused.
    #[test]
    fn a_story_on_a_loopback_client_shows_what_the_server_sent() {
        let directory = std::env::temp_dir().join(format!("rur-loopback-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let root = directory.join("root");
        std::fs::create_dir_all(root.join("ReplicatedStorage")).unwrap();
        let (server, fixture) = (directory.join("server.luau"), directory.join("labels.luau"));
        std::fs::write(&server, SERVER).unwrap();
        std::fs::write(&fixture, FIXTURE).unwrap();
        let repo = crate::engine::default_repo();
        let argv = |time: &str, extra: &[&str]| -> Args {
            let mut list: Vec<String> = vec![
                "--repo".into(), repo.to_string_lossy().into_owned(),
                "--roots".into(), root.to_string_lossy().into_owned(),
                "--fixture".into(), fixture.to_string_lossy().into_owned(),
                "--viewport".into(), "400x200".into(), "--time".into(), time.into(), "--no-snapshots".into(),
                "--mode".into(), "play".into(),
            ];
            list.extend(extra.iter().map(|arg| arg.to_string()));
            parse(&list).unwrap()
        };
        let server_path = server.to_string_lossy().into_owned();
        let fonts = own_fonts(&directory, &repo);
        let with_server = |time: &str| run_once_with_fonts(&argv(time, &["--run-context", "loopback", "--server-harness", &server_path]), &fonts).unwrap();

        let (printed, glyphs) = shown(&with_server("0.1"));
        for (label, text) in [("Side", "client true"), ("Greeting", "hello from the server"), ("Count", "nothing yet")] {
            assert!(printed.contains(&format!("{label}: {text}")), "{printed:?}");
            assert!(glyphs.contains(&(label.to_string(), glyphs_of(text))), "{label} draws {text:?}: {glyphs:?}");
        }
        assert!(!printed.iter().any(|line| line.contains("changed by the server")), "{printed:?}");

        let (printed, glyphs) = shown(&with_server("0.5"));
        for (label, text) in [("Greeting", "changed by the server"), ("Count", "the server sent 42")] {
            assert!(printed.contains(&format!("{label}: {text}")), "{printed:?}");
            assert!(glyphs.contains(&(label.to_string(), glyphs_of(text))), "{label} draws {text:?}: {glyphs:?}");
        }

        let refused = run_once(&argv("0.1", &["--server-harness", &server_path]));
        assert!(refused.is_err_and(|error| error.contains("--run-context loopback")));
        let _ = std::fs::remove_dir_all(&directory);
    }
}

#[cfg(test)]
mod font_tests {
    use super::*;
    use crate::engine::{
        GuiEngine,
        text::{FontPaths, TextEngine},
    };
    use crate::js::Json as Value;

    const FAMILY: &str = "rbxasset://fonts/families/Ubuntu.json";
    const FIXTURE: &str = "return function(Target)\n\tlocal Button = Instance.new(\"TextButton\")\n\tButton.Name = \"Play\"\n\tButton.Size = UDim2.fromOffset(200, 50)\n\tButton.FontFace = Font.new(\"rbxasset://fonts/families/Ubuntu.json\")\n\tButton.Text = \"Play\"\n\tButton.Parent = Target\nend\n";

    /// The button's text node: what it says and how many lines it draws
    fn button(output: &SceneOutput) -> (String, usize) {
        let node = output.scene["nodes"].as_array().into_iter().flatten().find(|node| node["path"].as_str().is_some_and(|path| path.ends_with(".Play"))).expect("the button");
        (node["props"]["text"].as_str().unwrap_or("").to_string(), node["text"]["lines"].as_array().map_or(0, Vec::len))
    }

    /// With no font file to read (no Studio fonts, no cached cloud fonts: a machine without Studio and
    /// RUR_ROBLOX_CONTENT unset) a button's family and the Source Sans Pro fallback have no face, so its text is not
    /// drawn. The scene names the family (`diagnostics.unavailableFonts`, and a font diagnostic saying so), the summary
    /// `rur render` writes lists it in `missingFonts` and the button's note says why it is blank, instead of a summary
    /// with nothing missing. With the fallback face to read, the same button draws and nothing is unavailable.
    #[test]
    fn text_with_no_face_to_draw_with_is_a_missing_font() {
        let directory = std::env::temp_dir().join(format!("rur-no-fonts-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let (fixture, fonts, out) = (directory.join("button.luau"), directory.join("fonts"), directory.join("out"));
        std::fs::create_dir_all(&fonts).unwrap();
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(&fixture, FIXTURE).unwrap();
        let repo = crate::engine::default_repo();
        let args = parse(&[
            "--repo".into(), repo.to_string_lossy().into_owned(),
            "--fixture".into(), fixture.to_string_lossy().into_owned(),
            "--viewport".into(), "400x200".into(), "--time".into(), "0.1".into(), "--no-snapshots".into(),
        ])
        .unwrap();
        // (a frozen font set, as RUR_FONT_ROOT names one, with nothing in it)
        let paths = FontPaths { fonts: Some(fonts.to_string_lossy().replace('\\', "/")), ..FontPaths::for_repo(&repo.to_string_lossy()) };
        let run = |paths: &FontPaths| run_once_with(&args, |engine| *engine.text().borrow_mut() = Some(TextEngine::new(paths.clone()))).unwrap();

        let output = run(&paths);
        let diagnostics = &output.scene["diagnostics"];
        assert_eq!(button(&output), ("Play".to_string(), 0));
        assert_eq!(diagnostics["unavailableFonts"], serde_json::json!([FAMILY]), "{diagnostics}");
        let said = diagnostics["fonts"].as_array().into_iter().flatten().find(|entry| entry["key"] == FAMILY).and_then(|entry| entry["message"].as_str()).unwrap_or("");
        assert!(said.contains("is not drawn") && !said.contains("using Source Sans Pro metrics"), "{diagnostics}");
        std::fs::write(out.join("scene.json"), &output.text).unwrap();
        let summary = crate::summary::render_summary(&out, false, &Value::parse(r#"{"status": "ok", "renderErrors": []}"#).unwrap(), 0.0, None).unwrap();
        assert!(summary.get("missingFonts").items().contains(&Value::from(FAMILY)), "{}", summary.stringify(0));
        assert_eq!(summary.get("missingGlyphs").items().len(), 0, "(no glyph was laid out to be missing)");
        let notes = crate::summary::scene_diagnostics(&Value::parse(&output.text).unwrap()).notes;
        assert!(notes.values().flatten().any(|note| note.starts_with("text not drawn") && note.contains(FAMILY)), "{notes:?}");

        if let Some(font) = crate::assets::tests::test_font() {
            let faces = fonts.join("studio").join("content").join("fonts");
            std::fs::create_dir_all(&faces).unwrap();
            std::fs::copy(font, faces.join("SourceSansPro-Regular.ttf")).unwrap();
            let output = run(&paths);
            assert_eq!(button(&output), ("Play".to_string(), 1));
            assert!(output.scene["diagnostics"].get("unavailableFonts").is_none(), "{}", output.scene["diagnostics"]);
        }
        let _ = std::fs::remove_dir_all(&directory);
    }
}
