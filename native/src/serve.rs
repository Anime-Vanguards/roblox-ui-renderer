//! The render service: `rur render` without the start-up costs.
//!
//!   rur-native serve --repo <renderer checkout> --port <port> [--idle 300] [--code <stamp>]
//!
//! cli/client.mjs starts it (detached) on first use and talks to it over HTTP on 127.0.0.1. It keeps
//! warm what a one-shot `rur-native render` would start every time: a headless GPU device with the
//! renderer's pipelines, the glyph rasterizer's fonts and bitmaps, the image resolver, and one standby
//! VM per recently used startup configuration (engine + project already loaded, engine/cli.rs
//! [`Preloaded`] on a thread of its own): a render only mounts its story, runs, exports, writes and
//! draws. Taking a standby starts its replacement at once.
//!
//! The last render's run is kept, its story mounted and its clock at the captured moment
//! (engine/cli.rs `Kept`): a render of the same story with the same options at that moment or a
//! later one goes on from it, as a run from the start would show that moment, so a loop of `render
//! --time t` over an animation costs a frame each instead of a mount each. It is let go when a render
//! cannot go on from it (another story or options, an earlier moment, a file it read changed) and
//! with the caches, 10 s after the last render.
//!
//! A standby is used only while the files it read at load are unchanged: the renderer's data/, the
//! project roots' directory structure and the files read at mount (models, meta files), Studio
//! snapshots and the cloud-font index. That fingerprint is taken before it loads and again while the
//! story runs; a difference throws the run away and runs the story in a fresh VM (script sources are
//! read at require time, so their edits never need this: the module sources the process keeps between
//! VMs are checked against the files before each standby run, as the standby's listing predates them).
//!
//!   GET  /health                  { service: "rur-serve", root, stateDir, pid, port, code, executable, renders }
//!   GET  /stories?project=<dir>   the project's stories with their place ({file, place, key, slug})
//!   POST /render                  { argv: engine arguments, out, png, offline, env } -> { result } (story_render.rs)
//!                                 With `pixels: "rgba"` the frame comes back in the answer instead of
//!                                 render.png: one JSON line { result (+ width, height, pixels, summary) },
//!                                 a newline, then width x height straight-alpha RGBA8 rows, the pixels
//!                                 render.png would hold (Content-Type application/x-rur-pixels)
//!   POST /sequence                { argv, out, offline, env, sequence: { from, to | frames | duration,
//!                                 fps, crop, pad, png, scenes, video } } -> { result }: the story captured
//!                                 frame by frame at exact moments (sequence.rs), mounted once
//!   GET  /memory[?collect=1]      where the process's memory is: caches, allocator, committed regions
//!   POST /shutdown
//!
//! Every answer carries root and stateDir (the client uses a service only for its own checkout and
//! state dir). Requests need an `X-RUR: 1` header and a local Host. A render whose engine environment
//! (RUR_CLOCK, RUR_ORDER, RUR_PLAYER... as `env`) differs from the service's is refused with 409 and
//! runs in a process of its own instead. The service exits --idle seconds after its last render: other requests
//! (health checks, ror's keep-alive pings) do not keep it up.
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use serde_json::{Value, json};

use crate::{
    engine::cli::{Args, Preloaded, Resumed, SceneOutput, SequenceMessage, SequenceSpec, run_key, run_once, startup_key},
    story_render::{RenderOptions, StoryRenderer},
};

/// What a standby VM runs: one render, or one sequence (its frames go back as they are captured);
/// after a render whose run it keeps (engine/cli.rs `Kept`), the renders that go on from it
enum StandbyJob {
    /// A render: its output, and the moment the run stands at when the VM keeps it for a later one
    Render(Args, std::sync::mpsc::Sender<Result<(SceneOutput, Option<f64>), String>>),
    Sequence(Args, SequenceSpec, std::sync::mpsc::SyncSender<SequenceMessage>),
    /// A later moment of the kept run's story (true: no file under the project's roots changed since they were last
    /// found unchanged): what the run did, the moment it stands at when it is still kept, and how long its check of
    /// the files it read took
    Resume(Args, bool, std::sync::mpsc::Sender<(Resumed, Option<f64>, Duration)>),
}

/// A [`Preloaded`] project on a thread of its own, waiting for one run; after a render, its run kept
/// for later moments of the story, until the service lets go of it (drops its sender) or a render
/// cannot go on from it.
struct Standby {
    runs: std::sync::mpsc::Sender<StandbyJob>,
}

