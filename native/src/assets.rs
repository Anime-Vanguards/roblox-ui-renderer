//! Incremental assets. The render thread only discovers requirements and polls completions;
//! resolution, glyph rasterization, file IO and image decoding belong to the worker.
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use crate::scene::SceneStore;
use serde_json::{Value, json};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct AssetId(pub String);

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct GlyphKey(pub String);

impl GlyphKey {
    pub fn new(font: u32, ppem: f64, glyph: u32, stroke: f64, style: Option<&str>) -> Self {
        let mut key = format!("{font}|{}|{glyph}|{}", number_key(ppem), number_key(stroke));
        if let Some(style) = style.filter(|style| !style.is_empty()) {
            key.push('|');
            key.push_str(style);
        }
        Self(key)
    }
}

fn number_key(value: f64) -> String {
    if value == 0.0 {
        "0".into()
    } else {
        value.to_string()
    }
}

#[derive(Clone, Debug)]
pub struct TextureAsset {
    pub id: AssetId,
    pub width: u32,
    pub height: u32,
    /// Straight, unpremultiplied RGBA8. Sampler/mipmap preparation owns premultiplication.
    pub rgba: Arc<[u8]>,
}

#[derive(Clone, Debug)]
pub struct Glyph {
    pub key: GlyphKey,
    pub page: AssetId,
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub left: i32,
    pub top: i32,
    /// A colour glyph's layers on the same page (the atlas row's `layers`), bottom first
    pub layers: Vec<crate::glyphs::AtlasLayer>,
}

#[derive(Clone, Debug)]
pub enum AssetEvent {
    TextureReady(AssetId),
    TextureEvicted(AssetId),
    Diagnostic(String),
    /// New cloud font files require a fresh runtime VM to shape with their metrics.
    FontsChanged,
}

struct Resident {
    texture: TextureAsset,
    used: u64,
}
struct Job {
    id: u64,
    images: Vec<String>,
    scene: Value,
    signatures: Vec<String>,
    reload: Vec<(AssetId, PathBuf)>,
    diagnostics: Option<Value>,
    generation: u64,
    manifest: Option<PathBuf>,
}
enum Completion {
    Texture(TextureAsset, PathBuf, u64),
    Atlas(AssetId, Value, u64),
    Failed(Option<AssetId>, String, u64),
    FontsChanged,
    Done {
        id: u64,
        images: Vec<String>,
        signatures: Vec<String>,
        success: bool,
        generation: u64,
        manifest: bool,
        font_request: Option<String>,
    },
}

pub struct AssetStore {
    requests: Option<mpsc::SyncSender<Job>>,
    /// The glyph worker's queue (glyph pages, reloads): apart from the helper's downloads
    glyph_requests: Option<mpsc::SyncSender<Job>>,
    /// Glyph-worker jobs in flight (their ids; `jobs` counts both workers')
    glyph_jobs: HashSet<u64>,
    completed: mpsc::Receiver<Completion>,
    child: Arc<Mutex<Option<Child>>>,
    textures: HashMap<AssetId, Resident>,
    files: HashMap<AssetId, PathBuf>,
    glyphs: HashMap<GlyphKey, Glyph>,
    atlases: HashMap<AssetId, Value>,
    signatures: HashMap<String, Vec<AssetId>>,
    pending_signatures: HashSet<String>,
    pending_images: HashSet<String>,
    failed_images: HashSet<String>,
    pending_reloads: HashSet<AssetId>,
    diagnostics: Vec<String>,
    bytes: usize,
    byte_limit: usize,
    clock: u64,
    next_job: u64,
    jobs: usize,
    font_requests: HashSet<String>,
    active_fonts: Value,
    generation: u64,
    manifest_job: Option<u64>,
    asset_revision: u64,
    active_images: HashSet<String>,
    viewport_last: HashMap<String, AssetId>,
    active_pages: HashSet<AssetId>,
    budget_blocked: HashSet<AssetId>,
    image_errors: HashMap<String, String>,
    active_generation: Arc<AtomicU64>,
    retry_after: HashMap<String, Instant>,
    /// The web build's in-process worker and its completion sender
    #[cfg(feature = "web")]
    web: Option<(Worker, Done)>,
    /// Short signature ids by exact glyph key (signature_of)
    signature_ids: rustc_hash::FxHashMap<Vec<u64>, String>,
    next_signature: u64,
    /// What each scene node needs (its glyph signature and images), kept while the node is the same
    /// value (its Arc): request_missing reads them instead of encoding every text node's glyphs again
    needs: rustc_hash::FxHashMap<crate::scene::NodeId, NodeNeeds>,
    /// What `needs` was computed with: the scale, the scene's image list and fonts (their Arcs)
    needs_inputs: Option<(u64, Arc<Vec<Value>>, Arc<Value>)>,
    /// Bumped by every completion and font change: request_missing's result can only change with it
    /// or with the scene's needs
    request_state: u64,
    /// The request_state of the last full request_missing pass, when that pass found nothing to request
    clean_at: Option<u64>,
}

/// A scene node's asset needs (AssetStore::needs), with the node they were computed from
struct NodeNeeds {
    /// Held weakly: the scene store moves a patched node out of its Arc when nothing else holds it
    node: std::sync::Weak<Value>,
    signature: Option<String>,
    images: Vec<String>,
}

impl AssetStore {
    pub fn new(repo: impl Into<PathBuf>, offline: bool) -> Self {
        let megabytes = std::env::var("RUR_NATIVE_ASSET_MB")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(512);
        Self::with_budget(repo, offline, megabytes.saturating_mul(1024 * 1024))
    }

    pub fn with_budget(repo: impl Into<PathBuf>, offline: bool, byte_limit: usize) -> Self {
        Self::with_timeout(repo, offline, byte_limit, Duration::from_secs(60))
    }

    fn with_timeout(
        repo: impl Into<PathBuf>,
        offline: bool,
        byte_limit: usize,
        response_timeout: Duration,
    ) -> Self {
        let (send, receive) = mpsc::sync_channel(4);
        let child = Arc::new(Mutex::new(None));
        let control = child.clone();
        let active_generation = Arc::new(AtomicU64::new(0));
        let worker_generation = active_generation.clone();
        let repo = repo.into();
        // Glyphs are rasterized on a worker of their own (it never starts the helper): a glyph job queued
        // behind an image download kept the display thread in wait_for_glyphs for 50 ms a frame until the
        // download finished
        let (glyph_send, glyph_receive) = mpsc::sync_channel::<Job>(4);
        #[cfg(not(feature = "web"))]
        let completed = {
            let (done, completed) = mpsc::sync_channel(8);
            {
                let (repo, done, generation) = (repo.clone(), done.clone(), active_generation.clone());
                std::thread::Builder::new()
                    .name("rur-glyphs".into())
                    .spawn(move || {
                        worker(
                            repo,
                            offline,
                            byte_limit.min(256 * 1024 * 1024),
                            glyph_receive,
                            done,
                            Arc::new(Mutex::new(None)),
                            generation,
                            response_timeout,
                        );
                    })
                    .expect("starting glyph worker");
            }
            std::thread::Builder::new()
                .name("rur-assets".into())
                .spawn(move || {
                    worker(
                        repo,
                        offline,
                        byte_limit.min(256 * 1024 * 1024),
                        receive,
                        done,
                        control,
                        worker_generation,
                        response_timeout,
                    );
                })
                .expect("starting asset worker");
            completed
        };
        // The web build has no threads: `send_job` runs a job at once, its completions wait in an
        // unbounded channel for the next poll
        #[cfg(feature = "web")]
        let (web, completed) = {
            drop(receive);
            drop(glyph_receive);
            let (done, completed) = mpsc::channel();
            let worker = Worker::new(repo, offline, byte_limit.min(256 * 1024 * 1024), control, worker_generation, response_timeout);
            (Some((worker, done)), completed)
        };
        Self {
            #[cfg(feature = "web")]
            web,
            requests: Some(send),
            glyph_requests: Some(glyph_send),
            glyph_jobs: HashSet::new(),
            completed,
            child,
            textures: HashMap::new(),
            files: HashMap::new(),
            glyphs: HashMap::new(),
            atlases: HashMap::new(),
            signatures: HashMap::new(),
            pending_signatures: HashSet::new(),
            pending_images: HashSet::new(),
            failed_images: HashSet::new(),
            pending_reloads: HashSet::new(),
            diagnostics: Vec::new(),
            bytes: 0,
            byte_limit,
            clock: 0,
            next_job: 1,
            jobs: 0,
            font_requests: HashSet::new(),
            active_fonts: Value::Null,
            generation: 0,
            manifest_job: None,
            asset_revision: 0,
            active_images: HashSet::new(),
            viewport_last: HashMap::new(),
            active_pages: HashSet::new(),
            budget_blocked: HashSet::new(),
            image_errors: HashMap::new(),
            active_generation,
            retry_after: HashMap::new(),
            signature_ids: Default::default(),
            next_signature: 0,
            needs: Default::default(),
            needs_inputs: None,
            request_state: 0,
            clean_at: None,
        }
    }

    /// Brings `needs` up to the scene: nodes whose value (Arc) changed are read again. Returns whether
    /// any node's needs changed (or nodes were added or removed).
    fn refresh_needs(&mut self, scene: &SceneStore) -> bool {
        let scale = scene.viewport().scale;
        let inputs_same = self.needs_inputs.as_ref().is_some_and(|(s, images, fonts)| {
            *s == scale.to_bits() && Arc::ptr_eq(images, scene.images_arc()) && Arc::ptr_eq(fonts, scene.fonts_arc())
        });
        let mut changed = !inputs_same;
        if !inputs_same {
            self.needs.clear();
            self.needs_inputs = Some((scale.to_bits(), scene.images_arc().clone(), scene.fonts_arc().clone()));
        }
        let mut count = 0;
        for (id, node) in scene.ordered_arcs() {
            count += 1;
            if self.needs.get(&id).is_some_and(|needs| std::ptr::eq(needs.node.as_ptr(), Arc::as_ptr(node))) {
                continue;
            }
            let signature = self.signature_of(node, scale);
            let images = node_images(node, scale, scene.images());
            if self
                .needs
                .get(&id)
                .is_none_or(|old| old.signature != signature || old.images != images)
            {
                changed = true;
            }
            self.needs.insert(id, NodeNeeds { node: Arc::downgrade(node), signature, images });
        }
        if self.needs.len() != count {
            self.needs.retain(|id, _| scene.node(*id).is_some());
            changed = true;
        }
        changed
    }

