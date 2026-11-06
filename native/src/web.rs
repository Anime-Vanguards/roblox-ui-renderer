//! Browser host (feature "web", wasm32-unknown-emscripten): the headless replay's frame loop
//! (headless.rs) without threads or child processes, driven by the page. The engine (engine::cli::Host:
//! the Luau VM and the story's session) runs in process; the scene, assets, scroll presentation and
//! draw preparation are the native ones; the GPU renderer draws through wgpu's GLES backend on
//! Emscripten's WebGL2 canvas, and a blit copies the frame target to the canvas surface texel for texel.
//!
//! The file system is Emscripten's in-memory one: tools/web/bundle.mjs packs the story, every file the
//! engine reads (project tree, data/, fonts, hinted metrics, snapshots) and the images it can show, at
//! virtual paths; the page writes them before [`init`]. JavaScript calls [`frame`] once per display
//! frame with that frame's input (the session protocol's event objects, logical viewport coordinates).
//! A second [`init`] replaces the story; the GPU device, canvas surface, draw caches and decoded assets
//! stay.
use std::{cell::RefCell, time::Instant};

use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};

use crate::{
    assets::AssetStore,
    engine::cli::Host,
    presentation::ScrollPresentation,
    render::{DrawPreparer, gpu::GpuRenderer},
    scene::SceneStore,
};

/// What outlives a story: the GPU and its canvas, the draw caches and the decoded assets
struct Shared {
    gpu: GpuRenderer,
    present: Presenter,
    preparer: DrawPreparer,
    assets: AssetStore,
}

/// The mounted story
struct Story {
    host: Host,
    scene: SceneStore,
    presentation: ScrollPresentation,
    last_drawn: Option<Value>,
    drawn: u64,
}

thread_local! {
    static SHARED: RefCell<Option<Shared>> = const { RefCell::new(None) };
    static STORY: RefCell<Option<Story>> = const { RefCell::new(None) };
}

fn ms(since: Instant) -> f64 {
    since.elapsed().as_secs_f64() * 1000.0
}

/// The engine arguments of a story, as the desktop runtime passes them (runtime.rs launch + story
/// args): `config` is {story, roots [..], viewport [w, h], scale, time, margin, snapshotDir, repo,
/// controls {..}}
fn engine_args(config: &Value) -> Result<crate::engine::cli::Args> {
    let text = |key: &str| config[key].as_str().map(str::to_owned);
    let story = text("story").context("config.story is required")?;
    let roots: Vec<String> = config["roots"].as_array().into_iter().flatten().filter_map(|root| root.as_str().map(str::to_owned)).collect();
    let viewport = config["viewport"].as_array().and_then(|v| Some((v.first()?.as_u64()?, v.get(1)?.as_u64()?))).unwrap_or((1200, 900));
    let scale = config["scale"].as_f64().unwrap_or(1.5);
    let time = config["time"].as_f64().unwrap_or(1.0);
    let margin = config["margin"].as_u64().unwrap_or(0);
    let mut argv: Vec<String> = vec![
        "--roots".into(),
        roots.join(";"),
        "--viewport".into(),
        format!("{}x{}", viewport.0, viewport.1),
        "--scale".into(),
        scale.to_string(),
        "--session".into(),
        "1".into(),
    ];
    if margin > 0 {
        argv.extend(["--margin".into(), margin.to_string()]);
    }
    match text("snapshotDir") {
        Some(dir) => argv.extend(["--snapshot-dir".into(), dir]),
        None => argv.push("--no-snapshots".into()),
    }
    argv.extend(["--story".into(), story, "--time".into(), time.to_string()]);
    if let Some(controls) = config.get("controls").filter(|c| c.as_object().is_some_and(|c| !c.is_empty())) {
        argv.extend(["--controls".into(), controls.to_string()]);
    }
    argv.extend(["--repo".into(), text("repo").unwrap_or_else(|| "/rur".into())]);
    crate::engine::cli::parse(&argv).map_err(|error| anyhow!("engine arguments: {error}"))
}

