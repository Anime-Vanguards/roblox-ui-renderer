use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Parser;
use serde_json::Value;

#[cfg(feature = "desktop")]
pub mod app;
pub mod assets;
#[cfg(feature = "desktop")]
pub mod catalog;
pub mod engine;
pub mod glyphs;
#[cfg(feature = "desktop")]
pub mod headless;
#[cfg(feature = "desktop")]
pub mod input;
pub mod memtrace;
#[cfg(feature = "desktop")]
pub mod microprofiler_ui;
// (written by story_render, which the browser module does not have)
#[cfg(feature = "desktop")]
pub mod placeholder;
pub mod places;
pub mod rojo;
pub mod paths;
pub mod presentation;
#[cfg(feature = "desktop")]
pub mod profiler;
#[cfg(feature = "desktop")]
pub mod profile_cli;
pub mod render;
pub mod resolve;
#[cfg(feature = "desktop")]
pub mod runtime;
pub mod scene;
#[cfg(feature = "desktop")]
pub mod sequence;
#[cfg(feature = "desktop")]
pub mod serve;
#[cfg(feature = "desktop")]
pub mod story_render;
#[cfg(feature = "desktop")]
pub mod watch;
/// The browser host (wasm32-unknown-emscripten): one story, driven frame by frame from JavaScript
#[cfg(feature = "web")]
pub mod web;

/// Shared launch configuration for the desktop app and native headless renderer.
#[derive(Clone, Debug, Parser)]
#[command(
    name = "rur-native",
    version,
    about = "Render Roblox UI natively: the desktop storybook, headless captures, rur render and its service"
)]
pub struct NativeOptions {
    #[arg(long, default_value_os_t = paths::default_repo())]
    pub repo: PathBuf,
    // The project whose places the storybook lists and runs (src/places.rs). The default is a path
    // under the checkout in checkout mode, which resolve() makes relative to --repo, and an
    // absolute path in standalone mode (see paths.rs).
    #[arg(long, default_value_os_t = paths::default_project(Path::new("")))]
    pub project: PathBuf,
    #[arg(long, conflicts_with = "scene")]
    pub story: Option<PathBuf>,
    #[arg(long, conflicts_with = "story")]
    pub scene: Option<PathBuf>,
    #[arg(long, conflicts_with_all = ["story", "scene", "actions", "benchmark"])]
    pub batch: Option<PathBuf>,
    #[arg(long, default_value_os_t = paths::default_output())]
    pub out: PathBuf,
    #[arg(long)]
    pub headless: bool,
    #[arg(long, num_args = 0..=1, default_missing_value = "120")]
    pub benchmark: Option<u32>,
    #[arg(long)]
    pub actions: Option<PathBuf>,
    #[arg(long, default_value_t = 1.0 / 120.0)]
    pub dt: f64,
    #[arg(long, default_value_t = 30)]
    pub warmup: u32,
    #[arg(long)]
    pub save_frames: bool,
    /// Writes frames.json in --out: the FNV-1a hash of every measured frame's pixels (as render.png
    /// holds them), for frame-by-frame comparisons without the PNGs (tools/web/frame-identity.mjs)
    #[arg(long)]
    pub frame_hashes: bool,
    #[arg(long)]
    pub strict_assets: bool,
    #[arg(long, default_value = "1200x900", value_parser = parse_viewport)]
    pub viewport: [u32; 2],
    #[arg(long, default_value_t = 1.5)]
    pub scale: f64,
    #[arg(long, default_value_t = 1.0)]
    pub time: f64,
    /// Canvas around the story target (logical px), as Studio's viewport shows around a UI Labs story.
    /// Defaults to 48 in the desktop app and 0 for headless captures and benchmarks.
    #[arg(long)]
    pub margin: Option<u32>,
    #[arg(long, default_value = "{}", value_parser = parse_controls)]
    pub controls: Value,
    #[arg(long)]
    pub offline: bool,
    #[arg(long)]
    pub roots: Option<String>,
    /// A Rojo project (`*.project.json`) whose mounts the stories run with instead of the project's places; also
    /// `--project <file.project.json>`, and in a release build a project folder with a default.project.json and no
    /// rur.json places
    #[arg(long, conflicts_with = "roots")]
    pub rojo: Option<PathBuf>,
    #[arg(long)]
    pub snapshot_dir: Option<PathBuf>,
    #[arg(long)]
    pub no_snapshots: bool,
    /// Cap the desktop app at the display's refresh rate. Off by default: frames present uncapped
    /// (mailbox where the driver has it, else immediate).
    #[arg(long)]
    pub vsync: bool,
}

