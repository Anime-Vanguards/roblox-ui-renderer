//! The engine as rur runs it: roblox-engine-emulator (the emulated Roblox engine: instances,
//! reflection, values, signals, the scheduler, services, project mounting, the 3D world and the
//! recorder), re-exported here so that `engine::dom`, `engine::lua`, `engine::scheduler`, ... are its
//! modules, plus the GUI side this crate adds through the emulator's layer hooks ([`GuiLayer`]):
//! layout, text measurement (TextService, ContentText), input, the scene export, story mounting,
//! sessions and the engine CLI. notes/rust-engine.md is the GUI side's contract; the emulator's
//! notes/engine.md the core's.

pub use roblox_engine_emulator::*;

pub mod cache_counts;
pub mod chrome;
pub mod cli;
pub mod export;
pub mod input;
pub mod layers;
pub mod layout;
pub mod order;
pub mod path2d_layout;
pub mod session;
pub mod story;
pub mod text;
pub mod text_service;

#[cfg(test)]
mod session_tests;
#[cfg(test)]
mod tests;

use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
};

/// The baked checkout, or the standalone state directory for a release build.
pub fn default_repo() -> PathBuf {
    crate::paths::default_repo()
}

/// Engine options with this checkout as the repo (the emulator's own default is its checkout).
pub fn options() -> Options {
    Options { repo: default_repo(), ..Options::default() }
}

/// The GUI layer's state in an engine (an emulator [`Extension`]): the text port's engine (fonts and
/// layout caches, made on first use), the layout port's state (records, invalidation stamps,
/// caches; set by `layout::install`), runtime/input's (`input::install`) and the LayerCollectors'
/// draw order among equal DisplayOrders (`layers::install`), and the rectangles Path2Ds resolve against.
#[derive(Default)]
pub struct Gui {
    pub text: RefCell<Option<text::TextEngine>>,
    pub layout: RefCell<Option<layout::Layout>>,
    pub input: RefCell<Option<Rc<input::Input>>>,
    pub layer_order: RefCell<layers::LayerOrder>,
    /// Where the parents of Path2Ds were at the last frame's layout (`path2d_layout::install`)
    pub path_frames: RefCell<path2d_layout::Frames>,
    /// With `--glyph-scale` (the engine then lays out at that scale): the device scale the picture is drawn at
    /// (`--scale`), the scene header's `viewport.scale` beside its `glyphScale` (export.rs viewport_json); None without
    pub draw_scale: std::cell::Cell<Option<f64>>,
    /// Roblox's own GUI on a playing client's screen (chrome.rs): the core GUI switches the game set, the instances drawn
    pub chrome: chrome::Chrome,
}

impl Extension for Gui {
    fn teardown(&self) {
        drop(self.input.borrow_mut().take());
        drop(self.layout.borrow_mut().take());
        drop(self.text.borrow_mut().take());
        drop(std::mem::take(&mut *self.layer_order.borrow_mut()));
    }
}

/// What rur adds to every engine (Engine.Start installs it after the core's behaviours, derived
/// views and world, before the DataModel): TextService's measurements and ContentText, the layout
/// (Absolute* / TextBounds getters, CanvasPosition, pages, the Render-phase pass) and runtime/input
/// (live UserInputService state, GuiState, scrolling, TextBox, drag), the LayerCollectors' tie
/// order (`--all-layers`), and where a Path2D's parent is (the emulator's curve queries ask).
pub struct GuiLayer;

impl Layer for GuiLayer {
    fn install(&self, engine: &Rc<Engine>) {
        engine.extension_or_insert_with(Gui::default);
        text_service::install(engine, &mut engine.behaviors.borrow_mut());
        layout::install(engine);
        path2d_layout::install(engine);
        input::install(engine);
        layers::install(engine);
        chrome::install(engine);
    }
}

/// The GUI layer's state of an engine.
pub trait GuiEngine {
    fn gui(&self) -> &Gui;

    fn text(&self) -> &RefCell<Option<text::TextEngine>> {
        &self.gui().text
    }

    fn layout(&self) -> &RefCell<Option<layout::Layout>> {
        &self.gui().layout
    }

    fn input(&self) -> &RefCell<Option<Rc<input::Input>>> {
        &self.gui().input
    }
}

