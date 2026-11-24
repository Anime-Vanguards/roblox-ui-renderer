//! ViewportFrames' 3D pictures from a worker process (docs/viewports.md). A ViewportFrame's contents
//! leave the engine as a content-addressed snapshot (`engine/export/viewport.rs`, the image URI
//! `rur-viewport://<slot>/<digest>`); this module starts the worker, asks it for each snapshot's
//! picture and hands back the PNG, which then takes the ordinary image path. The worker is
//! roblox-object-renderer's `ror viewport-worker`; it keeps the GPU and its caches warm between
//! requests and exits when its stdin closes.
//!
//! The same worker answers the model bundle requests of a project that mounts bundles lazily
//! (`catalog`, `bundle`; engine/mod.rs).
use std::{
    collections::{HashMap, HashSet},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{Arc, Mutex, OnceLock, mpsc},
    time::Duration,
};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// How long one request may take before the worker is stopped (a first render loads the GPU and
/// downloads meshes and textures)
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(90);

/// One-shot renders keep the newest pictures under their snapshot's key, so a new process (a
/// restarted render service) does not start the worker for a picture it drew before.
const PERSISTED: usize = 512;

/// How the message for a missing worker starts
const NOT_FOUND: &str = "ViewportFrames are drawn by ror (roblox-object-renderer 0.5.0 or later), which was not found";

/// The command that installs ror with rokit (ror's public repository coordinate)
const ROKIT_INSTALL: &str =
    "rokit add --global Anime-Vanguards/roblox-object-renderer ror";

/// What ror's releases page is, for the message that says how to get it
const ROR_RELEASES: &str = "https://github.com/Anime-Vanguards/roblox-object-renderer/releases";

/// The worker executable: `RUR_VIEWPORT_WORKER`, else ror built in a checkout beside this one (a
/// checkout build), else `ror` on the PATH.
pub fn executable(repo: &Path) -> Result<PathBuf, String> {
    find_worker(repo, std::env::var_os("RUR_VIEWPORT_WORKER").map(PathBuf::from), std::env::var_os("PATH"))
}


/// [`executable`] with its inputs given: the variable's value (a path, or a program's name looked up on
/// the PATH), the checkout (none in a release build) and the PATH
fn find_worker(repo: &Path, named: Option<PathBuf>, search: Option<std::ffi::OsString>) -> Result<PathBuf, String> {
    let on_path = |name: &Path| {
        let names = if cfg!(windows) && name.extension().is_none() { vec![name.with_extension("exe"), name.to_path_buf()] } else { vec![name.to_path_buf()] };
        search.iter().flat_map(std::env::split_paths).flat_map(|folder| names.iter().map(move |name| folder.join(name))).find(|file| file.is_file())
    };
    if let Some(file) = named.filter(|file| !file.as_os_str().is_empty()) {
        let bare = file.components().count() == 1 && !file.is_file();
        return Ok(match bare.then(|| on_path(&file)).flatten() {
            Some(found) => found,
            None => std::path::absolute(&file).unwrap_or(file),
        });
    }
    let name = if cfg!(windows) { "ror.exe" } else { "ror" };
    if !crate::paths::is_standalone() {
        let sibling = repo.join("..").join("roblox-object-renderer").join("target").join("release").join(name);
        if sibling.is_file() {
            return Ok(sibling);
        }
    }
    if let Some(found) = on_path(Path::new(name)) {
        return Ok(found);
    }
    Err(format!(
        "{NOT_FOUND}: install it with `{ROKIT_INSTALL}` or from {ROR_RELEASES}, or set RUR_VIEWPORT_WORKER to its executable"
    ))
}

/// Whether `error` means no worker can be had (none found, it can't start, or it exits without
/// answering), so the other frames of a scene would fail the same way
pub fn is_unavailable(error: &str) -> bool {
    error.starts_with(NOT_FOUND) || error.starts_with("starting the viewport worker") || error.contains("exited without answering")
}

