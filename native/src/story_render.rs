//! `rur render`: a story (module, model, fixture) run once and written into an output directory:
//! scene.json, dependencies.json, the glyph atlas (glyphs.png + glyphs.json), the images it uses
//! (assets/), manifest.json and render.png, drawn by the native renderer. `rur-native render ...`
//! does one in a process of its own; the render service (serve.rs) keeps a [`StoryRenderer`] with its
//! GPU device, glyph and texture state warm and runs the story in a preloaded VM.
//! cli/rur.mjs builds summary.json from the scene and the result this returns.
use std::{
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Map, Value, json};

use crate::{
    assets::AssetStore,
    engine::{
        cli::{Args, SceneOutput},
        profiler,
    },
    presentation::ScrollPresentation,
    render::{DrawPreparer, gpu::GpuRenderer},
    resolve::Resolver,
    scene::SceneStore,
};

const ASSET_TIMEOUT: Duration = Duration::from_secs(300);

/// What a render writes besides the scene.
#[derive(Clone, Copy, Debug, Default)]
pub struct RenderOptions {
    /// Draw render.png
    pub png: bool,
    /// No downloads: images and fonts from the caches only
    pub offline: bool,
    /// Draw a labelled placeholder for each image that could not be had (placeholder.rs); it is still
    /// reported missing
    pub placeholders: bool,
    /// Draw, but hand the frame back as straight-alpha RGBA (the render service's `pixels: "rgba"`)
    /// instead of encoding render.png
    pub pixels: bool,
}

/// A frame as straight-alpha RGBA8 rows (render.png's pixels)
pub struct Pixels {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub struct StoryRenderer {
    repo: PathBuf,
    resolver: Resolver,
    /// Created on the first scene with text (FreeType, the faces and a bitmap cache)
    glyphs: Option<crate::glyphs::Rasterizer>,
    draw: Option<Draw>,
    /// The render being written draws placeholders for missing images (RenderOptions::placeholders)
    placeholders: bool,
}

struct Draw {
    gpu: GpuRenderer,
    preparer: DrawPreparer,
    assets: AssetStore,
}

impl StoryRenderer {
    pub fn new(repo: impl Into<PathBuf>) -> Self {
        let repo = repo.into();
        Self { resolver: Resolver::new(repo.clone(), false), glyphs: None, draw: None, placeholders: false, repo }
    }

    /// What this renderer keeps between renders, in bytes (the render service's /memory)
    pub fn memory(&self) -> Value {
        let mut report = Map::new();
        if let Some(draw) = &self.draw {
            report.insert("assetsResident".into(), json!(draw.assets.resident_bytes()));
            report.insert("prepare".into(), draw.preparer.memory());
            report.insert("gpu".into(), draw.gpu.memory());
        }
        report.insert("glyphRasterizer".into(), json!(self.glyphs.is_some()));
        Value::Object(report)
    }

    /// Gives back the frame-sized buffers a draw keeps for the next one (the readback buffer) and
    /// lets the GPU free what finished work left (upload staging): the render service, between bursts
    pub fn trim(&self) {
        if let Some(draw) = &self.draw {
            draw.gpu.release_readback();
            let _ = draw.gpu.device().poll(wgpu::PollType::Poll);
        }
    }

    /// Frees what renders keep for the renders after them, once none came for a while: the draw caches
    /// (layers, mips, records, uploaded textures), the decoded images and the asset store's residents.
    /// The GPU device, its pipelines, the glyph rasterizer's faces and the resolver stay, so the next
    /// render only decodes and uploads again what it draws.
    pub fn release(&mut self) {
        if let Some(draw) = &mut self.draw {
            draw.preparer = DrawPreparer::new();
            draw.gpu.release_scene_textures();
            draw.assets = AssetStore::new(self.repo.clone(), true);
            let _ = draw.gpu.device().poll(wgpu::PollType::wait_indefinitely());
        }
        crate::assets::clear_decoded_images();
    }

    /// Creates the GPU device ahead of the first PNG (the service does it while idle).
    pub fn warm_gpu(&mut self) -> Result<()> {
        self.draw()?;
        Ok(())
    }