impl Standby {
    /// Starts loading the engine and project of `args` (their startup part) on a new thread.
    fn start(args: &Args) -> Result<Standby, String> {
        let (runs, receive) = std::sync::mpsc::channel::<StandbyJob>();
        let startup = args.clone();
        thread::Builder::new()
            .name("rur-standby".into())
            // Deep Luau / layout recursion (the main thread's default is larger than a spawned one's)
            .stack_size(64 * 1024 * 1024)
            .spawn(move || {
                let loaded = Preloaded::load(&startup);
                let mut kept = match receive.recv() {
                    Ok(StandbyJob::Render(args, reply)) => match loaded.and_then(|loaded| loaded.run_kept(&args)) {
                        Ok((output, kept)) => {
                            let keep = kept.refusal().is_none();
                            let _ = reply.send(Ok((output, keep.then(|| kept.reached()))));
                            keep.then_some(kept)
                        }
                        Err(error) => {
                            let _ = reply.send(Err(error));
                            None
                        }
                    },
                    Ok(StandbyJob::Sequence(args, spec, frames)) => {
                        match loaded {
                            Ok(loaded) => loaded.sequence(&args, &spec, &mut |message| frames.send(message).is_ok()),
                            Err(error) => {
                                let _ = frames.send(SequenceMessage::End(Err(error)));
                            }
                        }
                        None
                    }
                    Ok(StandbyJob::Resume(_, _, reply)) => {
                        let _ = reply.send((Resumed::Refused("no run kept".into()), None, Duration::ZERO));
                        None
                    }
                    Err(_) => None,
                };
                // The kept run goes on to later moments of its story until a render cannot go on from it
                while let Some(run) = kept.as_mut() {
                    let Ok(StandbyJob::Resume(args, roots_unchanged, reply)) = receive.recv() else { break };
                    let resumed = run.resume(&args, roots_unchanged);
                    let keep = matches!(resumed, Resumed::Ran(Ok(_))) && run.refusal().is_none();
                    let _ = reply.send((resumed, keep.then(|| run.reached()), run.checked()));
                    if !keep {
                        break;
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Standby { runs })
    }

    /// Runs `args` in the preloaded VM, waiting for the load first when it is still in progress; the
    /// standby again, with the moment its run stands at, when it keeps the run for later moments
    fn run(self, args: Args) -> Result<(SceneOutput, Option<(Standby, f64)>), String> {
        let (reply, result) = std::sync::mpsc::channel();
        self.runs.send(StandbyJob::Render(args, reply)).map_err(|_| "the standby VM exited".to_string())?;
        let (output, reached) = result.recv().map_err(|_| "the standby VM exited".to_string())??;
        Ok((output, reached.map(|reached| (self, reached))))
    }

    /// The kept run gone on to `args`' moment (with the moment it stands at when it is still kept, and
    /// how long its check of the files it read took), or why it cannot be (after which its VM is gone)
    fn resume(&self, args: Args, roots_unchanged: bool) -> (Resumed, Option<f64>, Duration) {
        let (reply, result) = std::sync::mpsc::channel();
        if self.runs.send(StandbyJob::Resume(args, roots_unchanged, reply)).is_err() {
            return (Resumed::Refused("the kept VM exited".into()), None, Duration::ZERO);
        }
        result.recv().unwrap_or_else(|_| (Resumed::Refused("the kept VM exited".into()), None, Duration::ZERO))
    }

    /// Starts the sequence of `args` in the preloaded VM (after its load); its frames come back as the
    /// VM captures them
    fn sequence(self, args: Args, spec: SequenceSpec) -> Result<std::sync::mpsc::Receiver<SequenceMessage>, String> {
        let (frames, receive) = crate::sequence::frame_channel();
        self.runs.send(StandbyJob::Sequence(args, spec, frames)).map_err(|_| "the standby VM exited".to_string())?;
        Ok(receive)
    }
}

/// Environment variables the engine reads: a render with other values than the service's own runs
/// elsewhere (cli/client.mjs sends them)
pub const ENGINE_ENV: [&str; 21] = [
    "RUR_CAMERA",
    "RUR_CLOCK",
    "RUR_ENGINE_DUMP",
    "RUR_ENGINE_TIMINGS",
    "RUR_EPOCH",
    "RUR_FONT_ROOT",
    "RUR_HINTED_MAX_SIZE",
    "RUR_LAYOUT_STATS",
    "RUR_LUAU_PROFILE",
    "RUR_NATIVE_ASSET_MB",
    "RUR_ORDER",
    "RUR_PLACE_ID",
    "RUR_PLAYER",
    "RUR_PLAYER_DISPLAY",
    "RUR_PLAYER_ID",
    "RUR_PROFILE_CALLBACKS",
    "RUR_REAL_CLOCK",
    "RUR_ROBLOX_CONTENT",
    "RUR_ROTATION_CLIPPING",
    "RUR_TEXT_SUBPIXEL",
    "RUR_VERBOSE",
];

/// Standby VMs kept (one per startup configuration, least recently used out): each holds an engine with
/// its place and project loaded, a few hundred MB, so one (the last configuration rendered) is kept
const MAX_STANDBYS: usize = 1;

struct Entry {
    key: String,
    args: Args,
    standby: Standby,
    fingerprint: u64,
    /// [`project_changes`] before the VM loaded
    seen: Option<Vec<u64>>,
}

/// The last render's run, kept for a later moment of its story (engine/cli.rs `Kept`): a render of
/// the same story with the same options at that moment or a later one goes on from it instead of
/// mounting the story again
struct KeptRun {
    standby: Standby,
    /// engine/cli.rs `run_key`: a render with other options does not ask the VM
    key: String,
    /// The startup arguments, and the fingerprint of what the VM read at load (taken before it)
    args: Args,
    fingerprint: u64,
    /// The source, for the log and /health
    name: String,
    /// The engine moment the run stands at
    reached: f64,
    /// [`project_changes`] when no file the run read had changed yet (before its VM loaded, or before the
    /// last check that found them all unchanged): while they are the same, the files are not stamped again
    seen: Option<Vec<u64>>,
}

impl KeptRun {
    fn new(standby: Standby, args: &Args, startup: Args, fingerprint: u64, reached: f64, seen: Option<Vec<u64>>) -> KeptRun {
        KeptRun { standby, key: run_key(args), args: startup, fingerprint, name: source_name(args), reached, seen }
    }
}

/// How many changes (watch.rs, any file written, made, removed or renamed) the trees a run's project is mounted
/// from (its roots and mounted directories) have seen; None while one cannot be watched. Equal counts at two
/// moments: no file in them changed in between, so a kept run's 1,500 required files need no stamping again.
fn project_changes(args: &Args) -> Option<Vec<u64>> {
    let prepared = crate::engine::cli::prepare(args).ok()?;
    let mounted = prepared.mounts.iter().filter_map(|mount| match &mount.target {
        crate::engine::project::MountTarget::Path(path) => Some(path.clone()),
        _ => None,
    });
    prepared.roots.iter().cloned().chain(mounted).map(|tree| crate::watch::changes(Path::new(&tree), crate::watch::Scope::Everything)).collect()
}

struct Service {
    repo: PathBuf,
    state_dir: PathBuf,
    port: u16,
    code: String,
    renderer: Mutex<StoryRenderer>,
    standbys: Mutex<Vec<Entry>>,
    kept: Mutex<Option<KeptRun>>,
    /// The last story listing ([`Service::stories`]): the project, its roots' change counts then, the list
    stories: Mutex<Option<(PathBuf, Vec<u64>, Value)>>,
    /// Bumped when cached fonts change: a replacement started before that is not kept
    font_generation: AtomicU64,
    active: AtomicUsize,
    renders: AtomicU64,
    /// When the last render finished (the service's start before the first): other requests are not activity
    last_render: Mutex<Instant>,
    /// serve.log, kept open
    log: Mutex<Option<std::fs::File>>,
    env: BTreeMap<String, String>,
}

fn engine_env() -> BTreeMap<String, String> {
    ENGINE_ENV
        .iter()
        .filter_map(|name| std::env::var(name).ok().filter(|value| !value.is_empty()).map(|value| (name.to_string(), value)))
        .collect()
}

pub fn cli(argv: &[String]) -> i32 {
    let option = |name: &str| argv.iter().position(|arg| arg == name).and_then(|index| argv.get(index + 1)).cloned();
    let repo = option("--repo").map(PathBuf::from).unwrap_or_else(crate::engine::default_repo);
    let repo = std::path::absolute(&repo).unwrap_or(repo);
    let Some(port) = option("--port").and_then(|port| port.parse::<u16>().ok()) else {
        eprintln!("rur-native serve: --port <port> is required");
        return 2;
    };
    let idle = Duration::from_secs_f64(option("--idle").and_then(|value| value.parse().ok()).unwrap_or(IDLE_EXIT.as_secs_f64()));
    let listener = match TcpListener::bind(("127.0.0.1", port)) {
        Ok(listener) => listener,
        // Another service has the port: it serves the request instead
        Err(_) => return 0,
    };
    let state_dir = crate::state_dir(&repo);
    // Renders' rasters and source walks gain nothing past 8 threads; a pool per core only wakes and spins
    if std::env::var_os("RAYON_NUM_THREADS").is_none() {
        let _ = rayon::ThreadPoolBuilder::new().num_threads(thread::available_parallelism().map_or(8, |cores| cores.get().min(8))).build_global();
    }
    // Module sources and bytecode carry over from VM to VM (no native code generation), decoded images
    // from render to render
    crate::engine::lua::modules::reuse_compiled_modules();
    crate::assets::reuse_decoded_images(1 << 30);
    let service = Arc::new(Service::new(repo, state_dir, port, option("--code").unwrap_or_default()));
    service.log(&format!("listening on 127.0.0.1:{port} (pid {})", std::process::id()));
    service.start_gpu(crate::story_render::headless_gpu);
    {
        let service = service.clone();
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let service = service.clone();
                // A fresh VM runs on the connection's thread: deep Luau / layout recursion needs the stack
                let _ = thread::Builder::new().stack_size(64 * 1024 * 1024).spawn(move || {
                    if let Err(error) = service.connection(stream) {
                        service.log(&format!("request failed: {error}"));
                    }
                });
            }
        });
    }
    // The main thread keeps the process tidy between bursts of renders: once none has run for a second
    // it gives back what the last ones left (Service::trim; the heap's abandoned per-request memory
    // can only be collected from the main thread), and it ends the service when idle
    let (mut trimmed, mut released, mut unloaded) = (0, 0, 0);
    loop {
        thread::sleep(Duration::from_millis(500));
        let since_render = service.last_render.lock().unwrap().elapsed();
        let busy = service.active.load(Ordering::Acquire) > 0;
        if !busy && since_render >= idle {
            service.log("exiting: idle");
            std::process::exit(0);
        }
        let renders = service.renders.load(Ordering::Acquire);
        if !busy && renders != trimmed && since_render >= TRIM_AFTER {
            trimmed = renders;
            service.trim(false);
        }
        if !busy && renders != released && since_render >= RELEASE_AFTER {
            released = renders;
            // A kept run is for the renders of a loop, which come back to back: its VM goes with the caches
            if service.kept.lock().unwrap().take().is_some() {
                service.log("kept run let go after a quiet stretch");
            }
            service.trim(true);
        }
        if !busy && renders != unloaded && since_render >= STANDBYS_AFTER {
            unloaded = renders;
            let dropped = std::mem::take(&mut *service.standbys.lock().unwrap());
            if !dropped.is_empty() {
                drop(dropped);
                service.log("standby VMs let go after a quiet stretch");
                service.trim(true);
            }
        }
    }
}

