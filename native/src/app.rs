use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use egui::{Color32, TextureId, Vec2};
use serde_json::{Value, json};

mod viewport_resize;
use viewport_resize::ViewportResize;

use crate::{
    NativeOptions,
    assets::{AssetEvent, AssetStore},
    catalog::{Catalog, CatalogRow, CatalogView},
    input::{PreviewInput, push},
    presentation::ScrollPresentation,
    profiler::{PhaseSample, ProfileFrame, Profiler},
    render::{
        DrawPreparer, DrawStats,
        gpu::{GpuRenderer, GpuStats},
    },
    runtime::{RuntimeEvent, RuntimeOptions, RuntimeWorker},
    scene::SceneStore,
};

pub fn run(options: NativeOptions) -> Result<()> {
    // Named worker lanes in the MicroProfiler (the raster jobs run on this pool)
    let _ = rayon::ThreadPoolBuilder::new()
        .thread_name(|index| format!("Worker {index}"))
        .build_global();
    let mut restore: Option<Restart> = None;
    let mut options = options;
    let mut restarts: Vec<Instant> = Vec::new();
    // A lost GPU device (a driver reset, another application's GPU hang) cannot be recreated inside
    // eframe: the window closes and a new one starts with a fresh device, the same story and settings
    loop {
        DEVICE_LOST.store(false, std::sync::atomic::Ordering::SeqCst);
        let restart = Arc::new(std::sync::Mutex::new(None));
        let window = restore.as_ref().and_then(|restore| restore.window);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_window(options.clone(), restore.take(), window, restart.clone())
        }));
        // egui-wgpu's panic when it cannot allocate on a device whose loss wgpu has not reported yet
        let staging_failed = result.as_ref().err().is_some_and(|panic| {
            panic
                .downcast_ref::<String>()
                .is_some_and(|message| message.starts_with("Failed to create staging buffer"))
        });
        let lost = DEVICE_LOST.load(std::sync::atomic::Ordering::SeqCst) || staging_failed;
        let next = restart.lock().ok().and_then(|mut slot| slot.take());
        match (result, lost) {
            (Ok(result), false) => return result,
            (Err(panic), false) => std::panic::resume_unwind(panic),
            (result, true) => {
                if let Err(panic) = &result {
                    let message = panic
                        .downcast_ref::<String>()
                        .map(String::as_str)
                        .or_else(|| panic.downcast_ref::<&str>().copied())
                        .unwrap_or("panic");
                    eprintln!("rur-native: after the GPU device was lost: {message}");
                }
                // A device that keeps failing (a broken driver) ends the app instead of looping
                restarts.retain(|at: &Instant| at.elapsed() < Duration::from_secs(60));
                restarts.push(Instant::now());
                if restarts.len() > 5 {
                    anyhow::bail!("The GPU device was lost 6 times within a minute");
                }
                eprintln!("rur-native: starting again with a new GPU device");
                if let Some(next) = next {
                    options = next.options.clone();
                    restore = Some(next);
                }
            }
        }
    }
}

fn run_window(
    options: NativeOptions,
    restore: Option<Restart>,
    window: Option<egui::Rect>,
    restart: Arc<std::sync::Mutex<Option<Restart>>>,
) -> Result<()> {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("RUR — Native Storybook")
        .with_inner_size([1200.0, 800.0])
        .with_min_inner_size([800.0, 600.0]);
    if let Some(rect) = window {
        viewport = viewport
            .with_position(rect.min)
            .with_inner_size(rect.size());
    }
    // RUR_NO_ACTIVATE=1: the window opens without taking the keyboard focus (automated desktop runs)
    if std::env::var("RUR_NO_ACTIVATE").as_deref() == Ok("1") {
        viewport = viewport.with_active(false);
    }
    let mut native_options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport,
        vsync: options.vsync,
        ..Default::default()
    };
    native_options.wgpu_options.present_mode = if options.vsync {
        wgpu::PresentMode::AutoVsync
    } else {
        wgpu::PresentMode::AutoNoVsync
    };
    if let egui_wgpu::WgpuSetup::CreateNew(setup) = &mut native_options.wgpu_options.wgpu_setup {
        let defaults = setup.device_descriptor.clone();
        setup.device_descriptor = Arc::new(move |adapter| {
            let mut descriptor = defaults(adapter);
            descriptor.required_features |= adapter.features() & wgpu::Features::TIMESTAMP_QUERY;
            descriptor
        });
    }
    eframe::run_native(
        "RUR — Native Storybook",
        native_options,
        Box::new(move |context| {
            let mut app = NativeApp::new(context, options, restart)?;
            if let Some(restore) = restore {
                app.fit = restore.fit;
                app.show_profiler = restore.show_profiler;
            }
            Ok(Box::new(app))
        }),
    )
    .map_err(|error| anyhow::anyhow!(error.to_string()))
}

pub struct NativeApp {
    options: NativeOptions,
    catalog: Catalog,
    catalog_view: CatalogView,
    search: String,
    place: String,
    open_path: String,
    runtime: Option<RuntimeWorker>,
    /// A VM with the engine and a place's project already loaded, taken by the next story of
    /// that place so opening or switching stories skips the load.
    standby: Option<RuntimeWorker>,
    scene: SceneStore,
    presentation: ScrollPresentation,
    drawn_motion: Option<Value>,
    assets: AssetStore,
    preparer: DrawPreparer,
    gpu: GpuRenderer,
    draw_stats: Option<GpuStats>,
    prepare_stats: Option<DrawStats>,
    render_state: egui_wgpu::RenderState,
    texture: Option<(TextureId, u64, [u32; 2])>,
    input: PreviewInput,
    viewport_resize: ViewportResize,
    clipboard_echo: Option<String>,
    pending_events: Vec<Value>,
    controls: Vec<Value>,
    profiler: Profiler,
    show_profiler: bool,
    micro: crate::microprofiler_ui::MicroProfilerView,
    show_micro: bool,
    micro_frames: u64,
    fit: bool,
    paused: bool,
    loading: bool,
    dirty: bool,
    error: Option<String>,
    draw_error: Option<String>,
    runtime_failed: bool,
    font_reloads: u8,
    pending_fonts: Option<Value>,
    status: String,
    started: Instant,
    last_frame: Instant,
    last_request: Instant,
    request_started: Option<Instant>,
    /// Control-to-frame latency (the trace's `controlToFrameMs`): when the earliest queued control change
    /// was made, when the change carried by the request in flight was made, and when the one whose reply
    /// this update applied was made (recorded at the end of the update that draws it)
    control_queued: Option<Instant>,
    control_sent: Option<Instant>,
    control_applied: Option<Instant>,
    /// This update's receive (replies, asset completions) and, within it, the wait for glyph jobs
    receive_ms: f64,
    /// This update's scroll projection (ScrollPresentation::project), part of the preview integration
    project_ms: f64,
    glyph_wait_ms: f64,
    /// The previous update's own duration (the interval measured at the next update covers it)
    previous_update_ms: Option<f64>,
    /// Where the next story opens after the GPU device was lost (run() starts the app again)
    restart: Arc<std::sync::Mutex<Option<Restart>>>,
    /// Frames drawn since start (RUR_TEST_DEVICE_LOSS=<n> destroys the device after n frames)
    frames: u64,
    /// `--actions` in the desktop app: timed input played into the preview once the story is shown
    replay: Option<DesktopReplay>,
}