    /// A sequence (sequence.rs) written into `out`: `start` starts its engine half for `args` (a standby
    /// VM's or a fresh one, on a thread of its own) and returns its frames; when the first frame's mount
    /// reports cloud fonts that are not cached yet, they are fetched, `fonts_changed` is called and the
    /// sequence starts again in a fresh engine (as a render runs again: text is laid out with the real
    /// faces). Returns the result cli/rur.mjs makes summary.json from.
    pub fn sequence(
        &mut self,
        args: &Args,
        out: &Path,
        options: &crate::sequence::SequenceOutput,
        start: &mut dyn FnMut(&Args) -> Result<std::sync::mpsc::Receiver<crate::engine::cli::SequenceMessage>, String>,
        mut fonts_changed: impl FnMut(),
    ) -> Result<Value> {
        use crate::engine::cli::SequenceMessage;
        let started = Instant::now();
        std::fs::create_dir_all(out).with_context(|| format!("Creating {}", out.display()))?;
        self.resolver.set_offline(options.offline);
        // A process of its own starts its GPU device while the story mounts (the service's is warm)
        let gpu = self.draw.is_none().then(|| thread::spawn(|| pollster::block_on(GpuRenderer::headless())));
        let first = |frames: &std::sync::mpsc::Receiver<SequenceMessage>| match frames.recv() {
            Ok(SequenceMessage::Frame(frame)) => Ok(frame),
            Ok(SequenceMessage::End(Err(error))) => Err(anyhow!("runtime failed: {error}")),
            Ok(SequenceMessage::End(Ok(_))) | Err(_) => Err(anyhow!("runtime failed: the sequence ended before its first frame")),
        };
        let mut frames = start(args).map_err(|error| anyhow!("runtime failed: {error}"))?;
        let mut frame = first(&frames)?;
        for _ in 0..3 {
            let diagnostics = &frame.reply["diagnostics"];
            if options.offline || !diagnostics["missingFontAssets"].as_array().is_some_and(|fonts| !fonts.is_empty()) {
                break;
            }
            if !self.resolver.fetch_fonts(diagnostics).unwrap_or(false) {
                break;
            }
            fonts_changed();
            // (the engine stops at its next frame once nobody takes them)
            drop(frames);
            frames = start(args).map_err(|error| anyhow!("runtime failed: {error}"))?;
            frame = first(&frames)?;
        }
        if let Some(gpu) = gpu {
            let gpu = gpu.join().map_err(|_| anyhow!("GPU start panicked"))??;
            self.draw = Some(Draw { gpu, preparer: DrawPreparer::new(), assets: AssetStore::new(self.repo.clone(), true) });
        }
        let Self { draw, resolver, .. } = self;
        let draw = draw.as_mut().expect("the GPU device was started above");
        crate::sequence::draw_frames(&mut draw.gpu, &mut draw.preparer, &mut draw.assets, resolver, frame, &frames, out, options, started)
    }

    fn draw(&mut self) -> Result<&mut Draw> {
        if self.draw.is_none() {
            let gpu = pollster::block_on(GpuRenderer::headless())?;
            self.draw = Some(Draw { gpu, preparer: DrawPreparer::new(), assets: AssetStore::new(self.repo.clone(), true) });
        }
        Ok(self.draw.as_mut().unwrap())
    }

    /// Runs `args` (the first run through `first`, e.g. a preloaded VM; runs again after cloud fonts
    /// arrived in a VM of its own) and writes the output directory `out`. `fonts_changed` is called
    /// when new fonts were downloaded (preloaded VMs are then stale). Returns the result the CLI's
    /// summary is made from: `{phases: {runtime, glyphs, images, png}, renderErrors, status, nodes}`.
    pub fn render(
        &mut self,
        args: &Args,
        out: &Path,
        options: RenderOptions,
        first: impl FnOnce(&Args) -> Result<SceneOutput, String>,
        fonts_changed: impl FnMut(),
    ) -> Result<(Value, Option<Pixels>)> {
        self.placeholders = options.placeholders;
        let (mut result, scene, images) = self.write_outputs(args, out, options.offline, true, first, fonts_changed)?;
        let mut pixels = None;
        if options.pixels {
            result["summary"] = pixels_summary(&scene, &images);
            pixels = self.png_into(&mut result, &scene, out, false);
        } else if options.png {
            self.png_into(&mut result, &scene, out, true);
        }
        Ok((result, pixels))
    }