/// A `rur-viewport://<slot>/<digest>` URI's slot and snapshot digest
pub fn parse_uri(uri: &str) -> Option<(u64, &str)> {
    let (slot, digest) = uri.strip_prefix("rur-viewport://")?.split_once('/')?;
    let slot = slot.parse().ok()?;
    (digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())).then_some((slot, digest))
}

/// The running worker: its stdin, and its replies as lines from a reader thread
struct Running {
    child: Child,
    input: ChildStdin,
    replies: mpsc::Receiver<String>,
    executable: PathBuf,
}

/// The worker of one process, started on the first request
pub struct ViewportWorker {
    repo: PathBuf,
    state: PathBuf,
    offline: bool,
    /// The program to start instead of the found executable (tests)
    command: Option<(PathBuf, Vec<String>)>,
    running: Option<Running>,
    /// The snapshot each slot's file holds
    snapshots: HashMap<u64, String>,
    /// This worker's folder of live pictures, one file per slot
    directory: Option<PathBuf>,
    /// The worker build's identity, keying persisted pictures
    stamp: Option<String>,
    /// How long one request may take
    timeout: Duration,
    /// The worker's warnings since they were last taken (what it drew but not as Studio would)
    warnings: Vec<String>,
}

/// Stand-in workers for tests, by the state dir they serve: `ViewportWorker::new` for that dir starts
/// the stand-in instead of the found executable
#[cfg(test)]
pub(crate) static STAND_INS: std::sync::Mutex<Vec<(PathBuf, PathBuf, Vec<String>)>> = std::sync::Mutex::new(Vec::new());

type ModelWorkers = Mutex<HashMap<(PathBuf, bool), Arc<Mutex<ViewportWorker>>>>;
static MODEL_WORKERS: OnceLock<ModelWorkers> = OnceLock::new();

/// Process-wide model-bundle worker keyed by `(state_dir, offline)`: every engine in the process
/// shares one worker instead of starting one child process per VM.
pub fn shared_model_worker(repo: &Path, offline: bool) -> Arc<Mutex<ViewportWorker>> {
    let key = (crate::state_dir(repo), offline);
    let mut map = MODEL_WORKERS.get_or_init(Default::default).lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    map.entry(key)
        .or_insert_with(|| Arc::new(Mutex::new(ViewportWorker::new(repo, offline))))
        .clone()
}

/// Drops idle process-wide model workers so the service frees their memory between bursts.
pub fn release_shared_workers() {
    if let Some(workers) = MODEL_WORKERS.get() {
        let drained: Vec<Arc<Mutex<ViewportWorker>>> = {
            let mut map = workers.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            map.drain().map(|(_, worker)| worker).collect()
        };
        drop(drained);
    }
}

impl ViewportWorker {
    pub fn new(repo: &Path, offline: bool) -> Self {
        let state = crate::state_dir(repo);
        #[cfg(test)]
        let command = STAND_INS.lock().unwrap().iter().find(|(dir, ..)| *dir == state).map(|(_, program, args)| (program.clone(), args.clone()));
        #[cfg(not(test))]
        let command = None;
        Self { repo: repo.to_path_buf(), state, offline, command, running: None, snapshots: HashMap::new(), directory: None, stamp: None, timeout: REQUEST_TIMEOUT, warnings: Vec::new() }
    }

