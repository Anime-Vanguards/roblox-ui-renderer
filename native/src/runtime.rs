//! One engine per mounted story (engine::cli::Host: the engine, its Luau VM and the story's
//! session), confined to its worker thread; standbys preload a place's engine and project.
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use mlua::prelude::*;
use serde_json::Value;
use tokio::sync::mpsc as async_channel;

type InterruptCallback = unsafe extern "C-unwind" fn(*mut mlua::ffi::lua_State, i32);
#[derive(Clone, Copy)]
struct InterruptGate {
    generation: *const AtomicU64,
    stopped: *const AtomicBool,
    /// The generation this VM's story belongs to; zero while a standby VM awaits its story.
    mounted: *const AtomicU64,
    slow: InterruptCallback,
}
thread_local! {
    static INTERRUPT_GATE: std::cell::Cell<Option<InterruptGate>> = const { std::cell::Cell::new(None) };
}
struct ClearInterruptGate;
impl Drop for ClearInterruptGate {
    fn drop(&mut self) {
        INTERRUPT_GATE.with(|gate| gate.set(None));
    }
}
unsafe extern "C-unwind" fn cancellation_interrupt(state: *mut mlua::ffi::lua_State, gc: i32) {
    // Like mlua, never raise from a GC interrupt. Its original protected callback still owns
    // error creation/unwinding; the overwhelmingly common continuing path only reads atomics.
    if gc >= 0 {
        return;
    }
    crate::luau_profile::sample(state);
    let gate = INTERRUPT_GATE.with(|gate| gate.get());
    if let Some(gate) = gate {
        unsafe {
            let mounted = (*gate.mounted).load(Ordering::Relaxed);
            if (*gate.stopped).load(Ordering::Relaxed)
                || (mounted != 0 && (*gate.generation).load(Ordering::Relaxed) != mounted)
            {
                (gate.slow)(state, gc);
            }
        }
    }
}

fn canonical_path(path: &std::path::Path) -> std::io::Result<PathBuf> {
    let resolved = path.canonicalize()?;
    // Luau's project lookup compares normalized path strings. Keep Windows' verbatim prefix
    // out of mounted roots so they match script paths and the CLI's existing path convention.
    #[cfg(windows)]
    {
        let text = resolved.to_string_lossy();
        if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
            return Ok(PathBuf::from(format!(r"\\{unc}")));
        }
        if let Some(local) = text.strip_prefix(r"\\?\") {
            return Ok(PathBuf::from(local));
        }
    }
    Ok(resolved)
}

#[derive(Clone, Debug)]
pub struct RuntimeOptions {
    pub repo: PathBuf,
    pub project: PathBuf,
    pub story: PathBuf,
    pub viewport: [u32; 2],
    pub scale: f64,
    pub time: f64,
    pub margin: u32,
    pub controls: Value,
    pub roots: Option<String>,
    pub snapshot_dir: Option<PathBuf>,
    pub no_snapshots: bool,
    pub offline: bool,
}

impl RuntimeOptions {
    pub fn from_native(options: &crate::NativeOptions, story: PathBuf) -> Self {
        Self {
            repo: options.repo.clone(),
            project: options.project.clone(),
            story,
            viewport: options.viewport,
            scale: options.scale,
            time: options.time,
            margin: options
                .margin
                .unwrap_or(if options.headless { 0 } else { 48 }),
            controls: options.controls.clone(),
            roots: options.roots.clone(),
            snapshot_dir: options.snapshot_dir.clone(),
            no_snapshots: options.no_snapshots,
            offline: options.offline,
        }
    }
}

/// Arguments that load the engine and project, shared by every story of one place. A standby VM
/// preloaded with these arguments can mount any story whose options produce the same key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StandbyKey(Vec<String>);

#[derive(Clone)]
struct Launch {
    repo: PathBuf,
    place: Vec<String>,
    story: PathBuf,
}