/// Timed input events (a `--actions` file of {t, type, ...} live session events in canvas logical
/// coordinates: the story target sits `--margin` px in from the canvas corner) fed to the
/// desktop preview as if they came from the pointer, from the first drawn frame of the story; the
/// profiler trace is exported to `--out` one second after the last event (RUR_REPLAY_EXIT=1 then closes
/// the window). Measures the real display path (egui, present, vsync) under a scripted scroll.
struct DesktopReplay {
    events: Vec<(f64, Value)>,
    next: usize,
    started: Option<Instant>,
    done: bool,
}

impl DesktopReplay {
    fn load(path: &std::path::Path) -> Result<Self, String> {
        let value: Value = serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        let mut events: Vec<(f64, Value)> = value
            .as_array()
            .ok_or("Actions must be an array of timed input events")?
            .iter()
            .filter(|event| event["type"].is_string())
            .map(|event| {
                let mut event = event.clone();
                let t = event["t"].as_f64().unwrap_or(0.0);
                event.as_object_mut().map(|map| map.remove("t"));
                (t, event)
            })
            .collect();
        events.sort_by(|a, b| a.0.total_cmp(&b.0));
        Ok(Self { events, next: 0, started: None, done: false })
    }
}

/// A new trace file in `dir`: trace-YYYYMMDD-HHMMSS.json (local time), with -2, -3, ... when that exists
fn trace_path(dir: &std::path::Path) -> PathBuf {
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let mut path = dir.join(format!("trace-{stamp}.json"));
    let mut next = 2;
    while path.exists() {
        path = dir.join(format!("trace-{stamp}-{next}.json"));
        next += 1;
    }
    path
}

/// What a new app window restores after the GPU device was lost
pub struct Restart {
    options: NativeOptions,
    fit: bool,
    show_profiler: bool,
    window: Option<egui::Rect>,
}

/// Set by the device-lost callback: the device, its surfaces and every GPU resource are gone
static DEVICE_LOST: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

impl NativeApp {
    fn new(
        context: &eframe::CreationContext<'_>,
        options: NativeOptions,
        restart: Arc<std::sync::Mutex<Option<Restart>>>,
    ) -> Result<Self> {
        let render_state = context
            .wgpu_render_state
            .clone()
            .context("The native renderer requires a wgpu device")?;
        // egui-wgpu panics without a cause once the device is gone ("Failed to create staging
        // buffer"): report why it was lost or which call failed first
        render_state.device.set_device_lost_callback(|reason, message| {
            eprintln!("rur-native: GPU device lost ({reason:?}): {message}");
            DEVICE_LOST.store(true, std::sync::atomic::Ordering::SeqCst);
        });
        render_state.device.on_uncaptured_error(Arc::new(|error| {
            eprintln!("rur-native: wgpu error: {error}");
        }));
        let mut gpu = GpuRenderer::new(
            Arc::new(render_state.device.clone()),
            Arc::new(render_state.queue.clone()),
        )?;
        // A timestamp (and a submission-to-completion time) for every drawn frame: the profiler's GPU lane
        gpu.set_timestamp_interval(1);
        let now = Instant::now();
        let catalog = Catalog::load(options.project.clone(), options.roots.clone());
        let assets = AssetStore::new(options.repo.clone(), options.offline);
        let mut app = Self {
            options,
            catalog,
            catalog_view: CatalogView::default(),
            search: String::new(),
            place: "all".into(),
            open_path: String::new(),
            runtime: None,
            standby: None,
            scene: SceneStore::default(),
            presentation: ScrollPresentation::default(),
            drawn_motion: None,
            assets,
            preparer: DrawPreparer::new(),
            gpu,
            draw_stats: None,
            prepare_stats: None,
            render_state,
            texture: None,
            input: PreviewInput::default(),
            viewport_resize: ViewportResize::default(),
            clipboard_echo: None,
            pending_events: Vec::new(),
            controls: Vec::new(),
            profiler: Profiler::default(),
            show_profiler: true,
            micro: Default::default(),
            show_micro: false,
            micro_frames: 0,
            fit: true,
            paused: false,
            loading: false,
            dirty: false,
            error: None,
            draw_error: None,
            runtime_failed: false,
            font_reloads: 0,
            pending_fonts: None,
            status: "Choose a story to begin".into(),
            started: now,
            last_frame: now,
            last_request: now,
            request_started: None,
            control_queued: None,
            control_sent: None,
            control_applied: None,
            receive_ms: 0.0,
            project_ms: 0.0,
            glyph_wait_ms: 0.0,
            previous_update_ms: None,
            restart,
            frames: 0,
            replay: None,
        };
        if let Some(path) = app.options.actions.clone() {
            match DesktopReplay::load(&path) {
                Ok(replay) => app.replay = Some(replay),
                Err(error) => app.error = Some(format!("Actions {}: {error}", path.display())),
            }
        }
        if let Some(path) = app.options.story.clone() {
            app.open_story(path);
        } else if app.options.scene.is_none() {
            // The standby starts with the project's default place and follows the last opened story.
            let place = crate::places::places(&app.options.project).swap_remove(0).roots.into_iter().next().unwrap_or_else(|| app.options.project.clone());
            app.prepare_standby(RuntimeOptions::from_native(&app.options, place));
        } else if let Some(path) = app.options.scene.clone() {
            match std::fs::read(&path)
                .map_err(|e| e.to_string())
                .and_then(|bytes| {
                    serde_json::from_slice::<Value>(&bytes).map_err(|e| e.to_string())
                }) {
                Ok(value) => match app.scene.apply(&value) {
                    Ok(_) => {
                        app.dirty = true;
                        app.status = format!("Scene: {}", path.display());
                        app.assets.load_manifest(&app.scene, &path);
                        app.assets.request_missing(&app.scene);
                    }
                    Err(error) => app.error = Some(error),
                },
                Err(error) => app.error = Some(error),
            }
        }
        Ok(app)
    }