    /// Everything but render.png: the run, scene.json, dependencies.json, the glyph atlas, the
    /// images and manifest.json. Returns the result and the scene. `runs`: false for a saved scene
    /// (`first` returns it; nothing runs again and there is no dependencies.json).
    fn write_outputs(
        &mut self,
        args: &Args,
        out: &Path,
        offline: bool,
        runs: bool,
        first: impl FnOnce(&Args) -> Result<SceneOutput, String>,
        mut fonts_changed: impl FnMut(),
    ) -> Result<(Value, Value, Map<String, Value>)> {
        std::fs::create_dir_all(out).with_context(|| format!("Creating {}", out.display()))?;
        self.resolver.set_offline(offline);
        let mut args = args.clone();
        args.offline = offline;
        args.dump_scene = Some(out.join("scene.json").to_string_lossy().into_owned());
        if runs {
            args.dependencies = Some(out.join("dependencies.json").to_string_lossy().into_owned());
            let _ = std::fs::remove_file(out.join("scene.json"));
        }
        let mut phases = Map::new();
        let mut clock = Instant::now();
        let mut phase = |name: &str, phases: &mut Map<String, Value>| {
            phases.insert(name.into(), seconds(clock));
            clock = Instant::now();
        };

        let mut output = first(&args).map_err(|error| anyhow!("runtime failed: {error}"))?;
        // Cloud fonts the UI uses that are not cached yet: fetched, then the run repeats with them
        // (a family's JSON first, then its faces), as the text is laid out with the real faces
        for _ in 0..3 {
            if offline || !runs {
                break;
            }
            let diagnostics = &output.scene["diagnostics"];
            if !diagnostics["missingFontAssets"].as_array().is_some_and(|fonts| !fonts.is_empty()) {
                break;
            }
            if !self.resolver.fetch_fonts(diagnostics).unwrap_or(false) {
                break;
            }
            fonts_changed();
            output = crate::engine::cli::run_once(&args).map_err(|error| anyhow!("runtime failed: {error}"))?;
        }
        output.write(&args, Instant::now()).map_err(|error| anyhow!(error))?;
        phase("runtime", &mut phases);

        // Glyph atlas: every glyph variant the scene draws (glyphs.rs)
        if self.glyphs.is_none() {
            self.glyphs = Some(crate::glyphs::Rasterizer::new()?);
        }
        let atlas = crate::glyphs::build_scene_atlas(self.glyphs.as_mut().unwrap(), &output.scene, out)?.unwrap_or(Value::Null);
        phase("glyphs", &mut phases);

        // Images: resolved, linked into assets/ (link_or_copy; a stale entry, e.g. a refetched asset
        // under the same name, is replaced)
        let uris: Vec<String> = output.scene["images"].as_array().into_iter().flatten().filter_map(|uri| uri.as_str().map(str::to_string)).collect();
        let mut images = Map::new();
        if !uris.is_empty() {
            let assets = out.join("assets");
            std::fs::create_dir_all(&assets)?;
            for (uri, resolution) in self.resolver.resolve_scene(&output.scene, &uris) {
                let entry = match resolution {
                    Ok(file) => {
                        let name = file.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
                        let target = assets.join(&name);
                        if copy_stale(&file, &target) {
                            link_or_copy(&file, &target)?;
                        }
                        crate::assets::note_linked_image(&target, &file);
                        json!({"src": format!("assets/{name}")})
                    }
                    // --placeholders: drawn with a labelled stand-in, and still reported missing (its error)
                    Err(error) => match self.placeholders.then(|| crate::placeholder::write(&assets, &uri)) {
                        Some(Ok(name)) => json!({"src": format!("assets/{name}"), "error": error, "placeholder": true}),
                        _ => json!({"error": error}),
                    },
                };
                images.insert(uri, entry);
            }
        }
        let manifest = json!({"images": images, "atlas": atlas, "reference": null});
        std::fs::write(out.join("manifest.json"), pretty(&manifest))?;
        phase("images", &mut phases);
        let result = json!({"status": output.status, "nodes": output.nodes, "phases": phases, "renderErrors": []});
        let Value::Object(manifest) = manifest else { unreachable!() };
        let images = match manifest.into_iter().find(|(key, _)| key == "images") {
            Some((_, Value::Object(images))) => images,
            _ => Map::new(),
        };
        Ok((result, output.scene, images))
    }