impl NativeOptions {
    pub fn resolve(mut self) -> Result<Self> {
        let cwd = std::env::current_dir().context("Reading current working directory")?;
        self.repo = absolute(&cwd, &self.repo);
        // Relative --project and --out follow the checkout in checkout mode, else the current directory
        let base = if paths::is_standalone() { &cwd } else { &self.repo };
        self.project = absolute(base, &self.project);
        self.out = absolute(base, &self.out);
        self.story = self.story.map(|path| absolute(&cwd, &path));
        self.scene = self.scene.map(|path| absolute(&cwd, &path));
        self.batch = self.batch.map(|path| absolute(&cwd, &path));
        self.snapshot_dir = self.snapshot_dir.map(|path| absolute(&cwd, &path));
        self.actions = self.actions.map(|path| absolute(&cwd, &path));
        self.roots = self.roots.map(|roots| {
            roots
                .split(';')
                .filter(|root| !root.is_empty())
                .map(|root| {
                    absolute(&cwd, Path::new(root))
                        .to_string_lossy()
                        .into_owned()
                })
                .collect::<Vec<_>>()
                .join(";")
        });
        self.rojo = self.rojo.map(|path| absolute(&cwd, &path));
        // A Rojo project: `--project <file.project.json>`, or (release builds) a project folder holding a
        // default.project.json and no rur.json places (rojo.rs)
        if self.rojo.is_none() && rojo::is_project_file(&self.project.to_string_lossy()) {
            self.rojo = Some(self.project.clone());
            self.project = self.project.parent().map(Path::to_path_buf).unwrap_or_default();
        } else if self.rojo.is_none() && self.roots.is_none() && paths::is_standalone() {
            let found = self.project.join(rojo::DEFAULT_PROJECT_FILE);
            if found.is_file() && !rojo::declares_places(&self.project) {
                self.rojo = Some(found);
            }
        }
        if let (Some(file), None) = (&self.rojo, &self.roots) {
            let mut warnings = Vec::new();
            let mounts = rojo::mounts(file, None, &mut warnings).map_err(anyhow::Error::msg)?;
            for warning in warnings {
                eprintln!("{warning}");
            }
            let listed: Vec<&str> = mounts.iter().map(|mount| mount.path.as_str()).collect();
            eprintln!("rur: Rojo project {}: mounted {}", file.display(), if listed.is_empty() { "nothing".to_string() } else { listed.join(", ") });
        }
        self.headless |= self.benchmark.is_some() || self.batch.is_some();
        if self.headless && self.story.is_none() && self.scene.is_none() && self.batch.is_none() {
            bail!("--headless requires --story, --scene, or --batch");
        }
        if !self.scale.is_finite() || self.scale <= 0.0 || self.scale > 8.0 {
            bail!("--scale must be finite and greater than zero, at most 8");
        }
        if !self.time.is_finite() || self.time < 0.0 {
            bail!("--time must be a finite nonnegative number");
        }
        if self.benchmark == Some(0) {
            bail!("--benchmark must request at least one frame");
        }
        if !self.dt.is_finite() || self.dt <= 0.0 || self.dt > 0.25 {
            bail!("--dt must be finite and greater than zero, at most 0.25 seconds");
        }
        Ok(self)
    }
}