    /// Queue new requirements without waiting for IO. Moving text reuses its glyph-input signature.
    pub fn request_missing(&mut self, scene: &SceneStore) -> usize {
        self.clock = self.clock.wrapping_add(1);
        if self
            .needs_inputs
            .as_ref()
            .is_none_or(|(_, _, fonts)| !Arc::ptr_eq(fonts, scene.fonts_arc()))
        {
            self.activate_fonts(scene.fonts());
        }
        let needs_changed = self.refresh_needs(scene);
        // Nothing to request last time, and neither the scene's needs nor the assets changed since: the
        // same pass would request nothing again. The resident textures it uses are marked used.
        if !needs_changed && self.clean_at == Some(self.request_state) {
            for uri in &self.active_images {
                if let Some(texture) = self.textures.get_mut(&AssetId(uri.clone())) {
                    texture.used = self.clock;
                }
            }
            for page in &self.active_pages {
                if let Some(texture) = self.textures.get_mut(page) {
                    texture.used = self.clock;
                }
            }
            return 0;
        }
        self.clean_at = None;
        let state = self.request_state;
        if needs_changed {
            let active_images: HashSet<String> =
                self.needs.values().flat_map(|needs| needs.images.iter().cloned()).collect();
            if self.active_images != active_images {
                self.active_images = active_images;
                self.clear_budget_failures();
            }
        }
        self.viewport_last.retain(|slot, _| self.active_images.iter().any(|uri| viewport_slot(uri) == Some(slot.as_str())));
        self.files.retain(|id, _| viewport_slot(&id.0).is_none() || self.active_images.contains(&id.0) || self.viewport_last.values().any(|last| last == id));
        self.failed_images.retain(|uri| viewport_slot(uri).is_none() || self.active_images.contains(uri));
        self.image_errors.retain(|uri, _| viewport_slot(uri).is_none() || self.active_images.contains(uri));
        for uri in &self.active_images {
            if let Some(error) = self.image_errors.get(uri) {
                if !self.diagnostics.contains(error) && self.diagnostics.len() < 128 {
                    self.diagnostics.push(error.clone());
                }
            }
        }
        if self.manifest_job.is_some() || (self.glyph_jobs.len() >= 4 && self.helper_jobs() >= 4) {
            return 0;
        }
        let viewport = scene.viewport();
        let mut nodes = Vec::new();
        let mut signatures = Vec::new();
        let mut seen = HashSet::new();
        let mut uncovered = 0;
        let previous_pages = std::mem::take(&mut self.active_pages);
        let texts: Vec<(String, &Arc<Value>)> = scene
            .ordered_arcs()
            .filter_map(|(id, node)| Some((self.needs.get(&id)?.signature.clone()?, node)))
            .collect();
        for (signature, node) in texts {
            let node = node.as_ref();
            if !seen.insert(signature.clone()) {
                continue;
            }
            if let Some(pages) = self
                .signatures
                .get(&signature)
                .filter(|pages| pages.iter().all(|page| self.files.contains_key(page)))
            {
                self.active_pages.extend(pages.iter().cloned());
            } else {
                // Preserve partial page ownership while the worker is still producing this node's
                // other pages. Missing keys must not unpin its already completed glyphs.
                let requirements =
                    crate::render::text::glyph_requirements(node, scene.fonts(), viewport.scale);
                let mut pages = Vec::new();
                let mut covered = true;
                for alternatives in requirements {
                    if let Some(page) = alternatives.iter().find_map(|key| {
                        self.glyphs
                            .get(key)
                            .filter(|glyph| self.files.contains_key(&glyph.page))
                            .map(|glyph| glyph.page.clone())
                    }) {
                        self.active_pages.insert(page.clone());
                        pages.push(page);
                    } else {
                        covered = false;
                    }
                }
                if covered {
                    pages.sort_by(|a, b| a.0.cmp(&b.0));
                    pages.dedup();
                    self.signatures.insert(signature, pages);
                    continue;
                }
                uncovered += 1;
                if !self.pending_signatures.contains(&signature)
                    && self
                        .retry_after
                        .get(&signature)
                        .is_none_or(|at| Instant::now() >= *at)
                {
                    nodes.push(node.clone());
                    signatures.push(signature);
                }
            }
        }
        if previous_pages
            .iter()
            .any(|page| !self.active_pages.contains(page))
        {
            self.clear_budget_failures();
        }
        let mut images = Vec::new();
        let mut reload = Vec::new();
        for uri in &self.active_images {
            let id = AssetId(uri.clone());
            if let Some(texture) = self.textures.get_mut(&id) {
                texture.used = self.clock;
                continue;
            }
            if self.pending_images.contains(uri)
                || self.budget_blocked.contains(&id)
                || self.pending_reloads.contains(&id)
                || self.failed_images.contains(uri)
            {
                continue;
            }
            if let Some(file) = self.files.get(&id).filter(|_| viewport_slot(uri).is_none()) {
                reload.push((id, file.clone()));
            } else {
                // Coalesce moving viewports while the prior ROR job finishes.
                if viewport_slot(uri).is_none() || self.helper_jobs() == 0 {
                    images.push(uri.clone());
                }
            }
        }
        for page in &self.active_pages {
            if let Some(texture) = self.textures.get_mut(page) {
                texture.used = self.clock;
            } else if !self.pending_reloads.contains(page) && !self.budget_blocked.contains(page) {
                if let Some(file) = self.files.get(page) {
                    reload.push((page.clone(), file.clone()));
                }
            }
        }
        reload.sort_by(|a, b| a.0.0.cmp(&b.0.0));
        reload.dedup_by(|a, b| a.0 == b.0);
        if images.is_empty() && nodes.is_empty() && reload.is_empty() {
            if uncovered == 0 {
                self.clean_at = Some(state);
            }
            return 0;
        }
        // Glyphs (and reloads) go in a job of their own to the glyph worker, new images in a second one to
        // the helper's: text must not wait for an image download. Images draw nothing until they arrive,
        // as in Roblox.
        let mut count = 0;
        if (!nodes.is_empty() || !reload.is_empty()) && self.glyph_jobs.len() < 4 {
            let job = Job {
                id: self.next_job,
                images: Vec::new(),
                signatures: signatures.clone(),
                scene: json!({"fonts":scene.fonts(), "viewport":{"scale":viewport.scale}, "nodes":nodes}),
                reload: reload.clone(),
                diagnostics: None,
                generation: self.generation,
                manifest: None,
            };
            if !self.send_job(job) {
                return 0;
            }
            count += nodes.len() + reload.len();
            self.pending_signatures.extend(signatures);
            self.pending_reloads
                .extend(reload.into_iter().map(|(id, _)| id));
        }
        if !images.is_empty() && self.helper_jobs() < 4 {
            let job = Job {
                id: self.next_job,
                images: images.clone(),
                signatures: Vec::new(),
                scene: json!({"viewports": viewport_requests(scene.ordered_nodes(), &images), "viewportSlots": self.active_images.iter().filter_map(|uri| viewport_slot(uri)).collect::<Vec<_>>()}),
                reload: Vec::new(),
                diagnostics: None,
                generation: self.generation,
                manifest: None,
            };
            // Left unrequested when the queue is full: the next call asks again
            if self.send_job(job) {
                count += images.len();
                self.pending_images.extend(images);
            }
        }
        count
    }

    /// Glyph and reload jobs go to the glyph worker, image downloads to the helper's worker: a glyph job
    /// never waits behind a download (the display thread waits for glyphs: wait_for_glyphs)
    fn send_job(&mut self, job: Job) -> bool {
        #[cfg(feature = "web")]
        if let Some((worker, done)) = self.web.as_mut() {
            worker.run(job, done);
            self.next_job += 1;
            self.jobs += 1;
            return true;
        }
        let glyph = job.images.is_empty() && job.diagnostics.is_none() && job.manifest.is_none();
        let Some(sender) = (if glyph { &self.glyph_requests } else { &self.requests }) else {
            return false;
        };
        let id = job.id;
        if sender.try_send(job).is_err() {
            return false;
        }
        if glyph {
            self.glyph_jobs.insert(id);
        }
        self.next_job += 1;
        self.jobs += 1;
        true
    }
    /// A text node's glyph signature: a short id per distinct glyph_key (exact), assigned on first
    /// sight, so the per-frame discovery pass hashes and clones short strings, not the key's text
    fn signature_of(&mut self, node: &Value, scale: f64) -> Option<String> {
        let key = glyph_key(node, scale)?;
        if let Some(id) = self.signature_ids.get(&key) {
            return Some(id.clone());
        }
        // Bounded like `signatures`; ids are never reused, so a forgotten key only asks the worker again
        if self.signature_ids.len() >= 16_384 {
            self.signature_ids.clear();
        }
        let id = format!("g{}", self.next_signature);
        self.next_signature += 1;
        self.signature_ids.insert(key, id.clone());
        Some(id)
    }
    /// Jobs in flight on the helper's worker (images, fonts, manifests)
    fn helper_jobs(&self) -> usize {
        self.jobs.saturating_sub(self.glyph_jobs.len())
    }

    fn activate_fonts(&mut self, fonts: &Value) {
        if &self.active_fonts != fonts {
            self.request_state = self.request_state.wrapping_add(1);
            self.active_fonts = fonts.clone();
            self.generation = self.generation.wrapping_add(1);
            self.active_generation
                .store(self.generation, Ordering::Release);
            self.signatures.clear();
            self.active_pages.clear();
            self.clear_budget_failures();
            self.retry_after.clear();
            self.pending_signatures.clear();
            self.pending_images.clear();
            self.pending_reloads.clear();
            self.glyphs.clear();
            self.atlases.clear();
            self.asset_revision = self.asset_revision.wrapping_add(1);
        }
    }

    /// Switch diagnostic/glyph ownership while retaining immutable decoded image pixels.
    pub fn begin_scene(&mut self, scene: &SceneStore) {
        self.viewport_last.clear();
        self.needs.clear();
        self.needs_inputs = None;
        self.clean_at = None;
        self.active_fonts = Value::Null;
        self.activate_fonts(scene.fonts());
        self.diagnostics.clear();
        self.active_images = image_requirements(scene);
        for uri in &self.active_images {
            if let Some(error) = self.image_errors.get(uri) {
                if self.diagnostics.len() < 128 {
                    self.diagnostics.push(error.clone());
                }
            }
        }
    }

    /// Images the caller resolved itself (a render service sequence: its Resolver answers cached and
    /// local files in process and downloads the rest, so this store, offline there, never starts the
    /// Node helper): a file is decoded as its URI's texture when a scene shows it (the reload path), an
    /// error is that URI's failure.
    pub fn provide_images(&mut self, resolved: impl IntoIterator<Item = (String, Result<PathBuf, String>)>) {
        for (uri, resolution) in resolved {
            match resolution {
                Ok(file) => {
                    self.failed_images.remove(&uri);
                    if let Some(error) = self.image_errors.remove(&uri) {
                        self.diagnostics.retain(|message| message != &error);
                    }
                    self.files.insert(AssetId(uri), file);
                }
                Err(error) => {
                    let message = format!("Missing image {uri}: {error}");
                    if self.active_images.contains(&uri) && !self.diagnostics.contains(&message) && self.diagnostics.len() < 128 {
                        self.diagnostics.push(message.clone());
                    }
                    self.image_errors.insert(uri.clone(), message);
                    self.failed_images.insert(uri);
                }
            }
        }
        self.request_state = self.request_state.wrapping_add(1);
        self.clean_at = None;
    }

    /// Load a frozen scene's sibling manifest without resolving or rasterizing its assets again.
    pub fn load_manifest(&mut self, scene: &SceneStore, path: impl Into<PathBuf>) -> bool {
        self.begin_scene(scene);
        if self.jobs >= 4 || self.manifest_job.is_some() {
            return false;
        }
        if self.requests.is_none() {
            return false;
        }
        let scale = scene.viewport().scale;
        let mut unique = HashSet::new();
        for node in scene.ordered_nodes() {
            if let Some(signature) = self.signature_of(node, scale) {
                unique.insert(signature);
            }
        }
        let signatures: Vec<String> = unique.into_iter().collect();
        let Some(sender) = &self.requests else {
            return false;
        };
        let images: Vec<String> = self.active_images.iter().cloned().collect();
        let job = Job {
            id: self.next_job,
            images: images.clone(),
            scene: Value::Null,
            signatures: signatures.clone(),
            reload: Vec::new(),
            diagnostics: None,
            generation: self.generation,
            manifest: Some(path.into()),
        };
        if sender.try_send(job).is_err() {
            return false;
        }
        self.manifest_job = Some(self.next_job);
        self.next_job += 1;
        self.jobs += 1;
        self.pending_images.extend(images);
        self.pending_signatures.extend(signatures);
        true
    }

    /// Called for ready diagnostics, not every frame. Each missing-font set is attempted once.
    pub fn request_fonts(&mut self, diagnostics: &Value) -> bool {
        // (the web build downloads nothing: its fonts are the page bundle's)
        if cfg!(feature = "web") {
            return false;
        }
        let Some(missing) = diagnostics["missingFontAssets"]
            .as_array()
            .filter(|items| !items.is_empty())
        else {
            return false;
        };
        let key = serde_json::to_string(missing).unwrap_or_default();
        if self.font_requests.contains(&key) || self.jobs >= 4 {
            return false;
        }
        let Some(sender) = &self.requests else {
            return false;
        };
        let job = Job {
            id: self.next_job,
            images: Vec::new(),
            scene: Value::Null,
            signatures: Vec::new(),
            reload: Vec::new(),
            diagnostics: Some(diagnostics.clone()),
            generation: self.generation,
            manifest: None,
        };
        if sender.try_send(job).is_err() {
            return false;
        }
        self.font_requests.insert(key);
        self.next_job += 1;
        self.jobs += 1;
        true
    }