    fn open_story(&mut self, path: PathBuf) {
        self.viewport_resize.clear();
        let maximum = self.render_state.device.limits().max_texture_dimension_2d;
        if self
            .options
            .viewport
            .iter()
            .any(|size| (*size as f64 * self.options.scale).ceil() > maximum as f64)
        {
            self.error = Some(format!(
                "Viewport × scale exceeds this GPU's {maximum}-pixel texture limit. Reduce the viewport or scale."
            ));
            return;
        }
        if self
            .options
            .story
            .as_ref()
            .is_some_and(|current| current != &path)
        {
            self.options.controls = json!({});
        }
        self.font_reloads = 0;
        self.pending_fonts = None;
        self.options.story = Some(path.clone());
        self.options.scene = None;
        self.open_path = path.to_string_lossy().into_owned();
        self.scene = SceneStore::default();
        self.presentation = ScrollPresentation::default();
        self.drawn_motion = None;
        self.preparer.reset_resources();
        self.input = PreviewInput::default();
        self.clipboard_echo = None;
        self.pending_events.clear();
        self.controls.clear();
        self.error = None;
        self.draw_error = None;
        self.runtime_failed = false;
        self.loading = true;
        self.dirty = false;
        self.paused = false;
        self.last_request = Instant::now();
        self.request_started = None;
        self.control_queued = None;
        self.control_sent = None;
        self.control_applied = None;
        self.status = format!("Loading {}", path.display());
        if let Some((id, _, _)) = self.texture.take() {
            self.render_state.renderer.write().free_texture(&id);
        }
        let runtime_options = RuntimeOptions::from_native(&self.options, path);
        // Replacing the worker stops the previous story on its own thread; the new one does not
        // wait for that teardown.
        let result = match self
            .standby
            .take()
            .filter(|standby| standby.standby_matches(&runtime_options))
        {
            Some(standby) => standby
                .begin_story(runtime_options)
                .map(|()| self.runtime = Some(standby)),
            None => {
                RuntimeWorker::spawn(runtime_options).map(|runtime| self.runtime = Some(runtime))
            }
        };
        if let Err(error) = result {
            self.loading = false;
            self.runtime_failed = true;
            self.error = Some(error);
        }
    }

    fn prepare_standby(&mut self, options: RuntimeOptions) {
        if self
            .standby
            .as_ref()
            .is_some_and(|standby| standby.standby_matches(&options))
        {
            return;
        }
        self.standby = RuntimeWorker::spawn_standby(options).ok();
    }

    fn receive(&mut self, ctx: &egui::Context) {
        self.clipboard_echo = None;
        self.catalog.poll();
        loop {
            let Some(event) = self.runtime.as_ref().and_then(RuntimeWorker::poll) else {
                break;
            };
            match event {
                RuntimeEvent::Reply(mut reply) => {
                    if let Some(text) = reply.get("clipboard").and_then(Value::as_str) {
                        ctx.copy_text(text.to_owned());
                        self.clipboard_echo = Some(text.to_owned());
                        for event in &mut self.pending_events {
                            if event["type"] == "paste" {
                                event["text"] = json!(text);
                            }
                        }
                    }
                    if reply["type"] == "ready" {
                        self.last_request = Instant::now();
                        // Prepared only after the story is ready, so both loads never compete.
                        if let Some(path) = self.options.story.clone() {
                            self.prepare_standby(RuntimeOptions::from_native(&self.options, path));
                        }
                    }
                    if let Some(started) = self.request_started.take() {
                        self.presentation
                            .observe_response(started.elapsed().as_secs_f64());
                    }
                    if let Some(made) = self.control_sent.take() {
                        self.control_applied.get_or_insert(made);
                    }
                    self.profiler.record_runtime(&reply);
                    if self.font_reloads < 3 {
                        if let Some(diagnostics) = reply.get("diagnostics") {
                            if diagnostics["missingFontAssets"]
                                .as_array()
                                .is_some_and(|assets| !assets.is_empty())
                            {
                                self.pending_fonts = Some(diagnostics.clone());
                            }
                        }
                    }
                    if reply["type"] == "error" {
                        self.error = Some(display_value(&reply["message"]));
                        self.loading = false;
                        self.runtime_failed = true;
                        continue;
                    }
                    if let Some(controls) = reply.get("controls").and_then(Value::as_array) {
                        self.controls = controls.clone();
                        restore_control_values(&mut self.controls, &self.options.controls);
                    }
                    if let Some(error) = reply.get("error") {
                        self.error = Some(display_value(error));
                        self.runtime_failed = true;
                    }
                    match self.scene.apply_owned(&mut reply) {
                        Ok(outcome) => {
                            if reply["type"] == "ready" {
                                self.assets.begin_scene(&self.scene);
                            }
                            self.dirty |= outcome.changed;
                            self.dirty |= self
                                .presentation
                                .update(&self.scene, reply.get("presentation"));
                            if outcome.changed {
                                self.assets.request_missing(&self.scene);
                            }
                            self.loading = false;
                            self.status = format!("{} nodes · native GPU", self.scene.node_count());
                            if let Some(failure) = self
                                .scene
                                .header()
                                .get("failure")
                                .filter(|failure| !failure.is_null())
                            {
                                self.error = Some(display_value(failure));
                                self.runtime_failed = true;
                            }
                        }
                        Err(error) => {
                            self.error = Some(error);
                            self.runtime_failed = true;
                        }
                    }
                }
                RuntimeEvent::Error(error) => {
                    self.error = Some(error);
                    self.loading = false;
                    self.runtime_failed = true;
                }
                RuntimeEvent::Stopped => {
                    self.loading = false;
                    self.status = "Runtime stopped".into();
                    self.runtime = None;
                }
            }
        }
        let asset_events = self.assets.poll();
        let fonts_changed = asset_events
            .iter()
            .any(|event| matches!(event, AssetEvent::FontsChanged));
        if fonts_changed {
            // Fonts are preloaded when a VM starts; a standby from before the download is stale.
            self.standby = None;
        }
        if fonts_changed && self.font_reloads < 3 {
            if let Some(path) = self.options.story.clone() {
                let pass = self.font_reloads + 1;
                self.open_story(path);
                self.font_reloads = pass;
                self.status = "Reloading story with downloaded fonts".into();
            }
        }
        if !asset_events.is_empty() {
            self.dirty = true;
            self.assets.request_missing(&self.scene);
        }
        // Text whose glyphs are still being rasterized would draw empty for a frame (a flicker the
        // first time a size appears, e.g. a hover tween): wait for them briefly, as Roblox rasterizes
        // glyphs before drawing
        let waited = Instant::now();
        let glyph_events = self.assets.wait_for_glyphs(Duration::from_millis(50));
        self.glyph_wait_ms = waited.elapsed().as_secs_f64() * 1000.0;
        if !glyph_events.is_empty() {
            self.dirty = true;
            self.assets.request_missing(&self.scene);
        }
        if self
            .pending_fonts
            .as_ref()
            .is_some_and(|diagnostics| self.assets.request_fonts(diagnostics))
        {
            self.pending_fonts = None;
        }
    }