/// How long the service waits for another render before it exits (`--idle`): about what holding it costs in a restart
const IDLE_EXIT: Duration = Duration::from_secs(30);

/// How long the service keeps the caches of its last renders (decoded images, draw caches, textures)
/// before it frees them (StoryRenderer::release): renders that follow each other share them, a render
/// after a pause decodes and uploads again what it draws (Studio snapshots, modules and standby VMs
/// stay warm)
const RELEASE_AFTER: Duration = Duration::from_secs(10);

/// How long a standby VM is kept without renders: past it the next render starts its VM (a few seconds)
/// rather than the service holding one through a long pause
const STANDBYS_AFTER: Duration = Duration::from_secs(120);

/// How long the service waits after a render before it gives back what renders left (Service::trim)
const TRIM_AFTER: Duration = Duration::from_secs(1);

struct Request {
    method: String,
    path: String,
    query: BTreeMap<String, String>,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
}

fn read_request(stream: &mut TcpStream) -> Result<Request, String> {
    let mut reader = BufReader::new(stream.try_clone().map_err(|error| error.to_string())?);
    let mut line = String::new();
    reader.read_line(&mut line).map_err(|error| error.to_string())?;
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let target = parts.next().unwrap_or("/").to_string();
    let mut headers = BTreeMap::new();
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).map_err(|error| error.to_string())? == 0 {
            break;
        }
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }
    let length: usize = headers.get("content-length").and_then(|value| value.parse().ok()).unwrap_or(0);
    if length > 64 * 1024 * 1024 {
        return Err("request body too large".into());
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).map_err(|error| error.to_string())?;
    let (path, query) = target.split_once('?').unwrap_or((&target, ""));
    let query = query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            (url_decode(key), url_decode(value))
        })
        .collect();
    Ok(Request { method, path: path.to_string(), query, headers, body })
}

fn url_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let hex = |byte: u8| (byte as char).to_digit(16);
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 3 <= bytes.len() => match (hex(bytes[index + 1]), hex(bytes[index + 2])) {
                (Some(high), Some(low)) => {
                    out.push((high * 16 + low) as u8);
                    index += 3;
                    continue;
                }
                _ => out.push(b'%'),
            },
            b'+' => out.push(b' '),
            byte => out.push(byte),
        }
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

impl Service {
    /// A service with nothing loaded, its state directory made and serve.log emptied
    fn new(repo: PathBuf, state_dir: PathBuf, port: u16, code: String) -> Service {
        let service = Service {
            renderer: Mutex::new(StoryRenderer::new(repo.clone())),
            state_dir,
            port,
            code,
            standbys: Mutex::new(Vec::new()),
            kept: Mutex::new(None),
            stories: Mutex::new(None),
            font_generation: AtomicU64::new(0),
            active: AtomicUsize::new(0),
            renders: AtomicU64::new(0),
            last_render: Mutex::new(Instant::now()),
            log: Mutex::new(None),
            env: engine_env(),
            repo,
        };
        let _ = std::fs::create_dir_all(&service.state_dir);
        let _ = std::fs::write(service.log_file(), "");
        service
    }

    /// Starts the GPU device and the renderer's pipelines (`start`) before the first PNG needs them, on a thread of
    /// their own: the renderer is held only to note the start, so a render without a PNG never waits for it. Without
    /// a GPU (Windows CI runners) the device takes tens of seconds to start in software or to fail, and every render
    /// waited behind it while the start held the renderer.
    fn start_gpu(self: &Arc<Self>, start: impl FnOnce() -> anyhow::Result<crate::render::gpu::GpuRenderer> + Send + 'static) {
        let service = self.clone();
        self.renderer.lock().unwrap().start_gpu(start, move |error| service.log(&format!("GPU start failed: {error:#}")));
    }

    fn log_file(&self) -> PathBuf {
        self.state_dir.join("serve.log")
    }

