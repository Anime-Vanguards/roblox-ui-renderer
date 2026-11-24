use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Parser;
use serde_json::Value;

#[cfg(feature = "desktop")]
pub mod app;
pub mod assets;
#[cfg(feature = "desktop")]
pub mod catalog;
/// Asset downloads, the account and Studio reads (`rur-native assets`); the web build only draws its bundle
#[cfg(feature = "desktop")]
pub mod download;
#[cfg(feature = "desktop")]
pub mod doctor;
/// `rur diff` (rur-native diff): a source rendered at a git ref and in the working tree, compared
#[cfg(feature = "desktop")]
pub mod diff;
/// `rur-native debug-report`: a shareable report of the project's stories (report.rs)
#[cfg(feature = "desktop")]
pub mod report;
pub mod engine;
/// `--git-ref`: a run's sources extracted at a git commit
#[cfg(feature = "desktop")]
pub mod gitref;
pub mod glyphs;
#[cfg(feature = "desktop")]
pub mod headless;
#[cfg(feature = "desktop")]
pub mod input;
/// What the Node CLI's JavaScript did with values (JSON in key order, toFixed, localeCompare), for the commands that
/// moved from it
#[cfg(feature = "desktop")]
pub mod js;
pub mod memtrace;
#[cfg(feature = "desktop")]
pub mod microprofiler_ui;
// (written by story_render, which the browser module does not have)
#[cfg(feature = "desktop")]
pub mod placeholder;
pub mod places;
pub mod hints;
pub mod rojo;
pub mod term;
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
pub mod story_args;
#[cfg(feature = "desktop")]
pub mod story_render;
/// A render's summary.json and scene diagnostics
#[cfg(feature = "desktop")]
pub mod summary;
/// `rur sweep` (rur-native sweep): every story of a project rendered and reported on
#[cfg(feature = "desktop")]
pub mod sweep;
#[cfg(feature = "desktop")]
pub mod viewports;
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
    /// The project to open, as the folder you would run rur-native from (`rur-native ../my-game`, `.` for the
    /// current one) or a Rojo project file; relative to the current directory. It replaces `--project`.
    #[arg(value_name = "PROJECT", conflicts_with_all = ["rojo", "roots"])]
    pub path: Option<PathBuf>,
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
    /// What [`NativeOptions::resolve`] found about the project (the Rojo project it mounts, `$path`s that don't
    /// exist, project files it would not choose between), for the storybook to show
    #[arg(skip)]
    pub notes: Vec<String>,
}