    fn add_events(&mut self, events: Vec<Value>) {
        for mut event in events {
            // The OS clipboard command is applied after this display callback. A paste
            // captured before that write must still see the copy we just acknowledged.
            if event["type"] == "paste" {
                if let Some(text) = &self.clipboard_echo {
                    event["text"] = json!(text);
                }
            }
            if event["type"] == "control" {
                let path = event["path"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(".");
                self.options.controls[path] = event["value"].clone();
                self.control_queued.get_or_insert_with(Instant::now);
            }
            if event["type"] == "wheel" {
                let taker = self.presentation.wheel(
                    &self.scene,
                    event["x"].as_f64().unwrap_or_default(),
                    event["y"].as_f64().unwrap_or_default(),
                    event["notches"].as_f64().unwrap_or_default(),
                );
                if let Some(taker) = taker {
                    event["presentationTaker"] = json!(taker);
                }
                event["presentation"] = self.presentation.snapshot();
            } else {
                event["presentation"] = self.presentation.snapshot_at(self.drawn_motion.as_ref());
            }
            push(&mut self.pending_events, event);
        }
    }

    fn request_frame(&mut self) {
        if self.loading || (self.paused && self.control_queued.is_none()) || self.runtime_failed {
            return;
        }
        let Some(runtime) = &self.runtime else { return };
        if runtime.is_busy() {
            return;
        }
        let now = Instant::now();
        let dt = if self.paused {
            0.0
        } else {
            now.duration_since(self.last_request).as_secs_f64().min(0.25)
        };
        let command = json!({"cmd":"frame", "dt":dt,
            "cadence":"display", "events":self.pending_events, "presentation":self.presentation.snapshot()});
        match runtime.try_command(command) {
            Ok(()) => {
                self.pending_events.clear();
                self.last_request = now;
                self.request_started = Some(now);
                self.control_sent = self.control_queued.take();
            }
            Err(error) => {
                self.error = Some(error);
                self.runtime_failed = true;
            }
        }
    }

    fn draw_preview(&mut self, dt: f64) -> f64 {
        self.draw_stats = None;
        self.prepare_stats = None;
        let moved = !self.paused && self.presentation.advance(&self.scene, dt);
        if (!self.dirty && !moved) || self.scene.revision == 0 {
            return 0.0;
        }
        let started = Instant::now();
        let scene = self.presentation.project(&self.scene);
        self.project_ms = started.elapsed().as_secs_f64() * 1000.0;
        let result = self
            .preparer
            .prepare(&scene, &self.assets)
            .map_err(|e| e.to_string())
            .and_then(|packet| self.gpu.render(&packet).map_err(|e| e.to_string()));
        match result {
            Ok(frame) => {
                self.draw_stats = Some(frame.stats.clone());
                self.prepare_stats = Some(self.preparer.last_stats().clone());
                if self
                    .texture
                    .as_ref()
                    .is_none_or(|(_, generation, _)| *generation != frame.generation)
                {
                    let mut renderer = self.render_state.renderer.write();
                    if let Some((id, _, _)) = self.texture.take() {
                        renderer.free_texture(&id);
                    }
                    let texture = renderer.register_native_texture(
                        &self.render_state.device,
                        &frame.view,
                        wgpu::FilterMode::Linear,
                    );
                    self.texture = Some((texture, frame.generation, [frame.width, frame.height]));
                }
                self.drawn_motion = Some(self.presentation.inspect());
                self.dirty = false;
                self.draw_error = None;
            }
            Err(error) => {
                self.preparer.reset_resources();
                self.draw_error = Some(error);
                self.dirty = false;
            }
        }
        started.elapsed().as_secs_f64() * 1000.0
    }

    /// What a new window restores after a lost device (run())
    fn save_restart(&self, ctx: &egui::Context) {
        if let Ok(mut slot) = self.restart.lock() {
            *slot = Some(Restart {
                options: self.options.clone(),
                fit: self.fit,
                show_profiler: self.show_profiler,
                // The window's outer position and inner size (what the viewport builder takes)
                window: ctx.input(|input| {
                    let viewport = input.viewport();
                    Some(egui::Rect::from_min_size(
                        viewport.outer_rect?.min,
                        viewport.inner_rect?.size(),
                    ))
                }),
            });
        }
    }

    /// Feeds the desktop replay's due events (DesktopReplay); exports the trace when it has finished
    fn step_replay(&mut self, ctx: &egui::Context) {
        let Some(replay) = self.replay.as_mut() else {
            return;
        };
        if replay.done || self.loading || self.texture.is_none() {
            return;
        }
        let started = *replay.started.get_or_insert_with(Instant::now);
        let elapsed = started.elapsed().as_secs_f64();
        let mut due = Vec::new();
        while replay.next < replay.events.len() && replay.events[replay.next].0 <= elapsed {
            due.push(replay.events[replay.next].1.clone());
            replay.next += 1;
        }
        let finished = replay.next == replay.events.len()
            && elapsed >= replay.events.last().map_or(0.0, |event| event.0) + 1.0;
        if finished {
            replay.done = true;
        }
        self.add_events(due);
        if finished {
            let path = trace_path(&self.options.out);
            match std::fs::create_dir_all(&self.options.out).and_then(|_| self.profiler.export(&path)) {
                Ok(()) => {
                    self.status = format!("Replay finished; saved {}", path.display());
                    eprintln!("rur-native: replay finished; saved {}", path.display());
                }
                Err(error) => self.error = Some(error.to_string()),
            }
            if std::env::var("RUR_REPLAY_EXIT").as_deref() == Ok("1") {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    fn export_png(&mut self) {
        let result = (|| -> Result<PathBuf> {
            let (_, _, [width, height]) = self.texture.context("Open a story before exporting")?;
            let rgba = self.gpu.readback_png_rgba()?;
            std::fs::create_dir_all(&self.options.out)?;
            let path = self.options.out.join("render.png");
            image::save_buffer_with_format(
                &path,
                &rgba,
                width,
                height,
                image::ColorType::Rgba8,
                image::ImageFormat::Png,
            )?;
            Ok(path)
        })();
        match result {
            Ok(path) => self.status = format!("Saved {}", path.display()),
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn toolbar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.heading("RUR");
                if ui
                    .add_enabled(self.options.story.is_some(), egui::Button::new("Reload"))
                    .clicked()
                {
                    self.open_story(self.options.story.clone().unwrap());
                }
                if ui
                    .selectable_label(self.paused, if self.paused { "Resume" } else { "Pause" })
                    .clicked()
                {
                    self.paused = !self.paused;
                    self.last_request = Instant::now();
                }
                ui.checkbox(&mut self.fit, "Fit");
                ui.checkbox(&mut self.show_profiler, "Profiler");
                ui.checkbox(&mut self.show_micro, "MicroProfiler").on_hover_text("Ctrl+F6");
                if ui
                    .add_enabled(self.texture.is_some(), egui::Button::new("Export PNG"))
                    .clicked()
                {
                    self.export_png();
                }
                if ui.button("Export trace").clicked() {
                    // A new file every time (trace-<date>-<time>.json): an export never replaces an earlier one
                    let path = trace_path(&self.options.out);
                    match std::fs::create_dir_all(&self.options.out)
                        .and_then(|_| self.profiler.export(&path))
                    {
                        Ok(()) => self.status = format!("Saved {}", path.display()),
                        Err(error) => self.error = Some(error.to_string()),
                    }
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("Viewport");
                ui.add(
                    egui::DragValue::new(&mut self.options.viewport[0])
                        .range(64..=8192)
                        .speed(10.0),
                );
                ui.label("×");
                ui.add(
                    egui::DragValue::new(&mut self.options.viewport[1])
                        .range(64..=8192)
                        .speed(10.0),
                );
                ui.label("Scale");
                ui.add(
                    egui::DragValue::new(&mut self.options.scale)
                        .range(0.25..=4.0)
                        .speed(0.05),
                );
                if ui
                    .add_enabled(self.options.story.is_some(), egui::Button::new("Apply"))
                    .clicked()
                {
                    self.open_story(self.options.story.clone().unwrap());
                }
                ui.separator();
                ui.add(
                    egui::TextEdit::singleline(&mut self.open_path)
                        .hint_text("Story file path")
                        .desired_width(240.0),
                );
                if ui.button("Open").clicked() && !self.open_path.trim().is_empty() {
                    let path = PathBuf::from(self.open_path.trim());
                    self.open_story(if path.is_absolute() {
                        path
                    } else {
                        self.options.repo.join(path)
                    });
                }
            });
        });
    }

    fn catalog_panel(&mut self, ctx: &egui::Context) {
        let mut selected = None;
        egui::SidePanel::left("stories")
            .default_width(240.0)
            .width_range(160.0..=ctx.content_rect().width() * 0.3)
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Stories");
                    if ui.small_button("Refresh").clicked() {
                        self.catalog =
                            Catalog::load(self.options.project.clone(), self.options.roots.clone());
                        self.catalog_view = CatalogView::default();
                    }
                });
                ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("Search stories…"));
                ui.horizontal(|ui| {
                    // "all", then the places the catalog's stories belong to, in catalog order
                    let mut places: Vec<&str> = vec!["all"];
                    for entry in &self.catalog.entries {
                        if !places.contains(&entry.place.as_str()) {
                            places.push(&entry.place);
                        }
                    }
                    for place in places {
                        ui.selectable_value(&mut self.place, place.into(), place);
                    }
                });
                if self.catalog.loading {
                    ui.spinner();
                }
                if let Some(error) = &self.catalog.error {
                    ui.colored_label(Color32::LIGHT_RED, error);
                }
                self.catalog_view.update(
                    &self.catalog.entries,
                    self.catalog.revision,
                    &self.search,
                    &self.place,
                );
                ui.label(format!("{} stories", self.catalog_view.story_count));
                let row_height = ui
                    .spacing()
                    .interact_size
                    .y
                    .max(ui.text_style_height(&egui::TextStyle::Body));
                egui::ScrollArea::vertical()
                    .id_salt("catalog-scroll")
                    .show_rows(
                        ui,
                        row_height,
                        self.catalog_view.rows.len(),
                        |ui, visible| {
                            for row in &self.catalog_view.rows[visible] {
                                match *row {
                                    CatalogRow::Category(index) => {
                                        let category = &self.catalog.entries[index].category;
                                        ui.add_sized(
                                            [ui.available_width(), row_height],
                                            egui::Label::new(egui::RichText::new(category).weak())
                                                .truncate(),
                                        )
                                        .on_hover_text(category);
                                    }
                                    CatalogRow::Story(index) => {
                                        let entry = &self.catalog.entries[index];
                                        ui.push_id(&entry.file, |ui| {
                                            if ui
                                                .add_sized(
                                                    [ui.available_width(), row_height],
                                                    egui::Button::selectable(
                                                        self.options.story.as_ref()
                                                            == Some(&entry.file),
                                                        &entry.name,
                                                    )
                                                    .truncate(),
                                                )
                                                .on_hover_text(&entry.key)
                                                .clicked()
                                            {
                                                selected = Some(entry.file.clone());
                                            }
                                        });
                                    }
                                }
                            }
                        },
                    );
            });
        if let Some(path) = selected {
            self.open_story(path);
        }
    }

    fn inspector(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("inspector")
            .default_width(320.0)
            .width_range(220.0..=ctx.content_rect().width() * 0.35)
            .resizable(true)
            .show(ctx, |ui| {
                let mut events = Vec::new();
                egui::ScrollArea::vertical()
                    .id_salt("controls-scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.heading("Controls");
                        if self.controls.is_empty() {
                            ui.weak("This story has no controls.");
                        }
                        let mut group = String::new();
                        for (index, control) in self.controls.iter_mut().enumerate() {
                            let path: Vec<_> = control["path"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .filter_map(Value::as_str)
                                .map(str::to_string)
                                .collect();
                            let next_group = path[..path.len().saturating_sub(1)].join(" / ");
                            if group != next_group {
                                group = next_group;
                                ui.separator();
                                ui.strong(&group);
                            }
                            ui.push_id(index, |ui| {
                                if let Some(value) = control_widget(
                                    ui,
                                    control,
                                    path.last().map(String::as_str).unwrap_or("Control"),
                                ) {
                                    events.push(
                                        json!({"type":"control", "path":path, "value":value}),
                                    );
                                }
                            });
                        }
                        if self.show_profiler {
                            ui.separator();
                            let _scope = crate::microprofile::scope("Profiler UI");
                            self.profiler.show(ui);
                        }
                        if !self.assets.diagnostics().is_empty() {
                            ui.separator();
                            ui.collapsing("Asset diagnostics", |ui| {
                                for error in self.assets.diagnostics() {
                                    ui.colored_label(Color32::LIGHT_RED, error);
                                }
                            });
                        }
                    });
                self.add_events(events);
            });
    }
}