    fn log(&self, message: &str) {
        let mut file = self.log.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if file.is_none() {
            *file = std::fs::OpenOptions::new().create(true).append(true).open(self.log_file()).ok();
        }
        if let Some(file) = file.as_mut() {
            let _ = writeln!(file, "{} {message}", chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ"));
        }
    }

    /// What the renders before an idle stretch leave behind, given back: the renderer's frame-sized
    /// buffers (StoryRenderer::trim) and the heap's free pages, those of the request and VM threads that
    /// ended included (collected from the main thread, the only one that can take them back)
    fn trim(&self, release: bool) {
        let before = crate::memtrace::process_bytes();
        if let Ok(mut renderer) = self.renderer.try_lock() {
            if release {
                renderer.release();
            } else {
                renderer.trim();
            }
        }
        // Every thread's own free pages (the raster pool's), then the abandoned ones and the arenas
        rayon::broadcast(|_| allocator::collect());
        allocator::collect();
        let after = crate::memtrace::process_bytes();
        if let (Some((before, _)), Some((after, _))) = (before, after) {
            self.log(&format!("{}: private {} MB -> {} MB", if release { "released caches" } else { "trimmed" }, before >> 20, after >> 20));
        }
    }

    fn send(&self, stream: &mut TcpStream, status: u16, mut body: Value) -> std::io::Result<()> {
        if let Value::Object(map) = &mut body {
            map.insert("root".into(), json!(self.repo));
            map.insert("stateDir".into(), json!(self.state_dir));
        }
        let text = body.to_string();
        let reason = match status {
            200 => "OK",
            403 => "Forbidden",
            404 => "Not Found",
            409 => "Conflict",
            _ => "Error",
        };
        write!(stream, "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}", text.len())?;
        stream.flush()
    }

    /// A render's answer with its pixels: the JSON line, a newline, the RGBA rows
    fn send_pixels(&self, stream: &mut TcpStream, mut result: Value, pixels: crate::story_render::Pixels) -> std::io::Result<()> {
        result["width"] = json!(pixels.width);
        result["height"] = json!(pixels.height);
        result["pixels"] = json!("rgba8");
        let head = json!({"result": result, "root": self.repo, "stateDir": self.state_dir}).to_string();
        let length = head.len() + 1 + pixels.rgba.len();
        write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/x-rur-pixels\r\nCache-Control: no-store\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n{head}\n")?;
        stream.write_all(&pixels.rgba)?;
        stream.flush()
    }

    fn connection(self: &Arc<Self>, mut stream: TcpStream) -> Result<(), String> {
        let request = read_request(&mut stream)?;
        let host = request.headers.get("host").cloned().unwrap_or_default();
        let io = |error: std::io::Error| error.to_string();
        if host != format!("127.0.0.1:{}", self.port) && host != format!("localhost:{}", self.port) {
            return self.send(&mut stream, 403, json!({"error": "forbidden host"})).map_err(io);
        }
        if request.headers.get("x-rur").map(String::as_str) != Some("1") {
            return self.send(&mut stream, 403, json!({"error": "missing X-RUR header"})).map_err(io);
        }
        match (request.method.as_str(), request.path.as_str()) {
            ("GET", "/health") => {
                let body = json!({
                    "service": "rur-serve",
                    "pid": std::process::id(),
                    "port": self.port,
                    "code": self.code,
                    "executable": std::env::current_exe().ok(),
                    "renders": self.renders.load(Ordering::Relaxed),
                    "active": self.active.load(Ordering::Relaxed),
                    "kept": self.kept.lock().unwrap().as_ref().map(|kept| kept.name.clone()),
                });
                self.send(&mut stream, 200, body).map_err(io)
            }
            ("GET", "/memory") => {
                let body = self.memory(request.query.get("collect").is_some_and(|value| value == "1"));
                self.send(&mut stream, 200, body).map_err(io)
            }
            ("GET", "/stories") => {
                // Without ?project= the listing uses the same default project as a render
                let requested = request.query.get("project");
                if let Some(project) = requested {
                    // An empty value is a mistake, not "no project": it would list the cwd
                    if let Err(error) = crate::paths::validate_path_value("project", project) {
                        return self.send(&mut stream, 400, json!({"error": error})).map_err(io);
                    }
                }
                let project = requested.map(PathBuf::from).unwrap_or_else(|| crate::paths::default_project(&self.repo));
                let project = std::path::absolute(&project).unwrap_or(project);
                self.send(&mut stream, 200, json!({"stories": self.stories(&project)})).map_err(io)
            }
            ("POST", "/render") => {
                self.active.fetch_add(1, Ordering::AcqRel);
                let result = serde_json::from_slice::<Value>(&request.body).map_err(|error| error.to_string()).and_then(|input| self.render(&input));
                *self.last_render.lock().unwrap() = Instant::now();
                // The last render running gives back the free memory renders left (this thread's, and
                // the heap's arenas: a frame's readback, rasters and scene)
                if self.active.fetch_sub(1, Ordering::AcqRel) == 1 {
                    allocator::collect();
                }
                match result {
                    Ok(Ok((result, Some(pixels)))) => {
                        self.renders.fetch_add(1, Ordering::Relaxed);
                        self.send_pixels(&mut stream, result, pixels)
                    }
                    Ok(Ok((result, None))) => {
                        self.renders.fetch_add(1, Ordering::Relaxed);
                        self.send(&mut stream, 200, json!({"result": result}))
                    }
                    Ok(Err(mismatch)) => self.send(&mut stream, 409, json!({"error": mismatch, "envMismatch": true})),
                    Err(error) => {
                        self.log(&format!("render failed: {error}"));
                        self.send(&mut stream, 500, json!({"error": error}))
                    }
                }
                .map_err(io)
            }
            ("POST", "/sequence") => {
                self.active.fetch_add(1, Ordering::AcqRel);
                let result = serde_json::from_slice::<Value>(&request.body).map_err(|error| error.to_string()).and_then(|input| self.sequence(&input));
                *self.last_render.lock().unwrap() = Instant::now();
                if self.active.fetch_sub(1, Ordering::AcqRel) == 1 {
                    allocator::collect();
                }
                match result {
                    Ok(Ok(result)) => {
                        self.renders.fetch_add(1, Ordering::Relaxed);
                        self.send(&mut stream, 200, json!({"result": result}))
                    }
                    Ok(Err(mismatch)) => self.send(&mut stream, 409, json!({"error": mismatch, "envMismatch": true})),
                    Err(error) => {
                        self.log(&format!("sequence failed: {error}"));
                        self.send(&mut stream, 500, json!({"error": error}))
                    }
                }
                .map_err(io)
            }
            ("POST", "/shutdown") => {
                self.send(&mut stream, 200, json!({"ok": true})).map_err(io)?;
                self.log("exiting: requested");
                std::process::exit(0);
            }
            _ => self.send(&mut stream, 404, json!({"error": "not found"})).map_err(io),
        }
    }

    /// One render (with its pixels when asked for them); Ok(Err(reason)) when the request's engine
    /// environment is not the service's.
    #[allow(clippy::type_complexity)]
    fn render(self: &Arc<Self>, input: &Value) -> Result<Result<(Value, Option<crate::story_render::Pixels>), String>, String> {
        let env: BTreeMap<String, String> = input["env"]
            .as_object()
            .map(|env| env.iter().filter_map(|(key, value)| Some((key.clone(), value.as_str()?.to_string()))).filter(|(_, value)| !value.is_empty()).collect())
            .unwrap_or_default();
        if env != self.env {
            return Ok(Err(format!("the render's engine environment {env:?} is not the service's {:?}", self.env)));
        }
        let argv: Vec<String> = input["argv"].as_array().ok_or("argv must be a list")?.iter().map(|arg| arg.as_str().unwrap_or_default().to_string()).collect();
        let args = crate::engine::cli::parse(&argv)?;
        let out = PathBuf::from(crate::paths::request_path(input, "out")?);
        let options = RenderOptions {
            png: input["png"].as_bool().unwrap_or(true),
            offline: input["offline"].as_bool().unwrap_or(false),
            placeholders: input["placeholders"].as_bool().unwrap_or(false),
            pixels: match input["pixels"].as_str() {
                None => false,
                Some("rgba") => true,
                Some(other) => return Err(format!("pixels: unknown format {other:?} (rgba)")),
            },
        };
        let started = Instant::now();
        let mut check_ms = None;
        let run_started = Instant::now();
        // The run: the kept run of this story gone on to this moment (no mount), else a standby's VM
        // (its sources checked while it runs), else a fresh one; the run is kept for a later moment
        let (output, keep, how) = match self.resume(&args) {
            Some((output, keep, from, checks)) => (Ok(output), keep, format!("kept run from {from:.4} s, checks {:.1} ms", checks.as_secs_f64() * 1000.0)),
            None => {
                let key = startup_key(&args);
                let taken = {
                    let mut standbys = self.standbys.lock().unwrap();
                    standbys.iter().position(|entry| entry.key == key).map(|index| standbys.remove(index))
                };
                self.replace_standby(&args);
                let how = if taken.is_some() { "standby" } else { "fresh VM" };
                let (output, keep) = match taken {
                    Some(entry) => {
                        let sources = entry.args.clone();
                        let repo = self.repo.clone();
                        let state_dir = self.state_dir.clone();
                        let check = thread::spawn(move || {
                            let started = Instant::now();
                            (fingerprint(&repo, &state_dir, &sources), started.elapsed())
                        });
                        // A standby's project listing (whose stamps the kept module sources are checked against) is
                        // as old as the standby: a module edited since would still match its old stamp, so the
                        // kept copies of changed files go before the run requires them
                        crate::engine::lua::modules::forget_stale_modules();
                        let ran = entry.standby.run(args.clone());
                        let (current, took) = check.join().unwrap_or((0, Duration::ZERO));
                        check_ms = Some(took.as_secs_f64() * 1000.0);
                        if current == entry.fingerprint {
                            match ran {
                                Ok((output, kept)) => (Ok(output), kept.map(|(standby, reached)| KeptRun::new(standby, &args, entry.args, entry.fingerprint, reached, entry.seen))),
                                Err(error) => (Err(error), None),
                            }
                        } else {
                            self.log("sources changed since the standby loaded: running in a fresh VM");
                            (run_once(&args), None)
                        }
                    }
                    None => {
                        // On a thread of its own, as a standby's, so that its run can be kept; what it reads at
                        // load is fingerprinted first
                        let seen = project_changes(&args);
                        let fingerprint = fingerprint(&self.repo, &self.state_dir, &args);
                        match Standby::start(&args).and_then(|standby| standby.run(args.clone())) {
                            Ok((output, kept)) => (Ok(output), kept.map(|(standby, reached)| KeptRun::new(standby, &args, args.clone(), fingerprint, reached, seen))),
                            Err(error) => (Err(error), None),
                        }
                    }
                };
                (output, keep, how.to_string())
            }
        };
        let runtime_ms = run_started.elapsed().as_secs_f64() * 1000.0;
        // Cloud fonts fetched for the scene: the renderer ran the story again in a VM of its own, and no
        // kept run (laid out without them) is gone on from
        let fonts_fetched = std::cell::Cell::new(false);
        let mut renderer = self.renderer.lock().unwrap();
        let generation = &self.font_generation;
        let standbys = &self.standbys;
        let kept = &self.kept;
        let rendered = renderer.render(&args, &out, options, move |_| output, || {
            fonts_fetched.set(true);
            generation.fetch_add(1, Ordering::AcqRel);
            standbys.lock().unwrap().clear();
            kept.lock().unwrap().take();
        });
        drop(renderer);
        let (mut result, pixels) = rendered.map_err(|error| format!("{error:#}"))?;
        if let Some(keep) = keep.filter(|_| !fonts_fetched.get()) {
            *self.kept.lock().unwrap() = Some(keep);
        }
        // The run happened before the renderer's own phases
        if let Some(runtime) = result["phases"]["runtime"].as_f64() {
            result["phases"]["runtime"] = json!(((runtime * 1000.0 + runtime_ms).round()) / 1000.0);
        }
        let check = check_ms.map(|ms| format!(", source check {ms:.0} ms")).unwrap_or_default();
        let mut steps = result["pngSteps"].clone();
        if let Some(steps) = steps.as_object_mut() {
            steps.remove("draw");
        }
        self.log(&format!("render {} ({how}, run {runtime_ms:.0} ms{check}, total {:.0} ms, phases {}, png steps {steps}) -> {}", source_name(&args), started.elapsed().as_secs_f64() * 1000.0, result["phases"], out.display()));
        Ok(Ok((result, pixels)))
    }

    /// [`story_entries`] of `project`, listed again only once an entry under its roots was made, removed
    /// or renamed (watch.rs counts those; the counts are read before the listing, so a change during it
    /// lists again next time). Every render of a story by name asked for it: in a project of a few
    /// thousand directories, 70-100 ms.
    fn stories(&self, project: &Path) -> Value {
        let counts: Option<Vec<u64>> = crate::places::roots(project).iter().map(|(root, _)| crate::watch::changes(root, crate::watch::Scope::Structure)).collect();
        if let (Some(counts), Some((listed, then, stories))) = (&counts, self.stories.lock().unwrap().as_ref())
            && listed == project
            && then == counts
        {
            return stories.clone();
        }
        let stories = Value::Array(story_entries(project));
        if let Some(counts) = counts {
            *self.stories.lock().unwrap() = Some((project.to_path_buf(), counts, stories.clone()));
        }
        stories
    }

    /// The kept run gone on to `args`' moment: its output, the run again while it is still kept, and the
    /// moment it went on from. None when no kept run can be (another story or options, an earlier
    /// moment, changed files: the run is let go, and the render runs as without one).
    fn resume(&self, args: &Args) -> Option<(SceneOutput, Option<KeptRun>, f64, Duration)> {
        let kept = self.kept.lock().unwrap().take()?;
        if kept.key != run_key(args) {
            return None;
        }
        let checking = Instant::now();
        if fingerprint(&self.repo, &self.state_dir, &kept.args) != kept.fingerprint {
            self.log(&format!("kept run of {} let go: the project changed", kept.name));
            return None;
        }
        // Counted before the check: a change after it counts for the next render
        let seen = project_changes(&kept.args);
        let roots_unchanged = seen.is_some() && seen == kept.seen;
        if !roots_unchanged {
            // A module edited since it was kept is read again when the run first requires it
            crate::engine::lua::modules::forget_stale_modules();
        }
        let checks = checking.elapsed();
        let from = kept.reached;
        match kept.standby.resume(args.clone(), roots_unchanged) {
            (Resumed::Ran(Ok(output)), reached, checked) => Some((output, reached.map(|reached| KeptRun { reached, seen, ..kept }), from, checks + checked)),
            (Resumed::Ran(Err(error)), _, _) => {
                self.log(&format!("kept run of {} failed going on from {from:.4} s ({}); running from the start", kept.name, error.lines().next().unwrap_or_default()));
                None
            }
            (Resumed::Refused(why), _, _) => {
                self.log(&format!("kept run of {} not gone on from {from:.4} s: {why}", kept.name));
                None
            }
        }
    }

    /// One sequence (sequence.rs) into `out`: the story mounted once in a standby VM of its startup
    /// configuration (one whose sources are unchanged; else a fresh VM), captured frame by frame and
    /// drawn with the service's warm GPU device and caches. Ok(Err(reason)) when the request's engine
    /// environment is not the service's.
    fn sequence(self: &Arc<Self>, input: &Value) -> Result<Result<Value, String>, String> {
        let env: BTreeMap<String, String> = input["env"]
            .as_object()
            .map(|env| env.iter().filter_map(|(key, value)| Some((key.clone(), value.as_str()?.to_string()))).filter(|(_, value)| !value.is_empty()).collect())
            .unwrap_or_default();
        if env != self.env {
            return Ok(Err(format!("the sequence's engine environment {env:?} is not the service's {:?}", self.env)));
        }
        let argv: Vec<String> = input["argv"].as_array().ok_or("argv must be a list")?.iter().map(|arg| arg.as_str().unwrap_or_default().to_string()).collect();
        let args = crate::engine::cli::parse(&argv)?;
        let out = PathBuf::from(crate::paths::request_path(input, "out")?);
        let offline = input["offline"].as_bool().unwrap_or(false);
        let (spec, options) = crate::sequence::parse_options(&input["sequence"], offline)?;
        let started = Instant::now();
        let key = startup_key(&args);
        let mut taken = {
            let mut standbys = self.standbys.lock().unwrap();
            standbys.iter().position(|entry| entry.key == key).map(|index| standbys.remove(index))
        };
        // A standby whose project changed since it loaded would run the story against old files
        if taken.as_ref().is_some_and(|entry| fingerprint(&self.repo, &self.state_dir, &entry.args) != entry.fingerprint) {
            self.log("sources changed since the standby loaded: running the sequence in a fresh VM");
            taken = None;
        }
        self.replace_standby(&args);
        let warm = taken.is_some();
        let mut standby = taken.map(|entry| entry.standby);
        crate::engine::lua::modules::forget_stale_modules();
        let mut start = |args: &Args| match standby.take() {
            Some(standby) => standby.sequence(args.clone(), spec.clone()),
            None => crate::sequence::start_fresh(args, &spec),
        };
        let mut renderer = self.renderer.lock().unwrap();
        let generation = &self.font_generation;
        let standbys = &self.standbys;
        let kept = &self.kept;
        let result = renderer
            .sequence(&args, &out, &options, &mut start, || {
                generation.fetch_add(1, Ordering::AcqRel);
                standbys.lock().unwrap().clear();
                kept.lock().unwrap().take();
            })
            .map_err(|error| format!("{error:#}"))?;
        drop(renderer);
        self.log(&format!(
            "sequence {} ({}, {} frames at {} fps, total {:.0} ms, phases {}, per frame {}) -> {}",
            source_name(&args),
            if warm { "standby" } else { "fresh VM" },
            result["frames"],
            spec.fps,
            started.elapsed().as_secs_f64() * 1000.0,
            result["phases"],
            result["perFrame"],
            out.display()
        ));
        Ok(Ok(result))
    }

    /// Where the process's memory is: its private bytes and working set, the allocator's own view, the
    /// renderer's caches, the decoded images and the standby VMs (`collect`: the allocator returns what
    /// it can first, and the report says how much that was)
    fn memory(&self, collect: bool) -> Value {
        let process = |bytes: Option<(u64, u64)>| bytes.map(|(private, working)| json!({"private": private, "workingSet": working}));
        let before = crate::memtrace::process_bytes();
        if collect {
            allocator::collect();
        }
        let (decoded, decoded_entries) = crate::assets::decoded_image_memory();
        json!({
            "process": process(crate::memtrace::process_bytes()),
            "beforeCollect": if collect { process(before) } else { None },
            "allocator": allocator::stats(),
            "regions": regions::summary(),
            "heapByPhase": crate::memtrace::live_json(),
            "decodedImages": {"bytes": decoded, "entries": decoded_entries},
            "standbys": self.standbys.lock().unwrap().len(),
            "kept": self.kept.lock().unwrap().is_some(),
            "renderer": self.renderer.try_lock().map(|renderer| renderer.memory()).unwrap_or(Value::Null),
        })
    }

    /// Starts the standby that replaces a taken one (or the first for a new configuration): its
    /// fingerprint first, then the load, on a thread of its own.
    fn replace_standby(self: &Arc<Self>, args: &Args) {
        let service = self.clone();
        let args = args.clone();
        thread::spawn(move || {
            let key = startup_key(&args);
            let generation = service.font_generation.load(Ordering::Acquire);
            let seen = project_changes(&args);
            let fingerprint = fingerprint(&service.repo, &service.state_dir, &args);
            let Ok(standby) = Standby::start(&args) else { return };
            let mut standbys = service.standbys.lock().unwrap();
            if service.font_generation.load(Ordering::Acquire) != generation || standbys.iter().any(|entry| entry.key == key) {
                return;
            }
            standbys.push(Entry { key, args, standby, fingerprint, seen });
            if standbys.len() > MAX_STANDBYS {
                standbys.remove(0);
            }
        });
    }
}

fn source_name(args: &Args) -> String {
    [&args.story, &args.module, &args.fixture, &args.model]
        .into_iter()
        .flatten()
        .next()
        .map(|file| Path::new(file).file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default())
        .unwrap_or_default()
}

/// What a preloaded VM has read (see the module comment): a hash of paths, sizes and modification
/// times. A tree the service watches (watch.rs) is not listed again: its count of the changes that
/// matter stands for its listing (scripts, read at require, are not among them in project roots).
pub fn fingerprint(repo: &Path, state_dir: &Path, args: &Args) -> u64 {
    use rayon::prelude::*;
    use std::hash::{Hash, Hasher};
    fn stamp(path: &Path, metadata: &std::fs::Metadata) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        path.hash(&mut hasher);
        metadata.len().hash(&mut hasher);
        metadata.modified().ok().hash(&mut hasher);
        hasher.finish()
    }
    /// Read at project mount: models and meta files (scripts and JSON modules are read at require)
    fn mount_read(name: &str) -> bool {
        crate::watch::mount_read(name)
    }
    // Order-independent, so directories are visited in parallel
    fn walk(path: &Path, files: bool) -> u64 {
        let Ok(entries) = std::fs::read_dir(path) else {
            return 0;
        };
        let entries: Vec<_> = entries.flatten().collect();
        entries
            .par_iter()
            .map(|entry| {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                let path = entry.path();
                // The directory listing's own data (no extra system call per entry on Windows); a
                // symbolic link is followed
                let Ok(mut metadata) = entry.metadata() else {
                    return 0;
                };
                if metadata.file_type().is_symlink() {
                    let Ok(target) = std::fs::metadata(&path) else { return 0 };
                    metadata = target;
                }
                if metadata.is_dir() {
                    if name.starts_with('.') || name == "node_modules" {
                        return 0;
                    }
                    stamp(&path, &metadata).rotate_left(1) ^ walk(&path, files)
                } else if files || mount_read(&name) {
                    stamp(&path, &metadata)
                } else {
                    0
                }
            })
            .reduce(|| 0, |a, b| a ^ b)
    }
    let mut sources: Vec<(PathBuf, bool)> = vec![(repo.join("data"), true), (state_dir.join("assets").join("fonts.json"), true)];
    if let Ok(prepared) = crate::engine::cli::prepare(args) {
        sources.extend(prepared.roots.iter().map(|root| (PathBuf::from(root), false)));
        if let Some(dir) = &prepared.snapshot_dir {
            sources.push((PathBuf::from(dir), true));
        }
        for mount in &prepared.mounts {
            if let crate::engine::project::MountTarget::Path(path) = &mount.target {
                sources.push((PathBuf::from(path), false));
            }
        }
    }
    for snapshot in &args.snapshots {
        sources.push((PathBuf::from(snapshot), true));
    }
    sources
        .iter()
        .map(|(path, files)| {
            let root = std::fs::metadata(path).map_or(0, |metadata| stamp(path, &metadata));
            let scope = if *files { crate::watch::Scope::Everything } else { crate::watch::Scope::Structure };
            let tree = match crate::watch::changes(path, scope) {
                Some(changes) => {
                    let mut hasher = std::collections::hash_map::DefaultHasher::new();
                    (path, files, changes).hash(&mut hasher);
                    hasher.finish()
                }
                None => walk(path, *files),
            };
            root ^ tree.rotate_left(7)
        })
        .fold(0, |a, b| a ^ b)
}