impl NativeOptions {
    pub fn resolve(mut self) -> Result<Self> {
        let cwd = std::env::current_dir().context("Reading current working directory")?;
        self.repo = absolute(&cwd, &self.repo);
        // Relative --project and --out follow the checkout in checkout mode, else the current directory
        let base = if paths::is_standalone() { &cwd } else { &self.repo };
        self.project = absolute(base, &self.project);
        // The positional project: a folder or a *.project.json, from the current directory in either mode
        let named = self.path.take().map(|path| absolute(&cwd, &path));
        if let Some(path) = &named {
            if !path.is_dir() && !(path.is_file() && rojo::is_project_file(&path.to_string_lossy())) {
                bail!("{} is not a project folder or a *.project.json file", path.display());
            }
            self.project = path.clone();
        }
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
        // A Rojo project: `--project <file.project.json>`, or (release builds) a project folder holding a project file
        // (its default.project.json, else its only *.project.json; a package's own tree gives way to the one DataModel
        // project beside it that mounts the story, or the package) and no rur.json places (rojo.rs)
        let mut why = None;
        if self.rojo.is_none() && rojo::is_project_file(&self.project.to_string_lossy()) {
            self.rojo = Some(self.project.clone());
            self.project = self.project.parent().map(Path::to_path_buf).unwrap_or_default();
        } else if self.rojo.is_none() && self.roots.is_none() && (paths::is_standalone() || named.is_some()) && !rojo::declares_places(&self.project) {
            match rojo::project_for_source(&self.project, self.story.as_deref()) {
                Ok(found) => (self.rojo, why) = found.map_or((None, None), |(file, why)| (Some(file), why)),
                Err(several) => self.notes.push(format!("rur: {}", rojo::several_projects(&several))),
            }
        }
        // A project at a drive or filesystem root (`rur-native /`, `--project \`) would make the whole drive its
        // root: the storybook would look for stories on all of it and never finish
        let full = std::path::absolute(&self.project).unwrap_or_else(|_| self.project.clone());
        if self.roots.is_none() && full.parent().is_none() && !rojo::declares_places(&full) {
            bail!(
                "{} is the root of the drive, not a project folder. Run rur-native from the project folder, or name the folder: rur-native <project folder> (rur-native . for the current one)",
                full.display()
            );
        }
        if let (Some(file), None) = (&self.rojo, &self.roots) {
            let mounts = rojo::mounts(file, None, &mut self.notes).map_err(anyhow::Error::msg)?;
            let why = why.map(|why| format!(" ({why}; --rojo picks another)")).unwrap_or_default();
            self.notes.insert(0, format!("rur: Rojo project {}{why}: mounted {}", file.display(), rojo::mounted_list(&mounts)));
        }
        for note in &self.notes {
            term::project_line(note);
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
    fn a_project_folder_named_on_its_own_opens_like_running_from_it() {
        let example = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("examples/rojo-wally");
        let options = NativeOptions::try_parse_from(["rur-native".as_ref(), example.as_os_str()]).unwrap().resolve().unwrap();
        assert_eq!(options.project, example);
        assert_eq!(options.rojo, Some(example.join("default.project.json")));
        // A Rojo project file names its folder as the project
        let file = example.join("default.project.json");
        let options = NativeOptions::try_parse_from(["rur-native".as_ref(), file.as_os_str()]).unwrap().resolve().unwrap();
        assert_eq!((options.project, options.rojo), (example.clone(), Some(file)));
        assert!(NativeOptions::try_parse_from(["rur-native".as_ref(), example.as_os_str(), "--rojo".as_ref(), "x.project.json".as_ref()]).is_err());
        let missing = NativeOptions::try_parse_from(["rur-native".as_ref(), example.join("missing").as_os_str()]).unwrap().resolve().unwrap_err();
        assert!(missing.to_string().contains("is not a project folder"), "{missing}");
    }

    /// A Wally package folder: the storybook and headless captures mount the DataModel dev project beside the package's
    /// own default.project.json, which holds the package's dependencies, and the project line says why
    #[test]
    fn a_package_folder_opens_its_data_model_project() {
        let dir = std::env::temp_dir().join(format!("rur-options-package-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (file, text) in [
            ("default.project.json", r#"{"name":"Kit","tree":{"$path":"src"}}"#),
            ("dev.project.json", r#"{"name":"dev","tree":{"$className":"DataModel","ReplicatedStorage":{"Packages":{"$path":"Packages","Kit":{"$path":"src"}}}}}"#),
            ("Packages/Signal.lua", "return {}"),
            ("src/Components/Thing.story.lua", "return {}"),
        ] {
            std::fs::create_dir_all(dir.join(file).parent().unwrap()).unwrap();
            std::fs::write(dir.join(file), text).unwrap();
        }
        let story = dir.join("src/Components/Thing.story.lua");
        for args in [vec![dir.as_os_str(), "--story".as_ref(), story.as_os_str(), "--headless".as_ref()], vec![dir.as_os_str()]] {
            let mut argv: Vec<&std::ffi::OsStr> = vec!["rur-native".as_ref()];
            argv.extend(&args);
            let options = NativeOptions::try_parse_from(argv).unwrap().resolve().unwrap();
            assert_eq!(options.rojo, Some(dir.join("dev.project.json")), "{args:?}");
            let what = if args.len() > 1 { "Thing.story.lua" } else { "the package" };
            assert!(
                options.notes[0].contains(&format!("dev.project.json (default.project.json beside it is the package's own tree, not a DataModel, and this is the one DataModel project there that mounts {what}; --rojo picks another): mounted ReplicatedStorage.Packages, ReplicatedStorage.Packages.Kit")),
                "{:?}",
                options.notes
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_drive_root_is_refused_rather_than_searched_for_stories() {
        for args in [&["rur-native", "/"][..], &["rur-native", "--project", "/"]] {
            let error = NativeOptions::try_parse_from(args).unwrap().resolve().unwrap_err();
            assert!(error.to_string().contains("is the root of the drive, not a project folder"), "{error}");
        }
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