    /// render.png of a written directory (`encode`), else its pixels; its time and any error in `result`
    fn png_into(&mut self, result: &mut Value, scene: &Value, out: &Path, encode: bool) -> Option<Pixels> {
        let clock = Instant::now();
        let pixels = match self.draw_png(scene, out, encode) {
            Ok((steps, pixels)) => {
                result["pngSteps"] = steps;
                pixels
            }
            Err(error) => {
                result["renderErrors"].as_array_mut().unwrap().push(json!(format!("{error:#}")));
                None
            }
        };
        result["phases"]["png"] = seconds(clock);
        pixels
    }

    /// Draws the written directory's scene with its manifest (as `rur-native --headless --scene`)
    /// into render.png (`encode`), else returns the pixels render.png would hold.
    fn draw_png(&mut self, scene_value: &Value, out: &Path, encode: bool) -> Result<(Value, Option<Pixels>)> {
        let mut steps = Map::new();
        let mut clock = Instant::now();
        let mut step = |name: &str| {
            steps.insert(name.into(), json!((clock.elapsed().as_secs_f64() * 10000.0).round() / 10.0));
            clock = Instant::now();
        };
        let draw = self.draw()?;
        step("device");
        let mut scene = SceneStore::default();
        scene.apply(scene_value).map_err(|error| anyhow!(error))?;
        // The draw caches carry over from render to render: they are keyed by what shapes the pixels
        // (node records by the live node, layers by content, textures by their decoded allocation, glyph
        // pages by the asset revision), as between the frames of a story in the desktop app
        step("scene");
        draw.assets.load_manifest(&scene, out.join("scene.json"));
        wait_assets(&mut draw.assets)?;
        draw.assets.request_missing(&scene);
        wait_assets(&mut draw.assets)?;
        step("assets");
        let mut presentation = ScrollPresentation::default();
        presentation.update(&scene, None);
        presentation.advance(&scene, 0.0);
        let presented = presentation.project(&scene);
        let packet = draw.preparer.prepare(&presented, &draw.assets)?;
        step("prepare");
        let stats = serde_json::to_value(draw.preparer.last_stats()).unwrap_or_default();
        let frame = draw.gpu.render(&packet)?;
        draw.gpu.wait_idle()?;
        step("gpu");
        let rgba = draw.gpu.readback_png_rgba()?;
        step("readback");
        steps.insert("draw".into(), stats);
        if !encode {
            return Ok((Value::Object(steps), Some(Pixels { width: frame.width, height: frame.height, rgba })));
        }
        write_png(&out.join("render.png"), &rgba, frame.width, frame.height)?;
        steps.insert("encode".into(), json!((clock.elapsed().as_secs_f64() * 10000.0).round() / 10.0));
        Ok((Value::Object(steps), None))
    }
}

/// What cli/rur.mjs's summary reports of a render, for a client that takes its pixels and reads no
/// file: the scene's status, whether it has a background, the images that failed (manifest.json's
/// `images` entries with an error, as `uri: error`), the missing cloud fonts and the script errors
fn pixels_summary(scene: &Value, images: &Map<String, Value>) -> Value {
    let missing_images: Vec<String> = images
        .iter()
        .filter_map(|(uri, entry)| {
            let error = entry.get("error")?;
            // JavaScript's truthiness, as cli/rur.mjs filters the entries
            let truthy = match error {
                Value::Null => false,
                Value::Bool(flag) => *flag,
                Value::Number(number) => number.as_f64().is_some_and(|number| number != 0.0),
                Value::String(text) => !text.is_empty(),
                _ => true,
            };
            truthy.then(|| format!("{uri}: {}", error.as_str().map(str::to_string).unwrap_or_else(|| error.to_string())))
        })
        .collect();
    let diagnostics = &scene["diagnostics"];
    let script_errors = diagnostics["output"].as_array().into_iter().flatten().filter(|entry| entry["kind"] == "error").count();
    json!({
        "status": scene["status"],
        "transparent": scene.get("background").is_some_and(Value::is_null),
        "missingImages": missing_images,
        "missingFonts": diagnostics["missingFontAssets"].as_array().cloned().unwrap_or_default(),
        "scriptErrors": script_errors,
    })
}

fn wait_assets(assets: &mut AssetStore) -> Result<()> {
    let deadline = Instant::now() + ASSET_TIMEOUT;
    loop {
        assets.poll();
        if assets.pending_count() == 0 {
            return Ok(());
        }
        if Instant::now() >= deadline {
            bail!("Timed out preparing native assets");
        }
        thread::sleep(Duration::from_millis(1));
    }
}

/// An RGBA PNG, fast compression (the pixels are what matter; `render.png` was never byte-stable)
pub fn write_png(path: &Path, rgba: &[u8], width: u32, height: u32) -> Result<()> {
    use image::ImageEncoder;
    let file = std::io::BufWriter::new(std::fs::File::create(path).with_context(|| format!("Writing {}", path.display()))?);
    image::codecs::png::PngEncoder::new_with_quality(file, image::codecs::png::CompressionType::Fast, image::codecs::png::FilterType::Adaptive)
        .write_image(rgba, width, height, image::ExtendedColorType::Rgba8)
        .with_context(|| format!("Writing {}", path.display()))
}

/// A saved scene as a run's output
fn saved_scene(file: &Path) -> Result<SceneOutput, String> {
    let text = std::fs::read_to_string(file).map_err(|error| format!("{}: {error}", file.display()))?;
    let scene: Value = serde_json::from_str(&text).map_err(|error| format!("{}: {error}", file.display()))?;
    Ok(SceneOutput {
        status: scene["status"].as_str().unwrap_or("ok").to_string(),
        nodes: scene["nodes"].as_array().map_or(0, Vec::len),
        lines: scene["diagnostics"]["output"].as_array().map_or(0, Vec::len),
        dependencies: Vec::new(),
        scene,
        text,
        frames: 0,
        seconds: 0.0,
        fps: 0.0,
    })
}

fn seconds(since: Instant) -> Value {
    json!((since.elapsed().as_secs_f64() * 1000.0).round() / 1000.0)
}

/// cli/rur.mjs assetCopyStale: missing, a different size, or older than the cache file
fn copy_stale(source: &Path, target: &Path) -> bool {
    let (Ok(from), Ok(to)) = (std::fs::metadata(source), std::fs::metadata(target)) else {
        return true;
    };
    from.len() != to.len() || matches!((from.modified(), to.modified()), (Ok(a), Ok(b)) if a > b)
}

/// `target` as a hard link to `source` (no bytes written: an output directory costs nothing per image),
/// a copy where links fail (another volume, a file system without them). An existing `target` is
/// removed first: writing into it could write through a link into the cache file. Cache files are
/// replaced by rename, never rewritten in place (cli/assets.mjs writeAtomic), so a link keeps the
/// content it was made with.
pub fn link_or_copy(source: &Path, target: &Path) -> Result<()> {
    match std::fs::remove_file(target) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            return Err(error).with_context(|| format!("Removing {}", target.display()));
        }
        _ => {}
    }
    if std::fs::hard_link(source, target).is_err() {
        std::fs::copy(source, target).with_context(|| format!("Copying {}", source.display()))?;
    }
    Ok(())
}