/// The engine's Luau profiler, MicroProfiler scopes and the state dir (cli/paths.mjs: RUR_STATE_DIR,
/// else RUR_CACHE_DIR, else `<repo>/.cache`) are the emulator's
pub use engine::{luau_profile, microprofile, shared_state_path, state_dir};
/// The layer's cache counters (a profile's `caches`), at the root as the text module reaches them (it is also compiled on
/// its own, by native/tests/text_corpus.rs)
pub use engine::cache_counts;

fn absolute(base: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

fn parse_viewport(value: &str) -> Result<[u32; 2], String> {
    let (width, height) = value
        .split_once(['x', 'X'])
        .ok_or("Expected WIDTHxHEIGHT")?;
    let width = width.parse::<u32>().map_err(|_| "Invalid viewport width")?;
    let height = height
        .parse::<u32>()
        .map_err(|_| "Invalid viewport height")?;
    if width == 0 || height == 0 || width > 16384 || height > 16384 {
        return Err("Viewport dimensions must be between 1 and 16384".into());
    }
    Ok([width, height])
}

fn parse_controls(value: &str) -> Result<Value, String> {
    let value: Value = serde_json::from_str(value).map_err(|error| error.to_string())?;
    if !value.is_object() {
        return Err("Controls must be a JSON object".into());
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkout_project_and_output_defaults_stay_relative_to_the_checkout() {
        let options = NativeOptions::try_parse_from(["rur-native"]).unwrap().resolve().unwrap();
        assert_eq!(options.repo, Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap());
        assert!(!paths::is_standalone());
        let state_dir_is_redirected =
            std::env::var_os("RUR_STATE_DIR").is_some() || std::env::var_os("RUR_CACHE_DIR").is_some();
        if !state_dir_is_redirected {
            assert_eq!(state_dir(&options.repo), options.repo.join(".cache"));
        }
        assert_eq!(options.out, options.repo.join("out/native"));
        if std::env::var_os("RUR_PROJECT").is_none() {
            assert_eq!(options.project, options.repo.join("examples/project"));
        }
    }

    #[test]
    fn scene_preview_stays_interactive_and_benchmark_is_headless() {
        let preview = NativeOptions::try_parse_from([
            "rur-native",
            "--scene",
            "scene.json",
            "--viewport",
            "800x600",
        ])
        .unwrap()
        .resolve()
        .unwrap();
        assert!(!preview.headless);
        assert_eq!(preview.viewport, [800, 600]);
        assert_eq!(preview.scale, 1.5);
        let benchmark =
            NativeOptions::try_parse_from(["rur-native", "--scene", "scene.json", "--benchmark"])
                .unwrap()
                .resolve()
                .unwrap();
        assert!(benchmark.headless);
        assert_eq!(benchmark.benchmark, Some(120));
        let batch = NativeOptions::try_parse_from(["rur-native", "--batch", "batch.json"])
            .unwrap()
            .resolve()
            .unwrap();
        assert!(batch.headless);
        assert!(batch.batch.unwrap().is_absolute());
        assert!(
            NativeOptions::try_parse_from([
                "rur-native",
                "--batch",
                "batch.json",
                "--story",
                "story.luau"
            ])
            .is_err()
        );
    }

    #[test]
    fn rejects_invalid_headless_and_timing_requests() {
        assert!(
            NativeOptions::try_parse_from(["rur-native", "--headless"])
                .unwrap()
                .resolve()
                .is_err()
        );
        assert!(
            NativeOptions::try_parse_from(["rur-native", "--dt", "NaN"])
                .unwrap()
                .resolve()
                .is_err()
        );
        assert!(
            NativeOptions::try_parse_from(["rur-native", "--dt", "0.5"])
                .unwrap()
                .resolve()
                .is_err()
        );
        assert!(NativeOptions::try_parse_from(["rur-native", "--controls", "[]"]).is_err());
        assert!(NativeOptions::try_parse_from(["rur-native", "--viewport", "0x900"]).is_err());
    }
}