impl GuiEngine for Engine {
    fn gui(&self) -> &Gui {
        self.extension_or_insert_with(Gui::default)
    }
}

/// An engine as rur runs it: with the GUI layer and (on the desktop) manifest bundles from the 3D
/// worker that draws ViewportFrames (viewports.rs). A server (a loopback run's, or `--run-context
/// server`) has neither: it lays nothing out and draws no ViewportFrame (bundles it mounts come
/// through the emulator's own asset worker).
pub fn new_engine(options: Options) -> Result<Rc<Engine>, String> {
    if options.run_context == RunContext::Server {
        return Engine::new(options);
    }
    let engine = Engine::with_layers(options, vec![Rc::new(GuiLayer)])?;
    #[cfg(feature = "desktop")]
    engine.set_bundle_source(Box::new(WorkerBundles::shared(&engine.options.repo, engine.options.offline)));
    Ok(engine)
}

/// rur as an emulator host: `rur-native record` runs the emulator's recorder with rur's engines.
pub fn host() -> launch::Host {
    launch::Host { name: "rur", repo: default_repo(), new_engine: Rc::new(new_engine) }
}

/// Manifest bundles from the 3D worker (viewports.rs `model` requests), sharing one process-wide
/// worker and a content-addressed immutable catalog cache on disk (`<state>/model-bundles/catalog-v1-<sha256>.json`).
#[cfg(feature = "desktop")]
struct WorkerBundles {
    state: PathBuf,
    worker: std::sync::Arc<std::sync::Mutex<crate::viewports::ViewportWorker>>,
}

#[cfg(feature = "desktop")]
const CATALOG_SCHEMA_VERSION: u32 = 1;
#[cfg(feature = "desktop")]
const BUNDLE_SCHEMA_VERSION: u32 = 1;

#[cfg(feature = "desktop")]
fn manifest_path(av: &Path) -> Option<PathBuf> {
    // The manifest module RUR_BUNDLE_MANIFEST names (relative to the project); without it the catalog is asked every time
    let manifest = std::env::var_os("RUR_BUNDLE_MANIFEST").map(|file| av.join(file));
    manifest
}

#[cfg(feature = "desktop")]
impl WorkerBundles {
    fn shared(repo: &Path, offline: bool) -> Self {
        Self {
            state: crate::state_dir(repo),
            worker: crate::viewports::shared_model_worker(repo, offline),
        }
    }

    #[cfg(test)]
    fn with_worker(state: PathBuf, worker: crate::viewports::ViewportWorker) -> Self {
        Self { state, worker: std::sync::Arc::new(std::sync::Mutex::new(worker)) }
    }