    /// How long one request may take before the worker is stopped
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }

    /// The warnings of the pictures drawn since the last call
    pub fn take_warnings(&mut self) -> Vec<String> {
        std::mem::take(&mut self.warnings)
    }

    /// A worker started as `program args...` (tests)
    #[cfg(test)]
    pub(crate) fn with_command(state: &Path, program: PathBuf, args: Vec<String>) -> Self {
        Self { repo: state.to_path_buf(), state: state.to_path_buf(), offline: false, command: Some((program, args)), running: None, snapshots: HashMap::new(), directory: None, stamp: None, timeout: REQUEST_TIMEOUT, warnings: Vec::new() }
    }

    pub fn set_offline(&mut self, offline: bool) {
        if self.offline != offline {
            self.stop();
        }
        self.offline = offline;
    }

    /// The picture of `snapshot` (the `viewport` of the node whose image is `uri`). With `persist`
    /// (one-shot renders) the picture is also kept under the snapshot's key for later processes.
    pub fn render(&mut self, uri: &str, snapshot: &Value, persist: bool) -> Result<PathBuf, String> {
        let (slot, digest) = parse_uri(uri).ok_or("Missing ViewportFrame snapshot")?;
        if !snapshot.is_object() {
            return Err("Missing ViewportFrame snapshot".into());
        }
        let persisted = if persist {
            let stamp = self.stamp()?;
            let file = self.state.join("viewports-persisted").join(format!("{}.png", hex(&Sha256::digest(format!("{digest}\0{stamp}").as_bytes()))));
            if file.is_file() {
                // the newest are kept: touching it counts as using it
                let _ = std::fs::File::options().write(true).open(&file).and_then(|handle| handle.set_modified(std::time::SystemTime::now()));
                return Ok(file);
            }
            Some(file)
        } else {
            None
        };
        let directory = self.directory()?;
        let file = directory.join(format!("viewport-{slot}.png"));
        if self.snapshots.get(&slot).map(String::as_str) != Some(digest) || !file.is_file() {
            let reply = self.request(json!({"viewport": snapshot, "out": file}))?;
            // The frame's fill is part of its picture (docs/viewports.md): a worker that drew a transparent
            // picture (an older one) has it laid over the fill here
            if let Some(fill) = snapshot.get("background").filter(|fill| fill.is_object()) {
                if reply["background"] != true {
                    lay_over_fill(&file, fill)?;
                }
            }
            self.warnings.extend(reply["warnings"].as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_owned));
            self.snapshots.insert(slot, digest.to_owned());
        }
        let Some(persisted) = persisted else {
            return Ok(file);
        };
        // copied beside its name and renamed, so a reader never sees half of it
        let folder = persisted.parent().unwrap();
        std::fs::create_dir_all(folder).map_err(|error| error.to_string())?;
        let partial = persisted.with_extension(format!("{}.tmp", std::process::id()));
        std::fs::copy(&file, &partial).and_then(|_| std::fs::rename(&partial, &persisted)).map_err(|error| {
            let _ = std::fs::remove_file(&partial);
            format!("keeping the ViewportFrame picture: {error}")
        })?;
        Ok(persisted)
    }

    /// Prunes persisted pictures down to the cache limit without removing any picture in `protect`
    /// (the pictures resolved for the current render batch).
    pub fn prune_persisted(&self, protect: &HashSet<PathBuf>) {
        prune(&self.state.join("viewports-persisted"), PERSISTED, protect);
    }

    /// Forgets the pictures of slots no longer shown, so the folder holds only what the scene draws
    pub fn retain(&mut self, slots: &[u64]) {
        let Some(directory) = &self.directory else {
            return;
        };
        self.snapshots.retain(|slot, _| {
            let keep = slots.contains(slot);
            if !keep {
                let _ = std::fs::remove_file(directory.join(format!("viewport-{slot}.png")));
            }
            keep
        });
    }

    /// A model bundle request: `{"command": "catalog", "av": <project>}` answers the bundle keys,
    /// `{"command": "bundle", "av": <project>, "path": <key>}` the file the bundle is written to
    pub fn model(&mut self, request: Value) -> Result<Value, String> {
        if request["command"] == "catalog" {
            return self.request(request);
        }
        let mut request = request;
        if request["out"].as_str().is_none() {
            let folder = self.state.join("model-bundles");
            std::fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
            // Named by the project and the key; the worker replaces it atomically, since a render may have
            // hard-linked an earlier one into its output
            let name = hex(&Sha256::digest(serde_json::to_string(&json!([request["av"], request["path"]])).unwrap_or_default().as_bytes()));
            request["out"] = json!(folder.join(format!("{name}.rbxm")));
        } else if let Some(parent) = request["out"].as_str().and_then(|out| Path::new(out).parent()) {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        self.request(request)
    }

    fn directory(&mut self) -> Result<PathBuf, String> {
        if let Some(directory) = &self.directory {
            return Ok(directory.clone());
        }
        let base = self.state.join("viewports-live");
        std::fs::create_dir_all(&base).map_err(|error| error.to_string())?;
        let unique = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|time| time.as_nanos()).unwrap_or_default();
        let directory = base.join(format!("worker-{}-{unique}", std::process::id()));
        std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        self.directory = Some(directory.clone());
        Ok(directory)
    }

    /// The executable's path, size and modification time: a rebuilt or updated worker draws anew
    fn stamp(&mut self) -> Result<String, String> {
        if let Some(stamp) = &self.stamp {
            return Ok(stamp.clone());
        }
        let file = match &self.command {
            Some((program, args)) => return Ok(self.stamp.insert(format!("{program:?} {args:?}")).clone()),
            None => executable(&self.repo)?,
        };
        let metadata = std::fs::metadata(&file).map_err(|error| format!("{}: {error}", file.display()))?;
        let modified = metadata.modified().ok().and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok()).map(|time| time.as_nanos()).unwrap_or_default();
        let stamp = format!("{}\0{}\0{modified}", file.display(), metadata.len());
        self.stamp = Some(stamp.clone());
        Ok(stamp)
    }

    fn start(&mut self) -> Result<&mut Running, String> {
        if let Some(running) = &mut self.running {
            if matches!(running.child.try_wait(), Ok(None)) {
                return Ok(self.running.as_mut().unwrap());
            }
            self.running = None;
        }
        let (program, mut args) = match &self.command {
            Some((program, args)) => (program.clone(), args.clone()),
            None => (executable(&self.repo)?, vec!["viewport-worker".to_owned()]),
        };
        if self.offline {
            args.push("--offline".into());
        }
        let mut command = Command::new(&program);
        command.args(&args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
        // It downloads meshes and textures with an account of its own, never with rur's API key
        crate::download::without_api_key(&mut command);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        let mut child = command.spawn().map_err(|error| format!("starting the viewport worker {}: {error}", program.display()))?;
        crate::process::attach_child(&child);
        let input = child.stdin.take().ok_or("the viewport worker has no stdin")?;
        let output = child.stdout.take().ok_or("the viewport worker has no stdout")?;
        let (send, replies) = mpsc::channel();
        std::thread::Builder::new()
            .name("rur-viewport-worker".into())
            .spawn(move || {
                for line in BufReader::new(output).lines() {
                    let Ok(line) = line else { break };
                    if send.send(line).is_err() {
                        break;
                    }
                }
            })
            .map_err(|error| error.to_string())?;
        Ok(self.running.insert(Running { child, input, replies, executable: program }))
    }

    /// One request and its reply; a reply with `error` is a failure. A worker that does not answer in
    /// time, or exits, is stopped and started again by the next request.
    fn request(&mut self, request: Value) -> Result<Value, String> {
        let timeout = self.timeout;
        let running = self.start()?;
        // A write fails, or the replies end, when the worker has exited; no reply in time leaves it running
        let sent = writeln!(running.input, "{request}").and_then(|_| running.input.flush()).is_ok();
        let reply = if sent { running.replies.recv_timeout(timeout) } else { Err(mpsc::RecvTimeoutError::Disconnected) };
        let line = match reply {
            Ok(line) => line,
            Err(failure) => {
                let executable = running.executable.clone();
                self.stop();
                return Err(match failure {
                    mpsc::RecvTimeoutError::Disconnected => exit_reason(&executable),
                    mpsc::RecvTimeoutError::Timeout => format!("the viewport worker did not answer within {} s", timeout.as_secs_f32()),
                });
            }
        };
        let reply: Value = serde_json::from_str(&line).map_err(|_| format!("the viewport worker answered something other than JSON: {}", line.chars().take(200).collect::<String>()))?;
        match reply["error"].as_str() {
            Some(error) => Err(error.to_owned()),
            None => Ok(reply),
        }
    }

    fn stop(&mut self) {
        if let Some(mut running) = self.running.take() {
            drop(running.input);
            // a moment to exit on its own (stdin closed), then it is stopped
            for _ in 0..20 {
                if !matches!(running.child.try_wait(), Ok(None)) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            let _ = running.child.kill();
            let _ = running.child.wait();
        }
    }
}

impl Drop for ViewportWorker {
    fn drop(&mut self) {
        self.stop();
        if let Some(directory) = self.directory.take() {
            // a worker just stopped may still hold its last file for a moment
            for _ in 0..3 {
                if std::fs::remove_dir_all(&directory).is_ok() || !directory.exists() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
}

/// Why a worker exited before answering: most often a ror too old to have the command
fn exit_reason(executable: &Path) -> String {
    let version = crate::download::without_account(&mut Command::new(executable))
        .arg("--version")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .filter(|version| !version.is_empty());
    match version {
        Some(version) => format!("the viewport worker {} ({version}) exited without answering: ViewportFrames need ror 0.5.0 or later ({ROR_RELEASES})", executable.display()),
        None => format!("the viewport worker {} exited without answering", executable.display()),
    }
}

/// A byte as rur writes one: floor(v * 256), clamped
fn byte(value: f64) -> u8 {
    (value * 256.0).floor().clamp(0.0, 255.0) as u8
}

/// Lays a transparent picture over the frame's fill (`color` 0..1, `transparency`), as Studio's target
/// holds it: a texel the world covers in part (coverage c, its alpha) takes the straight mean of its
/// colour and the fill's, W c + B (1 - c), at alpha c + b (1 - c), so a part's edge holds the fill's
/// colour at full strength whatever its opacity (Studio 0.747, fixtures/viewport-probe-fade.luau).
/// Written beside the file and renamed over it.
fn lay_over_fill(file: &Path, fill: &Value) -> Result<(), String> {
    let colour = [0, 1, 2].map(|index| byte(fill["color"][index].as_f64().unwrap_or(1.0)) as f64);
    let opacity = byte(1.0 - fill["transparency"].as_f64().unwrap_or(0.0)) as f64 / 255.0;
    let mut picture = image::open(file).map_err(|error| format!("reading the ViewportFrame picture: {error}"))?.to_rgba8();
    for texel in picture.pixels_mut() {
        if texel[3] == 255 {
            continue;
        }
        let coverage = texel[3] as f64 / 255.0;
        for channel in 0..3 {
            texel[channel] = (texel[channel] as f64 * coverage + colour[channel] * (1.0 - coverage)).round() as u8;
        }
        texel[3] = ((coverage + opacity * (1.0 - coverage)) * 255.0).round() as u8;
    }
    let partial = file.with_extension(format!("{}.layer.png", std::process::id()));
    picture.save(&partial).and_then(|_| std::fs::rename(&partial, file).map_err(image::ImageError::IoError)).map_err(|error| {
        let _ = std::fs::remove_file(&partial);
        format!("laying the ViewportFrame picture over its fill: {error}")
    })
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Keeps the newest `keep` pictures of `folder`, never removing any file in `protect` (so a scene
/// with more than `keep` ViewportFrames does not evict its own pictures before rendering reads them).
fn prune(folder: &Path, keep: usize, protect: &HashSet<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    let mut pictures: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|extension| extension == "png"))
        .map(|entry| (entry.metadata().and_then(|metadata| metadata.modified()).unwrap_or(std::time::UNIX_EPOCH), entry.path()))
        .collect();
    if pictures.len() <= keep {
        return;
    }
    pictures.sort();
    let mut excess = pictures.len() - keep;
    for (_, file) in &pictures {
        if excess == 0 {
            break;
        }
        if protect.contains(file) {
            continue;
        }
        let _ = std::fs::remove_file(file);
        excess -= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let unique = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let folder = std::env::temp_dir().join(format!("rur-viewports-{name}-{}-{unique}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    const DIGEST: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn viewport_uris_name_a_slot_and_a_snapshot() {
        assert_eq!(parse_uri(&format!("rur-viewport://17/{DIGEST}")), Some((17, DIGEST)));
        assert_eq!(parse_uri("rur-viewport://17/abc"), None);
        assert_eq!(parse_uri(&format!("rur-viewport://x/{DIGEST}")), None);
        assert_eq!(parse_uri(&format!("rbxassetid://17/{DIGEST}")), None);
    }

    /// RUR_VIEWPORT_WORKER first, then ror beside a checkout, then the first `ror` on the PATH;
    /// without any, the message says how to get ror
    #[test]
    fn the_worker_is_named_found_beside_the_checkout_or_on_the_path() {
        let root = folder("find");
        let name = if cfg!(windows) { "ror.exe" } else { "ror" };
        let (first, second) = (root.join("first"), root.join("second"));
        std::fs::create_dir_all(first.join(name)).unwrap(); // a folder of that name is not the program
        std::fs::create_dir_all(&second).unwrap();
        std::fs::write(second.join(name), "").unwrap();
        let path = std::env::join_paths([root.join("missing"), first.clone(), second.clone()]).unwrap();
        let repo = root.join("roblox-ui-renderer");
        assert_eq!(find_worker(&repo, Some(PathBuf::from("named")), Some(path.clone())).unwrap(), std::path::absolute("named").unwrap());
        // a program's bare name is looked up on the PATH
        assert_eq!(find_worker(&repo, Some(PathBuf::from("ror")), Some(path.clone())).unwrap(), second.join(name));
        assert_eq!(find_worker(&repo, None, Some(path.clone())).unwrap(), second.join(name));
        let sibling = root.join("roblox-object-renderer/target/release");
        std::fs::create_dir_all(&sibling).unwrap();
        std::fs::write(sibling.join(name), "").unwrap();
        let beside = find_worker(&repo, None, Some(path)).unwrap();
        if crate::paths::is_standalone() {
            assert_eq!(beside, second.join(name), "a release build has no checkout to look beside");
        } else {
            assert_eq!(beside, repo.join("..").join("roblox-object-renderer/target/release").join(name));
        }
        let missing = find_worker(&root.join("nowhere/rur"), None, None).unwrap_err();
        assert!(missing.contains(ROKIT_INSTALL) && missing.contains("RUR_VIEWPORT_WORKER"), "{missing}");
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A stand-in worker (Node): answers each viewport request by copying a PNG to `out` (with
    /// `background` as the snapshot asks), the catalog with one key, a bundle with its `out`, and an
    /// `error` for the request whose snapshot says so
    fn stand_in(root: &Path, picture: &Path) -> ViewportWorker {
        let script = format!(
            "const fs=require('fs');const rl=require('readline').createInterface({{input:process.stdin}});\
             rl.on('line',line=>{{const r=JSON.parse(line);let reply;\
             if(r.command==='catalog')reply={{bundles:['Models/Thing']}};\
             else if(r.command==='bundle'){{fs.writeFileSync(r.out,'model');reply={{file:r.out}};}}\
             else if(r.viewport.fail)reply={{error:'cannot draw this'}};\
             else{{fs.copyFileSync({},r.out);fs.appendFileSync({},'x');reply={{file:r.out,background:!r.viewport.noFill}};}}\
             process.stdout.write(JSON.stringify(reply)+'\\n');}});",
            serde_json::to_string(picture).unwrap(),
            serde_json::to_string(&root.join("count")).unwrap()
        );
        let node = std::env::var_os("RUR_NODE").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("node"));
        ViewportWorker::with_command(root, node, vec!["-e".into(), script])
    }

    fn renders(root: &Path) -> usize {
        std::fs::read(root.join("count")).map(|bytes| bytes.len()).unwrap_or(0)
    }

    #[test]
    fn a_snapshot_is_drawn_once_per_slot_and_kept_when_asked() {
        let root = folder("render");
        let picture = root.join("picture.png");
        std::fs::write(&picture, b"png").unwrap();
        let snapshot = json!({"version": 1, "background": {"color": [0, 0, 0], "transparency": 0}});
        let uri = format!("rur-viewport://3/{DIGEST}");
        {
            let mut worker = stand_in(&root, &picture);
            let live = worker.render(&uri, &snapshot, false).unwrap();
            assert_eq!(std::fs::read(&live).unwrap(), b"png");
            assert_eq!(worker.render(&uri, &snapshot, false).unwrap(), live, "the same snapshot in the same slot");
            assert_eq!(renders(&root), 1);
            // kept: a later worker (another process) finds it without drawing
            let kept = worker.render(&uri, &snapshot, true).unwrap();
            assert!(kept.starts_with(root.join("viewports-persisted")) && kept.is_file());
            worker.retain(&[]);
            assert!(!live.exists(), "a slot no longer shown loses its picture");
        }
        assert!(std::fs::read_dir(root.join("viewports-live")).unwrap().next().is_none(), "the live folder goes with the worker");
        let mut later = stand_in(&root, &picture);
        assert_eq!(later.render(&uri, &snapshot, true).unwrap().parent(), Some(root.join("viewports-persisted").as_path()));
        assert_eq!(renders(&root), 1, "the kept picture is not drawn again");
        drop(later);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_worker_error_is_the_frames_failure() {
        let root = folder("errors");
        let picture = root.join("picture.png");
        std::fs::write(&picture, b"png").unwrap();
        let mut worker = stand_in(&root, &picture);
        let uri = format!("rur-viewport://1/{DIGEST}");
        assert_eq!(worker.render(&uri, &json!({"fail": true}), false).unwrap_err(), "cannot draw this");
        assert!(worker.render("rbxassetid://1", &json!({}), false).is_err());
        drop(worker);
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A transparent picture (a worker that leaves the fill out) is laid over the fill: where nothing covers
    /// it the fill, an edge the straight mean at the mean opacity, a covered texel as drawn, and a clear
    /// fill's dark fringe (the fill's colour at full strength)
    #[test]
    fn a_transparent_picture_is_laid_over_the_frames_fill() {
        let root = folder("fill");
        let file = root.join("picture.png");
        let write = || image::RgbaImage::from_raw(3, 1, vec![0, 0, 0, 0, 255, 255, 255, 64, 255, 0, 0, 255]).unwrap().save(&file).unwrap();
        write();
        lay_over_fill(&file, &json!({"color": [30.0 / 255.0, 36.0 / 255.0, 50.0 / 255.0], "transparency": 0.5})).unwrap();
        let out = image::open(&file).unwrap().to_rgba8();
        assert_eq!(out.get_pixel(0, 0).0, [30, 36, 50, 128], "the fill where nothing covers it");
        let a = 64.0 / 255.0;
        let mean = [30.0, 36.0, 50.0].map(|fill: f64| (255.0 * a + fill * (1.0 - a)).round() as u8);
        assert_eq!(out.get_pixel(1, 0).0, [mean[0], mean[1], mean[2], ((a + 128.0 / 255.0 * (1.0 - a)) * 255.0).round() as u8], "an edge");
        assert_eq!(out.get_pixel(2, 0).0, [255, 0, 0, 255], "what the world covers replaces the fill");
        write();
        lay_over_fill(&file, &json!({"color": [0, 0, 0], "transparency": 1})).unwrap();
        assert_eq!(image::open(&file).unwrap().to_rgba8().get_pixel(1, 0).0, [64, 64, 64, 64], "a dark fringe at BackgroundTransparency 1");
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A worker that answers without `background: true` has its picture laid over the fill by rur
    #[test]
    fn an_older_workers_picture_gets_the_fill_from_rur() {
        let root = folder("older");
        let picture = root.join("picture.png");
        image::RgbaImage::from_raw(1, 1, vec![0, 0, 0, 0]).unwrap().save(&picture).unwrap();
        let mut worker = stand_in(&root, &picture);
        let drawn = worker.render(&format!("rur-viewport://1/{DIGEST}"), &json!({"noFill": true, "background": {"color": [1, 0, 0], "transparency": 0}}), false).unwrap();
        assert_eq!(image::open(&drawn).unwrap().to_rgba8().get_pixel(0, 0).0, [255, 0, 0, 255]);
        drop(worker);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn model_bundles_come_from_the_worker() {
        let root = folder("bundles");
        let mut worker = stand_in(&root, &root.join("unused.png"));
        assert_eq!(worker.model(json!({"command": "catalog", "av": "project"})).unwrap()["bundles"], json!(["Models/Thing"]));
        let file = worker.model(json!({"command": "bundle", "av": "project", "path": "Models/Thing"})).unwrap();
        let file = PathBuf::from(file["file"].as_str().unwrap());
        assert!(file.starts_with(root.join("model-bundles")) && std::fs::read(&file).unwrap() == b"model");
        drop(worker);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_worker_that_exits_without_answering_says_so() {
        let root = folder("exit");
        let node = std::env::var_os("RUR_NODE").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("node"));
        let mut worker = ViewportWorker::with_command(&root, node, vec!["-e".into(), "process.exit(2)".into()]);
        let error = worker.render(&format!("rur-viewport://1/{DIGEST}"), &json!({}), false).unwrap_err();
        assert!(error.contains("exited without answering"), "{error}");
        drop(worker);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn persisted_pruning_preserves_the_current_batch_and_evicts_older_entries() {
        let root = folder("prune");
        let persisted = root.join("viewports-persisted");
        std::fs::create_dir_all(&persisted).unwrap();
        let make = |name: &str, secs: u64| {
            let file = persisted.join(format!("{name}.png"));
            std::fs::write(&file, b"png").unwrap();
            let modified = std::time::UNIX_EPOCH + Duration::from_secs(secs);
            std::fs::File::options().write(true).open(&file).unwrap().set_modified(modified).unwrap();
            file
        };
        let stale = make("stale", 10);
        let batch: Vec<PathBuf> = (0..5).map(|i| make(&format!("batch-{i}"), 20 + i as u64)).collect();
        let protected: HashSet<PathBuf> = batch.iter().cloned().collect();
        // A batch larger than the cache limit keeps all of its own pictures and still evicts stale ones.
        prune(&persisted, 3, &protected);
        assert!(!stale.exists(), "older picture outside the batch is pruned");
        for file in &batch {
            assert!(file.is_file(), "picture in the current batch must survive pruning: {}", file.display());
        }
        // A later smaller batch brings the cache back down to the limit while protecting its own hit.
        let keep_one: HashSet<PathBuf> = [batch[0].clone()].into_iter().collect();
        prune(&persisted, 3, &keep_one);
        assert!(batch[0].is_file(), "protected oldest picture is kept");
        let remaining = std::fs::read_dir(&persisted).unwrap().count();
        assert_eq!(remaining, 3, "cache trims back to the limit");
        std::fs::remove_dir_all(root).unwrap();
    }
}