/// cli/rur.mjs storyEntries for `--place all`: every story under the roots of each of the project's
/// places (src/places.rs); a story under a root two places share belongs to the first.
pub fn story_entries(project: &Path) -> Vec<Value> {
    use rayon::prelude::*;
    // Every story file under `dir`, subdirectories visited in parallel
    fn walk(dir: &Path) -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
        let entries: Vec<_> = entries.flatten().collect();
        entries
            .par_iter()
            .flat_map_iter(|entry| {
                let path = entry.path();
                match entry.file_type() {
                    Ok(kind) if kind.is_dir() => walk(&path),
                    Ok(_) => {
                        let name = entry.file_name();
                        let name = name.to_string_lossy();
                        if name.ends_with(".story.luau") || name.ends_with(".story.lua") { vec![path] } else { Vec::new() }
                    }
                    Err(_) => Vec::new(),
                }
            })
            .collect()
    }
    let roots: Vec<(PathBuf, Vec<PathBuf>)> = crate::places::roots(project).into_par_iter().map(|(root, _)| {
        let files = walk(&root);
        (root, files)
    }).collect();
    let files = |dir: &Path| roots.iter().find(|(root, _)| root == dir).map(|(_, files)| files.clone()).unwrap_or_default();
    let mut places: Vec<(PathBuf, String)> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for place in crate::places::places(project) {
        let mut listed: Vec<PathBuf> = place.roots.iter().flat_map(|root| files(root)).collect();
        listed.sort();
        for file in listed {
            if seen.insert(file.clone()) {
                places.push((file, place.name.clone()));
            }
        }
    }
    let mut entries: Vec<(String, Value)> = places
        .into_iter()
        .map(|(file, place)| {
            let relative = file.strip_prefix(project).ok().map(|relative| relative.to_string_lossy().replace('\\', "/"));
            let key = relative.unwrap_or_else(|| file.to_string_lossy().replace('\\', "/"));
            let slug = story_slug(&key);
            (key.clone(), json!({"file": file, "place": place, "key": key, "slug": slug}))
        })
        .collect();
    entries.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()).then(a.0.cmp(&b.0)));
    entries.into_iter().map(|(_, entry)| entry).collect()
}