/// Mounts the story `config` describes (see [`engine_args`]; also `env` {name: value}, set before the
/// first story, and `firstDraw` (default true) to draw the mounted scene at once) and returns the
/// ready summary: viewport, canvas size, node count, status, timings.
pub fn init(config: &str) -> Result<Value> {
    let started = Instant::now();
    let config: Value = serde_json::from_str(config).context("web config is not JSON")?;
    if let Some(env) = config["env"].as_object() {
        for (name, value) in env {
            if let Some(value) = value.as_str() {
                // Single-threaded: nothing reads the environment concurrently
                unsafe { std::env::set_var(name, value) };
            }
        }
    }
    // Rayon's pool is the calling thread alone (no threads without SharedArrayBuffer): the draw code's
    // `current_thread_index().is_none()` checks then take their sequential paths
    let _ = rayon::ThreadPoolBuilder::new().num_threads(1).use_current_thread().build_global();
    // The previous story's VM first: two engines never live at once
    STORY.with(|slot| slot.borrow_mut().take());

    let args = engine_args(&config)?;
    let mut host = Host::start(&args).map_err(|error| anyhow!("engine start: {error}"))?;
    let engine_ms = ms(started);
    let mount_at = Instant::now();
    let ready = host.mount(&args).map_err(|error| anyhow!("story mount: {error}"))?;
    let mount_ms = ms(mount_at);
    if ready["type"] == "error" {
        bail!("{}", ready["message"].as_str().unwrap_or("story failed"));
    }
    let mut scene = SceneStore::default();
    scene.apply(&ready).map_err(|error| anyhow!(error))?;

    let gpu_at = Instant::now();
    let created = create_shared(config["repo"].as_str().unwrap_or("/rur"))?;
    let gpu_ms = ms(gpu_at);
    let mut story = Story { host, scene, presentation: ScrollPresentation::default(), last_drawn: None, drawn: 0 };
    let summary = SHARED.with(|slot| -> Result<Value> {
        let mut slot = slot.borrow_mut();
        let shared = slot.as_mut().context("no GPU")?;
        if !created {
            shared.preparer.reset_resources();
            shared.gpu.reset_scene_resources();
        }
        let asset_at = Instant::now();
        shared.assets.begin_scene(&story.scene);
        settle(&mut shared.assets, &story.scene);
        let assets_ms = ms(asset_at);
        story.presentation.update(&story.scene, None);
        let draw_at = Instant::now();
        let canvas = if config["firstDraw"].as_bool().unwrap_or(true) { Some(story.draw(shared, 0.0)?) } else { None };
        let draw_ms = ms(draw_at);
        let viewport = story.scene.viewport();
        Ok(json!({
            "viewport": {"width": viewport.width, "height": viewport.height, "scale": viewport.scale},
            "canvas": canvas.map(|(w, h)| json!([w, h])),
            "status": story.scene.header().get("status"),
            "failure": story.scene.header().get("failure"),
            "nodes": story.scene.node_count(),
            "adapter": shared.gpu.adapter_info().map(|info| format!("{} ({:?})", info.name, info.backend)),
            "diagnostics": shared.assets.diagnostics(),
            "engineDiagnostics": ready.get("diagnostics"),
            "timings": {"engineStartMs": engine_ms, "mountMs": mount_ms, "assetsMs": assets_ms, "gpuMs": gpu_ms,
                "firstDrawMs": draw_ms, "totalMs": ms(started)},
        }))
    })?;
    STORY.with(|slot| *slot.borrow_mut() = Some(story));
    Ok(summary)
}

/// The GPU device, canvas surface and renderer pipelines, once; true when this call created them
fn create_shared(repo: &str) -> Result<bool> {
    SHARED.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_some() {
            return Ok(false);
        }
        let (gpu, present) = Presenter::create()?;
        *slot = Some(Shared { gpu, present, preparer: DrawPreparer::new(), assets: AssetStore::new(repo, true) });
        Ok(true)
    })
}

/// Creates the GPU side before any story (the page calls it while the bundle still downloads: the
/// browser compiles the renderer's shaders here, seconds on a first visit)
pub fn prepare_gpu() -> Result<Value> {
    let started = Instant::now();
    let created = create_shared("/rur")?;
    let adapter = SHARED.with(|slot| {
        slot.borrow().as_ref().and_then(|shared| shared.gpu.adapter_info().map(|info| format!("{} ({:?})", info.name, info.backend)))
    });
    Ok(json!({"created": created, "adapter": adapter, "ms": ms(started)}))
}