/// The emulator's profiling suite on a render or a sequence (its docs/cli.md "Profiles and caches"): `--profile <kinds>
/// [--profile-out <dir>] [--profile-top <n>] [--profile-phase <a,b|all>] [--profile-alloc-step <bytes>]`, taken out of
/// `argv` (the rest is returned: the engine's arguments), or `REE_PROFILE=<kinds>`. The profile directory defaults to
/// `<out>/profile`; `--profile boundary` alone is the boundary profiler, its report in that directory.
fn profile_options(argv: &[String], out: &Path) -> Result<(Option<profiler::Config>, Vec<String>), String> {
    let (mut profile, mut rest) = (Vec::new(), Vec::new());
    let mut index = 0;
    while index < argv.len() {
        let count = match argv[index].as_str() {
            "--profile" => profiler::profile_arguments(argv, index),
            "--profile-out" | "--profile-top" | "--profile-phase" | "--profile-alloc-step" => 2,
            _ => {
                rest.push(argv[index].clone());
                index += 1;
                continue;
            }
        };
        profile.extend(argv[index..(index + count).min(argv.len())].iter().cloned());
        index += count;
    }
    let default = || out.join("profile");
    let config = match profiler::profile_value(&profile).as_deref() {
        Some("boundary") => {
            let dir = profile.iter().position(|arg| arg == "--profile-out").and_then(|at| profile.get(at + 1)).map(PathBuf::from).unwrap_or_else(default);
            Some(profiler::Config::new(profiler::Kinds::BOUNDARY, dir))
        }
        None if profile.iter().any(|arg| arg == "--profile") => {
            let kinds: Vec<&str> = profiler::Kinds::NAMES.iter().map(|(name, _, _)| *name).collect();
            return Err(format!("--profile takes the kinds to profile: {}, all", kinds.join(", ")));
        }
        _ => profiler::Config::from_args(&profile, default)?,
    };
    Ok((config.map(|mut config| {
        config.out = std::path::absolute(&config.out).unwrap_or(config.out);
        config
    }), rest))
}

