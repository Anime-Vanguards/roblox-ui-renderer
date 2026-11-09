//! Deterministic capture and replay through the same runtime, assets and GPU as the desktop app.
use std::{
    fs,
    path::Path,
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use crate::{
    NativeOptions,
    assets::{AssetEvent, AssetStore},
    presentation::ScrollPresentation,
    profiler::{PhaseSample, ProfileFrame, Profiler},
    render::{DrawPreparer, gpu::GpuRenderer},
    runtime::{RuntimeEvent, RuntimeOptions, RuntimeWorker},
    scene::SceneStore,
};

const LOAD_TIMEOUT: Duration = Duration::from_secs(300);

/// Reads a batch manifest, a JSON array of `{scene, out}` entries.
fn read_manifest(batch: &Path) -> Result<Value> {
    let bytes = fs::read(batch).with_context(|| format!("Reading {}", batch.display()))?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn manifest_entries(manifest: &Value) -> Result<&Vec<Value>> {
    manifest
        .as_array()
        .context("Batch must be an array of {scene, out} paths")
}

/// Checks every entry's `scene` and `out`, so a release binary refuses an unusable path as a plain
/// error instead of a missing file or an output written to the wrong place.
fn validate_manifest_paths(manifest: &Value) -> Result<()> {
    for entry in manifest_entries(manifest)? {
        for field in ["scene", "out"] {
            crate::paths::request_path(entry, field).map_err(anyhow::Error::msg)?;
        }
    }
    Ok(())
}

pub fn run(options: NativeOptions) -> Result<()> {
    // A release binary validates the whole manifest before it starts a GPU, so a bad later entry
    // cannot leave the outputs of the earlier ones behind.
    let checked_manifest = match &options.batch {
        Some(batch) if crate::paths::is_standalone() => {
            let manifest = read_manifest(batch)?;
            validate_manifest_paths(&manifest)?;
            Some(manifest)
        }
        _ => None,
    };
    let mut assets = AssetStore::new(options.repo.clone(), options.offline);
    let mut gpu = pollster::block_on(GpuRenderer::headless())?;
    let mut preparer = DrawPreparer::new();
    if let Some(batch) = &options.batch {
        let manifest = match checked_manifest {
            Some(manifest) => manifest,
            None => read_manifest(batch)?,
        };
        let entries = manifest_entries(&manifest)?;
        let base = batch.parent().unwrap_or(Path::new("."));
        let mut results = Vec::new();
        for (index, entry) in entries.iter().enumerate() {
            eprintln!("Batch {}/{}: {}", index + 1, entries.len(), entry["scene"]);
            let result = (|| -> Result<Captured> {
                let scene = entry["scene"]
                    .as_str()
                    .context("Batch entry needs a scene path")?;
                let out = entry["out"]
                    .as_str()
                    .context("Batch entry needs an output directory")?;
                let mut capture = options.clone();
                capture.batch = None;
                capture.scene = Some(base.join(scene));
                capture.out = base.join(out);
                // The draw caches carry over from scene to scene (as in the render service): they are
                // keyed by what shapes the pixels. RUR_BATCH_RESET=1 starts every scene cold instead
                // (tools/regress/renderer.mjs --cold compares the two).
                if std::env::var("RUR_BATCH_RESET").as_deref() == Ok("1") {
                    preparer.reset_resources();
                    gpu.reset_scene_resources();
                }
                capture_scene(&capture, &mut assets, &mut gpu, &mut preparer)
            })();
            results.push(match result {
                Ok(frame) => json!({"scene":entry["scene"],"out":entry["out"],"ok":true,"width":frame.width,"height":frame.height,"fnv1a":frame.fnv1a}),
                Err(error) => {
                    eprintln!("Failed: {error:#}");
                    json!({"scene":entry["scene"],"out":entry["out"],"ok":false,"error":format!("{error:#}")})
                }
            });
        }
        fs::create_dir_all(&options.out)?;
        fs::write(
            options.out.join("batch.json"),
            serde_json::to_vec_pretty(&results)?,
        )?;
        if std::env::var("RUR_VERIFY_LAYER_CACHE").as_deref() == Ok("1") {
            use std::sync::atomic::Ordering;
            eprintln!(
                "Layer cache verification: {} hits checked, {} differed",
                crate::render::LAYER_CACHE_VERIFIED.load(Ordering::Relaxed),
                crate::render::LAYER_CACHE_MISMATCHES.load(Ordering::Relaxed)
            );
        }
        let failures = results.iter().filter(|row| row["ok"] == false).count();
        if failures > 0 {
            bail!(
                "{failures}/{} batch scenes failed; see {}/batch.json",
                entries.len(),
                options.out.display()
            );
        }
        // A standalone batch writes under the state directory by default, which the user did not
        // choose, so say where the summary went
        if crate::paths::is_standalone() {
            eprintln!("Saved {}", options.out.join("batch.json").display());
        }
        return Ok(());
    }
    capture_scene(&options, &mut assets, &mut gpu, &mut preparer).map(|_| ())
}

/// The last frame of a capture (render.png): its size and the FNV-1a hash of its RGBA bytes
/// (tools/regress/renderer.mjs compares these)
pub struct Captured {
    pub width: u32,
    pub height: u32,
    pub fnv1a: String,
}

fn capture_scene(
    options: &NativeOptions,
    assets: &mut AssetStore,
    gpu: &mut GpuRenderer,
    preparer: &mut DrawPreparer,
) -> Result<Captured> {
    fs::create_dir_all(&options.out).context("Creating native output directory")?;
    let started = Instant::now();
    let mut ready_ms = None;
    let mut startup_timings = Value::Null;
    let mut scene = SceneStore::default();
    let mut standby: Option<RuntimeWorker> = None;
    let mut runtime = if let Some(story) = &options.story {
        eprintln!("Loading story {}", story.display());
        let worker = RuntimeWorker::spawn(RuntimeOptions::from_native(options, story.clone()))
            .map_err(anyhow::Error::msg)?;
        let mut ready = receive(&worker)?;
        // Font downloads can change shaping. A fresh VM must preload the new font files before
        // its scene is captured, matching desktop story remount behavior.
        for _ in 0..3 {
            if !ready
                .get("diagnostics")
                .is_some_and(|diagnostics| assets.request_fonts(diagnostics))
            {
                break;
            }
            if !wait_assets(assets)?
                .iter()
                .any(|event| matches!(event, AssetEvent::FontsChanged))
            {
                break;
            }
            worker
                .begin_story(RuntimeOptions::from_native(options, story.clone()))
                .map_err(anyhow::Error::msg)?;
            ready = receive(&worker)?;
        }
        if options.strict_assets
            && ready
                .pointer("/diagnostics/missingFontAssets")
                .and_then(Value::as_array)
                .is_some_and(|fonts| !fonts.is_empty())
        {
            bail!("Story still has unresolved font assets after native font preparation");
        }
        ready_ms = Some(started.elapsed().as_secs_f64() * 1000.0);
        startup_timings = ready
            .pointer("/diagnostics/timings")
            .cloned()
            .unwrap_or(Value::Null);
        scene.apply(&ready).map_err(anyhow::Error::msg)?;
        check_scene(&scene)?;
        assets.begin_scene(&scene);
        Some(worker)
    } else {
        let path = options
            .scene
            .as_ref()
            .context("A story or scene is required")?;
        let value: Value = serde_json::from_slice(
            &fs::read(path).with_context(|| format!("Reading {}", path.display()))?,
        )?;
        scene.apply(&value).map_err(anyhow::Error::msg)?;
        assets.load_manifest(&scene, path);
        wait_assets(assets)?;
        None
    };
    settle_assets(assets, &scene, options.strict_assets)?;
    let mut presentation = ScrollPresentation::default();
    presentation.update(&scene, None);
    let mut profiler = Profiler::new();
    profiler
        .metadata
        .insert("source".into(), "headless replay".into());
    profiler.metadata.insert(
        "completion".into(),
        "GPU queue completed for each measured frame".into(),
    );
    if let Some(adapter) = gpu.adapter_info() {
        profiler.metadata.insert(
            "adapter".into(),
            format!("{} ({:?})", adapter.name, adapter.backend),
        );
    }
    let mut replay = Replay::load(options.actions.as_deref(), options.dt)?;
    let runs_runtime = options.benchmark.is_some() || !replay.commands.is_empty();
    let frame_count = options
        .benchmark
        .unwrap_or_else(|| replay.frame_count().max(1));
    if runtime.is_none() && !replay.commands.is_empty() {
        bail!("Input replay requires --story; a saved scene has no running input handlers");
    }
    let mut last_drawn = None;
    let mut captured = None;

    if options.benchmark.is_some() {
        eprintln!(
            "Warming {} frames before measuring {frame_count}",
            options.warmup
        );
        for _ in 0..options.warmup {
            if let Some(worker) = &runtime {
                worker
                    .try_command(json!({"cmd":"frame","dt":options.dt,"cadence":"display","events":[],"presentation":presentation.snapshot()}))
                    .map_err(anyhow::Error::msg)?;
                let mut reply = receive(worker)?;
                scene.apply_owned(&mut reply).map_err(anyhow::Error::msg)?;
                presentation.update(&scene, reply.get("presentation"));
                settle_assets(assets, &scene, options.strict_assets)?;
            }
            presentation.advance(&scene, options.dt);
            let presented = presentation.project(&scene);
            let packet = preparer.prepare(&presented, assets)?;
            gpu.render(&packet)?;
            gpu.wait_idle()?;
            last_drawn = Some(presentation.inspect());
        }
    }

    let setup_ms = started.elapsed().as_secs_f64() * 1000.0;
    let mut measurements = Vec::with_capacity(frame_count as usize);
    let mut last_presented = scene.clone();
    let mut frame_hashes = Vec::new();
    let measured_at = Instant::now();
    for index in 0..frame_count {
        let frame_at = Instant::now();
        let mut runtime_ms = 0.0;
        let mut apply_ms = 0.0;
        let mut reply = Value::Null;
        let mut step_dt = 0.0;
        let mut reopen_ms = None;
        // A `reopen` event: the story is closed and opened again as the desktop app does it
        // (app.rs open_story: the display state reset, a standby VM taken or a new worker spawned,
        // the previous worker stopped on its own thread, the next standby prepared after "ready")
        if let (Some(story), true) = (&options.story, runs_runtime && replay.takes_reopen(index)) {
            let reopen_at = Instant::now();
            let runtime_options = RuntimeOptions::from_native(options, story.clone());
            scene = SceneStore::default();
            presentation = ScrollPresentation::default();
            last_drawn = None;
            preparer.reset_resources();
            let worker = match standby.take().filter(|standby| standby.standby_matches(&runtime_options)) {
                Some(standby) => standby.begin_story(runtime_options.clone()).map(|()| standby),
                None => RuntimeWorker::spawn(runtime_options.clone()),
            }
            .map_err(anyhow::Error::msg)?;
            let ready = receive(&worker)?;
            runtime = Some(worker);
            scene.apply(&ready).map_err(anyhow::Error::msg)?;
            check_scene(&scene)?;
            assets.begin_scene(&scene);
            presentation.update(&scene, None);
            standby = RuntimeWorker::spawn_standby(runtime_options).ok();
            reopen_ms = Some(reopen_at.elapsed().as_secs_f64() * 1000.0);
        }
        if let Some(worker) = runtime.as_ref().filter(|_| runs_runtime) {
            let mut command = replay.command(index, options.dt);
            step_dt = command.get("dt").and_then(Value::as_f64).unwrap_or(0.0);
            prepare_command(&mut command, &scene, &mut presentation, last_drawn.as_ref());
            let before_runtime = Instant::now();
            worker.try_command(command).map_err(anyhow::Error::msg)?;
            reply = receive(worker)?;
            runtime_ms = before_runtime.elapsed().as_secs_f64() * 1000.0;
            let apply_at = Instant::now();
            scene.apply_owned(&mut reply).map_err(anyhow::Error::msg)?;
            apply_ms = apply_at.elapsed().as_secs_f64() * 1000.0;
            check_scene(&scene)?;
            presentation.update(&scene, reply.get("presentation"));
            profiler.record_runtime(&reply);
        }
        let asset_at = Instant::now();
        let asset_request_ms = settle_assets(assets, &scene, options.strict_assets)?;
        let asset_ms = asset_at.elapsed().as_secs_f64() * 1000.0;
        let draw_at = Instant::now();
        presentation.advance(&scene, step_dt);
        let presented = presentation.project(&scene);
        let project_ms = draw_at.elapsed().as_secs_f64() * 1000.0;
        let packet = preparer.prepare(&presented, assets)?;
        let prepare_ms = draw_at.elapsed().as_secs_f64() * 1000.0;
        let submit_at = Instant::now();
        let frame = gpu.render(&packet)?;
        let submit_ms = submit_at.elapsed().as_secs_f64() * 1000.0;
        let cpu_draw_ms = draw_at.elapsed().as_secs_f64() * 1000.0;
        let wait_at = Instant::now();
        gpu.wait_idle()?;
        let gpu_wait_ms = wait_at.elapsed().as_secs_f64() * 1000.0;
        let completed_ms = frame_at.elapsed().as_secs_f64() * 1000.0;
        let stamp = measured_at.elapsed().as_secs_f64() * 1000.0;
        let motion = presentation.inspect();
        let draw_stats = preparer.last_stats();
        let mut draw_phases = [
            ("Draw backgrounds", draw_stats.background_ms),
            ("Draw images", draw_stats.images_ms),
            ("Draw text", draw_stats.text_ms),
            ("Draw strokes", draw_stats.strokes_ms),
            ("Draw shadows", draw_stats.shadows_ms),
            ("Draw groups", draw_stats.groups_ms),
            ("Draw scrollbars", draw_stats.scrollbars_ms),
            ("Draw cache", draw_stats.node_cache_ms),
            ("Draw rasterize (parallel)", draw_stats.deferred_raster_ms),
            ("Draw other", draw_stats.other_ms),
            ("GPU encode/submit", submit_ms),
        ]
        .into_iter()
        .map(|(name, duration_ms)| PhaseSample {
            name: name.into(),
            duration_ms,
        })
        .collect();
        let mut draw_counters = std::collections::BTreeMap::new();
        crate::profiler::append_draw_details(draw_stats, &mut draw_phases, &mut draw_counters);
        profiler.record(ProfileFrame {
            timestamp_ms: stamp,
            frame_interval_ms: completed_ms,
            cpu_draw_ms,
            phases: draw_phases,
            counters: draw_counters,
            scroll_active: presentation
                .states
                .values()
                .any(|state| state.owned && state.position != state.target),
            scroll_session: motion["active"].as_u64(),
            story: options
                .story
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_else(|| "saved scene".into())
                .into(),
            ..Default::default()
        });
        measurements.push(json!({"frame":index,"runtimeRoundTripMs":runtime_ms,"assetWaitMs":asset_ms,"assetRequestMs":asset_request_ms,
            "prepareMs":prepare_ms,"projectMs":project_ms,"projectedNodes":presentation.projected_nodes,"applyMs":apply_ms,"submitMs":submit_ms,"cpuDrawMs":cpu_draw_ms,"gpuQueueWaitMs":gpu_wait_ms,
            "completedFrameMs":completed_ms,"nodes":scene.node_count(),"timings":reply.get("timings"),"reopenMs":reopen_ms,"processBytes":crate::memtrace::process_bytes().map(|(private, _)| private),"memory":crate::memtrace::reporting().then(crate::memtrace::live_json),
            "scroll":motion,"draw":draw_stats,"gpu":{"drawCalls":frame.stats.draw_calls,"instances":frame.stats.instances,"passes":frame.stats.passes,"uploadBytes":frame.stats.upload_bytes,"redrawnPixels":frame.stats.redrawn_pixels,"atlasBatches":frame.stats.atlas_batches,"damageMs":frame.stats.damage_ms,"recordsMs":frame.stats.records_ms,"bindMs":frame.stats.bind_ms,"encodeMs":frame.stats.encode_ms,"queueSubmitMs":frame.stats.submit_ms,"finishMs":frame.stats.finish_ms,"gpuCpuMs":frame.stats.cpu_ms,"lastCompletedGpuMs":frame.stats.gpu_ms}}));
        last_drawn = Some(motion);
        if options.frame_hashes {
            frame_hashes.push(fnv1a(&gpu.readback_png_rgba()?));
        }
        if options.save_frames {
            let directory = options.out.join("frames");
            fs::create_dir_all(&directory)?;
            let store = FrameStore::new(&options.out)?;
            store.scene(&directory.join(format!("{index:05}.json")), &presented)?;
            store.png(
                gpu,
                &directory.join(format!("{index:05}.png")),
                frame.width,
                frame.height,
            )?;
        }
        if index + 1 == frame_count {
            let fnv1a = save_png(
                gpu,
                &options.out.join("render.png"),
                frame.width,
                frame.height,
            )?;
            captured = Some(Captured { width: frame.width, height: frame.height, fnv1a });
        }
        if (index + 1) % 120 == 0 {
            eprintln!("Rendered {}/{frame_count} frames", index + 1);
        }
        last_presented = presented;
    }
    save_scene(&options.out.join("scene.json"), &last_presented)?;
    if options.frame_hashes {
        fs::write(options.out.join("frames.json"), serde_json::to_vec(&json!({"frames": frame_hashes}))?)?;
    }
    profiler.export(&options.out.join("trace.json"))?;
    let summaries = [
        "runtimeRoundTripMs",
        "assetWaitMs",
        "assetRequestMs",
        "prepareMs",
        "projectMs",
        "applyMs",
        "submitMs",
        "cpuDrawMs",
        "gpuQueueWaitMs",
        "completedFrameMs",
    ]
    .into_iter()
    .map(|key| {
        (
            key.to_string(),
            distribution(
                measurements
                    .iter()
                    .filter_map(|row| row[key].as_f64())
                    .collect(),
            ),
        )
    })
    .collect::<serde_json::Map<_, _>>();
    let verified = {
        use std::sync::atomic::Ordering;
        json!({"verified":crate::render::LAYER_CACHE_VERIFIED.load(Ordering::Relaxed),
            "mismatches":crate::render::LAYER_CACHE_MISMATCHES.load(Ordering::Relaxed)})
    };
    let report = json!({"schema":1,"mode":"native-headless","setupMs":setup_ms,"readyMs":ready_ms,"layerCacheVerification":verified,
        "startupTimings":startup_timings,"frames":frame_count,
        "dt":options.dt,"warmup":if options.benchmark.is_some(){options.warmup}else{0},"summary":summaries,
        "measurements":measurements,"assetDiagnostics":assets.diagnostics(),"retainedPresentation":runtime.is_some() && runs_runtime,
        "notes":["Completed frame time includes runtime, CPU preparation/submission and a GPU queue wait.",
            "GPU queue wait is wall time, not a GPU timestamp measurement.",
            "This sequential throughput replay does not measure desktop presentation latency or monitor refresh.",
            "PNG readback and file writing are excluded from individual frame timings."]});
    fs::write(
        options.out.join("benchmark.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!(
        "{}",
        json!({"output":options.out,"frames":frame_count,"readyMs":ready_ms,"summary":report["summary"]})
    );
    if let Some(worker) = runtime {
        worker.stop();
    }
    captured.context("No frame was captured")
}

fn prepare_command(
    command: &mut Value,
    scene: &SceneStore,
    presentation: &mut ScrollPresentation,
    drawn: Option<&Value>,
) {
    if command["cmd"] != "frame" || command.get("presentation").is_some() {
        return;
    }
    if let Some(events) = command.get_mut("events").and_then(Value::as_array_mut) {
        for event in events {
            if event["type"] == "wheel" {
                let x = event["x"].as_f64().unwrap_or(0.0);
                let y = event["y"].as_f64().unwrap_or(0.0);
                let notches = event["notches"]
                    .as_f64()
                    .unwrap_or_else(|| -event["dy"].as_f64().unwrap_or(0.0) / 100.0);
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

fn receive(worker: &RuntimeWorker) -> Result<Value> {
    match worker
        .recv_timeout(LOAD_TIMEOUT)
        .context("Waiting for embedded story runtime")?
    {
        RuntimeEvent::Reply(value) => {
            if value["type"] == "error" {
                bail!(
                    "{}",
                    value["message"]
                        .as_str()
                        .unwrap_or("Runtime command failed")
                );
            }
            Ok(value)
        }
        RuntimeEvent::Error(error) => bail!("{error}"),
        RuntimeEvent::Stopped => bail!("Runtime stopped before replying"),
    }
}

fn check_scene(scene: &SceneStore) -> Result<()> {
    if scene.header().get("status").and_then(Value::as_str) == Some("error") {
        bail!(
            "Story failed: {}",
            scene.header().get("failure").unwrap_or(&Value::Null)
        );
    }
    Ok(())
}

/// Requests what the scene still needs and waits for it; returns the request's own time (ms)
fn settle_assets(assets: &mut AssetStore, scene: &SceneStore, strict: bool) -> Result<f64> {
    let _memory = crate::memtrace::scope(crate::memtrace::Tag::Assets);
    let started = Instant::now();
    assets.request_missing(scene);
    let request_ms = started.elapsed().as_secs_f64() * 1000.0;
    wait_assets(assets)?;
    if strict && !assets.diagnostics().is_empty() {
        bail!(
            "Asset preparation failed: {}",
            assets.diagnostics().join("; ")
        );
    }
    Ok(request_ms)
}

fn wait_assets(assets: &mut AssetStore) -> Result<Vec<AssetEvent>> {
    let deadline = Instant::now() + LOAD_TIMEOUT;
    let mut events = Vec::new();
    loop {
        events.extend(assets.poll());
        if assets.pending_count() == 0 {
            break;
        }
        if Instant::now() >= deadline {
            bail!("Timed out preparing native assets");
        }
        thread::sleep(Duration::from_millis(2));
    }
    Ok(events)
}

/// Writes the frame as a PNG; returns the FNV-1a hash (hex) of its RGBA bytes.
fn save_png(gpu: &mut GpuRenderer, path: &Path, width: u32, height: u32) -> Result<String> {
    let rgba = gpu.readback_png_rgba()?;
    crate::story_render::write_png(path, &rgba, width, height)?;
    Ok(fnv1a(&rgba))
}

/// FNV-1a (32-bit) of `bytes` as lower-case hex
pub fn fnv1a(bytes: &[u8]) -> String {
    let mut hash: u32 = 0x811c9dc5;
    for &byte in bytes {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    format!("{hash:x}")
}

/// `--save-frames` files by content: a frame's PNG and scene are written once into `<out>/.frames`
/// under their SHA-256 and `frames/NNNNN.*` are hard links to them (story_render.rs link_or_copy), so
/// a frame that repeats one already stored (a still stretch, an A/B side identical to the other in a
/// directory beside it) costs neither the bytes nor the PNG encode. The store is shared by the
/// directories under one parent (the `{before,after}-{run}` of tools/native-app/scroll-bench.mjs);
/// `rur clean` drops entries no frame links to any more. Standalone builds keep the store inside
/// the named output directory instead, so a capture writes only there.
struct FrameStore {
    directory: std::path::PathBuf,
}

impl FrameStore {
    fn new(out: &Path) -> Result<Self> {
        // A release build writes only inside the directory it was told to write to; checkout builds
        // keep the store beside it, shared with the sibling directories of one benchmark.
        let parent = if crate::paths::is_standalone() { out } else { out.parent().unwrap_or(out) };
        let directory = parent.join(".frames");
        fs::create_dir_all(&directory).with_context(|| format!("Creating {}", directory.display()))?;
        Ok(Self { directory })
    }

    fn png(&self, gpu: &mut GpuRenderer, path: &Path, width: u32, height: u32) -> Result<()> {
        use sha2::Digest;
        let rgba = gpu.readback_png_rgba()?;
        let digest = sha2::Sha256::new()
            .chain_update(&rgba)
            .chain_update(width.to_le_bytes())
            .chain_update(height.to_le_bytes())
            .finalize();
        let key: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
        let stored = self.directory.join(format!("{key}.png"));
        if !stored.exists() {
            let temporary = self.temporary(&stored);
            crate::story_render::write_png(&temporary, &rgba, width, height)?;
            fs::rename(&temporary, &stored).with_context(|| format!("Writing {}", stored.display()))?;
        }
        crate::story_render::link_or_copy(&stored, path)
    }

    fn scene(&self, path: &Path, scene: &SceneStore) -> Result<()> {
        let bytes = serde_json::to_vec(&scene.to_value())?;
        let stored = self.directory.join(format!("{}.json", crate::engine::text::hinted::sha256_hex(&bytes)));
        if !stored.exists() {
            let temporary = self.temporary(&stored);
            fs::write(&temporary, &bytes).with_context(|| format!("Writing {}", temporary.display()))?;
            fs::rename(&temporary, &stored).with_context(|| format!("Writing {}", stored.display()))?;
        }
        crate::story_render::link_or_copy(&stored, path)
    }

    /// Beside `stored`, unique to this process (two captures may store the same frame at once)
    fn temporary(&self, stored: &Path) -> std::path::PathBuf {
        stored.with_extension(format!("{}.tmp", std::process::id()))
    }
}

fn save_scene(path: &Path, scene: &SceneStore) -> Result<()> {
    fs::write(path, serde_json::to_vec(&scene.to_value())?)
        .with_context(|| format!("Writing {}", path.display()))
}

fn distribution(mut values: Vec<f64>) -> Value {
    values.retain(|value| value.is_finite());
    values.sort_by(f64::total_cmp);
    if values.is_empty() {
        return Value::Null;
    }
    let percentile = |p: f64| values[((values.len() - 1) as f64 * p).round() as usize];
    json!({"min":values[0],"median":percentile(0.5),"p95":percentile(0.95),"max":values[values.len()-1],"mean":values.iter().sum::<f64>()/values.len() as f64})
}

struct Replay {
    /// Frames that reopen the story first (`{"type":"reopen","t":..}` events)
    reopens: std::collections::BTreeSet<u32>,
    commands: Vec<Value>,
}

impl Replay {
    fn load(path: Option<&Path>, dt: f64) -> Result<Self> {
        let Some(path) = path else {
            return Ok(Self {
                commands: Vec::new(),
                reopens: Default::default(),
            });
        };
        let value: Value = serde_json::from_slice(
            &fs::read(path).with_context(|| format!("Reading actions {}", path.display()))?,
        )?;
        let rows = value
            .as_array()
            .context("Actions must be a JSON array of frame commands or timed input events")?;
        if rows.iter().all(|row| row.get("cmd").is_some()) {
            if rows.iter().any(|row| {
                row["cmd"] != "frame" && row["cmd"] != "control" && row["cmd"] != "scene"
            }) {
                bail!("Replay commands must be frame, control or scene");
            }
            return Ok(Self {
                commands: rows.clone(),
                reopens: Default::default(),
            });
        }
        let mut commands = Vec::new();
        let mut reopens = std::collections::BTreeSet::new();
        for event in rows {
            if !event.is_object() || event["type"].as_str().is_none() {
                bail!("Each replay event needs a type string");
            }
            // Located against the layout when a timeline reaches them (engine/input/actions.rs)
            if crate::engine::input::actions::is_compound(event) {
                bail!("click / drag actions run in `rur render --actions` timelines; a native replay takes move, down and up events");
            }
            let time = event.get("t").and_then(Value::as_f64).unwrap_or(0.0);
            if !time.is_finite() || time < 0.0 || time > 3600.0 {
                bail!("Replay event times must be finite and within one hour");
            }
            let frame = (time / dt).floor() as usize;
            while commands.len() <= frame {
                commands.push(json!({"cmd":"frame","dt":dt,"cadence":"display","events":[]}));
            }
            if event["type"] == "reopen" {
                reopens.insert(frame as u32);
                continue;
            }
            let mut event = event.clone();
            event.as_object_mut().unwrap().remove("t");
            commands[frame]["events"]
                .as_array_mut()
                .unwrap()
                .push(event);
        }
        Ok(Self { commands, reopens })
    }
    fn takes_reopen(&self, frame: u32) -> bool {
        self.reopens.contains(&frame)
    }
    fn frame_count(&self) -> u32 {
        self.commands.len() as u32
    }
    fn command(&mut self, frame: u32, dt: f64) -> Value {
        let mut value = self
            .commands
            .get(frame as usize)
            .cloned()
            .unwrap_or_else(|| json!({"cmd":"frame","dt":dt,"cadence":"display","events":[]}));
        if value["cmd"] == "frame" && value.get("dt").is_none() {
            value["dt"] = json!(dt);
        }
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn summary_reports_actual_percentiles_and_skips_nonfinite_values() {
        assert_eq!(
            distribution(vec![4.0, 1.0, 3.0, 2.0, f64::NAN]),
            json!({"min":1.0,"median":3.0,"p95":4.0,"max":4.0,"mean":2.5})
        );
        assert!(distribution(Vec::new()).is_null());
    }
    #[test]
    fn empty_replay_advances_with_explicit_display_cadence() {
        let mut replay = Replay::load(None, 1.0 / 120.0).unwrap();
        assert_eq!(replay.frame_count(), 0);
        assert_eq!(
            replay.command(0, 0.01),
            json!({"cmd":"frame","dt":0.01,"cadence":"display","events":[]})
        );
    }
}