impl eframe::App for NativeApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // MicroProfiler: record while its view is open and the profiler runs; one marker per frame
        crate::microprofile::set_enabled(self.show_micro && !self.profiler.is_paused());
        crate::microprofile::register_thread("Main");
        let started = crate::microprofile::now();
        {
            let _frame_scope = crate::microprofile::scope("Frame");
            self.update_frame(ctx, frame);
        }
        crate::microprofile::frame(self.micro_frames, started, crate::microprofile::now());
        self.micro_frames += 1;
    }
}

impl NativeApp {
    fn update_frame(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let now = Instant::now();
        let frame_interval = now.duration_since(self.last_frame).as_secs_f64();
        let dt = frame_interval.min(0.25);
        self.last_frame = now;
        // eframe's CPU time for the previous display callback: its update, input and egui painting,
        // without egui-wgpu's surface acquire, queue submission and present
        let callback_cpu_ms = frame.info().cpu_usage.map(|seconds| seconds as f64 * 1000.0);
        self.frames += 1;
        // RUR_TEST_DEVICE_LOSS=<n>: destroy the device after n frames, once per process (tests recovery)
        static TESTED_LOSS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if std::env::var("RUR_TEST_DEVICE_LOSS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .is_some_and(|after| after == self.frames)
            && !TESTED_LOSS.swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            eprintln!("rur-native: RUR_TEST_DEVICE_LOSS: destroying the GPU device");
            self.render_state.device.destroy();
            // The lost callback runs from a poll
            let _ = self.render_state.device.poll(wgpu::PollType::Poll);
        }
        if DEVICE_LOST.load(std::sync::atomic::Ordering::SeqCst) {
            // Nothing is drawn (egui-wgpu would fail to allocate the frame's buffers on the lost device)
            // and the window closes; run() opens a new one with a new device, the story and settings
            self.save_restart(ctx);
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        // Kept current (story, controls, window) in case egui-wgpu panics on a lost device before the
        // loss is reported
        if self.frames % 60 == 1 {
            self.save_restart(ctx);
        }
        // Ctrl+P pauses / resumes the profiler from anywhere (consumed: the story never sees it)
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::P)) {
            self.profiler.set_paused(!self.profiler.is_paused());
        }
        // Ctrl+F6 shows / hides the MicroProfiler (Roblox's shortcut)
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::F6)) {
            self.show_micro = !self.show_micro;
        }
        {
            let _scope = crate::microprofile::scope("Receive replies");
            let received = Instant::now();
            self.receive(ctx);
            self.receive_ms = received.elapsed().as_secs_f64() * 1000.0;
        }
        let panels = Instant::now();
        {
            let _scope = crate::microprofile::scope("UI panels");
            self.toolbar(ctx);
            self.catalog_panel(ctx);
            self.inspector(ctx);
        }
        let panels_ms = panels.elapsed().as_secs_f64() * 1000.0;
        let draw_ms = {
            let _scope = crate::microprofile::scope("Draw preview");
            self.draw_preview(dt)
        };
        if self.show_micro {
            egui::TopBottomPanel::bottom("microprofiler")
                .resizable(true)
                .default_height(460.0)
                .min_height(200.0)
                .show(ctx, |ui| {
                    let _scope = crate::microprofile::scope("MicroProfiler UI");
                    self.micro.show(ui, self.profiler.is_paused());
                });
        }
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if self.loading {
                    ui.spinner();
                }
                if let Some(error) = self.error.as_ref().or(self.draw_error.as_ref()) {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(error.lines().next().unwrap_or(error))
                                .color(Color32::LIGHT_RED),
                        ).truncate(),
                    ).on_hover_text(error);
                } else {
                    ui.add(egui::Label::new(&self.status).truncate())
                        .on_hover_text(&self.status);
                }
                let pending = self.assets.pending_count();
                if pending > 0 {
                    ui.label(format!("· {pending} assets loading"));
                }
            });
        });
        let mut resized_viewport = None;
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some((texture, _, size)) = self.texture {
                let actual_size =
                    Vec2::new(size[0] as f32, size[1] as f32) / ctx.pixels_per_point();
                let available = ui.available_size();
                let response = if self.fit {
                    let size = fitted_preview_size(
                        actual_size,
                        (available - Vec2::splat(16.0)).max(Vec2::ZERO),
                    );
                    let rect = egui::Rect::from_center_size(
                        ui.available_rect_before_wrap().center(), size,
                    );
                    ui.put(rect, egui::Image::new((texture, size))
                        .fit_to_exact_size(size)
                        .sense(egui::Sense::click_and_drag()))
                } else {
                    egui::ScrollArea::both().id_salt("preview-scroll").show(ui, |ui| {
                        ui.add(egui::Image::new((texture, actual_size)).fit_to_exact_size(actual_size)
                            .sense(egui::Sense::click_and_drag()))
                    }).inner
                };
                let viewport = self.scene.viewport();
                let resize = if self.options.story.is_some() && !self.loading {
                    let margin = self.options.margin.unwrap_or(48) as f32;
                    let logical_size = Vec2::new(viewport.width as f32, viewport.height as f32);
                    let target_size = logical_size - Vec2::splat(margin * 2.0);
                    let target_rect = response.rect
                        .shrink2(response.rect.size() / logical_size * margin);
                    let maximum = ((self.render_state.device.limits().max_texture_dimension_2d as f64
                        / self.options.scale).floor() as u32)
                        .saturating_sub(margin as u32 * 2).clamp(64, 8192);
                    self.viewport_resize.show(
                        ui, target_rect, [target_size.x as u32, target_size.y as u32], maximum,
                    )
                } else {
                    Default::default()
                };
                resized_viewport = resize.viewport;
                if !resize.captured
                    && (response.clicked() || response.drag_started() || response.is_pointer_button_down_on())
                {
                    response.request_focus();
                }
                ui.memory_mut(|memory| {
                    memory.set_focus_lock_filter(
                        response.id,
                        egui::EventFilter {
                            tab: true,
                            horizontal_arrows: true,
                            vertical_arrows: true,
                            escape: true,
                        },
                    )
                });
                if response.has_focus() {
                    if let Some(caret) = self
                        .scene
                        .ordered_nodes()
                        .find_map(|node| node.get("caret"))
                    {
                        let unit = response.rect.size() / Vec2::new(size[0] as f32, size[1] as f32);
                        let position = response.rect.min
                            + Vec2::new(
                                caret["x"].as_f64().unwrap_or_default() as f32,
                                caret["y"].as_f64().unwrap_or_default() as f32,
                            ) * unit;
                        let cursor_rect = egui::Rect::from_min_size(
                            position,
                            Vec2::new(1.0, caret["h"].as_f64().unwrap_or(16.0) as f32 * unit.y),
                        );
                        ctx.output_mut(|output| {
                            output.ime = Some(egui::output::IMEOutput {
                                rect: response.rect,
                                cursor_rect,
                            })
                        });
                    }
                }
                let (events, modifiers) =
                    ctx.input(|input| (input.events.clone(), input.modifiers));
                let events = self.input.collect(
                    &events,
                    response.rect,
                    if response.contains_pointer() && !resize.captured {
                        ui.clip_rect()
                    } else {
                        egui::Rect::NOTHING
                    },
                    [viewport.width, viewport.height],
                    response.has_focus() && !resize.captured,
                    modifiers,
                );
                self.add_events(events);
            } else {
                ui.centered_and_justified(|ui| {
                    ui.label(if self.loading {
                        "Loading story…"
                    } else {
                        "Select a story from the catalog or open a story file."
                    });
                });
            }
        });
        if let Some(viewport) = resized_viewport {
            self.options.viewport = viewport;
            if let Some(story) = self.options.story.clone() {
                self.open_story(story);
            }
        }
        self.step_replay(ctx);
        self.request_frame();
        let scrolling = self
            .drawn_motion
            .as_ref()
            .and_then(|motion| motion["scrolls"].as_array())
            .and_then(|scrolls| {
                scrolls.iter().find(|scroll| {
                    scroll["owned"] == true
                        && (scroll["position"] != scroll["target"]
                            || scroll["velocity"].as_array().is_some_and(|velocity| {
                                velocity
                                    .iter()
                                    .any(|value| value.as_f64().unwrap_or_default().abs() > 0.01)
                            }))
                })
            });
        let scroll_session = scrolling.map(|scroll| {
            scroll["id"]
                .as_u64()
                .unwrap_or_default()
                .wrapping_mul(1_000_003)
                ^ scroll["epoch"].as_u64().unwrap_or_default()
        });
        let mut counters = std::collections::BTreeMap::new();
        let mut phases = Vec::new();
        if let Some(stats) = &self.prepare_stats {
            phases.extend(
                [
                    ("Draw backgrounds", stats.background_ms),
                    ("Draw images", stats.images_ms),
                    ("Draw text", stats.text_ms),
                    ("Draw strokes", stats.strokes_ms),
                    ("Draw shadows", stats.shadows_ms),
                    ("Draw groups", stats.groups_ms),
                    ("Draw scrollbars", stats.scrollbars_ms),
                    ("Draw node cache", stats.node_cache_ms),
                    ("Draw rasterize (parallel)", stats.deferred_raster_ms),
                    ("Draw other preparation", stats.other_ms),
                ]
                .into_iter()
                .map(|(name, duration_ms)| PhaseSample {
                    name: name.into(),
                    duration_ms,
                }),
            );
            counters.extend(
                [
                    ("drawPreparationMs", stats.total_ms),
                    // Upload preparation is nested in the exclusive categories above.
                    ("drawNestedUploadMs", stats.upload_ms),
                    ("backgroundsPrepared", stats.backgrounds as f64),
                    ("imagesPrepared", stats.images as f64),
                    ("textsPrepared", stats.texts as f64),
                    ("strokesPrepared", stats.strokes as f64),
                    ("shadowsPrepared", stats.shadows as f64),
                    ("groupsPrepared", stats.groups as f64),
                    ("scrollbarsPrepared", stats.scrollbars as f64),
                    ("nodeCacheHits", stats.node_cache_hits as f64),
                    ("nodeCacheMisses", stats.node_cache_misses as f64),
                    ("nodesPrepared", stats.nodes_prepared as f64),
                    ("uploadCacheHits", stats.upload_cache_hits as f64),
                    ("recordTexels", stats.record_texels as f64),
                    ("drawBatches", stats.batches as f64),
                    ("cpuPixelBytesProcessed", stats.pixel_bytes_processed as f64),
                    (
                        "preparedTextureUploadBytes",
                        stats.texture_upload_bytes as f64,
                    ),
                    ("preparedTextureUploads", stats.texture_uploads as f64),
                    ("retainedTextureBytes", stats.retained_texture_bytes as f64),
                    ("retainedRecordBytes", stats.retained_record_bytes as f64),
                ]
                .into_iter()
                .map(|(name, value)| (name.into(), value)),
            );
        }
        if let Some(stats) = &self.prepare_stats {
            crate::profiler::append_draw_details(stats, &mut phases, &mut counters);
            counters.insert("presentation/projectedNodes".into(), self.presentation.projected_nodes as f64);
            counters.insert("presentation/owned".into(), self.presentation.states.values().filter(|s| s.owned).count() as f64);
        }
        if let Some(stats) = &self.draw_stats {
            phases.push(PhaseSample {
                name: "GPU encode and submit (CPU)".into(),
                duration_ms: stats.cpu_ms,
            });
            if let Some(prepared) = &self.prepare_stats {
                phases.push(PhaseSample {
                    name: "Preview integration".into(),
                    duration_ms: (draw_ms - prepared.total_ms - stats.cpu_ms).max(0.0),
                });
                phases.push(PhaseSample {
                    name: "Preview integration/scroll projection (included)".into(),
                    duration_ms: self.project_ms,
                });
            }
            counters.insert("drawCalls".into(), stats.draw_calls as f64);
            counters.insert("drawInstances".into(), stats.instances as f64);
            counters.insert("renderPasses".into(), stats.passes as f64);
            counters.insert("uploadBytes".into(), stats.upload_bytes as f64);
            counters.insert("gpuTextures".into(), stats.textures as f64);
            // GPU queries complete later; keep their provenance explicit instead of
            // attributing a previous submission's duration to this display frame.
            if let Some(duration) = stats.gpu_ms {
                counters.insert("lastCompletedGpuMs".into(), duration);
            }
            if let Some(duration) = stats.gpu_completion_ms {
                counters.insert("gpuSubmitToCompleteMs".into(), duration);
            }
        }
        // Where this callback interval went. The interval runs from the previous update's start to this
        // one's: that update, eframe / egui's CPU work around it (input, tessellation, egui's render
        // encoding: eframe's cpu_usage minus the update), and the rest: egui-wgpu's surface acquire, queue
        // submission and present (eframe leaves exactly these out of cpu_usage) and the event loop. This
        // update's own receive / glyph wait / panels are listed separately.
        let mut interval_parts = vec![
            ("Update/receive replies and assets", self.receive_ms),
            ("Update/glyph wait (included in receive)", self.glyph_wait_ms),
            ("Update/UI panels", panels_ms),
        ];
        if let (Some(previous), Some(cpu)) = (self.previous_update_ms, callback_cpu_ms) {
            interval_parts.extend([
                ("Previous callback/update", previous),
                ("Previous callback/egui and eframe CPU", (cpu - previous).max(0.0)),
                (
                    "Previous callback/acquire, submit, present and event loop",
                    (frame_interval * 1000.0 - cpu.max(previous)).max(0.0),
                ),
            ]);
        }
        phases.extend(interval_parts.into_iter().map(|(name, duration_ms)| PhaseSample {
            name: name.into(),
            duration_ms,
        }));
        // A control change whose reply this update applied: its result is in the frame drawn above (or it
        // changed nothing), so the time since the control was changed is the control-to-frame latency
        if !self.dirty {
            if let Some(made) = self.control_applied.take() {
                counters.insert("controlToFrameMs".into(), made.elapsed().as_secs_f64() * 1000.0);
            }
        }
        let gpu_ms = self.draw_stats.as_ref().and_then(|stats| stats.new_gpu_ms);
        self.profiler.record(ProfileFrame {
            timestamp_ms: self.started.elapsed().as_secs_f64() * 1000.0,
            frame_interval_ms: frame_interval * 1000.0,
            cpu_draw_ms: draw_ms,
            gpu_ms,
            scroll_active: scrolling.is_some() && !self.paused,
            scroll_session,
            counters,
            phases,
            story: self
                .options
                .story
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default()
                .into(),
            ..Default::default()
        });
        if (self.runtime.is_some() && !self.paused && !self.runtime_failed)
            || self.catalog.loading
            || self.assets.pending_count() > 0
        {
            ctx.request_repaint();
        } else {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        // A device lost since this update started (the lost callback runs from a poll): egui-wgpu
        // would panic allocating this pass's vertex / index buffers on it ("Failed to create staging
        // buffer"). The pass is discarded and the next one (the same frame) draws nothing and closes
        // the window; run() starts a new one with a new device.
        if !DEVICE_LOST.load(std::sync::atomic::Ordering::SeqCst) {
            let _ = self.render_state.device.poll(wgpu::PollType::Poll);
        }
        if DEVICE_LOST.load(std::sync::atomic::Ordering::SeqCst) {
            ctx.request_discard("GPU device lost");
        }        self.previous_update_ms = Some(now.elapsed().as_secs_f64() * 1000.0);
    }
}