impl RuntimeOptions {
    fn launch(&self) -> Result<Launch, String> {
        let repo =
            canonical_path(&self.repo).map_err(|error| format!("renderer repository: {error}"))?;
        let project =
            canonical_path(&self.project).map_err(|error| format!("project: {error}"))?;
        let story = if self.story.is_absolute() {
            self.story.clone()
        } else {
            project.join(&self.story)
        };
        let story = canonical_path(&story).map_err(|error| format!("story: {error}"))?;
        // Without --roots: the roots of the project's place that holds the story (src/places.rs)
        let roots = self.roots.clone().unwrap_or_else(|| crate::places::joined(&crate::places::place_of(&project, &story)));
        let mut place = vec![
            "--roots".into(),
            roots,
            "--viewport".into(),
            format!("{}x{}", self.viewport[0], self.viewport[1]),
            "--scale".into(),
            self.scale.to_string(),
            "--session".into(),
            "1".into(),
        ];
        if self.margin > 0 {
            place.extend(["--margin".into(), self.margin.to_string()]);
        }
        if self.offline {
            place.push("--offline".into());
        }
        // Studio snapshots load only when given (--snapshot-dir)
        let snapshot = self.snapshot_dir.clone();
        if let Some(snapshot) = snapshot {
            place.extend([
                "--snapshot-dir".into(),
                snapshot.to_string_lossy().into_owned(),
            ]);
        }
        Ok(Launch { repo, place, story })
    }

    /// Stories with equal keys can share a preloaded engine and project.
    pub fn standby_key(&self) -> Option<StandbyKey> {
        let launch = self.launch().ok()?;
        let mut key = launch.place;
        key.push(launch.repo.to_string_lossy().into_owned());
        Some(StandbyKey(key))
    }

    fn story_args(&self, launch: &Launch) -> Vec<String> {
        let mut args = vec![
            "--story".into(),
            launch.story.to_string_lossy().into_owned(),
            "--time".into(),
            self.time.to_string(),
        ];
        if let Some(controls) = self.controls_json() {
            args.extend(["--controls".into(), controls]);
        }
        args
    }

    fn controls_json(&self) -> Option<String> {
        self.controls
            .as_object()
            .is_some_and(|controls| !controls.is_empty())
            .then(|| self.controls.to_string())
    }
}

impl StandbyKey {
    /// Everything a preloaded engine has already read: data files, the project's directory
    /// structure (script sources are read at require time) and snapshots. A standby with a
    /// different fingerprint than a fresh computation is stale and must not mount a story.
    pub fn fingerprint(&self) -> u64 {
        use rayon::prelude::*;
        use std::hash::{Hash, Hasher};
        fn stamp(path: &std::path::Path, metadata: &std::fs::Metadata) -> u64 {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            path.hash(&mut hasher);
            metadata.len().hash(&mut hasher);
            metadata.modified().ok().hash(&mut hasher);
            hasher.finish()
        }
        // Order-independent, so directories can be visited in parallel.
        fn walk(path: &std::path::Path, files: bool) -> u64 {
            let Ok(entries) = std::fs::read_dir(path) else {
                return 0;
            };
            let entries: Vec<_> = entries.flatten().collect();
            entries
                .par_iter()
                .map(|entry| {
                    let name = entry.file_name();
                    let name = name.to_string_lossy();
                    let lower = name.to_lowercase();
                    let Ok(mut kind) = entry.file_type() else {
                        return 0;
                    };
                    if kind.is_symlink() {
                        match std::fs::metadata(entry.path()) {
                            Ok(metadata) => kind = metadata.file_type(),
                            Err(_) => return 0,
                        }
                    }
                    let path = entry.path();
                    if kind.is_dir() {
                        if !files && (name.starts_with('.') || lower.ends_with(".lua") || lower.ends_with(".luau")) {
                            return 0;
                        }
                        let own = std::fs::metadata(&path).map_or(0, |metadata| stamp(&path, &metadata));
                        own.rotate_left(1) ^ walk(&path, files)
                    } else if files {
                        std::fs::metadata(&path).map_or(0, |metadata| stamp(&path, &metadata))
                    } else {
                        0
                    }
                })
                .reduce(|| 0, |a, b| a ^ b)
        }
        let values = &self.0;
        let repo = PathBuf::from(values.last().cloned().unwrap_or_default());
        let mut sources = vec![(repo.join("data"), true)];
        for pair in values.windows(2) {
            match pair[0].as_str() {
                "--roots" => sources.extend(
                    pair[1]
                        .split(';')
                        .filter(|root| !root.is_empty())
                        .map(|root| (PathBuf::from(root), false)),
                ),
                "--snapshot-dir" => sources.push((PathBuf::from(&pair[1]), true)),
                _ => {}
            }
        }
        sources
            .par_iter()
            .map(|(path, files)| {
                let root = std::fs::metadata(path).map_or(0, |metadata| stamp(path, &metadata));
                root ^ walk(path, *files).rotate_left(7)
            })
            .reduce(|| 0, |a, b| a ^ b)
    }
}