    fn bundle_with_manifest(&mut self, av: &Path, key: &str, manifest: Option<&Path>) -> Result<PathBuf, String> {
        use std::collections::HashSet;
        use std::sync::{Condvar, Mutex, OnceLock};

        static IN_FLIGHT: OnceLock<(Mutex<HashSet<PathBuf>>, Condvar)> = OnceLock::new();

        struct FlightGuard<'a> {
            state: &'a (Mutex<HashSet<PathBuf>>, Condvar),
            target: PathBuf,
        }
        impl Drop for FlightGuard<'_> {
            fn drop(&mut self) {
                self.state.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).remove(&self.target);
                self.state.1.notify_all();
            }
        }

        let manifest_digest = manifest.and_then(|file| std::fs::read(file).ok()).map(|text| text::hinted::sha256_hex(&text));
        let Some(manifest_digest) = manifest_digest else {
            let reply = self
                .worker
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .model(serde_json::json!({ "command": "bundle", "av": av, "path": key }))?;
            return reply["file"].as_str().map(PathBuf::from).ok_or_else(|| "the model worker returned no model file".to_string());
        };

        let bundle_digest = text::hinted::sha256_hex(format!("{manifest_digest}\0{key}").as_bytes());
        let target = bundle_cache_path(&self.state, BUNDLE_SCHEMA_VERSION, &bundle_digest);
        if is_valid_rbxm(&target) {
            return Ok(target);
        }

        let flight = IN_FLIGHT.get_or_init(Default::default);
        let _guard = loop {
            let mut set = flight.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            if is_valid_rbxm(&target) {
                return Ok(target);
            }
            if set.insert(target.clone()) {
                break FlightGuard { state: flight, target: target.clone() };
            }
            set = flight.1.wait(set).unwrap_or_else(|poisoned| poisoned.into_inner());
        };

        if is_valid_rbxm(&target) {
            return Ok(target);
        }

        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let seq = SEQ.fetch_add(1, Ordering::Relaxed);
        let partial = target.with_extension(format!("rbxm.{}.{seq}.tmp", std::process::id()));
        let reply = self
            .worker
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .model(serde_json::json!({ "command": "bundle", "av": av, "path": key, "out": partial }))?;
        let produced = reply["file"].as_str().map(PathBuf::from).ok_or_else(|| "the model worker returned no model file".to_string())?;
        if produced != partial {
            return Ok(produced);
        }
        replace_immutable_file(&partial, &target, is_valid_rbxm).map_err(|error| format!("saving model bundle cache {}: {error}", target.display()))?;
        Ok(target)
    }

    fn catalog_with_manifest(&mut self, av: &Path, manifest: Option<&Path>) -> Result<Vec<String>, String> {
        use std::collections::HashSet;
        use std::sync::{Condvar, Mutex, OnceLock};

        #[derive(Default)]
        struct CatalogCache {
            entries: rustc_hash::FxHashMap<(PathBuf, String), Vec<String>>,
            in_flight: HashSet<(PathBuf, String)>,
        }
        static CATALOGS: OnceLock<(Mutex<CatalogCache>, Condvar)> = OnceLock::new();

        struct InFlightGuard<'a> {
            state: &'a (Mutex<CatalogCache>, Condvar),
            key: (PathBuf, String),
        }
        impl Drop for InFlightGuard<'_> {
            fn drop(&mut self) {
                let mut guard = self.state.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                guard.in_flight.remove(&self.key);
                self.state.1.notify_all();
            }
        }

        let digest = manifest.and_then(|file| std::fs::read(file).ok()).map(|text| text::hinted::sha256_hex(&text));
        let Some(digest) = digest else {
            let reply = self
                .worker
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .model(serde_json::json!({ "command": "catalog", "av": av }))?;
            return Ok(reply["bundles"].as_array().into_iter().flatten().filter_map(|v| v.as_str().map(str::to_string)).collect());
        };

        let key = (self.state.clone(), digest.clone());
        let cache = CATALOGS.get_or_init(Default::default);
        let cache_file = catalog_cache_path(&self.state, CATALOG_SCHEMA_VERSION, &digest);

        let _flight = loop {
            let mut guard = cache.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Some(keys) = guard.entries.get(&key).cloned() {
                return Ok(keys);
            }
            if !guard.in_flight.contains(&key) {
                guard.in_flight.insert(key.clone());
                break InFlightGuard { state: cache, key: key.clone() };
            }
            guard = cache.1.wait(guard).unwrap_or_else(|poisoned| poisoned.into_inner());
        };

        if let Some(keys) = read_catalog_cache(&cache_file, CATALOG_SCHEMA_VERSION, &digest) {
            cache.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).entries.insert(key, keys.clone());
            return Ok(keys);
        }

        let reply = self
            .worker
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .model(serde_json::json!({ "command": "catalog", "av": av }))?;
        let keys: Vec<String> = reply["bundles"].as_array().into_iter().flatten().filter_map(|v| v.as_str().map(str::to_string)).collect();
        let _ = write_catalog_cache(&cache_file, CATALOG_SCHEMA_VERSION, &digest, &keys);
        cache.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).entries.insert(key, keys.clone());
        Ok(keys)
    }
}

#[cfg(feature = "desktop")]
fn catalog_cache_path(state: &Path, version: u32, sha256: &str) -> PathBuf {
    state.join("model-bundles").join(format!("catalog-v{version}-{sha256}.json"))
}

#[cfg(feature = "desktop")]
fn bundle_cache_path(state: &Path, version: u32, sha256: &str) -> PathBuf {
    state.join("model-bundles").join(format!("bundle-v{version}-{sha256}.rbxm"))
}