/// Starts a profile on this thread, the layer's own caches listed with the engine's when it counts caches
pub(crate) fn begin_profile(config: profiler::Config) -> profiler::Session {
    if config.kinds.has(profiler::Kinds::CACHES) {
        crate::engine::cache_counts::register();
    }
    profiler::Session::begin(config)
}

/// The story, module, model or fixture a run of `args` shows (a profile's run names it)
pub(crate) fn source_file(args: &Args) -> Option<String> {
    args.story.clone().or_else(|| args.module.clone()).or_else(|| args.model.clone()).or_else(|| args.fixture.clone())
}

/// JSON.stringify(value, null, 1)
fn pretty(value: &Value) -> Vec<u8> {
    let mut out = Vec::new();
    let mut serializer = serde_json::Serializer::with_formatter(&mut out, serde_json::ser::PrettyFormatter::with_indent(b" "));
    serde::Serialize::serialize(value, &mut serializer).expect("JSON");
    out
}

/// `rur-native sequence --dir <out> [--from s] [--to s | --frames n | --duration s] [--fps n]
/// [--crop node|x,y,w,h] [--pad px] [--no-png] [--scenes] [--no-video] [--offline]
/// [--profile <kinds> [--profile-out <dir>]] <engine arguments>`: one sequence (sequence.rs) in this process, the
/// engine half in a fresh VM (profiled on its thread: [`crate::sequence::start_fresh_profiled`]). Prints the result as
/// one JSON line on stdout; returns the exit code.
pub fn sequence_cli(argv: &[String]) -> i32 {
    let mut out = None;
    let mut options = serde_json::Map::new();
    let mut offline = false;
    let mut engine_args = Vec::new();
    let mut index = 0;
    while index < argv.len() {
        let key = argv[index].as_str();
        let value = argv.get(index + 1).cloned();
        match key {
            "--dir" => out = value,
            "--from" | "--to" | "--frames" | "--duration" | "--fps" | "--crop" | "--pad" => {
                options.insert(key[2..].to_string(), value.map(Value::String).unwrap_or(Value::Null));
            }
            "--no-png" | "--no-video" | "--scenes" | "--offline" => {
                match key {
                    "--no-png" => options.insert("png".into(), Value::Bool(false)),
                    "--no-video" => options.insert("video".into(), Value::Bool(false)),
                    "--scenes" => options.insert("scenes".into(), Value::Bool(true)),
                    _ => {
                        offline = true;
                        None
                    }
                };
                index += 1;
                continue;
            }
            other => {
                engine_args.push(other.to_string());
                index += 1;
                continue;
            }
        }
        index += 2;
    }
    let Some(out) = out else {
        eprintln!("rur-native sequence: --dir <output directory> is required");
        return 2;
    };
    let out = std::path::absolute(&out).unwrap_or_else(|_| PathBuf::from(&out));
    let parsed = profile_options(&engine_args, &out).and_then(|(profile, engine_args)| {
        let args = crate::engine::cli::parse(&engine_args)?;
        crate::sequence::parse_options(&Value::Object(options), offline).map(|(spec, output)| (profile, args, spec, output))
    });
    let (profile, args, spec, output) = match parsed {
        Ok(parsed) => parsed,
        Err(error) => {
            eprintln!("rur-native sequence: {error}");
            return 2;
        }
    };
    let repo = args.repo.as_ref().map(PathBuf::from).unwrap_or_else(crate::engine::default_repo);
    let mut renderer = StoryRenderer::new(repo);
    let started = Instant::now();
    if let Some(config) = &profile {
        // (a profile of an earlier sequence there is not this one's)
        let _ = std::fs::remove_file(config.out.join("summary.txt"));
    }
    // (with a profile, the engine half profiles its own thread and writes the profile before its last message: it is
    // there once the sequence is)
    let result = renderer.sequence(&args, &out, &output, &mut |args| crate::sequence::start_fresh_profiled(args, &spec, profile.clone()), || {});
    let result = result.map(|mut result| {
        if let Some(config) = profile.as_ref().filter(|config| config.out.join("summary.txt").is_file()) {
            result["profile"] = json!(config.out);
        }
        result
    });
    let code = match result {
        Ok(result) => {
            eprintln!("rur: sequence {}; {} frames; {:.3}s -> {}", result["status"].as_str().unwrap_or("?"), result["frames"], started.elapsed().as_secs_f64(), out.display());
            println!("{result}");
            0
        }
        Err(error) => {
            eprintln!("rur-native sequence: {error:#}");
            1
        }
    };
    use std::io::Write;
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
    std::process::exit(code)
}