#[derive(Debug)]
pub enum RuntimeEvent {
    Reply(Value),
    Error(String),
    Stopped,
}

enum Request {
    Command(u64, Value),
    Begin(u64, RuntimeOptions),
    Stop,
}

enum Initial {
    Story(u64, RuntimeOptions),
    Standby(RuntimeOptions, StandbyKey, Arc<std::sync::OnceLock<u64>>),
}

pub struct RuntimeWorker {
    requests: async_channel::UnboundedSender<Request>,
    replies: mpsc::Receiver<(u64, RuntimeEvent)>,
    busy: Arc<AtomicU64>,
    generation: Arc<AtomicU64>,
    stopped: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
    /// For a standby: its place and the source fingerprint taken before its VM read any file.
    standby: Option<(StandbyKey, Arc<std::sync::OnceLock<u64>>)>,
}

impl RuntimeWorker {
    pub fn spawn(options: RuntimeOptions) -> Result<Self, String> {
        Self::start(Initial::Story(1, options))
    }

    /// Loads the engine and project for `options`' place, then waits for `begin_story`. A story of
    /// the same place mounts without repeating that work; any other story starts a fresh VM.
    pub fn spawn_standby(options: RuntimeOptions) -> Result<Self, String> {
        let key = options
            .standby_key()
            .ok_or("standby options do not name a loadable place")?;
        let fingerprint = Arc::new(std::sync::OnceLock::new());
        let mut worker = Self::start(Initial::Standby(options, key.clone(), fingerprint.clone()))?;
        worker.standby = Some((key, fingerprint));
        Ok(worker)
    }

    /// Whether this standby can mount `options`: the same place, with no runtime, data or
    /// project-structure change since it started loading.
    pub fn standby_matches(&self, options: &RuntimeOptions) -> bool {
        let Some((key, fingerprint)) = &self.standby else {
            return false;
        };
        if self.stopped.load(Ordering::Acquire) || options.standby_key().as_ref() != Some(key) {
            return false;
        }
        // Not fingerprinted yet means the VM has not read anything so far.
        fingerprint
            .get()
            .is_none_or(|before| *before == key.fingerprint())
    }

    fn start(initial: Initial) -> Result<Self, String> {
        let (requests, receiver) = async_channel::unbounded_channel();
        let (sender, replies) = mpsc::channel();
        let busy = Arc::new(AtomicU64::new(match initial {
            Initial::Story(id, _) => id,
            Initial::Standby(..) => 0,
        }));
        let generation = Arc::new(AtomicU64::new(1));
        let stopped = Arc::new(AtomicBool::new(false));
        let worker_busy = busy.clone();
        let worker_generation = generation.clone();
        let worker_stopped = stopped.clone();
        let thread = thread::Builder::new()
            .name("rur-engine".into())
            .spawn(move || {
                crate::microprofile::register_thread("Engine");
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run_engine_worker(initial, receiver, sender.clone(), worker_generation, worker_stopped.clone())
                }));
                if result.is_err() {
                    let _ = sender.send((0, RuntimeEvent::Error("engine worker panicked".into())));
                }
                worker_busy.store(0, Ordering::Release);
                worker_stopped.store(true, Ordering::Release);
                let _ = sender.send((0, RuntimeEvent::Stopped));
            })
            .map_err(|error| error.to_string())?;
        Ok(Self { requests, replies, busy, generation, stopped, thread: Some(thread), standby: None })
    }

    pub fn begin_story(&self, options: RuntimeOptions) -> Result<(), String> {
        if self.stopped.load(Ordering::Acquire) {
            return Err("runtime is stopped".into());
        }
        let generation = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
        self.busy.store(generation, Ordering::Release);
        self.requests
            .send(Request::Begin(generation, options))
            .map_err(|_| "runtime is stopped".into())
    }

    /// A rejected command is still owned by the caller; keep its input batch and retry after a reply.
    pub fn try_command(&self, command: Value) -> Result<(), String> {
        if self.stopped.load(Ordering::Acquire) {
            return Err("runtime is stopped".into());
        }
        if !command.is_object() || command.get("cmd").and_then(Value::as_str).is_none() {
            return Err("runtime command requires a cmd string".into());
        }
        if command["cmd"] == "quit" {
            self.stop();
            return Ok(());
        }
        let generation = self.generation.load(Ordering::Acquire);
        if self
            .busy
            .compare_exchange(0, generation, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err("runtime is busy".into());
        }
        if self
            .requests
            .send(Request::Command(generation, command))
            .is_err()
        {
            let _ = self
                .busy
                .compare_exchange(generation, 0, Ordering::AcqRel, Ordering::Acquire);
            return Err("runtime is stopped".into());
        }
        Ok(())
    }

    pub fn is_busy(&self) -> bool {
        self.busy.load(Ordering::Acquire) != 0
    }
    pub fn poll(&self) -> Option<RuntimeEvent> {
        while let Ok((generation, event)) = self.replies.try_recv() {
            if generation == 0 || generation == self.generation.load(Ordering::Acquire) {
                let _ =
                    self.busy
                        .compare_exchange(generation, 0, Ordering::AcqRel, Ordering::Acquire);
                return Some(event);
            }
        }
        None
    }
    pub fn recv_timeout(&self, timeout: Duration) -> Result<RuntimeEvent, mpsc::RecvTimeoutError> {
        let deadline = Instant::now() + timeout;
        loop {
            let (generation, event) = self
                .replies
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))?;
            if generation == 0 || generation == self.generation.load(Ordering::Acquire) {
                let _ =
                    self.busy
                        .compare_exchange(generation, 0, Ordering::AcqRel, Ordering::Acquire);
                return Ok(event);
            }
        }
    }
    pub fn stop(&self) {
        if !self.stopped.swap(true, Ordering::AcqRel) {
            self.generation.fetch_add(1, Ordering::AcqRel);
        }
        let _ = self.requests.send(Request::Stop);
    }
}