#[cfg(feature = "desktop")]
fn is_valid_rbxm(file: &Path) -> bool {
    use std::io::Read;
    let Ok(mut handle) = std::fs::File::open(file) else {
        return false;
    };
    let mut magic = [0u8; 8];
    // (or a Roblox archive, a ZIP, which the engine reads as the model it holds)
    handle.read_exact(&mut magic).is_ok() && (&magic == b"<roblox!" || magic.starts_with(b"PK\x03\x04"))
}

#[cfg(feature = "desktop")]
fn replace_immutable_file(partial: &Path, target: &Path, valid: impl Fn(&Path) -> bool) -> std::io::Result<()> {
    let mut attempt = 0;
    loop {
        match std::fs::rename(partial, target) {
            Ok(()) => return Ok(()),
            Err(error)
                if attempt < 7
                    && (error.kind() == std::io::ErrorKind::PermissionDenied
                        || error.kind() == std::io::ErrorKind::AlreadyExists) =>
            {
                if valid(target) {
                    let _ = std::fs::remove_file(partial);
                    return Ok(());
                }
                std::thread::sleep(std::time::Duration::from_millis(10 << attempt.min(3)));
                attempt += 1;
            }
            Err(error) => {
                let _ = std::fs::remove_file(partial);
                return Err(error);
            }
        }
    }
}

#[cfg(feature = "desktop")]
fn read_catalog_cache(file: &Path, version: u32, sha256: &str) -> Option<Vec<String>> {
    let bytes = std::fs::read(file).ok()?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    if value["version"].as_u64() != Some(u64::from(version)) || value["sha256"].as_str() != Some(sha256) {
        return None;
    }
    let bundles = value["bundles"].as_array()?;
    let mut keys = Vec::with_capacity(bundles.len());
    for entry in bundles {
        keys.push(entry.as_str()?.to_string());
    }
    Some(keys)
}

#[cfg(feature = "desktop")]
fn write_catalog_cache(file: &Path, version: u32, sha256: &str, keys: &[String]) -> std::io::Result<()> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let body = serde_json::to_vec(&serde_json::json!({
        "version": version,
        "sha256": sha256,
        "bundles": keys,
    }))
    .map_err(std::io::Error::other)?;
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let partial = file.with_extension(format!("json.{}.{seq}.tmp", std::process::id()));
    std::fs::write(&partial, &body)?;
    let mut attempt = 0;
    loop {
        match std::fs::rename(&partial, file) {
            Ok(()) => return Ok(()),
            Err(error) if attempt < 7 && (error.kind() == std::io::ErrorKind::PermissionDenied || error.kind() == std::io::ErrorKind::AlreadyExists) => {
                // Content-addressed immutable target: if another process already landed the exact file, we are done.
                if read_catalog_cache(file, version, sha256).is_some() {
                    let _ = std::fs::remove_file(&partial);
                    return Ok(());
                }
                std::thread::sleep(std::time::Duration::from_millis(10 << attempt.min(3)));
                attempt += 1;
            }
            Err(error) => {
                let _ = std::fs::remove_file(&partial);
                return Err(error);
            }
        }
    }
}

#[cfg(feature = "desktop")]
impl project::model_assets::BundleSource for WorkerBundles {
    /// The manifest's bundle keys, cached by content SHA-256 on disk and in memory (SingleFlight)
    /// so neither repeated engines nor concurrent cold starts spawn extra model workers.
    fn catalog(&mut self, av: &Path) -> Result<Vec<String>, String> {
        let manifest = manifest_path(av);
        self.catalog_with_manifest(av, manifest.as_deref())
    }

    fn bundle(&mut self, av: &Path, key: &str) -> Result<PathBuf, String> {
        let manifest = manifest_path(av);
        self.bundle_with_manifest(av, key, manifest.as_deref())
    }
}