    pub fn poll(&mut self) -> Vec<AssetEvent> {
        let mut events = Vec::new();
        loop {
            let completion = match self.completed.try_recv() {
                Ok(completion) => completion,
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.requests.take();
                    self.jobs = 0;
                    self.glyph_requests.take();
                    self.glyph_jobs.clear();
                    self.pending_images.clear();
                    self.pending_signatures.clear();
                    self.pending_reloads.clear();
                    self.manifest_job = None;
                    break;
                }
            };
            self.handle(completion, &mut events);
        }
        events
    }

    /// Blocks up to `timeout` for the glyph jobs in flight (Roblox rasterizes glyphs synchronously,
    /// so text is never drawn without them): the frame after a text size first appears waits for its
    /// glyphs instead of drawing that text empty.
    pub fn wait_for_glyphs(&mut self, timeout: Duration) -> Vec<AssetEvent> {
        let deadline = Instant::now() + timeout;
        let mut events = Vec::new();
        while !self.pending_signatures.is_empty() {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            match self.completed.recv_timeout(deadline - now) {
                Ok(completion) => self.handle(completion, &mut events),
                Err(_) => break,
            }
        }
        events.extend(self.poll());
        events
    }

    fn handle(&mut self, completion: Completion, events: &mut Vec<AssetEvent>) {
        self.request_state = self.request_state.wrapping_add(1);
        match completion {
            Completion::FontsChanged => events.push(AssetEvent::FontsChanged),
            Completion::Texture(texture, file, generation) => {
                if generation != self.generation {
                    return;
                }
                self.pending_reloads.remove(&texture.id);
                self.failed_images.remove(&texture.id.0);
                if let Some(error) = self.image_errors.remove(&texture.id.0) {
                    self.diagnostics.retain(|message| message != &error);
                }
                // A saved scene may bind the same URI (or overwrite the same file) to new pixels.
                // Decode happened off-thread; preserve the old Arc only when pixels are identical.
                self.files.insert(texture.id.clone(), file.clone());
                if self.textures.get(&texture.id).is_some_and(|resident| {
                    resident.texture.width == texture.width
                        && resident.texture.height == texture.height
                        && resident.texture.rgba == texture.rgba
                }) {
                    return;
                }
                let size = texture.rgba.len();
                if size > self.byte_limit {
                    self.report(
                        "Texture exceeds the native asset memory budget".into(),
                        events,
                    );
                    return;
                }
                if let Some(slot) = viewport_slot(&texture.id.0) {
                    if let Some(previous) = self.viewport_last.insert(slot.to_owned(), texture.id.clone()) {
                        if previous != texture.id {
                            self.files.remove(&previous);
                            if let Some(old) = self.textures.remove(&previous) {
                                self.bytes -= old.texture.rgba.len();
                                events.push(AssetEvent::TextureEvicted(previous));
                            }
                        }
                    }
                }
                if let Some(previous) = self.textures.remove(&texture.id) {
                    self.bytes -= previous.texture.rgba.len();
                    self.asset_revision = self.asset_revision.wrapping_add(1);
                }
                while self.bytes + size > self.byte_limit {
                    let Some(oldest) = self
                        .textures
                        .iter()
                        .filter(|(id, _)| {
                            !self.active_images.contains(&id.0)
                                && !self.active_pages.contains(*id)
                                && !self.viewport_last.values().any(|last| last == *id)
                        })
                        .min_by_key(|(_, entry)| entry.used)
                        .map(|(id, _)| id.clone())
                    else {
                        break;
                    };
                    if let Some(old) = self.textures.remove(&oldest) {
                        self.bytes -= old.texture.rgba.len();
                    }
                    self.asset_revision = self.asset_revision.wrapping_add(1);
                    events.push(AssetEvent::TextureEvicted(oldest));
                }
                if self.bytes + size > self.byte_limit {
                    self.budget_blocked.insert(texture.id);
                    self.report(format!("Active scene assets exceed the native decoded texture budget ({} MiB); increase RUR_NATIVE_ASSET_MB", self.byte_limit / (1024 * 1024)), events);
                    return;
                }
                self.bytes += size;
                self.asset_revision = self.asset_revision.wrapping_add(1);
                self.files.insert(texture.id.clone(), file);
                events.push(AssetEvent::TextureReady(texture.id.clone()));
                self.textures.insert(
                    texture.id.clone(),
                    Resident {
                        texture,
                        used: self.clock,
                    },
                );
            }
            Completion::Atlas(id, atlas, generation) => {
                if generation != self.generation {
                    return;
                }
                self.active_pages.insert(id.clone());
                self.asset_revision = self.asset_revision.wrapping_add(1);
                if !self.atlases.contains_key(&id) && self.atlases.len() >= 128 {
                    let oldest = self
                        .atlases
                        .keys()
                        .filter(|page| *page != &id && !self.active_pages.contains(*page))
                        .min_by_key(|page| {
                            self.textures.get(*page).map_or(0, |entry| entry.used)
                        })
                        .cloned();
                    if let Some(oldest) = oldest {
                        self.atlases.remove(&oldest);
                        self.files.remove(&oldest);
                        self.glyphs.retain(|_, glyph| glyph.page != oldest);
                        self.signatures.retain(|_, pages| !pages.contains(&oldest));
                        if let Some(old) = self.textures.remove(&oldest) {
                            self.bytes -= old.texture.rgba.len();
                        }
                        events.push(AssetEvent::TextureEvicted(oldest));
                    } else {
                        self.report(
                            "Active scene exceeds the native glyph atlas page limit (128)"
                                .into(),
                            events,
                        );
                        return;
                    }
                }
                if let Some(rows) = atlas["glyphs"].as_array() {
                    for row in rows {
                        let Some(key) = row["key"].as_array() else {
                            return;
                        };
                        let key = GlyphKey(
                            key.iter()
                                .map(|part| {
                                    part.as_str().map(str::to_owned).unwrap_or_else(|| {
                                        part.as_f64().map(number_key).unwrap_or_default()
                                    })
                                })
                                .collect::<Vec<_>>()
                                .join("|"),
                        );
                        let u = |field: &str| row[field].as_u64().unwrap_or(0) as u32;
                        let i = |field: &str| row[field].as_i64().unwrap_or(0) as i32;
                        let layers = row["layers"]
                            .as_array()
                            .map(|layers| {
                                layers
                                    .iter()
                                    .map(|layer| {
                                        let u = |field: &str| layer[field].as_u64().unwrap_or(0) as u32;
                                        let i = |field: &str| layer[field].as_i64().unwrap_or(0) as i32;
                                        let channel = |k: usize| layer["color"][k].as_u64().unwrap_or(255).min(255) as u8;
                                        crate::glyphs::AtlasLayer {
                                            x: u("x"),
                                            y: u("y"),
                                            w: u("w"),
                                            h: u("h"),
                                            left: i("left"),
                                            top: i("top"),
                                            color: [channel(0), channel(1), channel(2), channel(3)],
                                            foreground: layer["foreground"].as_bool() == Some(true),
                                        }
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        self.glyphs.insert(
                            key.clone(),
                            Glyph {
                                key,
                                page: id.clone(),
                                x: u("x"),
                                y: u("y"),
                                w: u("w"),
                                h: u("h"),
                                left: i("left"),
                                top: i("top"),
                                layers,
                            },
                        );
                    }
                }
                self.atlases.insert(id, atlas);
            }
            Completion::Failed(id, message, generation) => {
                if generation != self.generation {
                    return;
                }
                let mut visible = true;
                if let Some(id) = id {
                    self.pending_reloads.remove(&id);
                    self.files.remove(&id);
                    if let Some(previous) = self.textures.remove(&id) {
                        self.bytes -= previous.texture.rgba.len();
                        self.asset_revision = self.asset_revision.wrapping_add(1);
                        events.push(AssetEvent::TextureEvicted(id.clone()));
                    }
                    if id.0.starts_with("glyph-page:") {
                        self.files.remove(&id);
                        self.signatures.retain(|_, pages| !pages.contains(&id));
                    } else {
                        visible = self.active_images.contains(&id.0);
                        self.image_errors.insert(id.0.clone(), message.clone());
                        self.failed_images.insert(id.0);
                    }
                }
                if visible {
                    self.report(message, events);
                }
            }
            Completion::Done {
                id,
                images,
                signatures,
                success,
                generation,
                manifest,
                font_request,
            } => {
                self.jobs = self.jobs.saturating_sub(1);
                self.glyph_jobs.remove(&id);
                if self.manifest_job == Some(id) {
                    self.manifest_job = None;
                }
                if !success {
                    if let Some(key) = font_request {
                        self.font_requests.remove(&key);
                    }
                }
                if generation == self.generation {
                    for image in images {
                        self.pending_images.remove(&image);
                    }
                    for signature in signatures {
                        self.pending_signatures.remove(&signature);
                        // Coverage is bound per node on the next discovery pass. A job's full
                        // page set can also contain unrelated nodes that later disappear.
                        if !success && !manifest {
                            self.retry_after
                                .insert(signature, Instant::now() + Duration::from_secs(5));
                        }
                    }
                }
                // Discovery state is bounded independently of decoded pixels. Clearing causes a
                // later cache lookup in the helper, not duplicate glyph rasterization.
                if self.signatures.len() > 16_384 {
                    self.signatures.clear();
                }
            }
        }
    }

    fn clear_budget_failures(&mut self) {
        self.budget_blocked.clear();
        self.diagnostics.retain(|message| {
            !message.starts_with("Active scene assets exceed the native decoded texture budget")
                && !message.starts_with("Active scene exceeds the native glyph atlas page limit")
        });
    }

    fn report(&mut self, message: String, events: &mut Vec<AssetEvent>) {
        if !self.diagnostics.contains(&message) {
            if self.diagnostics.len() == 128 {
                self.diagnostics.remove(0);
            }
            self.diagnostics.push(message.clone());
            events.push(AssetEvent::Diagnostic(message));
        }
    }

    pub fn image(&self, uri: &str) -> Option<&TextureAsset> {
        self.texture(&AssetId(uri.into()))
    }
    pub fn texture(&self, id: &AssetId) -> Option<&TextureAsset> {
        self.textures.get(id).or_else(|| viewport_slot(&id.0).and_then(|slot| self.viewport_last.get(slot)).and_then(|last| self.textures.get(last)))
            .map(|entry| &entry.texture)
    }
    pub fn glyph(&self, key: &GlyphKey) -> Option<&Glyph> {
        self.glyphs.get(key)
    }
    pub fn glyph_for(
        &self,
        font: u32,
        ppem: f64,
        glyph: u32,
        stroke: f64,
        style: Option<&str>,
    ) -> Option<&Glyph> {
        self.glyph(&GlyphKey::new(font, ppem, glyph, stroke, style))
    }
    pub fn atlas_metadata(&self, id: &AssetId) -> Option<&Value> {
        self.atlases.get(id)
    }
    pub fn diagnostics(&self) -> &[String] {
        &self.diagnostics
    }
    pub fn pending_count(&self) -> usize {
        self.jobs
    }
    pub fn resident_bytes(&self) -> usize {
        self.bytes
    }
    pub fn render_revision(&self) -> u64 {
        self.asset_revision
    }
}

impl Drop for AssetStore {
    fn drop(&mut self) {
        self.requests.take();
        self.glyph_requests.take();
        self.active_generation.store(u64::MAX, Ordering::Release);
        if let Ok(mut child) = self.child.lock() {
            if let Some(child) = child.as_mut() {
                stop_child(child);
            }
        }
    }
}

pub(crate) fn viewport_slot(uri: &str) -> Option<&str> {
    uri.strip_prefix("rur-viewport://")?.split_once('/').map(|(slot, _)| slot)
}

pub(crate) fn viewport_requests<'a>(nodes: impl Iterator<Item = &'a Value>, images: &[String]) -> Value {
    let mut requests = serde_json::Map::new();
    for node in nodes {
        if let Some(uri) = node["image"]["uri"].as_str() {
            if uri.starts_with("rur-viewport://") && images.iter().any(|image| image == uri) && node["viewport"].is_object() {
                requests.insert(uri.into(), node["viewport"].clone());
            }
        }
    }
    Value::Object(requests)
}

fn image_requirements(scene: &SceneStore) -> HashSet<String> {
    let scale = scene.viewport().scale;
    scene
        .ordered_nodes()
        .flat_map(|node| node_images(node, scale, scene.images()))
        .collect()
}

/// The images one node draws: its image and its scroll bars' (with the @2x variant the scene lists)
fn node_images(node: &Value, scale: f64, listed: &[Value]) -> Vec<String> {
    let mut images = Vec::new();
    if let Some(uri) = node["image"]["uri"].as_str().filter(|uri| !uri.is_empty()) {
        images.push(uri.to_owned());
    }
    for key in ["topImage", "midImage", "bottomImage"] {
        if let Some(uri) = node["scroll"][key].as_str().filter(|uri| !uri.is_empty()) {
            images.push(uri.to_owned());
            if scale > 1.0 && uri.starts_with("rbxasset://") && uri.ends_with(".png") {
                let high = format!("{}@2x.png", &uri[..uri.len() - 4]);
                if listed.iter().any(|image| image.as_str() == Some(&high)) {
                    images.push(high);
                }
            }
        }
    }
    images
}

/// Appends an exact encoding of a JSON value to `out` (an absent value encodes like null, as json!
/// writes it): a tag word, then the payload (numbers as their integer or float bits, strings as their
/// length and bytes, containers as their length and items)
fn encode_value(out: &mut Vec<u64>, value: Option<&Value>) {
    match value {
        None | Some(Value::Null) => out.push(0),
        Some(Value::Bool(value)) => out.extend([1, *value as u64]),
        Some(Value::Number(number)) => {
            if let Some(value) = number.as_i64() {
                out.extend([2, value as u64])
            } else if let Some(value) = number.as_u64() {
                out.extend([3, value])
            } else {
                out.extend([4, number.as_f64().unwrap_or(f64::NAN).to_bits()])
            }
        }
        Some(Value::String(value)) => {
            out.extend([5, value.len() as u64]);
            out.extend(value.as_bytes().chunks(8).map(|chunk| {
                let mut word = [0u8; 8];
                word[..chunk.len()].copy_from_slice(chunk);
                u64::from_le_bytes(word)
            }));
        }
        Some(Value::Array(items)) => {
            out.extend([6, items.len() as u64]);
            for item in items {
                encode_value(out, Some(item));
            }
        }
        Some(Value::Object(map)) => {
            out.extend([7, map.len() as u64]);
            for (key, item) in map {
                encode_value(out, Some(&Value::String(key.clone())));
                encode_value(out, Some(item));
            }
        }
    }
}

/// The glyph inputs of a text node, exactly: its glyphs (font, id, size, variant), weights, bold and
/// stroke settings, and the scale. Layout coordinates and colours cannot create a new glyph (the
/// atlas consumes only these); the active font table is a generation boundary, not repeated per node.
/// Encoded without building values: it runs over every text node whenever the scene changes (every
/// frame of a scroll: the moved rows are new nodes).
fn glyph_key(node: &Value, scale: f64) -> Option<Vec<u64>> {
    let text = node.get("text")?;
    let lines = text.get("lines")?.as_array()?;
    let glyphs = || {
        lines
            .iter()
            .filter_map(|line| line["glyphs"].as_array())
            .flatten()
            .filter_map(|glyph| glyph.as_array())
            .filter(|glyph| glyph.len() >= 4)
    };
    glyphs().next()?;
    let mut key = Vec::with_capacity(64);
    key.push(scale.to_bits());
    for glyph in glyphs() {
        for value in [glyph.first(), glyph.get(1), glyph.get(3), glyph.get(4)] {
            encode_value(&mut key, value);
        }
    }
    // Separates the glyph list from what follows (no value encoding starts with this word)
    key.push(u64::MAX);
    encode_value(&mut key, text.get("weight"));
    let stroke = |key: &mut Vec<u64>, value: Option<&Value>| match value.filter(|value| !value.is_null()) {
        Some(value) => {
            key.push(1);
            encode_value(key, value.get("thickness"));
            encode_value(key, value.get("joins"));
        }
        None => key.push(0),
    };
    for run in text["runs"].as_array().into_iter().flatten() {
        key.push(u64::MAX - 1);
        encode_value(&mut key, run.get("weight"));
        encode_value(&mut key, run.get("bold"));
        encode_value(&mut key, run["face"].get("weight"));
        stroke(&mut key, run.get("stroke"));
    }
    for value in node["strokes"].as_array().into_iter().flatten() {
        if value["mode"] == "Contextual" {
            key.push(u64::MAX - 2);
            stroke(&mut key, Some(value));
        }
    }
    Some(key)
}

/// glyph_key as text (tests compare signatures through it; the store uses signature_of)
#[cfg(test)]
fn glyph_signature(node: &Value, _fonts: &Value, scale: f64) -> Option<String> {
    glyph_key(node, scale).map(|key| signature_text(&key))
}
#[cfg(test)]
fn signature_text(key: &[u64]) -> String {
    let mut text = String::with_capacity(key.len() * 16);
    for word in key {
        use std::fmt::Write;
        let _ = write!(text, "{word:016x}");
    }
    text
}

/// Glyph atlas pages of one asset worker, rasterized in process (crate::glyphs; cli/native-assets.mjs
/// did this with the Python rasterizer before). Pages hold at most `PAGE_GLYPHS` glyphs; at most
/// `MAX_PAGES` pages are kept. A page's pixels are rebuilt from the rasterizer's bitmap cache when it
/// is sent again (or reloaded after an eviction), so the worker keeps no page pixels of its own.
struct GlyphPages {
    rasterizer: Option<crate::glyphs::Rasterizer>,
    worker: u64,
    next_page: u64,
    /// JSON of each font-table entry: a changed entry unbinds its font index's glyphs
    fonts: Vec<String>,
    /// atlas key (`fontIndex|ppem|glyphId|strokePx[|style]`) -> page
    glyphs: HashMap<String, AssetId>,
    /// insertion order (eviction takes the oldest unused page first)
    order: Vec<AssetId>,
    pages: HashMap<AssetId, GlyphPage>,
}

struct GlyphPage {
    entries: Vec<crate::glyphs::AtlasEntry>,
    placements: Vec<crate::glyphs::AtlasGlyph>,
    width: u32,
    height: u32,
    freetype: String,
}

/// A page sent to the store: its pixels and its `Completion::Atlas` value
struct PreparedPage {
    texture: TextureAsset,
    atlas: Value,
}

const PAGE_GLYPHS: usize = 512;
const MAX_PAGES: usize = 128;

impl GlyphPages {
    fn new() -> Self {
        static WORKERS: AtomicU64 = AtomicU64::new(1);
        Self {
            rasterizer: None,
            worker: WORKERS.fetch_add(1, Ordering::Relaxed),
            next_page: 1,
            fonts: Vec::new(),
            glyphs: HashMap::new(),
            order: Vec::new(),
            pages: HashMap::new(),
        }
    }

    fn rasterizer(&mut self) -> Result<&mut crate::glyphs::Rasterizer, String> {
        if self.rasterizer.is_none() {
            self.rasterizer = Some(crate::glyphs::Rasterizer::new().map_err(|error| error.to_string())?);
        }
        Ok(self.rasterizer.as_mut().unwrap())
    }

    fn remove_page(&mut self, id: &AssetId) {
        if let Some(page) = self.pages.remove(id) {
            for entry in &page.entries {
                let key = entry.key();
                if self.glyphs.get(&key) == Some(id) {
                    self.glyphs.remove(&key);
                }
            }
        }
        self.order.retain(|page| page != id);
    }

    /// The page's pixels, rebuilt from the rasterizer (cached bitmaps). None when the page is gone or its
    /// fonts changed on disk since it was built (the placements would no longer match).
    fn pixels(&mut self, id: &AssetId) -> Option<TextureAsset> {
        let entries = self.pages.get(id)?.entries.clone();
        let atlas = self.rasterizer().ok()?.build_atlas(&entries).ok()?;
        let page = self.pages.get(id)?;
        if atlas.width != page.width || atlas.height != page.height || atlas.glyphs != page.placements {
            return None;
        }
        Some(TextureAsset { id: id.clone(), width: atlas.width, height: atlas.height, rgba: atlas.rgba.into() })
    }

    /// Reload of an evicted page texture
    fn reload(&mut self, id: &AssetId) -> Result<TextureAsset, String> {
        match self.pixels(id) {
            Some(texture) => Ok(texture),
            None => {
                self.remove_page(id);
                Err(format!("Glyph page {} is no longer available", id.0))
            }
        }
    }

    fn atlas_value(&self, id: &AssetId, page: &GlyphPage) -> Value {
        let glyphs: Vec<Value> = page
            .placements
            .iter()
            .filter(|glyph| {
                let key = glyph_key_string(&glyph.key);
                self.glyphs.get(&key) == Some(id)
            })
            .map(|glyph| {
                let mut row = json!({"key":glyph.key,"x":glyph.x,"y":glyph.y,"w":glyph.w,"h":glyph.h,"left":glyph.left,"top":glyph.top});
                if !glyph.layers.is_empty() {
                    row["layers"] = Value::Array(glyph.layers.iter().map(crate::glyphs::AtlasLayer::json).collect());
                }
                row
            })
            .collect();
        json!({"id":id.0,"file":id.0,"width":page.width,"height":page.height,"freetype":page.freetype,"glyphs":glyphs})
    }

    /// cli/native-assets.mjs' glyph step: rasterizes the scene's glyphs that no page holds yet (pages of
    /// up to 512) and returns every page the scene's glyphs are on, each listing the glyphs it owns,
    /// plus the error that stopped rasterization, if any.
    fn prepare(&mut self, scene: &Value) -> (Vec<PreparedPage>, Option<String>) {
        if let Some(fonts) = scene["fonts"].as_array() {
            let fonts: Vec<String> = fonts.iter().map(Value::to_string).collect();
            for index in 0..self.fonts.len() {
                if fonts.get(index) == self.fonts.get(index) {
                    continue;
                }
                let prefix = format!("{index}|");
                self.glyphs.retain(|key, _| !key.starts_with(&prefix));
            }
            self.fonts = fonts;
        }
        let entries = crate::glyphs::atlas_entries(scene);
        let mut error = None;
        let mut built: HashMap<AssetId, TextureAsset> = HashMap::new();
        // A page whose font files changed is dropped and its glyphs rasterized again (second pass)
        let mut wanted: Vec<AssetId> = Vec::new();
        for _pass in 0..2 {
            let missing: Vec<&crate::glyphs::AtlasEntry> =
                entries.iter().filter(|entry| !self.glyphs.contains_key(&entry.key())).collect();
            for batch in missing.chunks(PAGE_GLYPHS) {
                let batch: Vec<crate::glyphs::AtlasEntry> = batch.iter().map(|entry| (*entry).clone()).collect();
                let atlas = match self.rasterizer().and_then(|rasterizer| rasterizer.build_atlas(&batch).map_err(|error| format!("{error:#}"))) {
                    Ok(atlas) => atlas,
                    Err(message) => {
                        error = Some(message);
                        break;
                    }
                };
                let id = AssetId(format!("glyph-page:{}:{}", self.worker, self.next_page));
                self.next_page += 1;
                for entry in &batch {
                    self.glyphs.insert(entry.key(), id.clone());
                }
                built.insert(id.clone(), TextureAsset { id: id.clone(), width: atlas.width, height: atlas.height, rgba: atlas.rgba.into() });
                self.pages.insert(
                    id.clone(),
                    GlyphPage { entries: batch, placements: atlas.glyphs, width: atlas.width, height: atlas.height, freetype: atlas.freetype },
                );
                self.order.push(id);
            }
            wanted.clear();
            for entry in &entries {
                if let Some(id) = self.glyphs.get(&entry.key()) {
                    if !wanted.contains(id) {
                        wanted.push(id.clone());
                    }
                }
            }
            let mut stale = false;
            for id in &wanted {
                if built.contains_key(id) {
                    continue;
                }
                match self.pixels(id) {
                    Some(texture) => {
                        built.insert(id.clone(), texture);
                    }
                    None => {
                        self.remove_page(id);
                        stale = true;
                    }
                }
            }
            if !stale || error.is_some() {
                break;
            }
        }
        wanted.retain(|id| self.pages.contains_key(id) && built.contains_key(id));
        // Keep the page table bounded. Native pixels already uploaded remain valid; an evicted page can
        // be rasterized again if a later request needs it.
        let mut oversized = false;
        while self.pages.len() > MAX_PAGES {
            let unused = self.order.iter().find(|id| !wanted.contains(id)).cloned();
            oversized |= unused.is_none();
            let oldest = unused.unwrap_or_else(|| self.order[0].clone());
            self.remove_page(&oldest);
        }
        if oversized {
            error = Some("A glyph request exceeds the native atlas page limit".into());
        }
        let pages = wanted
            .iter()
            .filter(|id| self.pages.contains_key(*id))
            .map(|id| PreparedPage { atlas: self.atlas_value(id, &self.pages[id]), texture: built.remove(id).unwrap() })
            .collect();
        (pages, error)
    }
}

/// An atlas JSON key `[fontIndex, ppem, glyphId, strokePx(, style)]` joined like `GlyphKey`
fn glyph_key_string(key: &Value) -> String {
    key.as_array()
        .into_iter()
        .flatten()
        .map(|part| part.as_str().map(str::to_owned).unwrap_or_else(|| part.as_f64().map(crate::glyphs::js_number_string).unwrap_or_default()))
        .collect::<Vec<_>>()
        .join("|")
}

fn start_helper(
    repo: &Path,
    offline: bool,
    control: &Arc<Mutex<Option<Child>>>,
) -> Result<
    (
        mpsc::SyncSender<Value>,
        mpsc::Receiver<Result<String, String>>,
    ),
    String,
> {
    let mut command = Command::new(std::env::var_os("RUR_NODE").unwrap_or_else(|| "node".into()));
    command
        .arg(crate::paths::asset_helper(repo)?)
        .current_dir(&repo)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if offline {
        command.arg("--offline");
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command
        .spawn()
        .map_err(|_| "Native assets need Node.js on PATH (or RUR_NODE)")?;
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    if let Ok(mut slot) = control.lock() {
        *slot = Some(child);
    }
    let (send, requests) = mpsc::sync_channel::<Value>(1);
    let (responses, receive) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("rur-asset-pipe".into())
        .spawn(move || {
            for request in requests {
                let response = (|| -> Result<String, String> {
                    serde_json::to_writer(&mut input, &request)
                        .map_err(|_| "Writing asset request")?;
                    input
                        .write_all(b"\n")
                        .and_then(|_| input.flush())
                        .map_err(|_| "Asset helper closed")?;
                    let mut line = String::new();
                    if output
                        .read_line(&mut line)
                        .map_err(|_| "Reading asset response")?
                        == 0
                    {
                        return Err("Asset helper exited".into());
                    }
                    Ok(line)
                })();
                let failed = response.is_err();
                if responses.send(response).is_err() || failed {
                    break;
                }
            }
        })
        .map_err(|_| "Starting asset pipe worker")?;
    Ok((send, receive))
}

/// The web build's image map: RUR_WEB_IMAGES names a JSON file of {uri: file | {"error": text}}
/// (tools/web/bundle.mjs writes the Node asset helper's answers for every image the page can show)
#[cfg(feature = "web")]
fn web_images() -> &'static serde_json::Map<String, Value> {
    static IMAGES: std::sync::OnceLock<serde_json::Map<String, Value>> = std::sync::OnceLock::new();
    IMAGES.get_or_init(|| {
        std::env::var("RUR_WEB_IMAGES")
            .ok()
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .and_then(|value| value.as_object().cloned())
            .unwrap_or_default()
    })
}

/// The worker's end of the completion channel: bounded on the desktop (the worker waits for the render
/// thread), unbounded in the web build, whose jobs run synchronously inside `send_job`
#[cfg(not(feature = "web"))]
type Done = mpsc::SyncSender<Completion>;
#[cfg(feature = "web")]
type Done = mpsc::Sender<Completion>;

#[cfg(not(feature = "web"))]
fn worker(
    repo: PathBuf,
    offline: bool,
    byte_limit: usize,
    receive: mpsc::Receiver<Job>,
    done: Done,
    control: Arc<Mutex<Option<Child>>>,
    active_generation: Arc<AtomicU64>,
    response_timeout: Duration,
) {
    let mut worker = Worker::new(repo, offline, byte_limit, control, active_generation, response_timeout);
    for job in receive {
        if !worker.run(job, &done) {
            break;
        }
    }
    worker.stop();
}

/// What the asset worker keeps between jobs: the glyph pages it rasterized and the Node helper
/// (the web build: a local resolver over the page's bundled files instead of the helper)
struct Worker {
    repo: PathBuf,
    offline: bool,
    byte_limit: usize,
    control: Arc<Mutex<Option<Child>>>,
    active_generation: Arc<AtomicU64>,
    response_timeout: Duration,
    helper: Option<(mpsc::SyncSender<Value>, mpsc::Receiver<Result<String, String>>)>,
    glyphs: GlyphPages,
    resolver: crate::resolve::Resolver,
}

impl Worker {
    fn new(
        repo: PathBuf,
        offline: bool,
        byte_limit: usize,
        control: Arc<Mutex<Option<Child>>>,
        active_generation: Arc<AtomicU64>,
        response_timeout: Duration,
    ) -> Self {
        Self {
            // The web build has no helper and never downloads, so its resolver is always offline
            resolver: crate::resolve::Resolver::new(repo.clone(), cfg!(feature = "web") || offline),
            repo,
            offline,
            byte_limit,
            control,
            active_generation,
            response_timeout,
            helper: None,
            glyphs: GlyphPages::new(),
        }
    }

    #[cfg(not(feature = "web"))]
    fn stop(&mut self) {
        drop(self.helper.take());
        if let Ok(mut child) = self.control.lock() {
            if let Some(child) = child.as_mut() {
                stop_child(child);
            }
        }
    }

    /// Runs one job, sending its completions; false when the asset store is gone
    fn run(&mut self, job: Job, done: &Done) -> bool {
        let Worker { repo, offline, byte_limit, control, active_generation, response_timeout, helper, glyphs, .. } = self;
        let (repo, offline, byte_limit, response_timeout) = (&*repo, *offline, *byte_limit, *response_timeout);
        let resolver = &mut self.resolver;
        // Set while the Node helper owns the job: its failures restart the helper, glyph failures do not
        let mut helper_fault = false;
        // (the web build returns before the helper part)
        #[cfg_attr(feature = "web", allow(unreachable_code))]
        let result = (|| -> Result<Vec<AssetId>, String> {
            if job.generation != active_generation.load(Ordering::Acquire) {
                return Err("Asset job cancelled by scene change".into());
            }
            if let Some(path) = &job.manifest {
                return load_manifest(path, &job.images, job.generation, byte_limit, &done);
            }
            for (id, file) in &job.reload {
                let texture = if glyphs.pages.contains_key(id) {
                    glyphs.reload(id)
                } else {
                    decode_texture(id.clone(), file, byte_limit)
                };
                match texture {
                    Ok(texture) => {
                        done.send(Completion::Texture(texture, file.clone(), job.generation))
                            .map_err(|_| "Asset store closed")?;
                    }
                    Err(error) => {
                        done.send(Completion::Failed(Some(id.clone()), error, job.generation))
                            .map_err(|_| "Asset store closed")?;
                    }
                }
            }
            // Glyphs: rasterized here, in process
            let mut pages = Vec::new();
            let mut glyph_error = None;
            if !job.signatures.is_empty() && !job.scene.is_null() {
                let (prepared, error) = glyphs.prepare(&job.scene);
                glyph_error = error;
                for page in prepared {
                    let id = page.texture.id.clone();
                    if page.texture.rgba.len() > byte_limit {
                        glyph_error.get_or_insert_with(|| "Glyph page exceeds the native asset memory budget".into());
                        continue;
                    }
                    // Not a file: the page id stands for the worker's page (reloads rebuild it)
                    done.send(Completion::Texture(page.texture, PathBuf::from(&id.0), job.generation))
                        .map_err(|_| "Asset store closed")?;
                    done.send(Completion::Atlas(id.clone(), page.atlas, job.generation))
                        .map_err(|_| "Asset store closed")?;
                    pages.push(id);
                }
            }
            if job.images.is_empty() && job.diagnostics.is_none() {
                return match glyph_error {
                    Some(error) => Err(error),
                    None => Ok(pages),
                };
            }
            // Without the Node helper (the web build, or a binary outside a checkout) images are files
            // at their local paths (the asset cache, the Studio install), resolved here. The web
            // build's cloud fonts come with its bundle; a binary outside a checkout only reports the
            // fonts it is missing.
            if cfg!(feature = "web") || !crate::paths::has_asset_helper(repo) {
                if let Some(diagnostics) = job.diagnostics.as_ref().filter(|_| !cfg!(feature = "web")) {
                    let _ = resolver.fetch_fonts(diagnostics);
                }
                if job.diagnostics.is_none() {
                    // The bundle's answers (RUR_WEB_IMAGES: {uri: file | {error}}, what the Node
                    // helper answered when the page was bundled), else the local rules
                    #[cfg(feature = "web")]
                    let bundled = web_images();
                    #[cfg(not(feature = "web"))]
                    let bundled = Value::Null;
                    let mut resolved = resolver.resolve_all(&job.images.iter().filter(|uri| bundled.get(uri.as_str()).is_none()).cloned().collect::<Vec<_>>());
                    for uri in &job.images {
                        match bundled.get(uri.as_str()) {
                            Some(Value::String(file)) => {
                                resolved.insert(uri.clone(), Ok(PathBuf::from(file)));
                            }
                            Some(entry) => {
                                resolved.insert(uri.clone(), Err(entry["error"].as_str().unwrap_or("unavailable").to_owned()));
                            }
                            None => {}
                        }
                    }
                    for uri in &job.images {
                        let id = AssetId(uri.clone());
                        let completion = match resolved.get(uri) {
                            Some(Ok(file)) => match decode_texture(id.clone(), file, byte_limit) {
                                Ok(texture) => Completion::Texture(texture, file.clone(), job.generation),
                                Err(error) => Completion::Failed(Some(id), error, job.generation),
                            },
                            Some(Err(error)) => Completion::Failed(Some(id), format!("Missing image {uri}: {error}"), job.generation),
                            None => Completion::Failed(Some(id), format!("Missing image {uri}: unresolved"), job.generation),
                        };
                        done.send(completion).map_err(|_| "Asset store closed")?;
                    }
                }
                return match glyph_error {
                    Some(error) => Err(error),
                    None => Ok(pages),
                };
            }
            // Images and cloud fonts: the Node helper (cli/native-assets.mjs)
            helper_fault = true;
            let request = if let Some(diagnostics) = &job.diagnostics {
                json!({"id":job.id,"command":"fonts","diagnostics":diagnostics})
            } else {
                json!({"id":job.id,"images":job.images,"viewports":job.scene["viewports"],"viewportSlots":job.scene["viewportSlots"]})
            };
            if helper.is_some() {
                let exited = control
                    .lock()
                    .map(|mut slot| {
                        slot.as_mut()
                            .is_none_or(|child| !matches!(child.try_wait(), Ok(None)))
                    })
                    .unwrap_or(true);
                if exited {
                    *helper = None;
                }
            }
            if helper.is_none() {
                *helper = Some(start_helper(repo, offline, control)?);
            }
            let (input, output) = helper.as_mut().unwrap();
            input
                .try_send(request)
                .map_err(|_| "Asset helper request channel closed")?;
            let deadline = Instant::now() + response_timeout;
            let line = loop {
                if job.generation != active_generation.load(Ordering::Acquire) {
                    return Err("Asset job cancelled by scene change".into());
                }
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err("Asset helper timed out".into());
                }
                match output.recv_timeout(remaining.min(Duration::from_millis(50))) {
                    Ok(line) => break line?,
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        return Err("Asset helper exited".into());
                    }
                }
            };
            let response: Value =
                serde_json::from_str(&line).map_err(|_| "Invalid asset helper response")?;
            if let Some(error) = response["error"].as_str() {
                return Err(error.to_owned());
            }
            if response["id"].as_u64() != Some(job.id) {
                return Err("Asset response order mismatch".into());
            }
            if response["fontsChanged"] == true {
                done.send(Completion::FontsChanged)
                    .map_err(|_| "Asset store closed")?;
            }
            for image in response["images"].as_array().into_iter().flatten() {
                for warning in image["warnings"].as_array().into_iter().flatten().filter_map(Value::as_str) {
                    done.send(Completion::Failed(None, format!("ViewportFrame: {warning}"), job.generation))
                        .map_err(|_| "Asset store closed")?;
                }
                let Some(uri) = image["uri"].as_str() else {
                    continue;
                };
                let id = AssetId(uri.into());
                if let Some(file) = image["file"].as_str() {
                    match decode_texture(id.clone(), Path::new(file), byte_limit) {
                        Ok(texture) => {
                            done.send(Completion::Texture(texture, file.into(), job.generation))
                                .map_err(|_| "Asset store closed")?;
                        }
                        Err(error) => {
                            done.send(Completion::Failed(Some(id), error, job.generation))
                                .map_err(|_| "Asset store closed")?;
                        }
                    }
                } else {
                    done.send(Completion::Failed(
                        Some(id),
                        format!(
                            "Missing image {uri}: {}",
                            image["error"].as_str().unwrap_or("unavailable")
                        ),
                        job.generation,
                    ))
                    .map_err(|_| "Asset store closed")?;
                }
            }
            helper_fault = false;
            match glyph_error {
                Some(error) => Err(error),
                None => Ok(pages),
            }
        })();
        let success = result.is_ok();
        if helper_fault && job.manifest.is_none() {
            *helper = None;
            if let Ok(mut slot) = control.lock() {
                if let Some(mut child) = slot.take() {
                    stop_child(&mut child);
                }
            }
        }
        if let Err(error) = result {
            let _ = done.send(Completion::Failed(None, error, job.generation));
        }
        if done
            .send(Completion::Done {
                id: job.id,
                images: job.images,
                signatures: job.signatures,
                success,
                generation: job.generation,
                manifest: job.manifest.is_some(),
                font_request: job
                    .diagnostics
                    .as_ref()
                    .and_then(|value| value["missingFontAssets"].as_array())
                    .map(|missing| serde_json::to_string(missing).unwrap_or_default()),
            })
            .is_err()
        {
            return false;
        }
        true
    }
}