/// `rur-native render --dir <out> [--no-png] [--offline] [--placeholders] [--profile <kinds> [--profile-out <dir>]] <engine arguments>`:
/// one render in this process. Prints the result as one JSON line on stdout (with `profile`, the profile's directory,
/// default `<out>/profile`, whose summary goes to stderr: [`profile_options`]); returns the exit code.
pub fn cli(argv: &[String]) -> i32 {
    let mut out = None;
    let mut saved = None;
    let mut options = RenderOptions { png: true, ..Default::default() };
    let mut engine_args = Vec::new();
    let mut index = 0;
    while index < argv.len() {
        match argv[index].as_str() {
            "--dir" => {
                out = argv.get(index + 1).cloned();
                index += 2;
                continue;
            }
            "--scene" => {
                saved = argv.get(index + 1).cloned();
                index += 2;
                continue;
            }
            "--no-png" => options.png = false,
            "--offline" => options.offline = true,
            "--placeholders" => options.placeholders = true,
            other => engine_args.push(other.to_string()),
        }
        index += 1;
    }
    let Some(out) = out else {
        eprintln!("rur-native render: --dir <output directory> is required");
        return 2;
    };
    let out = std::path::absolute(&out).unwrap_or_else(|_| PathBuf::from(&out));
    let parsed = profile_options(&engine_args, &out).and_then(|(profile, engine_args)| Ok((profile, crate::engine::cli::parse(&engine_args)?)));
    let (profile, args) = match parsed {
        Ok(parsed) => parsed,
        Err(error) => {
            eprintln!("rur-native render: {error}");
            return 2;
        }
    };
    if profile.is_some() && saved.is_some() {
        eprintln!("rur-native render: --profile profiles a run, and --scene draws a saved scene without one");
        return 2;
    }
    let repo = args.repo.as_ref().map(PathBuf::from).unwrap_or_else(crate::engine::default_repo);
    let mut renderer = StoryRenderer::new(repo);
    renderer.placeholders = options.placeholders;
    // The GPU device starts while the story runs
    let gpu = options.png.then(|| thread::spawn(|| pollster::block_on(GpuRenderer::headless())));
    let started = Instant::now();
    // A profile of the run (the emulator's profiler): it starts before the engine does and run_once reads the VM after the
    // last frame; a run repeated with the cloud fonts it lacked starts it again, so it is the last run's
    let mut session = profile.clone().map(begin_profile);
    let mut clock = (0, 0.0, 0.0);
    let result = match &saved {
        // --scene <scene.json>: draw a saved (possibly hand-edited) scene, nothing runs
        Some(file) => saved_scene(Path::new(file)).map_err(|error| anyhow!(error)).and_then(|output| renderer.write_outputs(&args, &out, options.offline, false, move |_| Ok(output), || {})),
        None => renderer.write_outputs(
            &args,
            &out,
            options.offline,
            true,
            |args| {
                let output = crate::engine::cli::run_once(args)?;
                clock = (output.frames, output.seconds, output.fps);
                Ok(output)
            },
            || {
                if let Some(config) = &profile {
                    session = Some(begin_profile(config.clone()));
                }
            },
        ),
    };
    let result = result.and_then(|(mut result, scene, _)| {
        if let Some(gpu) = gpu {
            let gpu = gpu.join().map_err(|_| anyhow!("GPU start panicked"))??;
            renderer.draw = Some(Draw { gpu, preparer: DrawPreparer::new(), assets: AssetStore::new(renderer.repo.clone(), true) });
            renderer.png_into(&mut result, &scene, &out, true);
        }
        Ok(result)
    });
    // The profile's files and its summary (on stderr), once the render is written
    let result = match session {
        Some(session) => {
            let status = match &result {
                Ok(result) => result["status"].as_str().unwrap_or("?").to_string(),
                Err(_) => "failed".into(),
            };
            let (frames, seconds, fps) = clock;
            let run = profiler::Run { command: "render".into(), status, harness: source_file(&args), frames, seconds, fps };
            match session.finish(run) {
                Ok(summary) => {
                    eprint!("{summary}");
                    result.map(|mut result| {
                        result["profile"] = json!(profile.as_ref().map(|config| &config.out));
                        result
                    })
                }
                Err(error) => {
                    eprintln!("rur-native render: profile: {error}");
                    result
                }
            }
        }
        None => result,
    };
    let code = match result {
        Ok(result) => {
            eprintln!("rur: {}; {} nodes; {:.3}s -> {}", result["status"].as_str().unwrap_or("?"), result["nodes"], started.elapsed().as_secs_f64(), out.display());
            println!("{result}");
            0
        }
        Err(error) => {
            eprintln!("rur-native render: {error:#}");
            1
        }
    };
    // Everything is written: the process ends here, without tearing down the GPU device, the asset
    // workers and the VM one by one (the asset helper sees its stdin close and exits on its own)
    use std::io::Write;
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
    std::process::exit(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|arg| arg.to_string()).collect()
    }

    /// `--profile <kinds>` and its options come out of a render's or a sequence's arguments, the rest are the engine's;
    /// the profile directory defaults to `<out>/profile`, `--profile boundary` alone is the boundary profiler, and a
    /// `--profile` without kinds is refused
    #[test]
    fn profile_options_are_taken_out_of_the_engine_arguments() {
        let out = std::path::absolute("out-dir").unwrap();
        let (config, rest) = profile_options(&strings(&["--story", "a.luau", "--profile", "heap,work", "--time", "1"]), &out).unwrap();
        let config = config.unwrap();
        assert!(config.kinds.has(profiler::Kinds::HEAP) && config.kinds.has(profiler::Kinds::WORK) && !config.kinds.has(profiler::Kinds::TIME));
        assert_eq!(config.out, out.join("profile"));
        assert_eq!(rest, strings(&["--story", "a.luau", "--time", "1"]));
        let (config, rest) = profile_options(&strings(&["--profile", "all", "--profile-out", "elsewhere", "--profile-top", "5", "--story", "a.luau"]), &out).unwrap();
        let config = config.unwrap();
        assert_eq!((config.out, config.top), (std::path::absolute("elsewhere").unwrap(), 5));
        assert_eq!(rest, strings(&["--story", "a.luau"]));
        let (config, rest) = profile_options(&strings(&["--profile", "boundary", "--story", "a.luau"]), &out).unwrap();
        assert_eq!(config.unwrap().kinds, profiler::Kinds::BOUNDARY);
        assert_eq!(rest, strings(&["--story", "a.luau"]));
        assert!(profile_options(&strings(&["--profile", "--story", "a.luau"]), &out).unwrap_err().contains("--profile takes the kinds"));
        if std::env::var_os("REE_PROFILE").is_none() {
            let (config, rest) = profile_options(&strings(&["--story", "a.luau"]), &out).unwrap();
            assert!(config.is_none() && rest == strings(&["--story", "a.luau"]));
        }
    }
}