impl Drop for NativeApp {
    fn drop(&mut self) {
        if let Some(runtime) = &self.runtime {
            runtime.stop();
        }
        if let Some((id, _, _)) = self.texture.take() {
            self.render_state.renderer.write().free_texture(&id);
        }
    }
}

fn display_value(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| value.to_string())
}

fn fitted_preview_size(actual: Vec2, available: Vec2) -> Vec2 {
    let zoom = (available.x.max(0.0) / actual.x).min(available.y.max(0.0) / actual.y);
    actual * zoom
}

fn restore_control_values(controls: &mut [Value], overrides: &Value) {
    for control in controls {
        let path = control["path"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(".");
        let Some(value) = overrides.get(&path) else {
            continue;
        };
        control["value"] = if control["kind"] == "choose" {
            value
                .as_u64()
                .and_then(|index| index.checked_sub(1))
                .and_then(|index| usize::try_from(index).ok())
                .and_then(|index| {
                    control["options"]
                        .as_array()
                        .and_then(|options| options.get(index))
                })
                .unwrap_or(value)
                .clone()
        } else {
            value.clone()
        };
    }
}

fn control_widget(ui: &mut egui::Ui, control: &mut Value, label: &str) -> Option<Value> {
    let kind = control["kind"].as_str().unwrap_or_default().to_string();
    let mut changed = false;
    let mut outgoing = None;
    match kind.as_str() {
        "boolean" => {
            let mut value = control["value"].as_bool().unwrap_or_default();
            changed = ui.checkbox(&mut value, label).changed();
            if changed {
                control["value"] = json!(value);
            }
        }
        "number" => {
            let mut value = control["value"].as_f64().unwrap_or_default();
            if let (Some(min), Some(max)) = (control["min"].as_f64(), control["max"].as_f64()) {
                let mut slider = egui::Slider::new(&mut value, min..=max).text(label);
                if let Some(step) = control["step"].as_f64().filter(|step| *step > 0.0) {
                    slider = slider.step_by(step);
                }
                changed = ui.add(slider).changed();
            } else {
                ui.horizontal(|ui| {
                    ui.label(label);
                    changed = ui
                        .add(egui::DragValue::new(&mut value).speed(0.1))
                        .changed();
                });
            }
            if changed {
                control["value"] = json!(value);
            }
        }
        "string" => {
            ui.label(label);
            let mut value = control["value"].as_str().unwrap_or_default().to_string();
            changed = ui.text_edit_singleline(&mut value).changed();
            if changed {
                control["value"] = json!(value);
            }
        }
        "color" => {
            ui.horizontal(|ui| {
                ui.label(label);
                let mut color = std::array::from_fn::<_, 3, _>(|i| {
                    control["value"][i].as_f64().unwrap_or_default() as f32
                });
                changed = ui.color_edit_button_rgb(&mut color).changed();
                if changed {
                    control["value"] = json!(color);
                }
            });
        }
        "choose" => {
            let options = control["options"].as_array().cloned().unwrap_or_default();
            ui.label(label);
            egui::ComboBox::from_id_salt(label)
                .selected_text(display_value(&control["value"]))
                .show_ui(ui, |ui| {
                    for (index, option) in options.iter().enumerate() {
                        if ui
                            .selectable_label(control["value"] == *option, display_value(option))
                            .clicked()
                        {
                            control["value"] = option.clone();
                            outgoing = Some(json!(index + 1));
                        }
                    }
                });
        }
        _ => {
            ui.label(format!("{label}: {}", display_value(&control["value"])));
        }
    }
    outgoing.or_else(|| changed.then(|| control["value"].clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fitted_preview_stays_inside_narrow_and_short_panels() {
        let actual = Vec2::new(1944.0, 1494.0);
        for available in [
            Vec2::new(200.0, 500.0),
            Vec2::new(900.0, 120.0),
            Vec2::new(1.0, 1.0),
        ] {
            let fitted = fitted_preview_size(actual, available);
            assert!(fitted.x <= available.x && fitted.y <= available.y);
            assert!((fitted.x / fitted.y - actual.x / actual.y).abs() < 0.0001);
        }
    }

    #[test]
    fn retained_overrides_restore_nested_controls_without_changing_unset_defaults() {
        let mut controls = vec![
            json!({"path":["Group","Enabled"],"kind":"boolean","value":false}),
            json!({"path":["Amount"],"kind":"number","value":1}),
            json!({"path":["Title"],"kind":"string","value":"default"}),
            json!({"path":["Tint"],"kind":"color","value":[0,0,0]}),
            json!({"path":["Untouched"],"kind":"number","value":7}),
        ];
        restore_control_values(
            &mut controls,
            &json!({
                "Group.Enabled":true,"Amount":42,"Title":"retained","Tint":[1,0.5,0]
            }),
        );
        assert_eq!(controls[0]["value"], true);
        assert_eq!(controls[1]["value"], 42);
        assert_eq!(controls[2]["value"], "retained");
        assert_eq!(controls[3]["value"], json!([1, 0.5, 0]));
        assert_eq!(controls[4]["value"], 7);
    }

    #[test]
    fn retained_choose_index_displays_the_selected_option() {
        let mut controls = vec![json!({
            "path":["Mode"],"kind":"choose","options":["first","second"],"value":"first"
        })];
        restore_control_values(&mut controls, &json!({"Mode":2}));
        assert_eq!(controls[0]["value"], "second");
        restore_control_values(&mut controls, &json!({"Mode":"first"}));
        assert_eq!(controls[0]["value"], "first");
    }
}