fn stop_child(child: &mut Child) {
    if matches!(child.try_wait(), Ok(Some(_))) {
        return;
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = Command::new("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .creation_flags(0x08000000)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(unix)]
    {
        let _ = Command::new("kill")
            .args(["-KILL", "--", &format!("-{}", child.id())])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn load_manifest(
    path: &Path,
    required_images: &[String],
    generation: u64,
    byte_limit: usize,
    done: &Done,
) -> Result<Vec<AssetId>, String> {
    let manifest = if path.is_dir() {
        path.join("manifest.json")
    } else if path.file_name().is_some_and(|name| name == "manifest.json") {
        path.to_owned()
    } else {
        path.parent()
            .unwrap_or(Path::new("."))
            .join("manifest.json")
    };
    let directory = manifest.parent().unwrap_or(Path::new("."));
    let value: Value = serde_json::from_slice(
        &std::fs::read(&manifest).map_err(|_| "Saved scene has no readable manifest.json")?,
    )
    .map_err(|_| "Invalid saved scene manifest")?;
    if let Some(images) = value["images"].as_object() {
        for (uri, entry) in images {
            if !required_images.contains(uri) {
                continue;
            }
            let id = AssetId(uri.clone());
            if let Some(src) = entry["src"].as_str() {
                let file = directory.join(src);
                match decode_texture(id.clone(), &file, byte_limit) {
                    Ok(texture) => {
                        done.send(Completion::Texture(texture, file, generation))
                            .map_err(|_| "Asset store closed")?;
                    }
                    Err(error) => {
                        done.send(Completion::Failed(Some(id), error, generation))
                            .map_err(|_| "Asset store closed")?;
                    }
                }
            } else if let Some(error) = entry["error"].as_str() {
                done.send(Completion::Failed(
                    Some(id),
                    format!("Missing image {uri}: {error}"),
                    generation,
                ))
                .map_err(|_| "Asset store closed")?;
            }
        }
    }
    let mut pages = Vec::new();
    if let Some(src) = value["atlas"]["src"].as_str() {
        let file = directory.join(src);
        let id = AssetId(format!("glyph-page:manifest:{}", file.to_string_lossy()));
        let texture = decode_texture(id.clone(), &file, byte_limit)?;
        let mut atlas = value["atlas"].clone();
        if atlas["glyphs"].is_null() {
            let metadata = directory.join(atlas["json"].as_str().unwrap_or("glyphs.json"));
            atlas = serde_json::from_slice(
                &std::fs::read(metadata).map_err(|_| "Cannot read saved glyph metadata")?,
            )
            .map_err(|_| "Invalid saved glyph metadata")?;
        }
        done.send(Completion::Texture(texture, file, generation))
            .map_err(|_| "Asset store closed")?;
        done.send(Completion::Atlas(id.clone(), atlas, generation))
            .map_err(|_| "Asset store closed")?;
        pages.push(id);
    }
    Ok(pages)
}

/// Decoded images kept by a process that draws many scenes (the render service, story_render.rs),
/// by file path, size and modification time: an unchanged file is not decoded again, and its pixels
/// keep one allocation, so the renderer's caches keyed by that allocation hit across scenes. Off
/// unless [`reuse_decoded_images`] turned it on (the desktop app's asset budget stays the only owner).
struct DecodedImages {
    budget: usize,
    bytes: usize,
    clock: u64,
    entries: HashMap<PathBuf, DecodedImage>,
}

struct DecodedImage {
    stamp: (u64, Option<std::time::SystemTime>),
    width: u32,
    height: u32,
    rgba: Arc<[u8]>,
    used: u64,
}

static DECODED: std::sync::OnceLock<Mutex<DecodedImages>> = std::sync::OnceLock::new();

/// The image files a scene's assets/ entries link to (story_render.rs links every run's images into its
/// own output directory): an entry is decoded and kept under the file it links to, so a scene written to
/// a new directory reuses the pixels, and with them the renderer's textures, instead of decoding again.
static LINKED: std::sync::OnceLock<Mutex<HashMap<PathBuf, PathBuf>>> = std::sync::OnceLock::new();
const MAX_LINKED: usize = 1 << 16;

/// Records that `link` (an output directory's assets/ entry) holds the content of `source`
pub fn note_linked_image(link: &Path, source: &Path) {
    if DECODED.get().is_none() {
        return;
    }
    if let Ok(mut linked) = LINKED.get_or_init(Default::default).lock() {
        if linked.len() >= MAX_LINKED {
            linked.clear();
        }
        linked.insert(link.to_path_buf(), source.to_path_buf());
    }
}

fn linked_source(file: &Path) -> Option<PathBuf> {
    LINKED.get()?.lock().ok()?.get(file).cloned()
}

/// Keeps up to `budget` bytes of decoded images between scenes ([`DecodedImages`]).
pub fn reuse_decoded_images(budget: usize) {
    let _ = DECODED.set(Mutex::new(DecodedImages { budget, bytes: 0, clock: 0, entries: HashMap::new() }));
}

/// Empties the decoded image cache (the render service after an idle stretch)
pub fn clear_decoded_images() {
    if let Some(cache) = DECODED.get() {
        if let Ok(mut cache) = cache.lock() {
            cache.entries = HashMap::new();
            cache.bytes = 0;
        }
    }
    if let Some(linked) = LINKED.get() {
        if let Ok(mut linked) = linked.lock() {
            *linked = HashMap::new();
        }
    }
}

/// (bytes, entries) the decoded image cache holds
pub fn decoded_image_memory() -> (usize, usize) {
    DECODED.get().and_then(|cache| cache.lock().ok().map(|cache| (cache.bytes, cache.entries.len()))).unwrap_or((0, 0))
}

pub fn decode_texture(id: AssetId, file: &Path, byte_limit: usize) -> Result<TextureAsset, String> {
    let Some(cache) = DECODED.get() else {
        return decode_file(id, file, byte_limit);
    };
    let source = linked_source(file);
    let file = source.as_deref().unwrap_or(file);
    let metadata = std::fs::metadata(file).map_err(|_| format!("Cannot read image {}", id.0))?;
    let stamp = (metadata.len(), metadata.modified().ok());
    {
        let mut cache = cache.lock().map_err(|_| "Decoded image cache poisoned")?;
        cache.clock += 1;
        let clock = cache.clock;
        if let Some(entry) = cache.entries.get_mut(file).filter(|entry| entry.stamp == stamp && entry.rgba.len() <= byte_limit) {
            entry.used = clock;
            return Ok(TextureAsset { id, width: entry.width, height: entry.height, rgba: entry.rgba.clone() });
        }
    }
    let texture = decode_file(id, file, byte_limit)?;
    let mut cache = cache.lock().map_err(|_| "Decoded image cache poisoned")?;
    if texture.rgba.len() <= cache.budget / 4 {
        if let Some(old) = cache.entries.remove(file) {
            cache.bytes -= old.rgba.len();
        }
        while cache.bytes + texture.rgba.len() > cache.budget {
            let Some(oldest) = cache.entries.iter().min_by_key(|(_, entry)| entry.used).map(|(path, _)| path.clone()) else { break };
            let removed = cache.entries.remove(&oldest).unwrap();
            cache.bytes -= removed.rgba.len();
        }
        cache.bytes += texture.rgba.len();
        let used = cache.clock;
        cache.entries.insert(file.to_path_buf(), DecodedImage { stamp, width: texture.width, height: texture.height, rgba: texture.rgba.clone(), used });
    }
    Ok(texture)
}

fn decode_file(id: AssetId, file: &Path, byte_limit: usize) -> Result<TextureAsset, String> {
    let bytes = std::fs::read(file).map_err(|_| format!("Cannot read image {}", id.0))?;
    let svg = file
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"))
        || bytes
            .iter()
            .take(256)
            .copied()
            .collect::<Vec<_>>()
            .windows(4)
            .any(|part| part == b"<svg");
    let (width, height, rgba) = if svg {
        let tree = resvg::usvg::Tree::from_data(&bytes, &resvg::usvg::Options::default())
            .map_err(|_| "Invalid SVG image")?;
        let size = tree.size().to_int_size();
        if u64::from(size.width()) * u64::from(size.height()) * 4 > byte_limit as u64 {
            return Err("SVG exceeds the native asset memory budget".into());
        }
        let mut pixels = resvg::tiny_skia::Pixmap::new(size.width(), size.height())
            .ok_or("Invalid SVG dimensions")?;
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::identity(),
            &mut pixels.as_mut(),
        );
        let mut rgba = pixels.take();
        for pixel in rgba.chunks_exact_mut(4) {
            if pixel[3] != 0 {
                for channel in 0..3 {
                    pixel[channel] = ((u32::from(pixel[channel]) * 255 + u32::from(pixel[3]) / 2)
                        / u32::from(pixel[3]))
                    .min(255) as u8;
                }
            }
        }
        (size.width(), size.height(), rgba)
    } else {
        let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|_| "Unknown image format")?;
        let mut limits = image::Limits::default();
        limits.max_alloc = Some(byte_limit as u64);
        limits.max_image_width = Some(16384);
        limits.max_image_height = Some(16384);
        reader.limits(limits);
        let image = reader
            .decode()
            .map_err(|error| format!("Cannot decode image {}: {error}", id.0))?
            .into_rgba8();
        (image.width(), image.height(), image.into_raw())
    };
    if rgba.len() > byte_limit {
        return Err("Image exceeds the native asset memory budget".into());
    }
    Ok(TextureAsset {
        id,
        width,
        height,
        rgba: rgba.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moving_viewports_keep_completed_pixels_and_coalesce_requests() {
        let mut assets = AssetStore::new(std::env::temp_dir().join("rur-no-helper"), true);
        let (sender, receiver) = mpsc::sync_channel(8);
        assets.requests = Some(sender);
        let mut scene = SceneStore::default();
        let uri = |frame| format!("rur-viewport://42/{frame:064x}");
        scene.apply(&json!({"viewport":{},"nodes":[{"id":42,"image":{"uri":uri(1)},"viewport":{"version":1}}],"images":[],"fonts":[],"layers":[]})).unwrap();
        assets.begin_scene(&scene);
        assert_eq!(assets.request_missing(&scene), 1);
        let first = receiver.try_recv().unwrap();
        for frame in 2..20 {
            scene.apply(&json!({"delta":{"nodes":[{"id":42,"image":{"uri":uri(frame)},"viewport":{"version":1}}]}})).unwrap();
            assert_eq!(assets.request_missing(&scene), 0, "do not queue obsolete poses");
        }
        let mut events = Vec::new();
        assets.handle(Completion::Texture(TextureAsset { id: AssetId(uri(1)), width: 1, height: 1, rgba: Arc::from([255,0,0,255]) }, "viewport.png".into(), first.generation), &mut events);
        assets.handle(Completion::Done { id:first.id, images:first.images, signatures:vec![], success:true, generation:first.generation, manifest:false, font_request:None }, &mut events);
        assert_eq!(assets.image(&uri(19)).unwrap().rgba.as_ref(), &[255,0,0,255]);
        assert_eq!(assets.request_missing(&scene), 1);
        assert_eq!(receiver.try_recv().unwrap().images, vec![uri(19)]);
        assets.handle(Completion::Texture(TextureAsset { id: AssetId(uri(19)), width: 1, height: 1, rgba: Arc::from([0,255,0,255]) }, "viewport.png".into(), first.generation), &mut events);
        assert_eq!(assets.image(&uri(19)).unwrap().rgba.as_ref(), &[0,255,0,255]);
        assert!(!assets.files.contains_key(&AssetId(uri(1))), "overwritten files cannot serve obsolete hashes");
        assert_eq!(assets.textures.len(), 1, "one completed pose per viewport");
    }
    #[test]
    fn glyph_discovery_ignores_placement_but_tracks_style() {
        let mut node = json!({"text":{"weight":400,"lines":[{"x":10,"glyphs":[[0,42,2,18]]}]}});
        let first = glyph_signature(&node, &json!([]), 1.5);
        node["text"]["lines"][0]["x"] = json!(999);
        node["text"]["lines"][0]["glyphs"][0][2] = json!(300);
        assert_eq!(first, glyph_signature(&node, &json!([]), 1.5));
        node["text"]["weight"] = json!(700);
        assert_ne!(first, glyph_signature(&node, &json!([]), 1.5));
        node["strokes"] = json!([{"mode":"Contextual","thickness":1,"color":[1,0,0]}]);
        let stroked = glyph_signature(&node, &json!([]), 1.5);
        node["strokes"][0]["color"] = json!([0, 1, 0]);
        assert_eq!(stroked, glyph_signature(&node, &json!([]), 1.5));
        node["strokes"][0]["thickness"] = json!(2);
        assert_ne!(stroked, glyph_signature(&node, &json!([]), 1.5));
        assert_eq!(
            GlyphKey::new(2, 18.0, 42, -0.0, Some("miter+b128")).0,
            "2|18|42|0|miter+b128"
        );
    }

    /// A TrueType font for the glyph tests (RUR_TEST_FONT, else a system font); None skips them.
    fn test_font() -> Option<String> {
        [
            std::env::var("RUR_TEST_FONT").ok(),
            Some("C:/Windows/Fonts/arial.ttf".into()),
            Some("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf".into()),
            Some("/System/Library/Fonts/Supplemental/Arial.ttf".into()),
        ]
        .into_iter()
        .flatten()
        .find(|file| Path::new(file).is_file())
    }

    fn text_scene(fonts: Value, glyphs: Value) -> Value {
        json!({"fonts":fonts,"viewport":{"scale":1.5},"nodes":[{"text":{"weight":400,"lines":[{"glyphs":glyphs}]}}]})
    }

    fn keys(page: &PreparedPage) -> Vec<Value> {
        page.atlas["glyphs"].as_array().unwrap().iter().map(|glyph| glyph["key"].clone()).collect()
    }

    /// The glyph step of the former cli/native-assets.mjs (tools/tests/native-assets.mjs' glyph checks)
    #[test]
    fn glyph_pages_rasterize_incrementally_and_keep_current_bindings() {
        let mut pages = GlyphPages::new();
        let (missing, error) = pages.prepare(&text_scene(
            json!([{"file":std::env::temp_dir().join("missing-font.ttf").to_string_lossy(),"weight":400}]),
            json!([[0, 41, 0, 18]]),
        ));
        assert!(missing.is_empty());
        assert!(error.unwrap_or_default().contains("missing-font.ttf"));
        let Some(font) = test_font() else {
            eprintln!("glyph page checks skipped (set RUR_TEST_FONT)");
            return;
        };
        let mut pages = GlyphPages::new();
        let font_entry = json!({"file":font,"weight":400,"ascender":1900,"descender":-500,"unitsPerEm":2048});
        let scene = |glyph: u32| text_scene(json!([font_entry]), json!([[0, glyph, 0, 18]]));
        let (first, error) = pages.prepare(&scene(41));
        assert!(error.is_none(), "{error:?}");
        assert_eq!(first.len(), 1);
        assert_eq!(keys(&first[0]), [json!([0, 18, 41, 0])]);
        assert!(first[0].atlas["glyphs"][0]["w"].as_u64().unwrap() > 0);
        // Same pixels as a one-off atlas of the same entries
        let direct = crate::glyphs::Rasterizer::new()
            .unwrap()
            .build_atlas(&crate::glyphs::atlas_entries(&scene(41)))
            .unwrap();
        assert_eq!(&*first[0].texture.rgba, &direct.rgba[..]);
        let (repeated, _) = pages.prepare(&scene(41));
        assert_eq!(repeated[0].texture.id, first[0].texture.id, "unchanged glyph requirements reuse the atlas");
        assert_eq!(repeated[0].texture.rgba, first[0].texture.rgba, "a page's pixels are rebuilt identically");
        let (changed, _) = pages.prepare(&scene(42));
        assert_ne!(changed[0].texture.id, first[0].texture.id, "new glyphs after text updates get a new page");
        assert_eq!(keys(&changed[0]), [json!([0, 18, 42, 0])]);
        let mut switched = scene(41);
        switched["fonts"][0]["weight"] = json!(700);
        let (other_font, _) = pages.prepare(&switched);
        assert_ne!(other_font[0].texture.id, first[0].texture.id, "font identity changes invalidate font-index bindings");
        let (restored, _) = pages.prepare(&scene(41));
        assert_ne!(
            restored[0].texture.id, other_font[0].texture.id,
            "returning to an earlier font table cannot reuse another scene's glyph binding"
        );
        let reloaded = pages.reload(&restored[0].texture.id).unwrap();
        assert_eq!(reloaded.rgba, restored[0].texture.rgba);

        // A mixed older page cannot overwrite a newer binding, and eviction cannot unlink it
        let mut pages = GlyphPages::new();
        let fonts = json!([font_entry, {"file":font,"weight":400,"ascender":1900,"descender":-500,"unitsPerEm":2048,"tag":1}]);
        let both = text_scene(fonts.clone(), json!([[0, 42, 0, 18], [1, 43, 0, 18]]));
        let (built, _) = pages.prepare(&both);
        let old = built[0].texture.id.clone();
        let copy = AssetId("glyph-page:newer".into());
        let page = &pages.pages[&old];
        let newer = GlyphPage {
            entries: page.entries.clone(),
            placements: page.placements.clone(),
            width: page.width,
            height: page.height,
            freetype: page.freetype.clone(),
        };
        pages.pages.insert(copy.clone(), newer);
        pages.order.push(copy.clone());
        pages.glyphs.insert("0|18|42|0".into(), copy.clone());
        let (mixed, _) = pages.prepare(&both);
        let installed: HashMap<String, AssetId> = mixed
            .iter()
            .flat_map(|page| keys(page).into_iter().map(|key| (glyph_key_string(&key), page.texture.id.clone())))
            .collect();
        assert_eq!(installed["0|18|42|0"], copy, "old mixed pages cannot overwrite current font bindings");
        assert_eq!(installed["1|18|43|0"], old);
        for index in 0..127 {
            let id = AssetId(format!("filler-{index}"));
            pages.pages.insert(
                id.clone(),
                GlyphPage { entries: Vec::new(), placements: Vec::new(), width: 2048, height: 1, freetype: String::new() },
            );
            pages.order.push(id);
        }
        let (none, error) = pages.prepare(&json!({"fonts":fonts}));
        assert!(none.is_empty() && error.is_none());
        assert_eq!(pages.pages.len(), MAX_PAGES);
        assert_eq!(pages.glyphs.get("0|18|42|0"), Some(&copy), "old-page eviction cannot unlink a newer owner");
    }

    /// request_missing reads only the nodes that changed, and requests nothing again while neither the
    /// scene's needs nor the assets changed
    #[test]
    fn unchanged_needs_request_nothing_and_new_text_is_still_requested() {
        let Some(font) = test_font() else {
            return;
        };
        let text = |id: u64, glyph: u64, y: f64| json!({"id":id,"rect":[0,y,10,10],"text":{"weight":400,"lines":[{"glyphs":[[0,glyph,0,18]]}]}});
        let mut scene = SceneStore::default();
        scene.apply(&json!({"viewport":{"width":10,"height":10,"scale":1.5},"fonts":[{"file":font,"weight":400}],"images":[],"nodes":[text(1, 41, 0.0)],"layers":[]})).unwrap();
        let mut assets = AssetStore::new(std::env::temp_dir().join("rur-no-node-helper-here"), true);
        assets.begin_scene(&scene);
        assert_eq!(assets.request_missing(&scene), 1);
        assets.wait_for_glyphs(Duration::from_secs(10));
        assert_eq!(assets.request_missing(&scene), 0);
        assert_eq!(assets.clean_at, Some(assets.request_state));
        // The text moves: the same glyphs, nothing to request (without a full pass)
        scene.apply(&json!({"delta":{"nodes":[text(1, 41, 5.0)]}})).unwrap();
        assert_eq!(assets.request_missing(&scene), 0);
        assert_eq!(assets.clean_at, Some(assets.request_state));
        // New glyphs are requested
        scene.apply(&json!({"delta":{"nodes":[text(2, 42, 0.0)]}})).unwrap();
        assert_eq!(assets.request_missing(&scene), 1);
        assets.wait_for_glyphs(Duration::from_secs(10));
        assert_eq!(assets.request_missing(&scene), 0);
        assert!(assets.glyph_for(0, 18.0, 42, 0.0, None).is_some());
    }

    /// Dynamic glyphs need neither Node nor Python
    #[test]
    fn store_rasterizes_glyphs_without_a_helper() {
        let Some(font) = test_font() else {
            return;
        };
        let mut scene = SceneStore::default();
        scene.apply(&json!({"viewport":{"width":10,"height":10,"scale":1.5},"fonts":[{"file":font,"weight":400}],"images":[],"nodes":[{"id":1,"text":{"weight":400,"lines":[{"glyphs":[[0,41,0,18]]}]},"strokes":[{"mode":"Contextual","thickness":1,"joins":"Miter"}]}],"layers":[]})).unwrap();
        let mut assets = AssetStore::new(std::env::temp_dir().join("rur-no-node-helper-here"), true);
        assets.begin_scene(&scene);
        assert_eq!(assets.request_missing(&scene), 1);
        assets.wait_for_glyphs(Duration::from_secs(10));
        let deadline = Instant::now() + Duration::from_secs(10);
        while assets.pending_count() > 0 && Instant::now() < deadline {
            assets.poll();
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(assets.diagnostics().is_empty(), "{:?}", assets.diagnostics());
        let fill = assets.glyph_for(0, 18.0, 41, 0.0, None).expect("fill glyph").clone();
        let stroke = assets.glyph_for(0, 18.0, 41, 1.5, Some("miter")).expect("stroked glyph").clone();
        assert_eq!(fill.page, stroke.page);
        let page = assets.texture(&fill.page).expect("page texture");
        let covered = (fill.y..fill.y + fill.h).any(|y| {
            (fill.x..fill.x + fill.w).any(|x| page.rgba[(y * page.width + x) as usize * 4 + 3] > 0)
        });
        assert!(fill.w > 0 && covered, "the glyph has coverage");
        assert!(assets.child.lock().unwrap().is_none(), "glyphs do not start Node or Python");
        assert_eq!(assets.request_missing(&scene), 0);
    }

    #[test]
    fn decoding_preserves_transparent_png_colors_and_supports_svg() {
        let directory = std::env::temp_dir().join(format!(
            "rur-native-decode-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        let png = directory.join("pixels.png");
        image::RgbaImage::from_raw(2, 1, vec![91, 43, 27, 0, 12, 24, 36, 255])
            .unwrap()
            .save(&png)
            .unwrap();
        let pixels = decode_texture(AssetId("test-png".into()), &png, 1024).unwrap();
        assert_eq!((pixels.width, pixels.height), (2, 1));
        assert_eq!(&*pixels.rgba, &[91, 43, 27, 0, 12, 24, 36, 255]);
        assert!(decode_texture(AssetId("budget".into()), &png, 1).is_err());
        let svg = directory.join("pixels.svg");
        std::fs::write(&svg, r#"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="1"><rect width="2" height="1" fill="red"/></svg>"#).unwrap();
        let pixels = decode_texture(AssetId("test-svg".into()), &svg, 1024).unwrap();
        assert_eq!(&*pixels.rgba, &[255, 0, 0, 255, 255, 0, 0, 255]);
        std::fs::remove_file(png).unwrap();
        std::fs::remove_file(svg).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn saved_manifest_needs_no_helper_and_scene_generations_isolate_glyphs_and_errors() {
        let directory = std::env::temp_dir().join(format!(
            "rur-native-manifest-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        let png = directory.join("pixels.png");
        image::RgbaImage::from_raw(1, 1, vec![12, 24, 36, 255])
            .unwrap()
            .save(&png)
            .unwrap();
        let atlas = json!({"src":"pixels.png","width":1,"height":1,"glyphs":[{"key":[0,18,41,0],"x":0,"y":0,"w":1,"h":1,"left":0,"top":1}]});
        let manifest = directory.join("manifest.json");
        std::fs::write(
            &manifest,
            json!({"images":{"rbxassetid://123":{"src":"pixels.png"}},"atlas":atlas}).to_string(),
        )
        .unwrap();
        let mut scene = SceneStore::default();
        scene.apply(&json!({"viewport":{"width":10,"height":10,"scale":1},"fonts":[{"file":"intentionally-unavailable-font.ttf"}],"images":["rbxassetid://123"],"nodes":[{"id":1,"image":{"uri":"rbxassetid://123"},"text":{"weight":400,"lines":[{"glyphs":[[0,41,0,18]]}]}}],"layers":[]})).unwrap();
        let mut assets = AssetStore::new(directory.join("no-node-helper-here"), true);
        assert!(assets.load_manifest(&scene, &directory));
        assert_eq!(
            assets.request_missing(&scene),
            0,
            "manifest owns the initial requirements"
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while assets.pending_count() > 0 && std::time::Instant::now() < deadline {
            assets.poll();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert_eq!(assets.pending_count(), 0);
        assert!(
            assets.diagnostics().is_empty(),
            "{:?}",
            assets.diagnostics()
        );
        assert_eq!(
            &*assets.image("rbxassetid://123").unwrap().rgba,
            &[12, 24, 36, 255]
        );
        assert!(assets.glyph_for(0, 18.0, 41, 0.0, None).is_some());
        assert_eq!(assets.request_missing(&scene), 0);
        assert!(
            assets.child.lock().unwrap().is_none(),
            "saved manifests do not start Node or Python"
        );

        let old_generation = assets.generation;
        let (inject, completed) = mpsc::sync_channel(8);
        assets.completed = completed;
        assets.begin_scene(&scene);
        inject
            .send(Completion::Texture(
                TextureAsset {
                    id: AssetId("rbxassetid://123".into()),
                    width: 1,
                    height: 1,
                    rgba: Arc::from([99, 0, 0, 255]),
                },
                "stale.png".into(),
                old_generation,
            ))
            .unwrap();
        inject
            .send(Completion::Atlas(
                AssetId("old-page".into()),
                atlas,
                old_generation,
            ))
            .unwrap();
        inject
            .send(Completion::Failed(
                None,
                "old scene failure".into(),
                old_generation,
            ))
            .unwrap();
        inject
            .send(Completion::Failed(
                Some(AssetId("rbxassetid://123".into())),
                "stale source failed".into(),
                old_generation,
            ))
            .unwrap();
        assets.pending_images.insert("rbxassetid://123".into());
        inject
            .send(Completion::Done {
                id: 999,
                images: vec!["rbxassetid://123".into()],
                signatures: vec![],
                success: false,
                generation: old_generation,
                manifest: false,
                font_request: None,
            })
            .unwrap();
        assets.poll();
        assert!(
            assets.pending_images.contains("rbxassetid://123"),
            "stale Done cannot erase a newer request marker"
        );
        assert!(!assets.failed_images.contains("rbxassetid://123"));
        assert!(!assets.image_errors.contains_key("rbxassetid://123"));
        assert_eq!(
            &*assets.image("rbxassetid://123").unwrap().rgba,
            &[12, 24, 36, 255],
            "stale decoded images cannot replace the active source"
        );
        assert!(assets.glyph_for(0, 18.0, 41, 0.0, None).is_none());
        assert!(assets.diagnostics().is_empty());
        scene
            .apply(&json!({"delta":{"images":["rbxassetid://missing"],"nodes":[{"id":1,"image":{"uri":"rbxassetid://missing"}}]}}))
            .unwrap();
        assets.begin_scene(&scene);
        inject
            .send(Completion::Failed(
                Some(AssetId("rbxassetid://missing".into())),
                "referenced failure".into(),
                assets.generation,
            ))
            .unwrap();
        assets.poll();
        assert_eq!(assets.diagnostics().len(), 1);
        assets.begin_scene(&SceneStore::default());
        assert!(assets.diagnostics().is_empty());
        assets.begin_scene(&scene);
        assert_eq!(
            assets.diagnostics().len(),
            1,
            "referenced persistent failures return for the relevant scene"
        );
        drop(assets);
        std::fs::remove_file(png).unwrap();
        std::fs::remove_file(manifest).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn failed_or_hung_helper_restarts_after_scene_reload() {
        for hang in [false, true] {
            let directory = std::env::temp_dir().join(format!(
                "rur-native-helper-test-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir(&directory).unwrap();
            std::fs::create_dir(directory.join("cli")).unwrap();
            let png = directory.join("image.png");
            image::RgbaImage::from_raw(1, 1, vec![1, 2, 3, 255])
                .unwrap()
                .save(&png)
                .unwrap();
            let helper = directory.join("cli/native-assets.mjs");
            let script = format!(
                r#"import fs from 'node:fs';
process.stdin.on('data', bytes => {{
 const request = JSON.parse(bytes.toString());
 if (!fs.existsSync('attempted')) {{
  fs.writeFileSync('attempted','yes');
  if ({hang}) setInterval(() => {{}},1000); else process.exit(3);
 }} else process.stdout.write(JSON.stringify({{id:request.id,images:[{{uri:'rbxassetid://123',file:{png}}}],pages:[]}})+'\n');
}});
"#,
                hang = hang,
                png = serde_json::to_string(&png.to_string_lossy()).unwrap()
            );
            std::fs::write(&helper, script).unwrap();
            let mut scene = SceneStore::default();
            scene.apply(&json!({"viewport":{},"nodes":[{"id":1,"image":{"uri":"rbxassetid://123"}}],"fonts":[],"images":["rbxassetid://123"],"layers":[]})).unwrap();
            let mut assets =
                AssetStore::with_timeout(&directory, true, 1024 * 1024, Duration::from_secs(2));
            for attempt in 0..2 {
                assets.begin_scene(&scene);
                assert_eq!(assets.request_missing(&scene), 1);
                let deadline = Instant::now() + Duration::from_secs(5);
                while assets.pending_count() > 0 && Instant::now() < deadline {
                    assets.poll();
                    std::thread::sleep(Duration::from_millis(2));
                }
                assert_eq!(
                    assets.pending_count(),
                    0,
                    "worker deadline must release failed jobs"
                );
                if attempt == 0 {
                    assert!(!assets.diagnostics().is_empty());
                } else {
                    assert_eq!(
                        &*assets.image("rbxassetid://123").unwrap().rgba,
                        &[1, 2, 3, 255]
                    );
                    assert!(assets.diagnostics().is_empty());
                }
            }
            let child = assets.child.clone();
            drop(assets);
            if let Some(child) = child.lock().unwrap().as_mut() {
                let _ = child.wait();
            }
            std::fs::remove_file(png).unwrap();
            std::fs::remove_file(helper).unwrap();
            std::fs::remove_file(directory.join("attempted")).unwrap();
            std::fs::remove_dir(directory.join("cli")).unwrap();
            std::fs::remove_dir(directory).unwrap();
        }
    }

    /// A glyph job does not queue behind an image download: the display thread waits for glyphs
    /// (wait_for_glyphs, up to 50 ms a frame), and one worker for both kept it waiting for as long as
    /// the download took (the ~60 ms frames of a scroll into new rows)
    #[test]
    fn glyph_jobs_do_not_wait_for_a_slow_image_download() {
        let directory = std::env::temp_dir().join(format!(
            "rur-native-slow-helper-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(directory.join("cli")).unwrap();
        // A helper that never answers (a download that takes long)
        std::fs::write(
            directory.join("cli/native-assets.mjs"),
            "process.stdin.on('data', () => {}); setInterval(() => {}, 1000);\n",
        )
        .unwrap();
        let mut assets = AssetStore::with_timeout(&directory, true, 1024 * 1024, Duration::from_secs(20));
        let mut scene = SceneStore::default();
        scene.apply(&json!({"viewport":{"scale":1.5},"nodes":[{"id":1,"image":{"uri":"rbxassetid://123"}}],"fonts":[{"weight":400}],"images":["rbxassetid://123"],"layers":[]})).unwrap();
        assets.begin_scene(&scene);
        assert_eq!(assets.request_missing(&scene), 1);
        // New text scrolls in while the image is downloading
        scene.apply(&json!({"viewport":{"scale":1.5},"nodes":[{"id":1,"image":{"uri":"rbxassetid://123"}},
            {"id":2,"text":{"weight":400,"lines":[{"glyphs":[[0,41,0,18]]}]}}],"fonts":[{"weight":400}],"images":["rbxassetid://123"],"layers":[]})).unwrap();
        assets.request_missing(&scene);
        assert!(!assets.pending_signatures.is_empty());
        let started = Instant::now();
        while !assets.pending_signatures.is_empty() && started.elapsed() < Duration::from_secs(5) {
            assets.wait_for_glyphs(Duration::from_millis(50));
        }
        assert!(assets.pending_signatures.is_empty(), "the glyph job waited for the download");
        assert!(started.elapsed() < Duration::from_secs(2));
        assert_eq!(assets.helper_jobs(), 1, "the download is still in flight");
        let child = assets.child.clone();
        drop(assets);
        if let Some(child) = child.lock().unwrap().as_mut() {
            let _ = child.wait();
        }
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn partial_saved_atlases_queue_missing_fill_and_stroke_requirements() {
        let directory = std::env::temp_dir().join(format!(
            "rur-native-partial-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        image::RgbaImage::from_raw(1, 1, vec![255, 255, 255, 255])
            .unwrap()
            .save(directory.join("pixels.png"))
            .unwrap();
        for case in 0..3 {
            let atlas = if case == 0 {
                Value::Null
            } else {
                json!({"src":"pixels.png","glyphs":[{"key":[0,18,if case==1 {99}else{41},0],"x":0,"y":0,"w":1,"h":1,"left":0,"top":1}]})
            };
            std::fs::write(
                directory.join("manifest.json"),
                json!({"images":{},"atlas":atlas}).to_string(),
            )
            .unwrap();
            let mut node = json!({"id":1,"text":{"weight":400,"lines":[{"glyphs":[[0,41,0,18]]}]}});
            if case == 2 {
                node["strokes"] = json!([{"mode":"Contextual","thickness":1,"transparency":1}]);
            }
            let mut scene = SceneStore::default();
            scene.apply(&json!({"viewport":{"scale":1},"fonts":[{"file":"unavailable.ttf"}],"images":[],"nodes":[node],"layers":[]})).unwrap();
            let mut assets = AssetStore::new(directory.join("no-helper"), true);
            assert!(assets.load_manifest(&scene, &directory));
            let deadline = Instant::now() + Duration::from_secs(3);
            while assets.pending_count() > 0 && Instant::now() < deadline {
                assets.poll();
                std::thread::sleep(Duration::from_millis(1));
            }
            assert_eq!(assets.pending_count(), 0);
            assert!(
                assets.request_missing(&scene) > 0,
                "partial manifest case {case} must queue missing glyphs"
            );
        }
        std::fs::remove_file(directory.join("pixels.png")).unwrap();
        std::fs::remove_file(directory.join("manifest.json")).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }
    #[test]
    fn active_textures_are_pinned_and_budget_failures_do_not_thrash() {
        let mut scene = SceneStore::default();
        scene
            .apply(&json!({"viewport":{"scale":1.5},"nodes":[{"id":1,"image":{"uri":"A"}},{"id":2,"image":{"uri":"B"}}],"fonts":[],"images":["A","B"],"layers":[]}))
            .unwrap();
        let mut assets = AssetStore::with_budget("unused", true, 4);
        let (queued, queue) = mpsc::sync_channel(4);
        assets.requests = Some(queued.clone());
        assets.glyph_requests = Some(queued);
        assets.begin_scene(&scene);
        let (inject, completed) = mpsc::sync_channel(8);
        assets.completed = completed;
        let texture = |name: &str, color| TextureAsset {
            id: AssetId(name.into()),
            width: 1,
            height: 1,
            rgba: Arc::from([color, 0, 0, 255]),
        };
        inject
            .send(Completion::Texture(
                texture("A", 1),
                "A.png".into(),
                assets.generation,
            ))
            .unwrap();
        inject
            .send(Completion::Texture(
                texture("B", 2),
                "B.png".into(),
                assets.generation,
            ))
            .unwrap();
        assets.poll();
        assert!(
            assets.image("A").is_some(),
            "an active texture cannot be evicted for another active texture"
        );
        assert!(assets.image("B").is_none());
        assert_eq!(assets.resident_bytes(), 4);
        assert!(
            assets
                .diagnostics()
                .iter()
                .any(|message| message.contains("RUR_NATIVE_ASSET_MB"))
        );
        for _ in 0..3 {
            assert_eq!(
                assets.request_missing(&scene),
                0,
                "overflow must not trigger a decode/evict loop"
            );
        }
        scene
            .apply(&json!({"delta":{"images":["B"],"removed":[1]}}))
            .unwrap();
        assert_eq!(
            scene.images().len(),
            2,
            "discovery catalog intentionally remains cumulative"
        );
        assert_eq!(
            assets.request_missing(&scene),
            1,
            "released image pins recover in the same call"
        );
        assert_eq!(queue.try_recv().unwrap().reload[0].0, AssetId("B".into()));
        inject
            .send(Completion::Texture(
                texture("B", 2),
                "B.png".into(),
                assets.generation,
            ))
            .unwrap();
        assets.poll();
        assert!(
            assets.image("A").is_none(),
            "inactive pixels are evicted first"
        );
        assert!(assets.image("B").is_some());
        assert!(assets.diagnostics().is_empty());
        assert_eq!(assets.resident_bytes(), 4);
    }

    #[test]
    fn manifest_source_changes_replace_pixels_even_at_the_same_path() {
        let directory = std::env::temp_dir().join(format!(
            "rur-native-source-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        let mut scene = SceneStore::default();
        scene
            .apply(&json!({"viewport":{"scale":1.5},"nodes":[{"id":1,"image":{"uri":"same-uri"}}],"fonts":[],"images":["same-uri"],"layers":[]}))
            .unwrap();
        let mut assets = AssetStore::new(directory.join("no-helper"), true);
        let mut prior = None;
        for (file, color) in [
            ("one.png", 1),
            ("two.png", 2),
            ("two.png", 3),
            ("two.png", 3),
        ] {
            image::RgbaImage::from_raw(1, 1, vec![color, 0, 0, 255])
                .unwrap()
                .save(directory.join(file))
                .unwrap();
            std::fs::write(
                directory.join("manifest.json"),
                json!({"images":{"same-uri":{"src":file},"unused":{"src":"absent.png"}}})
                    .to_string(),
            )
            .unwrap();
            assert!(assets.load_manifest(&scene, &directory));
            let deadline = Instant::now() + Duration::from_secs(5);
            while assets.pending_count() > 0 && Instant::now() < deadline {
                assets.poll();
                std::thread::sleep(Duration::from_millis(1));
            }
            assert_eq!(assets.pending_count(), 0);
            assert!(
                assets.diagnostics().is_empty(),
                "unused manifest assets are not loaded"
            );
            let pixels = assets.image("same-uri").unwrap().rgba.clone();
            assert_eq!(&*pixels, &[color, 0, 0, 255]);
            if let Some((old_color, old_pixels)) = prior {
                assert_eq!(Arc::ptr_eq(&old_pixels, &pixels), old_color == color);
            }
            prior = Some((color, pixels));
        }
        for corrupt in [false, true] {
            image::RgbaImage::from_raw(1, 1, vec![7, 0, 0, 255])
                .unwrap()
                .save(directory.join("one.png"))
                .unwrap();
            for fail in [false, true] {
                let file = if fail && !corrupt {
                    "missing.png"
                } else {
                    "one.png"
                };
                if fail && corrupt {
                    std::fs::write(directory.join(file), b"not an image").unwrap();
                }
                std::fs::write(
                    directory.join("manifest.json"),
                    json!({"images":{"same-uri":{"src":file}}}).to_string(),
                )
                .unwrap();
                assert!(assets.load_manifest(&scene, &directory));
                let deadline = Instant::now() + Duration::from_secs(5);
                while assets.pending_count() > 0 && Instant::now() < deadline {
                    assets.poll();
                    std::thread::sleep(Duration::from_millis(1));
                }
                assert_eq!(assets.pending_count(), 0);
                assert_eq!(
                    assets.image("same-uri").is_none(),
                    fail,
                    "a failed new source cannot retain the previous source's pixels"
                );
                assert_eq!(
                    assets.diagnostics().is_empty(),
                    !fail,
                    "strict capture must observe the new source failure"
                );
            }
        }
        drop(assets);
        for file in ["one.png", "two.png", "manifest.json"] {
            std::fs::remove_file(directory.join(file)).unwrap();
        }
        std::fs::remove_dir(directory).unwrap();
    }
    #[test]
    fn released_glyph_pins_retry_blocked_pages_in_the_same_request() {
        let mut scene = SceneStore::default();
        scene
            .apply(&json!({"viewport":{"scale":1.5},"nodes":[
            {"id":1,"text":{"lines":[{"glyphs":[[0,41,0,18]]}]}},
            {"id":2,"text":{"lines":[{"glyphs":[[0,42,0,18]]}]}}
        ],"fonts":[],"images":[],"layers":[]}))
            .unwrap();
        let mut assets = AssetStore::with_budget("unused", true, 4);
        let (queued, queue) = mpsc::sync_channel(4);
        assets.requests = Some(queued.clone());
        assets.glyph_requests = Some(queued);
        let (inject, completed) = mpsc::sync_channel(8);
        assets.completed = completed;
        assets.begin_scene(&scene);
        for (node, page) in [(1, "glyph-page:A"), (2, "glyph-page:B")] {
            let id = AssetId(page.into());
            assets.active_pages.insert(id.clone());
            let signature = assets
                .signature_of(scene.node(node).unwrap(), scene.viewport().scale)
                .unwrap();
            assets.signatures.insert(signature, vec![id.clone()]);
            inject
                .send(Completion::Texture(
                    TextureAsset {
                        id,
                        width: 1,
                        height: 1,
                        rgba: Arc::from([1, 0, 0, 255]),
                    },
                    format!("{node}.png").into(),
                    assets.generation,
                ))
                .unwrap();
        }
        assets.poll();
        assert!(
            assets
                .budget_blocked
                .contains(&AssetId("glyph-page:B".into()))
        );
        assert_eq!(assets.request_missing(&scene), 0);
        scene.apply(&json!({"delta":{"removed":[1]}})).unwrap();
        assert_eq!(
            assets.request_missing(&scene),
            1,
            "recovery must not require another unrelated UI event"
        );
        let job = queue.try_recv().unwrap();
        assert_eq!(job.reload.len(), 1);
        assert_eq!(job.reload[0].0, AssetId("glyph-page:B".into()));
        assert!(
            !assets
                .active_pages
                .contains(&AssetId("glyph-page:A".into()))
        );
        assert!(assets.diagnostics().is_empty());
    }
    #[test]
    fn partial_pending_glyph_pages_remain_pinned() {
        let mut scene = SceneStore::default();
        scene.apply(&json!({"viewport":{"scale":1.5},"nodes":[{"id":1,"text":{"weight":400,"lines":[{"glyphs":[[0,41,0,18],[0,42,9,18]]}]}}],"fonts":[{"weight":400}],"images":[],"layers":[]})).unwrap();
        let mut assets = AssetStore::with_budget("unused", true, 4);
        assets.begin_scene(&scene);
        let (inject, completed) = mpsc::sync_channel(8);
        assets.completed = completed;
        let signature = assets
            .signature_of(scene.node(1).unwrap(), scene.viewport().scale)
            .unwrap();
        assets.pending_signatures.insert(signature);
        let first = AssetId("glyph-page:first".into());
        inject
            .send(Completion::Texture(
                TextureAsset {
                    id: first.clone(),
                    width: 1,
                    height: 1,
                    rgba: Arc::from([1, 0, 0, 255]),
                },
                "first.png".into(),
                assets.generation,
            ))
            .unwrap();
        inject
            .send(Completion::Atlas(
                first.clone(),
                json!({"glyphs":[{"key":[0,18,41,0],"x":0,"y":0,"w":1,"h":1,"left":0,"top":1}]}),
                assets.generation,
            ))
            .unwrap();
        assets.poll();
        assert_eq!(assets.request_missing(&scene), 0);
        assert!(
            assets.active_pages.contains(&first),
            "partial coverage remains pinned before Done"
        );
        inject
            .send(Completion::Texture(
                TextureAsset {
                    id: AssetId("glyph-page:second".into()),
                    width: 1,
                    height: 1,
                    rgba: Arc::from([1, 0, 0, 255]),
                },
                "second.png".into(),
                assets.generation,
            ))
            .unwrap();
        assets.poll();
        assert!(
            assets.texture(&first).is_some(),
            "a later page cannot evict completed active glyphs"
        );
        assert!(
            assets
                .diagnostics()
                .iter()
                .any(|message| message.contains("RUR_NATIVE_ASSET_MB"))
        );
    }
}