/// One display frame: `events` (a JSON array of session input events) go to the engine with `dt`
/// seconds of virtual time; the reply is applied, missing assets are prepared and the frame is drawn
/// to the canvas when anything changed (or `force`).
pub fn frame(dt: f64, events: &str, force: bool) -> Result<Value> {
    let events: Value = serde_json::from_str(events).unwrap_or(Value::Array(Vec::new()));
    SHARED.with(|shared| {
        STORY.with(|story| {
            let mut shared = shared.borrow_mut();
            let mut story = story.borrow_mut();
            let (Some(shared), Some(story)) = (shared.as_mut(), story.as_mut()) else {
                bail!("no story loaded");
            };
            story.frame(shared, dt, events, force)
        })
    })
}

/// Drops the mounted story (its VM and scene)
pub fn drop_story() {
    STORY.with(|slot| slot.borrow_mut().take());
}

/// The retained scene as JSON (debugging and scene-level comparisons)
pub fn scene_json() -> Result<String> {
    STORY.with(|slot| {
        let slot = slot.borrow();
        let story = slot.as_ref().context("no story loaded")?;
        Ok(serde_json::to_string(&story.scene.to_value())?)
    })
}

/// headless.rs settle_assets: every job runs when it is sent, so this ends when nothing is missing
fn settle(assets: &mut AssetStore, scene: &SceneStore) {
    for _ in 0..16 {
        assets.request_missing(scene);
        assets.poll();
        if assets.pending_count() == 0 {
            break;
        }
    }
}

impl Story {
    fn frame(&mut self, shared: &mut Shared, dt: f64, events: Value, force: bool) -> Result<Value> {
        let started = Instant::now();
        let mut command = json!({"cmd": "frame", "dt": dt, "cadence": "display", "events": events});
        prepare_command(&mut command, &self.scene, &mut self.presentation, self.last_drawn.as_ref());
        let engine_at = Instant::now();
        let mut reply = self.host.handle(&command).context("session closed")?;
        let engine_ms = ms(engine_at);
        if reply["type"] == "error" {
            bail!("{}", reply["message"].as_str().unwrap_or("frame failed"));
        }
        let apply_at = Instant::now();
        let outcome = self.scene.apply_owned(&mut reply).map_err(|error| anyhow!(error))?;
        self.presentation.update(&self.scene, reply.get("presentation"));
        let apply_ms = ms(apply_at);
        let asset_at = Instant::now();
        let revision = shared.assets.render_revision();
        settle(&mut shared.assets, &self.scene);
        let assets_ms = ms(asset_at);
        let moving = self.presentation.states.values().any(|state| state.owned && state.position != state.target);
        let draw_at = Instant::now();
        // Nothing changed and nothing moves: the canvas keeps the previous frame
        let drawn = force || outcome.changed || moving || revision != shared.assets.render_revision() || self.drawn == 0;
        if drawn {
            self.draw(shared, dt)?;
        } else {
            self.presentation.advance(&self.scene, dt);
        }
        let draw_ms = ms(draw_at);
        let stats = shared.preparer.last_stats();
        Ok(json!({
            "drawn": drawn,
            "nodes": self.scene.node_count(),
            "changedNodes": outcome.changed_nodes.len(),
            "engineMs": engine_ms, "applyMs": apply_ms, "assetsMs": assets_ms, "drawMs": draw_ms,
            "prepareMs": if drawn { json!(stats.total_ms) } else { Value::Null },
            "totalMs": ms(started),
            "moving": moving,
            "cursor": reply.get("cursor"),
            "diagnostics": shared.assets.diagnostics(),
        }))
    }

    /// Presentation step, draw preparation, GPU passes and the blit to the canvas
    fn draw(&mut self, shared: &mut Shared, dt: f64) -> Result<(u32, u32)> {
        self.presentation.advance(&self.scene, dt);
        let presented = self.presentation.project(&self.scene);
        let packet = shared.preparer.prepare(&presented, &shared.assets)?;
        let frame = shared.gpu.render(&packet)?;
        shared.present.blit(shared.gpu.device(), shared.gpu.queue(), &frame.view, frame.width, frame.height)?;
        self.last_drawn = Some(self.presentation.inspect());
        self.drawn += 1;
        Ok((frame.width, frame.height))
    }
}

