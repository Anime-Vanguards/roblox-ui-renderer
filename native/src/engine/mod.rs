//! The engine as rur runs it: roblox-engine-emulator (the emulated Roblox engine: instances,
//! reflection, values, signals, the scheduler, services, project mounting, the 3D world and the
//! recorder), re-exported here so that `engine::dom`, `engine::lua`, `engine::scheduler`, ... are its
//! modules, plus the GUI side this crate adds through the emulator's layer hooks ([`GuiLayer`]):
//! layout, text measurement (TextService, ContentText), input, the scene export, story mounting,
//! sessions and the engine CLI. notes/rust-engine.md is the GUI side's contract; the emulator's
//! notes/engine.md the core's.

pub use roblox_engine_emulator::*;

pub mod cache_counts;
pub mod cli;
pub mod export;
pub mod input;
pub mod layers;
pub mod layout;
pub mod order;
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

/// This checkout: the state dir (caches), fixtures and the default project (`--project`).
pub fn default_repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

/// Engine options with this checkout as the repo (the emulator's own default is its checkout).
pub fn options() -> Options {
    Options { repo: default_repo(), ..Options::default() }
}

/// The GUI layer's state in an engine (an emulator [`Extension`]): the text port's engine (fonts and
/// layout caches, made on first use), the layout port's state (records, invalidation stamps,
/// caches; set by `layout::install`), runtime/input's (`input::install`) and the LayerCollectors'
/// draw order among equal DisplayOrders (`layers::install`).
#[derive(Default)]
pub struct Gui {
    pub text: RefCell<Option<text::TextEngine>>,
    pub layout: RefCell<Option<layout::Layout>>,
    pub input: RefCell<Option<Rc<input::Input>>>,
    pub layer_order: RefCell<layers::LayerOrder>,
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
/// (live UserInputService state, GuiState, scrolling, TextBox, drag), and the LayerCollectors' tie
/// order (`--all-layers`).
pub struct GuiLayer;

impl Layer for GuiLayer {
    fn install(&self, engine: &Rc<Engine>) {
        engine.extension_or_insert_with(Gui::default);
        text_service::install(engine, &mut engine.behaviors.borrow_mut());
        layout::install(engine);
        input::install(engine);
        layers::install(engine);
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

/// An engine as rur runs it: with the GUI layer and (on the desktop) manifest bundles through the
/// asset helper (cli/native-assets.mjs and its 3D model worker, as for images and ViewportFrames).
pub fn new_engine(options: Options) -> Result<Rc<Engine>, String> {
    let engine = Engine::with_layers(options, vec![Rc::new(GuiLayer)])?;
    #[cfg(feature = "desktop")]
    engine.set_bundle_source(Box::new(AssetHelperBundles(crate::resolve::Resolver::new(&engine.options.repo, engine.options.offline))));
    Ok(engine)
}

/// rur as an emulator host: `rur-native record` runs the emulator's recorder with rur's engines.
pub fn host() -> launch::Host {
    launch::Host { name: "rur", repo: default_repo(), new_engine: Rc::new(new_engine) }
}

/// Manifest bundles through the asset helper's model worker (resolve.rs `model` requests).
#[cfg(feature = "desktop")]
struct AssetHelperBundles(crate::resolve::Resolver);

#[cfg(feature = "desktop")]
impl project::model_assets::BundleSource for AssetHelperBundles {
    /// The manifest's bundle keys, kept by the process while the manifest module's text is the same:
    /// every engine mounts the catalog, and asking for it started the asset helper and its model worker
    /// for each one (a render service makes an engine per render; 150 ms and two processes each)
    fn catalog(&mut self, av: &Path) -> Result<Vec<String>, String> {
        use std::hash::Hasher;
        type Catalogs = std::sync::Mutex<rustc_hash::FxHashMap<PathBuf, (u64, Vec<String>)>>;
        static CATALOGS: std::sync::OnceLock<Catalogs> = std::sync::OnceLock::new();
        // The manifest module RUR_BUNDLE_MANIFEST names (relative to the project); without it the catalog is asked every time
        let digest = std::env::var_os("RUR_BUNDLE_MANIFEST").and_then(|file| std::fs::read(av.join(file)).ok()).map(|text| {
            let mut hasher = rustc_hash::FxHasher::default();
            hasher.write(&text);
            hasher.write_usize(text.len());
            hasher.finish()
        });
        let catalogs = CATALOGS.get_or_init(Default::default);
        if let Some(digest) = digest {
            if let Some((_, keys)) = catalogs.lock().ok().and_then(|catalogs| catalogs.get(av).filter(|(known, _)| *known == digest).cloned()) {
                return Ok(keys);
            }
        }
        let catalog = self.0.model(serde_json::json!({ "command": "catalog", "av": av }))?;
        let keys: Vec<String> = catalog["bundles"].as_array().into_iter().flatten().filter_map(|v| v.as_str().map(str::to_string)).collect();
        if let (Some(digest), Ok(mut catalogs)) = (digest, catalogs.lock()) {
            catalogs.insert(av.to_path_buf(), (digest, keys.clone()));
        }
        Ok(keys)
    }

    fn bundle(&mut self, av: &Path, key: &str) -> Result<PathBuf, String> {
        let reply = self.0.model(serde_json::json!({ "command": "bundle", "av": av, "path": key }))?;
        reply["file"].as_str().map(PathBuf::from).ok_or_else(|| "the model worker returned no model file".to_string())
    }
}

#[cfg(all(test, feature = "desktop"))]
mod bundle_tests {
    use super::*;

    /// Manifest bundles reach the engine through the asset helper (here a stand-in
    /// cli/native-assets.mjs answering the catalog and every bundle with one model file).
    #[test]
    fn manifest_bundles_come_through_the_asset_helper() {
        use project::model_assets::BundleSource;
        use rbx_dom_weak::{InstanceBuilder, WeakDom, types::Vector3};
        let directory = std::env::temp_dir().join(format!("rur-model-mount-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(directory.join("cli")).unwrap();
        let model_file = directory.join("model.rbxm");
        let dom = WeakDom::new(InstanceBuilder::new("DataModel").with_child(InstanceBuilder::new("Part").with_name("RealPart").with_property("Size", Vector3::new(2., 3., 4.))));
        let mut bytes = Vec::new();
        rbx_binary::to_writer(&mut bytes, &dom, dom.root().children()).unwrap();
        std::fs::write(&model_file, bytes).unwrap();
        let script = format!("import readline from 'node:readline'; for await (const line of readline.createInterface({{input:process.stdin}})) {{ const request=JSON.parse(line); if(request.command==='quit') break; const reply = request.model.command==='catalog' ? {{bundles:['Models/Thing']}} : {{file:{}}}; process.stdout.write(JSON.stringify({{id:request.id,...reply}})+'\\n'); }}", serde_json::to_string(&model_file).unwrap());
        std::fs::write(directory.join("cli/native-assets.mjs"), script).unwrap();
        let mut source = AssetHelperBundles(crate::resolve::Resolver::new(&directory, true));
        assert_eq!(source.catalog(&directory).unwrap(), vec!["Models/Thing".to_string()]);
        assert_eq!(source.bundle(&directory, "Models/Thing").unwrap(), model_file);
        drop(source);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