impl Drop for RuntimeWorker {
    fn drop(&mut self) {
        self.stop();
        // Stop wakes an idle VM and interrupts active Luau at its next safepoint.
        // Join completed threads without making the UI wait for teardown.
        if self
            .thread
            .as_ref()
            .is_some_and(|thread| thread.is_finished())
        {
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }
}

// The engine in process: engine::cli::Host mounts the project and story like the engine CLI and
// answers the session commands with serde_json values (changed nodes as patches) that
// SceneStore::apply_owned takes as they are.

impl RuntimeOptions {
    /// The engine CLI arguments of the launch (place, then optionally the story).
    fn engine_args(&self, launch: &Launch, story: bool) -> Result<crate::engine::cli::Args, String> {
        let mut argv = launch.place.clone();
        if story {
            argv.extend(self.story_args(launch));
        }
        argv.extend(["--repo".into(), launch.repo.to_string_lossy().into_owned()]);
        crate::engine::cli::parse(&argv)
    }
}

/// Stops scripts at the Luau VM's safepoints when the story is stopped or switched (the error
/// comes from mlua's interrupt), and samples the Luau stack for RUR_LUAU_PROFILE: the VM's raw
/// interrupt callback becomes `cancellation_interrupt`, which only reads atomics on the common
/// path and calls mlua's callback when the story must stop. The gate's pointers stay valid while
/// the worker owns `generation` / `stopped` / `mounted`; the worker runs one VM at a time.
fn engine_interrupt(lua: &Lua, generation: &Arc<AtomicU64>, stopped: &Arc<AtomicBool>, mounted: &Arc<AtomicU64>) {
    let (interrupt_generation, interrupt_stopped, interrupt_mounted) = (generation.clone(), stopped.clone(), mounted.clone());
    lua.set_interrupt(move |_| {
        let current = interrupt_mounted.load(Ordering::Relaxed);
        if interrupt_stopped.load(Ordering::Relaxed) || (current != 0 && interrupt_generation.load(Ordering::Relaxed) != current) {
            Err(LuaError::runtime("native runtime cancelled"))
        } else {
            Ok(mlua::VmState::Continue)
        }
    });
    unsafe {
        let _ = lua.exec_raw::<()>((), |state| {
            let callbacks = mlua::ffi::lua_callbacks(state);
            if let Some(slow) = (*callbacks).interrupt {
                INTERRUPT_GATE.with(|gate| {
                    gate.set(Some(InterruptGate {
                        generation: Arc::as_ptr(generation),
                        stopped: Arc::as_ptr(stopped),
                        mounted: Arc::as_ptr(mounted),
                        slow,
                    }))
                });
                (*callbacks).interrupt = Some(cancellation_interrupt);
            }
        });
    }
}

/// Where the modules a story required at its last mount are listed (read ahead at the next
/// mount): `.cache/native-app/requires/<hash of the story, project and roots>.txt`.
fn requires_list(options: &RuntimeOptions) -> Option<PathBuf> {
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    options.story.hash(&mut hash);
    options.project.hash(&mut hash);
    options.roots.hash(&mut hash);
    Some(options.repo.join(".cache").join("native-app").join("requires").join(format!("{:016x}.txt", hash.finish())))
}

fn run_engine_worker(
    initial: Initial,
    mut receiver: async_channel::UnboundedReceiver<Request>,
    replies: mpsc::Sender<(u64, RuntimeEvent)>,
    generation: Arc<AtomicU64>,
    stopped: Arc<AtomicBool>,
) {
    use crate::engine::cli::Host;
    let _interrupt_scope = ClearInterruptGate;
    let _memory = crate::memtrace::scope(crate::memtrace::Tag::Script);
    let _profile = crate::luau_profile::begin();
    let mounted = Arc::new(AtomicU64::new(0));
    let mut next: Option<(u64, RuntimeOptions)> = None;
    // A standby: the engine and project of a place, waiting for a story of that place
    let mut standby: Option<(StandbyKey, Host)> = None;
    match initial {
        Initial::Story(id, options) => next = Some((id, options)),
        Initial::Standby(options, key, fingerprint) => {
            let _ = fingerprint.set(key.fingerprint());
            let host = options.launch().and_then(|launch| options.engine_args(&launch, false)).and_then(|args| Host::start(&args));
            if let Ok(host) = host {
                engine_interrupt(&host.lua, &generation, &stopped, &mounted);
                standby = Some((key, host));
            }
        }
    }
    loop {
        if stopped.load(Ordering::Acquire) {
            break;
        }
        let (id, options) = match next.take() {
            Some(pending) => pending,
            None => match receiver.blocking_recv() {
                Some(Request::Begin(id, options)) => (id, options),
                Some(Request::Command(id, _)) => {
                    let _ = replies.send((id, RuntimeEvent::Error("no mounted story; select a story first".into())));
                    continue;
                }
                _ => break,
            },
        };
        if id != generation.load(Ordering::Acquire) {
            continue;
        }
        let args = match options.launch().and_then(|launch| options.engine_args(&launch, true)) {
            Ok(args) => args,
            Err(error) => {
                let _ = replies.send((id, RuntimeEvent::Error(error)));
                continue;
            }
        };
        let host = match standby.take() {
            Some((key, host)) if options.standby_key().as_ref() == Some(&key) => Ok(host),
            _ => Host::start(&args).inspect(|host| engine_interrupt(&host.lua, &generation, &stopped, &mounted)),
        };
        let mut host = match host {
            Ok(host) => host,
            Err(error) => {
                let _ = replies.send((id, RuntimeEvent::Error(error)));
                continue;
            }
        };
        mounted.store(id, Ordering::Release);
        // The files this story required when it was last mounted are read and compiled to
        // bytecode on other threads meanwhile (the chunks stay interpreted: cli.rs interpret)
        let requires = requires_list(&options);
        if let Some(paths) = requires.as_ref().and_then(|file| std::fs::read_to_string(file).ok()) {
            let paths: Vec<String> = paths.lines().filter(|line| !line.is_empty()).map(str::to_string).collect();
            crate::engine::lua::modules::use_prefetch(&host.engine, crate::engine::lua::modules::Prefetch::start(paths));
        }
        match host.mount(&args) {
            Ok(ready) => {
                let _ = replies.send((id, RuntimeEvent::Reply(ready)));
                if let Some(file) = &requires {
                    let list = crate::engine::lua::modules::required_files(&host.engine).join("\n");
                    if std::fs::read_to_string(file).ok().as_deref() != Some(list.as_str()) {
                        let _ = file.parent().map(std::fs::create_dir_all);
                        let _ = std::fs::write(file, list);
                    }
                }
            }
            Err(error) => {
                if id == generation.load(Ordering::Acquire) && !stopped.load(Ordering::Acquire) {
                    let _ = replies.send((id, RuntimeEvent::Error(error)));
                }
                mounted.store(0, Ordering::Release);
                continue;
            }
        }
        loop {
            match receiver.blocking_recv() {
                Some(Request::Command(command_id, command))
                    if command_id == id && command_id == generation.load(Ordering::Acquire) =>
                {
                    crate::luau_profile::mark_frames();
                    match host.handle(&command) {
                        Some(reply) => {
                            if id == generation.load(Ordering::Acquire) && !stopped.load(Ordering::Acquire) {
                                let _ = replies.send((id, RuntimeEvent::Reply(reply)));
                            }
                        }
                        None => return,
                    }
                }
                Some(Request::Command(..)) => continue,
                Some(Request::Begin(next_id, next_options)) => {
                    next = Some((next_id, next_options));
                    break;
                }
                _ => return,
            }
        }
        mounted.store(0, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixture() -> RuntimeOptions {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_path_buf();
        let roots = repo.join("native/tests/fixtures");
        RuntimeOptions {
            story: roots.join("ReplicatedStorage/Worker.story.luau"),
            project: repo.clone(),
            repo,
            roots: Some(roots.to_string_lossy().into_owned()),
            viewport: [640, 480],
            scale: 1.0,
            time: 0.0,
            margin: 0,
            controls: json!({}),
            snapshot_dir: None,
            no_snapshots: true,
            offline: true,
        }
    }

    fn reply(worker: &RuntimeWorker) -> Value {
        match worker
            .recv_timeout(Duration::from_secs(30))
            .expect("worker response timeout")
        {
            RuntimeEvent::Reply(value) => value,
            other => panic!("unexpected worker event: {other:?}"),
        }
    }

    /// Command ordering, presentation, controls, scene, switching and stopping through the
    /// in-process engine session.
    #[test]
    fn engine_worker_orders_commands_switches_and_stops() {
        let options = fixture();
        let worker = RuntimeWorker::spawn(options.clone()).unwrap();
        assert!(worker.is_busy());
        let ready = reply(&worker);
        assert_eq!(ready["type"], "ready", "{ready}");
        assert_eq!(ready["delta"]["header"]["status"], "ok");
        assert_eq!(ready["input"], true);
        assert!(ready["delta"]["nodes"].as_array().is_some_and(|nodes| !nodes.is_empty()));
        let mut scene = crate::scene::SceneStore::default();
        let mut owned = ready.clone();
        scene.apply_owned(&mut owned).unwrap();
        worker
            .try_command(json!({"cmd":"frame","dt":0.016,"cadence":"display",
            "events":[{"type":"wheel","x":100,"y":100,"dy":120}],"presentation":{"version":1,"seq":0,"scrolls":[]}}))
            .unwrap();
        let mut frame = reply(&worker);
        assert_eq!(frame["type"], "frame");
        assert_eq!(frame["counters"]["inputEvents"], 1);
        assert!(frame["presentation"]["scrolls"].is_array(), "presentation snapshot missing: {frame}");
        scene.apply_owned(&mut frame).unwrap();
        worker.try_command(json!({"cmd":"control","path":["Missing"],"value":1})).unwrap();
        assert_eq!(reply(&worker)["type"], "error");
        worker.try_command(json!({"cmd":"scene"})).unwrap();
        assert_eq!(reply(&worker)["type"], "scene");
        worker.try_command(json!({"cmd":"frame","dt":0.016,"events":[]})).unwrap();
        worker.begin_story(options.clone()).unwrap();
        assert_eq!(reply(&worker)["type"], "ready", "stale previous-session reply escaped");
        worker.stop();
        assert!(matches!(worker.recv_timeout(Duration::from_secs(10)).unwrap(), RuntimeEvent::Stopped));

        // A standby preloads the engine and project and mounts a story of its place
        let standby = RuntimeWorker::spawn_standby(options.clone()).unwrap();
        assert!(standby.standby_matches(&options));
        standby.begin_story(options.clone()).unwrap();
        assert_eq!(reply(&standby)["type"], "ready");
    }

    #[test]
    fn standby_mounts_matching_stories_and_restarts_for_other_places() {
        let options = fixture();
        let standby = RuntimeWorker::spawn_standby(options.clone()).unwrap();
        assert!(!standby.is_busy());
        assert!(standby.standby_matches(&options));
        let mut elsewhere = options.clone();
        elsewhere.viewport = [320, 240];
        assert!(!standby.standby_matches(&elsewhere));
        standby.begin_story(options.clone()).unwrap();
        let ready = reply(&standby);
        assert_eq!(ready["type"], "ready");
        assert_eq!(ready["delta"]["header"]["status"], "ok");
        assert!(
            ready["delta"]["nodes"]
                .as_array()
                .is_some_and(|nodes| !nodes.is_empty())
        );
        standby
            .try_command(json!({"cmd":"frame","dt":0.016,"cadence":"display","events":[]}))
            .unwrap();
        assert_eq!(reply(&standby)["type"], "frame");
        // A mounted standby behaves like any worker: a later story starts a fresh VM.
        standby.begin_story(elsewhere.clone()).unwrap();
        assert_eq!(reply(&standby)["type"], "ready");

        let mismatched = RuntimeWorker::spawn_standby(options).unwrap();
        mismatched.begin_story(elsewhere).unwrap();
        assert_eq!(reply(&mismatched)["type"], "ready");
        mismatched.stop();
        assert!(matches!(
            mismatched.recv_timeout(Duration::from_secs(10)).unwrap(),
            RuntimeEvent::Stopped
        ));
    }

    /// `RUR_STANDBY_STORY=<story> cargo test --release standby_open_timing -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn standby_open_timing() {
        let Some(story) = std::env::var_os("RUR_STANDBY_STORY") else {
            return;
        };
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf();
        let options = RuntimeOptions {
            story: PathBuf::from(story),
            project: std::env::var_os("RUR_PROJECT").map(PathBuf::from).unwrap_or_else(|| repo.join("examples/project")),
            repo,
            roots: None,
            viewport: [1200, 900],
            scale: 1.5,
            time: 1.0,
            margin: 0,
            controls: json!({}),
            snapshot_dir: None,
            no_snapshots: false,
            offline: true,
        };
        for _ in 0..2 {
            let started = Instant::now();
            let fresh = RuntimeWorker::spawn(options.clone()).unwrap();
            assert_eq!(reply(&fresh)["type"], "ready");
            eprintln!("fresh engine open: {:?}", started.elapsed());
            let standby = RuntimeWorker::spawn_standby(options.clone()).unwrap();
            thread::sleep(Duration::from_secs(4));
            let started = Instant::now();
            assert!(standby.standby_matches(&options));
            let checked = started.elapsed();
            standby.begin_story(options.clone()).unwrap();
            assert_eq!(reply(&standby)["type"], "ready");
            eprintln!("standby open: {:?} (freshness check {checked:?})", started.elapsed());
        }
    }

    #[test]
    fn rejects_second_command_until_first_finishes() {
        let (requests, mut receiver) = async_channel::unbounded_channel();
        let (_sender, replies) = mpsc::channel();
        let worker = RuntimeWorker {
            requests,
            replies,
            busy: Arc::new(AtomicU64::new(0)),
            generation: Arc::new(AtomicU64::new(1)),
            stopped: Arc::new(AtomicBool::new(false)),
            thread: None,
            standby: None,
        };
        worker
            .try_command(json!({"cmd":"frame","events":[]}))
            .unwrap();
        assert!(worker.try_command(json!({"cmd":"scene"})).is_err());
        assert!(matches!(receiver.try_recv(), Ok(Request::Command(1, _))));
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn switch_and_stop_interrupt_non_yielding_story_callbacks() {
        let normal = fixture();
        let mut busy = normal.clone();
        busy.story = busy.story.with_file_name("Busy.story.luau");
        let worker = RuntimeWorker::spawn(busy.clone()).unwrap();
        assert_eq!(reply(&worker)["type"], "ready");
        worker
            .try_command(json!({"cmd":"frame","dt":0.016,"cadence":"display"}))
            .unwrap();
        assert!(matches!(
            worker.recv_timeout(Duration::from_millis(100)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        worker.begin_story(normal).unwrap();
        assert_eq!(
            reply(&worker)["type"],
            "ready",
            "switch did not interrupt the old engine"
        );
        worker.begin_story(busy).unwrap();
        assert_eq!(reply(&worker)["type"], "ready");
        worker
            .try_command(json!({"cmd":"frame","dt":0.016,"cadence":"display"}))
            .unwrap();
        assert!(matches!(
            worker.recv_timeout(Duration::from_millis(100)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        worker.stop();
        assert!(matches!(
            worker.recv_timeout(Duration::from_secs(5)).unwrap(),
            RuntimeEvent::Stopped
        ));
    }
}