#[cfg(all(test, feature = "desktop"))]
mod bundle_tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "rur-model-{name}-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn counting_worker(state: &Path, counter: &Path, bundles: &[&str]) -> crate::viewports::ViewportWorker {
        let script = format!(
            "const fs=require('fs');require('readline').createInterface({{input:process.stdin}}).on('line',line=>{{const r=JSON.parse(line);if(r.command==='catalog'){{fs.appendFileSync({counter},'c');process.stdout.write(JSON.stringify({{bundles:{bundles}}})+'\\n');}}}});",
            counter = serde_json::to_string(counter).unwrap(),
            bundles = serde_json::to_string(bundles).unwrap(),
        );
        let node = std::env::var_os("RUR_NODE").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("node"));
        crate::viewports::ViewportWorker::with_command(state, node, vec!["-e".into(), script])
    }

    fn call_count(counter: &Path) -> usize {
        std::fs::read(counter).map(|bytes| bytes.len()).unwrap_or(0)
    }

    /// Manifest bundles reach the engine from the 3D worker (here a stand-in answering the catalog and
    /// every bundle with one model file, written where the worker is told)
    #[test]
    fn manifest_bundles_come_from_the_worker() {
        use project::model_assets::BundleSource;
        use rbx_dom_weak::{InstanceBuilder, WeakDom, types::Vector3};
        let directory = temp_dir("mount");
        let model_file = directory.join("model.rbxm");
        let dom = WeakDom::new(InstanceBuilder::new("DataModel").with_child(InstanceBuilder::new("Part").with_name("RealPart").with_property("Size", Vector3::new(2., 3., 4.))));
        let mut bytes = Vec::new();
        rbx_binary::to_writer(&mut bytes, &dom, dom.root().children()).unwrap();
        std::fs::write(&model_file, bytes).unwrap();
        let script = format!("const fs=require('fs');require('readline').createInterface({{input:process.stdin}}).on('line',line=>{{const request=JSON.parse(line);const reply=request.command==='catalog'?{{bundles:['Models/Thing']}}:(fs.copyFileSync({model},request.out),{{file:request.out}});process.stdout.write(JSON.stringify(reply)+'\\n');}});", model = serde_json::to_string(&model_file).unwrap());
        let node = std::env::var_os("RUR_NODE").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("node"));
        let mut source = WorkerBundles::with_worker(directory.clone(), crate::viewports::ViewportWorker::with_command(&directory, node, vec!["-e".into(), script]));
        assert_eq!(source.catalog(&directory).unwrap(), vec!["Models/Thing".to_string()]);
        let bundle = source.bundle(&directory, "Models/Thing").unwrap();
        assert!(bundle.starts_with(directory.join("model-bundles")));
        assert_eq!(std::fs::read(&bundle).unwrap(), std::fs::read(&model_file).unwrap());
        drop(source);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn catalog_cache_ignores_half_written_tmp_and_corrupt_files() {
        let dir = temp_dir("corrupt");
        let state = dir.join("state");
        let manifest = dir.join("manifest.luau");
        let counter = dir.join("calls");
        std::fs::write(&manifest, b"return { a = 1 }").unwrap();
        let digest = text::hinted::sha256_hex(b"return { a = 1 }");
        let target = catalog_cache_path(&state, CATALOG_SCHEMA_VERSION, &digest);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        // Half-written sibling .tmp and truncated target must be ignored and rebuilt cleanly
        std::fs::write(target.with_extension("json.999.0.tmp"), b"{\"version\":1,\"bundles\":[").unwrap();
        std::fs::write(&target, b"{\"version\":1,\"sha256\":").unwrap();
        let mut source = WorkerBundles::with_worker(state.clone(), counting_worker(&state, &counter, &["Models/A"]));
        assert_eq!(source.catalog_with_manifest(&dir, Some(&manifest)).unwrap(), vec!["Models/A".to_string()]);
        assert_eq!(call_count(&counter), 1);
        assert_eq!(read_catalog_cache(&target, CATALOG_SCHEMA_VERSION, &digest), Some(vec!["Models/A".to_string()]));
        drop(source);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn catalog_cache_handles_concurrent_writers_for_same_key() {
        let dir = temp_dir("concurrent");
        let state = dir.join("state");
        let manifest = dir.join("manifest.luau");
        let counter = dir.join("calls");
        std::fs::write(&manifest, b"return { shared = true }").unwrap();
        let digest = text::hinted::sha256_hex(b"return { shared = true }");
        let target = catalog_cache_path(&state, CATALOG_SCHEMA_VERSION, &digest);

        // Concurrent atomic writes to the exact same key from several threads all succeed
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(6));
        let mut handles = Vec::new();
        for _ in 0..6 {
            let target = target.clone();
            let digest = digest.clone();
            let barrier = barrier.clone();
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                write_catalog_cache(&target, CATALOG_SCHEMA_VERSION, &digest, &["Models/Shared".into()]).unwrap();
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(read_catalog_cache(&target, CATALOG_SCHEMA_VERSION, &digest), Some(vec!["Models/Shared".to_string()]));

        // SingleFlight deduplicates concurrent in-process cold fetches for a fresh digest
        let manifest2 = dir.join("manifest2.luau");
        std::fs::write(&manifest2, b"return { race = 42 }").unwrap();
        let shared_worker = std::sync::Arc::new(std::sync::Mutex::new(counting_worker(&state, &counter, &["Models/Race"])));
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
        let mut handles = Vec::new();
        for _ in 0..4 {
            let state = state.clone();
            let dir = dir.clone();
            let manifest2 = manifest2.clone();
            let shared_worker = shared_worker.clone();
            let barrier = barrier.clone();
            handles.push(std::thread::spawn(move || {
                let mut src = WorkerBundles { state, worker: shared_worker };
                barrier.wait();
                src.catalog_with_manifest(&dir, Some(&manifest2)).unwrap()
            }));
        }
        for h in handles {
            assert_eq!(h.join().unwrap(), vec!["Models/Race".to_string()]);
        }
        assert_eq!(call_count(&counter), 1, "SingleFlight must ask the worker once");
        drop(shared_worker);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn catalog_cache_misses_and_rebuilds_for_older_schema_version() {
        let dir = temp_dir("version");
        let state = dir.join("state");
        let manifest = dir.join("manifest.luau");
        let counter = dir.join("calls");
        std::fs::write(&manifest, b"return { ver = 1 }").unwrap();
        let digest = text::hinted::sha256_hex(b"return { ver = 1 }");
        let old_file = catalog_cache_path(&state, 0, &digest);
        write_catalog_cache(&old_file, 0, &digest, &["Models/Stale".into()]).unwrap();
        // Also verify a current-path file stamped with an older schema version is rejected
        let current_file = catalog_cache_path(&state, CATALOG_SCHEMA_VERSION, &digest);
        write_catalog_cache(&current_file, 0, &digest, &["Models/Stale".into()]).unwrap();
        assert_eq!(read_catalog_cache(&current_file, CATALOG_SCHEMA_VERSION, &digest), None);

        let mut source = WorkerBundles::with_worker(state.clone(), counting_worker(&state, &counter, &["Models/Fresh"]));
        assert_eq!(source.catalog_with_manifest(&dir, Some(&manifest)).unwrap(), vec!["Models/Fresh".to_string()]);
        assert_eq!(call_count(&counter), 1);
        assert_eq!(read_catalog_cache(&current_file, CATALOG_SCHEMA_VERSION, &digest), Some(vec!["Models/Fresh".to_string()]));
        drop(source);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn catalog_cache_misses_immediately_when_manifest_changes_same_tick() {
        let dir = temp_dir("edit");
        let state = dir.join("state");
        let manifest = dir.join("manifest.luau");
        let counter = dir.join("calls");
        std::fs::write(&manifest, b"return { item = 'AAA' }").unwrap();
        let mut first = WorkerBundles::with_worker(state.clone(), counting_worker(&state, &counter, &["Models/First"]));
        assert_eq!(first.catalog_with_manifest(&dir, Some(&manifest)).unwrap(), vec!["Models/First".to_string()]);
        assert_eq!(call_count(&counter), 1);

        // Second source hits disk cache without calling worker
        let mut cached = WorkerBundles::with_worker(state.clone(), counting_worker(&state, &counter, &["Models/Unused"]));
        assert_eq!(cached.catalog_with_manifest(&dir, Some(&manifest)).unwrap(), vec!["Models/First".to_string()]);
        assert_eq!(call_count(&counter), 1);

        // Immediate same-length overwrite in the same filesystem tick must miss immediately
        std::fs::write(&manifest, b"return { item = 'BBB' }").unwrap();
        let mut edited = WorkerBundles::with_worker(state.clone(), counting_worker(&state, &counter, &["Models/Second"]));
        assert_eq!(edited.catalog_with_manifest(&dir, Some(&manifest)).unwrap(), vec!["Models/Second".to_string()]);
        assert_eq!(call_count(&counter), 2);
        drop((first, cached, edited));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