/// headless.rs prepare_command: wheel events take their scroll owner from the presentation, every event
/// carries the presentation state it was aimed at
fn prepare_command(command: &mut Value, scene: &SceneStore, presentation: &mut ScrollPresentation, drawn: Option<&Value>) {
    if let Some(events) = command.get_mut("events").and_then(Value::as_array_mut) {
        for event in events {
            if event["type"] == "wheel" {
                let x = event["x"].as_f64().unwrap_or(0.0);
                let y = event["y"].as_f64().unwrap_or(0.0);
                let notches = event["notches"].as_f64().unwrap_or_else(|| -event["dy"].as_f64().unwrap_or(0.0) / 100.0);
                if let Some(taker) = presentation.wheel(scene, x, y, notches) {
                    event["presentationTaker"] = json!(taker);
                }
                event["presentation"] = presentation.snapshot();
            } else {
                event["presentation"] = presentation.snapshot_at(drawn);
            }
        }
    }
    command["presentation"] = presentation.snapshot();
}

/// The canvas: a WebGL2 surface (Emscripten's EGL on `Module.canvas`) and a blit of the renderer's
/// Rgba8Unorm frame target into it (texel for texel, no filtering or colour conversion)
pub struct Presenter {
    surface: wgpu::Surface<'static>,
    format: wgpu::TextureFormat,
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    size: (u32, u32),
    alpha: wgpu::CompositeAlphaMode,
}

const BLIT: &str = r#"
@group(0) @binding(0) var source: texture_2d<f32>;
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(p.x * 2.0 - 1.0, 1.0 - p.y * 2.0, 0.0, 1.0);
}
@fragment fn fs(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    return textureLoad(source, vec2<i32>(position.xy), 0);
}
"#;

impl Presenter {
    fn create() -> Result<(GpuRenderer, Presenter)> {
        use raw_window_handle::{RawDisplayHandle, RawWindowHandle, WebDisplayHandle, WebWindowHandle};
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor { backends: wgpu::Backends::GL, ..Default::default() });
        // Emscripten's EGL draws to Module.canvas whatever the handle's id is
        let surface = unsafe {
            instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: RawDisplayHandle::Web(WebDisplayHandle::new()),
                raw_window_handle: RawWindowHandle::Web(WebWindowHandle::new(1)),
            })
        }
        .context("creating the WebGL2 canvas surface")?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .context("no WebGL2 adapter")?;
        let limits = wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits());
        let limits = wgpu::Limits {
            max_sampled_textures_per_shader_stage: adapter.limits().max_sampled_textures_per_shader_stage,
            max_texture_dimension_2d: adapter.limits().max_texture_dimension_2d,
            ..limits
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("RUR web device"),
            required_features: wgpu::Features::empty(),
            required_limits: limits,
            ..Default::default()
        }))
        .context("creating the WebGL2 device")?;
        let device = std::sync::Arc::new(device);
        let queue = std::sync::Arc::new(queue);
        let mut gpu = GpuRenderer::new(device.clone(), queue.clone())?;
        gpu.set_adapter_info(adapter.get_info());
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|format| !format.is_srgb())
            .or_else(|| caps.formats.first().copied())
            .context("the canvas surface has no formats")?;
        let alpha = if caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::Opaque) { wgpu::CompositeAlphaMode::Opaque } else { caps.alpha_modes[0] };
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("RUR canvas blit"), source: wgpu::ShaderSource::Wgsl(BLIT.into()) });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("RUR canvas blit source"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("RUR canvas blit"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("RUR canvas blit"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState { module: &module, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[] },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            }),
            multiview: None,
            cache: None,
        });
        Ok((gpu, Presenter { surface, format, pipeline, layout, size: (0, 0), alpha }))
    }

    fn blit(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, view: &wgpu::TextureView, width: u32, height: u32) -> Result<()> {
        if self.size != (width, height) {
            self.surface.configure(device, &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: self.format,
                width,
                height,
                present_mode: wgpu::PresentMode::Fifo,
                desired_maximum_frame_latency: 1,
                alpha_mode: self.alpha,
                view_formats: vec![],
            });
            self.size = (width, height);
        }
        let output = self.surface.get_current_texture().context("acquiring the canvas texture")?;
        let target = output.texture.create_view(&Default::default());
        let binding = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("RUR canvas blit source"),
            layout: &self.layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) }],
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("RUR canvas blit"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &binding, &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit([encoder.finish()]);
        output.present();
        Ok(())
    }
}