/// cli/rur.mjs storyEntries' output directory name of a story key
fn story_slug(key: &str) -> String {
    let mut slug = key.to_string();
    if slug.len() > 1 && slug.as_bytes()[1] == b':' && slug.as_bytes()[0].is_ascii_alphabetic() {
        slug = slug[2..].to_string();
    }
    for suffix in [".story.luau", ".story.lua"] {
        if let Some(stripped) = slug.strip_suffix(suffix) {
            slug = stripped.to_string();
            break;
        }
    }
    slug.split('/').filter(|part| !part.is_empty()).collect::<Vec<_>>().join(".")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn story_slugs_follow_the_cli() {
        assert_eq!(story_slug("src/ReplicatedStorage/Stories/Button.story.luau"), "src.ReplicatedStorage.Stories.Button");
        assert_eq!(story_slug("C:/a/b/c.story.lua"), "a.b.c");
        assert_eq!(url_decode("C%3A%5Cx+y"), "C:\\x y");
    }

    /// Without a GPU the device takes tens of seconds to start in software or to fail (Windows CI runners). The
    /// service starts it on a thread of its own: a render without a PNG goes on while it starts, and the first PNG
    /// waits for that start, reporting its error, instead of starting another device.
    #[test]
    fn renders_without_png_do_not_wait_for_the_gpu_start() {
        let repo = crate::engine::default_repo();
        let dir = std::env::temp_dir().join(format!("rur-serve-gpu-start-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let fixture = dir.join("frame.luau");
        std::fs::write(&fixture, "return function(Target) local f = Instance.new('Frame'); f.Name = 'Probe'; f.Parent = Target end").unwrap();
        let service = Arc::new(Service::new(repo.clone(), dir.join("state"), 0, String::new()));

        // A start that lasts until the test lets it go, then fails as one without a GPU does
        let (release, held) = std::sync::mpsc::channel::<()>();
        let starts = Arc::new(AtomicUsize::new(0));
        let counted = starts.clone();
        service.start_gpu(move || {
            counted.fetch_add(1, Ordering::AcqRel);
            let _ = held.recv();
            Err(anyhow::anyhow!("test GPU start failed"))
        });

        let render = |name: &str, png: bool| {
            let input = json!({
                "argv": ["--roots", repo.join("tools/tests/oracle-root"), "--fixture", fixture, "--viewport", "64x64", "--time", "0", "--no-snapshots"],
                "out": dir.join(name), "png": png, "offline": true, "env": service.env,
            });
            let (answer, answered) = std::sync::mpsc::channel();
            let service = service.clone();
            thread::spawn(move || answer.send(service.render(&input)));
            answered
        };
        let answer = render("no-png", false).recv_timeout(Duration::from_secs(120)).expect("a render without PNG waited for the GPU start");
        let (result, pixels) = answer.unwrap().unwrap();
        assert_eq!(result["status"], "ok", "{result}");
        assert!(pixels.is_none() && result["renderErrors"] == json!([]), "{result}");
        assert!(dir.join("no-png/scene.json").is_file());

        release.send(()).unwrap();
        let (result, _) = render("png", true).recv_timeout(Duration::from_secs(120)).unwrap().unwrap().unwrap();
        assert!(result["renderErrors"].to_string().contains("test GPU start failed"), "{result}");
        assert_eq!(starts.load(Ordering::Acquire), 1);
        let log = std::fs::read_to_string(dir.join("state/serve.log")).unwrap();
        assert!(log.contains("GPU start failed: test GPU start failed"), "{log}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// mimalloc's own counters and its collection (the rur-native binary's allocator, src/main.rs)
mod allocator {
    use libmimalloc_sys as mi;
    use serde_json::{Value, json};

    /// Frees this thread's empty pages and purges the heap's free arena memory now; on the main thread
    /// also takes back what ended threads left (their abandoned pages)
    pub fn collect() {
        unsafe { mi::mi_collect(true) }
    }

    unsafe extern "C" fn append(message: *const std::ffi::c_char, arg: *mut std::ffi::c_void) {
        let text = unsafe { std::ffi::CStr::from_ptr(message) }.to_string_lossy();
        unsafe { (*(arg as *mut String)).push_str(&text) };
    }

    /// The allocator's own reserved / committed / abandoned lines (mi_stats_print_out)
    fn own() -> Vec<String> {
        let mut text = String::new();
        unsafe { mi::mi_stats_print_out(Some(append), &mut text as *mut String as *mut std::ffi::c_void) };
        text.lines().filter(|line| ["reserved", "committed", "pages", "abandoned", "purged", "threads"].iter().any(|key| line.trim_start().starts_with(key))).map(|line| line.split_whitespace().collect::<Vec<_>>().join(" ")).collect()
    }

    pub fn stats() -> Value {
        let mut values = [0usize; 8];
        let [a, b, c, d, e, f, g, h] = &mut values;
        unsafe { mi::mi_process_info(a, b, c, d, e, f, g, h) };
        json!({"rss": values[3], "peakRss": values[4], "commit": values[5], "peakCommit": values[6], "pageFaults": values[7], "own": own()})
    }
}

/// Committed private memory by allocation (VirtualQuery over the address space): the heap's arenas,
/// the GPU driver's own allocations, thread stacks
#[cfg(windows)]
mod regions {
    use serde_json::{Value, json};
    use std::ffi::c_void;

    #[repr(C)]
    struct Info {
        base: *mut c_void,
        allocation_base: *mut c_void,
        allocation_protect: u32,
        partition: u16,
        region_size: usize,
        state: u32,
        protect: u32,
        kind: u32,
    }

    unsafe extern "system" {
        fn VirtualQuery(address: *const c_void, buffer: *mut Info, length: usize) -> usize;
    }

    pub fn summary() -> Value {
        const COMMIT: u32 = 0x1000;
        const PRIVATE: u32 = 0x20000;
        let mut allocations: std::collections::BTreeMap<usize, (usize, usize)> = Default::default();
        let mut address = 0usize;
        loop {
            let mut info: Info = unsafe { std::mem::zeroed() };
            if unsafe { VirtualQuery(address as *const c_void, &mut info, std::mem::size_of::<Info>()) } == 0 {
                break;
            }
            let entry = allocations.entry(info.allocation_base as usize).or_default();
            if info.state != 0x10000 {
                entry.1 += info.region_size;
            }
            if info.state == COMMIT && info.kind == PRIVATE {
                entry.0 += info.region_size;
            }
            address = info.base as usize + info.region_size;
            if address == 0 || info.region_size == 0 {
                break;
            }
        }
        let mut list: Vec<(usize, usize, usize)> = allocations.into_iter().filter(|(_, (committed, _))| *committed > 0).map(|(base, (committed, reserved))| (base, committed, reserved)).collect();
        list.sort_by(|a, b| b.1.cmp(&a.1));
        let total: usize = list.iter().map(|entry| entry.1).sum();
        let mut buckets = [0usize; 5];
        let mut counts = [0usize; 5];
        for (_, committed, reserved) in &list {
            let bucket = match *reserved {
                r if r < 1 << 20 => 0,
                r if r < 32 << 20 => 1,
                r if r < 256 << 20 => 2,
                r if r < 1 << 30 => 3,
                _ => 4,
            };
            buckets[bucket] += committed;
            counts[bucket] += 1;
        }
        json!({
            "committedPrivate": total,
            "allocations": list.len(),
            "byReservedSize": {"<1MB": [counts[0], buckets[0]], "1-32MB": [counts[1], buckets[1]], "32-256MB": [counts[2], buckets[2]], "256MB-1GB": [counts[3], buckets[3]], ">=1GB": [counts[4], buckets[4]]},
            "largest": list.iter().take(20).map(|(base, committed, reserved)| json!([format!("{base:#x}"), committed, reserved])).collect::<Vec<_>>(),
        })
    }
}

#[cfg(not(windows))]
mod regions {
    pub fn summary() -> serde_json::Value {
        serde_json::Value::Null
    }
}
