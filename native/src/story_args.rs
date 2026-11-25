//! The options rur's story commands take, for the commands rur-native runs on its own (`sweep`, `diff`, `git-ref`):
//! cli/rur.mjs's parsing (`--key value`, a bare `--key` is a flag) and its project options (`--project`, `--place`,
//! `--roots`, `--rojo`, `--mount`, `--load`, Studio snapshots, `--viewport WxH@physical`), turned into the engine
//! arguments of a `rur-native render` child. A render here is what `rur render --no-service` does: the child writes
//! the output directory, then summary.json is made from it (summary.rs).
//!
//! The global options of cli/paths.mjs are read too: `--state-dir <dir>` and `--text-subpixel on|off` reach the
//! children through their environment, `--isolated` and `--no-service` change nothing (no render service is used).
use std::{
    io::Read,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use crate::{js::Json, rojo::resolve};

/// Why a command stopped: a usage error (exit 64, nothing was run or written) or another error (exit 1)
#[derive(Debug)]
pub enum Failure {
    Usage(String),
    Error(String),
}

impl From<String> for Failure {
    fn from(message: String) -> Failure {
        Failure::Error(message)
    }
}

impl Failure {
    pub fn message(&self) -> &str {
        match self {
            Failure::Usage(message) | Failure::Error(message) => message,
        }
    }

    /// The exit code: 64 for a usage error, else 1
    pub fn code(&self) -> i32 {
        match self {
            Failure::Usage(_) => 64,
            Failure::Error(_) => 1,
        }
    }
}

/// One option's value as cli/rur.mjs parseArgs leaves it
#[derive(Clone, Debug, PartialEq)]
pub enum Opt {
    /// `--key` with no value (the next word is another option, or none follows)
    Flag,
    Value(String),
    /// `--mount` / `--load`, given more than once
    List(Vec<String>),
}

/// A command line as cli/rur.mjs parseArgs reads it
#[derive(Clone, Debug, Default)]
pub struct Args {
    pub positional: Vec<String>,
    options: Vec<(String, Opt)>,
}

/// The options that may be given more than once (collected into a list)
const REPEATABLE: [&str; 2] = ["--mount", "--load"];

/// An option's name without its dashes (`--out` is kept as `out`, as cli/rur.mjs keeps it)
fn bare(key: &str) -> &str {
    key.strip_prefix("--").unwrap_or(key)
}

impl Args {
    pub fn parse(argv: &[String]) -> Args {
        let mut args = Args::default();
        let mut index = 0;
        while index < argv.len() {
            let arg = &argv[index];
            if arg.starts_with("--") {
                let mut value = Opt::Flag;
                if let Some(next) = argv.get(index + 1).filter(|next| !next.starts_with("--")) {
                    value = Opt::Value(next.clone());
                    index += 1;
                }
                match value {
                    Opt::Value(item) if REPEATABLE.contains(&arg.as_str()) => {
                        let mut list = match args.get(arg) {
                            Some(Opt::List(list)) => list.clone(),
                            _ => Vec::new(),
                        };
                        list.push(item);
                        args.set(arg, Opt::List(list));
                    }
                    value => args.set(arg, value),
                }
            } else {
                args.positional.push(arg.clone());
            }
            index += 1;
        }
        args
    }

    /// An option's value; `key` is the option as it is spelt (`--out`)
    pub fn get(&self, key: &str) -> Option<&Opt> {
        let key = bare(key);
        self.options.iter().find(|(name, _)| name == key).map(|(_, value)| value)
    }

    pub fn set(&mut self, key: &str, value: Opt) {
        let key = bare(key);
        match self.options.iter_mut().find(|(name, _)| name == key) {
            Some(entry) => entry.1 = value,
            None => self.options.push((key.to_string(), value)),
        }
    }

    pub fn set_value(&mut self, key: &str, value: impl Into<String>) {
        self.set(key, Opt::Value(value.into()));
    }

    pub fn remove(&mut self, key: &str) {
        let key = bare(key);
        self.options.retain(|(name, _)| name != key);
    }

    /// `args[key] !== undefined`
    pub fn present(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    /// `args[key] !== undefined && args[key] !== true`: the value as given, also an empty one
    pub fn given(&self, key: &str) -> Option<String> {
        match self.get(key)? {
            Opt::Flag => None,
            Opt::Value(value) => Some(value.clone()),
            Opt::List(list) => Some(list.join(",")),
        }
    }

    /// `args[key] && args[key] !== true`: a value that is not empty
    pub fn value(&self, key: &str) -> Option<String> {
        self.given(key).filter(|value| !value.is_empty())
    }

    /// JavaScript's truthiness of `args[key]`
    pub fn truthy(&self, key: &str) -> bool {
        match self.get(key) {
            None => false,
            Some(Opt::Flag | Opt::List(_)) => true,
            Some(Opt::Value(value)) => !value.is_empty(),
        }
    }

    /// `Number(args[key])` (a flag is 1, a missing option NaN)
    pub fn number(&self, key: &str) -> f64 {
        match self.get(key) {
            None => f64::NAN,
            Some(Opt::Flag) => 1.0,
            Some(Opt::Value(value)) => crate::js::number(value),
            Some(Opt::List(list)) => crate::js::number(&list.join(",")),
        }
    }

    /// `--mount` / `--load` values (a bare flag has none)
    pub fn list(&self, key: &str) -> Vec<String> {
        match self.get(key) {
            Some(Opt::List(list)) => list.clone(),
            Some(Opt::Value(value)) => vec![value.clone()],
            _ => Vec::new(),
        }
    }

    /// Every option name passed on the command line, with its leading `--`.
    pub fn option_names(&self) -> Vec<String> {
        self.options.iter().map(|(name, _)| format!("--{name}")).collect()
    }
}

/// The global options of cli/paths.mjs, taken out of `argv`: `--state-dir <dir>` (`=` too), `--text-subpixel on|off`,
/// `--isolated`, `--no-service`; what they set reaches the children as environment variables
fn global_options(argv: &[String]) -> Result<(Vec<String>, Vec<(&'static str, Option<String>)>), Failure> {
    let mut rest = Vec::new();
    let mut env: Vec<(&'static str, Option<String>)> = Vec::new();
    let mut index = 0;
    while index < argv.len() {
        let arg = &argv[index];
        if arg == "--" {
            rest.extend(argv[index..].iter().cloned());
            break;
        }
        let (key, inline) = match arg.split_once('=') {
            Some((key, value)) if key == "--state-dir" || key == "--text-subpixel" => (key, Some(value.to_string())),
            _ => (arg.as_str(), None),
        };
        match key {
            "--state-dir" | "--text-subpixel" => {
                let value = match inline {
                    Some(value) => value,
                    None => {
                        index += 1;
                        argv.get(index).cloned().ok_or_else(|| Failure::Usage(format!("{key} needs a value")))?
                    }
                };
                if key == "--state-dir" {
                    env.push(("RUR_STATE_DIR", Some(resolve(Path::new(""), &value).display().to_string())));
                    env.push(("RUR_CACHE_DIR", None));
                } else {
                    match value.as_str() {
                        "on" => env.push(("RUR_TEXT_SUBPIXEL", Some("on".into()))),
                        "off" => env.push(("RUR_TEXT_SUBPIXEL", None)),
                        _ => return Err(Failure::Usage(format!("--text-subpixel takes on or off, not {value:?} (docs/text.md, Text modes)"))),
                    }
                }
            }
            "--isolated" | "--no-service" => {}
            _ => rest.push(arg.clone()),
        }
        index += 1;
    }
    Ok((rest, env))
}

/// A command line's arguments as [`Context::new`] reads them, its global options taken out
pub fn arguments(argv: &[String]) -> Result<Args, Failure> {
    let (argv, _) = global_options(argv)?;
    Ok(Args::parse(&argv))
}

/// The Studio snapshot each place loads implicitly (named after the Studio it is exported from)
const IMPLICIT_SNAPSHOTS: &[(&str, &str)] = &[];

/// Where a story command runs: the renderer checkout (`--repo`), its state and output directories, the project runs
/// mount by default, and this executable, which renders each story in a child process
pub struct Context {
    /// The renderer: `--repo`, else this build's checkout (a release build's state directory)
    pub repo: PathBuf,
    pub state: PathBuf,
    /// Where commands write without `--out`: RUR_OUT_DIR, else `<repo>/out`
    pub out: PathBuf,
    /// The project a run mounts beside its story: RUR_PROJECT, else the checkout's (a release build's: the current
    /// directory)
    pub default_project: PathBuf,
    /// The project whose places load Studio snapshots implicitly
    snapshot_project: PathBuf,
    pub exe: PathBuf,
    /// The variables the global options set for the children (None: removed)
    env: Vec<(&'static str, Option<String>)>,
}

impl Context {
    /// The context of a command line (its global options taken out) and the rest of its arguments
    pub fn new(argv: &[String]) -> Result<(Context, Args), Failure> {
        let (argv, env) = global_options(argv)?;
        let args = Args::parse(&argv);
        let repo = match args.value("--repo") {
            Some(repo) => resolve(Path::new(""), &repo),
            None => crate::engine::default_repo(),
        };
        let state = match env.iter().find(|(name, _)| *name == "RUR_STATE_DIR") {
            Some((_, Some(dir))) => PathBuf::from(dir),
            _ => crate::state_dir(&repo),
        };
        let variable = |name: &str| std::env::var(name).ok().filter(|value| !value.is_empty());
        let out = match variable("RUR_OUT_DIR") {
            Some(dir) => resolve(Path::new(""), &dir),
            None if crate::paths::is_standalone() => state.join("out"),
            None => repo.join("out"),
        };
        let default_project = match variable("RUR_PROJECT") {
            Some(project) => resolve(Path::new(""), &project),
            None => resolve(Path::new(""), &crate::paths::default_project(&repo).to_string_lossy()),
        };
        let snapshot_project = PathBuf::new();
        let exe = std::env::current_exe().map_err(|error| format!("this executable: {error}"))?;
        Ok((Context { repo, state, out, default_project, snapshot_project, exe, env }, args))
    }

    /// `path` made absolute against the current directory, as path.resolve does
    pub fn resolve(&self, path: &str) -> PathBuf {
        resolve(Path::new(""), path)
    }

    /// The run's project folder: `--project`, else the default project
    pub fn project(&self, args: &Args) -> PathBuf {
        args.value("--project").map_or_else(|| self.default_project.clone(), |project| self.resolve(&project))
    }

    /// The run's Rojo project file: `--rojo <file>`, or `--project <file.project.json>`
    pub fn rojo_file(&self, args: &Args) -> Option<String> {
        args.value("--rojo").or_else(|| args.given("--project").filter(|project| crate::rojo::is_project_file(project)))
    }

    /// Whether the run's project comes from `--rojo` / `--mount` instead of roots
    pub fn has_mounts(&self, args: &Args) -> bool {
        self.rojo_file(args).is_some() || (args.present("--mount") && args.get("--mount") != Some(&Opt::Flag))
    }

    /// The run's source roots: `--roots`, none beside a Rojo project or mounts, else the place's roots in the project
    /// (the ones that exist)
    pub fn project_roots(&self, args: &Args) -> Vec<String> {
        if let Some(roots) = args.value("--roots") {
            return roots.split(';').filter(|root| !root.is_empty()).map(|root| self.resolve(root).display().to_string()).collect();
        }
        if self.has_mounts(args) {
            return Vec::new();
        }
        let project = self.project(args);
        // --place, else the place whose roots hold the story (or module, model, fixture), else the default place
        let source = ["--story", "--module", "--model", "--fixture"].iter().find_map(|key| args.value(key));
        let place = args.value("--place").or_else(|| source.map(|source| crate::places::place_of(&project, &self.resolve(&source)).name));
        place_roots(&project, place.as_deref()).iter().filter(|dir| dir.exists()).map(|dir| dir.display().to_string()).collect()
    }

    /// The Studio-only instances a run loads (`--snapshot-dir`, else its place's implicit snapshot when the project is
    /// the one they belong to; none with `--no-snapshots`, `--roots` or mounts)
    pub fn snapshot_dir(&self, args: &Args) -> Option<PathBuf> {
        if let Some(dir) = args.value("--snapshot-dir") {
            return Some(self.resolve(&dir));
        }
        let place = args.value("--place").unwrap_or_default();
        let implicit = IMPLICIT_SNAPSHOTS.iter().find(|(name, _)| *name == place).map(|(_, snapshot)| *snapshot)?;
        if args.truthy("--no-snapshots") || args.truthy("--roots") || self.has_mounts(args) || crate::paths::is_standalone() {
            return None;
        }
        let normalize = |path: &Path| if cfg!(windows) { path.display().to_string().to_lowercase() } else { path.display().to_string() };
        if normalize(&self.project(args)) != normalize(&self.snapshot_project) {
            return None;
        }
        let studio = args.value("--studio").unwrap_or_else(|| implicit.to_string());
        // (the state directory's own, else the checkout's .cache copy when only that one exists: cli/paths.mjs
        // sharedStatePath)
        let relative = Path::new("snapshots").join(studio);
        let own = self.state.join(&relative);
        let dir = if own.exists() { own } else { self.repo.join(".cache").join(&relative) };
        dir.exists().then_some(dir)
    }

    /// Every mount of a run: its Rojo project's, then `--mount` entries (`source` sees each `$path`: --git-ref).
    /// A `$path` that does not exist is said on stderr and skipped.
    pub fn run_mounts(&self, args: &Args, source: &mut dyn FnMut(PathBuf) -> Result<PathBuf, String>) -> Result<Vec<crate::rojo::Mount>, String> {
        let mut mounts = Vec::new();
        if let Some(file) = self.rojo_file(args) {
            let mut warnings = Vec::new();
            mounts.extend(crate::rojo::mounts_with(&self.resolve(&file), args.value("--rojo-at").as_deref(), &mut warnings, source)?);
            for warning in warnings {
                crate::term::project_line(&warning);
            }
        }
        for item in args.list("--mount").iter().flat_map(|value| value.split(';')).filter(|item| !item.is_empty()) {
            match item.split_once('=').filter(|(at, _)| !at.is_empty()) {
                Some((at, value)) => mounts.push(crate::rojo::Mount { instance: false, path: at.to_string(), value: self.resolve(value).display().to_string() }),
                None => return Err(format!("--mount {item}: expected Service/Name=<dir|file>")),
            }
        }
        Ok(mounts)
    }

    /// `--load Service/Name=<file.rbxm>` (repeatable, or `;`-separated) as engine arguments
    pub fn load_args(&self, args: &Args) -> Result<Vec<String>, Failure> {
        let mut result = Vec::new();
        for item in args.list("--load").iter().flat_map(|value| value.split(';')).filter(|item| !item.is_empty()) {
            let (at, file) = match item.split_once('=').filter(|(at, _)| !at.is_empty()) {
                Some((at, file)) => (at.split(['/', '\\']).filter(|part| !part.is_empty()).collect::<Vec<_>>(), self.resolve(file)),
                None => (Vec::new(), PathBuf::new()),
            };
            if at.len() < 2 {
                return Err(Failure::Usage(format!("--load {item}: expected Service/Name=<file.rbxm>")));
            }
            let lower = file.to_string_lossy().to_lowercase();
            if !(lower.ends_with(".rbxm") || lower.ends_with(".rbxmx") || lower.ends_with(".rbx")) {
                return Err(Failure::Usage(format!("--load {item}: not an .rbxm / .rbxmx / .rbx file")));
            }
            if !file.exists() {
                return Err(Failure::Usage(format!("--load {item}: {} does not exist", file.display())));
            }
            result.extend(["--load".to_string(), format!("{}={}", at.join("/"), file.display())]);
        }
        Ok(result)
    }

    /// `--viewport` as logical `WxH`: `WxH@physical` (or `--viewport-from <png>`, its pixel size) divided by `--scale`
    /// (default 1.5), which must come out whole. None when no viewport is given (or a bare `--viewport`).
    pub fn resolve_viewport(&self, args: &Args) -> Result<Option<String>, Failure> {
        let scale = Some(args.number("--scale")).filter(|scale| *scale != 0.0 && !scale.is_nan()).unwrap_or(1.5);
        let (physical, what) = if let Some(file) = args.value("--viewport-from") {
            let file = self.resolve(&file);
            let mut header = [0u8; 24];
            std::fs::File::open(&file).and_then(|mut opened| opened.read_exact(&mut header)).map_err(|error| format!("{}: {error}", file.display()))?;
            if &header[1..4] != b"PNG" || &header[12..16] != b"IHDR" {
                return Err(Failure::Error(format!("--viewport-from {}: not a PNG", file.display())));
            }
            let size = [u32::from_be_bytes(header[16..20].try_into().unwrap()) as f64, u32::from_be_bytes(header[20..24].try_into().unwrap()) as f64];
            let name = file.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
            (size, format!("--viewport-from {name} ({}x{} physical px)", size[0], size[1]))
        } else if let Some(viewport) = args.value("--viewport") {
            match physical_size(&viewport) {
                Some(size) => (size, format!("--viewport {viewport}")),
                None => return Ok(Some(viewport)),
            }
        } else {
            return Ok(args.given("--viewport"));
        };
        let logical = physical.map(|value| value / scale);
        if logical.iter().any(|value| (value - value.round()).abs() > 1e-6) {
            let shown: Vec<String> = logical.iter().map(|value| crate::glyphs::js_number_string((value * 1000.0).round() / 1000.0)).collect();
            return Err(Failure::Error(format!(
                "{what} is {} logical px at --scale {}: not whole pixels (use a size divisible by the scale, or another --scale)",
                shown.join("x"),
                crate::glyphs::js_number_string(scale)
            )));
        }
        Ok(Some(logical.map(|value| crate::glyphs::js_number_string(value.round())).join("x")))
    }

    /// `--profile <kinds>` and its options, as `rur-native render` takes them
    pub fn profile_args(&self, args: &Args) -> Vec<String> {
        let Some(profile) = args.get("--profile") else { return Vec::new() };
        let mut result = vec!["--profile".to_string()];
        if let Opt::Value(kinds) = profile {
            result.push(kinds.clone());
        }
        for key in ["--profile-out", "--profile-top", "--profile-phase", "--profile-alloc-step"] {
            if let Some(value) = args.given(key) {
                result.push(key.to_string());
                result.push(if key == "--profile-out" { self.resolve(&value).display().to_string() } else { value });
            }
        }
        result
    }

    /// The output directory of a render: `--out`, else `<out dir>/<name>`; `--tag <tag>` puts it in a `<tag>`
    /// subdirectory of that, and `--keep` picks a numbered sibling (`<dir>-2`, `-3`, ...) instead of replacing an
    /// earlier render there
    pub fn render_out_dir(&self, args: &Args, name: &str) -> PathBuf {
        let mut dir = match args.value("--out") {
            Some(out) => self.resolve(&out),
            None => resolve(&self.out, name),
        };
        if let Some(tag) = args.value("--tag") {
            let tag: String = tag.chars().map(|c| if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || (c as u32) < 0x20 { '_' } else { c }).collect();
            dir = dir.join(tag);
        }
        if args.truthy("--keep") {
            let used = |candidate: &Path| candidate.join("summary.json").exists() || candidate.join("scene.json").exists();
            let mut candidate = dir.clone();
            let mut index = 2;
            while used(&candidate) {
                candidate = PathBuf::from(format!("{}-{index}", dir.display()));
                index += 1;
            }
            dir = candidate;
        }
        dir
    }

    /// The engine arguments of a run (cli/rur.mjs runtimeArgs): the renderer, the project (its roots and mounts, or
    /// `project` when they are resolved already: --git-ref), the source and the run's options
    pub fn runtime_args(&self, args: &Args, project: Option<Vec<String>>) -> Result<Vec<String>, Failure> {
        let mut result = vec!["engine".to_string(), "--repo".into(), self.repo.display().to_string()];
        match project {
            Some(project) => result.extend(project),
            None => {
                result.extend(["--roots".to_string(), self.project_roots(args).join(";")]);
                for mount in self.run_mounts(args, &mut |path| Ok(path))? {
                    result.extend(mount.arguments());
                }
            }
        }
        // A release build's run names its project, as its `rur-native render` chooses one: the Rojo project's folder, else
        // --project or the current directory (a checkout's run leaves it to the engine, as the Node CLI did)
        if crate::paths::is_standalone() {
            let project = match self.rojo_file(args) {
                Some(file) => self.resolve(&file).parent().map(Path::to_path_buf).unwrap_or_default(),
                None => self.project(args),
            };
            result.extend(["--project".to_string(), project.display().to_string()]);
        }
        result.extend(self.load_args(args)?);
        let viewport = self.resolve_viewport(args)?;
        if let Some(plugin) = args.get("--plugin") {
            // --plugin: empty settings, or a JSON object / file seeding plugin:GetSetting
            let value = match plugin {
                Opt::Flag => "1".to_string(),
                _ => args.given("--plugin").unwrap_or_default(),
            };
            let inline = value.trim().starts_with('{') || value == "1" || value == "0";
            result.extend(["--plugin".to_string(), if inline { value } else { self.resolve(&value).display().to_string() }]);
        }
        const KEYS: [&str; 19] = [
            "--story", "--module", "--model", "--fixture", "--viewport", "--scale", "--time", "--mode", "--player", "--background", "--place-name",
            "--snapshot-dir", "--actions", "--controls", "--camera", "--place-id", "--rotation-clipping", "--topbar-inset", "--device",
        ];
        for key in KEYS {
            let value = if key == "--viewport" { viewport.clone() } else { args.given(key) };
            let Some(value) = value else { continue };
            // --actions / --controls: a JSON file or inline JSON
            let inline = if key == "--actions" { "[" } else { "{" };
            let is_path = ["--story", "--module", "--model", "--fixture", "--snapshot-dir"].contains(&key) || ((key == "--actions" || key == "--controls") && !value.trim().starts_with(inline));
            let value = if is_path { self.resolve(&value).display().to_string() } else { value };
            // (--place-name names the engine's place: --place picks the roots)
            result.extend([if key == "--place-name" { "--place" } else { key }.to_string(), value]);
        }
        // --all-layers: every enabled ScreenGui under the PlayerGui and CoreGui is drawn, not only the story gui
        if args.present("--all-layers") && args.given("--all-layers").as_deref() != Some("0") {
            result.extend(["--all-layers".to_string(), "1".into()]);
        }
        if args.truthy("--verbose") {
            result.push("--verbose".into());
        }
        Ok(result)
    }

    /// A render's run, from the checks to the child's command line (cli/rur.mjs render): `args` name the source and
    /// the options, `name` the output directory's default name; `--git-ref` extracts the sources at the ref with
    /// `git_ref` (shared by the renders of one command, so each file is extracted once)
    pub fn prepare_render(&self, args: &Args, git_ref: Option<&std::sync::Mutex<crate::gitref::GitRef>>) -> Result<Render, Failure> {
        let mut args = args.clone();
        if args.present("--viewport") || args.present("--viewport-from") {
            match self.resolve_viewport(&args)? {
                Some(viewport) => args.set_value("--viewport", viewport),
                None => {}
            }
            args.remove("--viewport-from");
        }
        let source = ["--story", "--module", "--model", "--fixture"]
            .iter()
            .find_map(|key| args.value(key))
            .ok_or_else(|| Failure::Error("render needs --story, --module, --model, --fixture or --scene".into()))?;
        // A bad --actions list is a usage error before anything runs or is written
        if let Some(actions) = args.get("--actions") {
            let time = if args.given("--time").is_some() { args.number("--time") } else { 1.0 };
            check_actions(actions, time)?;
        }
        let name = source_name(&source, true);
        let out = self.render_out_dir(&args, &name);
        std::fs::create_dir_all(&out).map_err(|error| format!("{}: {error}", out.display()))?;
        let started = Instant::now();
        let mut run_args = args.clone();
        run_args.remove("--snapshot-dir");
        if let Some(dir) = self.snapshot_dir(&args) {
            run_args.set_value("--snapshot-dir", dir.display().to_string());
        }
        let mut described = None;
        let mut project = None;
        if args.value("--git-ref").is_some() {
            let git_ref = git_ref.ok_or_else(|| Failure::Error("--git-ref: no git ref state".into()))?;
            let mut git_ref = git_ref.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            let roots = self.project_roots(&run_args);
            let (mapped, fragment) = self.apply_git_ref(&run_args, &roots, &mut git_ref)?;
            run_args = mapped;
            project = Some(fragment);
            described = Some(git_ref.describe());
        }
        let engine = self.runtime_args(&run_args, project)?;
        let png = !args.truthy("--no-png");
        let mut argv = vec!["render".to_string(), "--dir".into(), out.display().to_string()];
        if !png {
            argv.push("--no-png".into());
        }
        if args.truthy("--offline") {
            argv.push("--offline".into());
        }
        if args.truthy("--placeholders") {
            argv.push("--placeholders".into());
        }
        argv.extend(self.profile_args(&args));
        argv.extend(engine);
        Ok(Render { out, argv, png, git_ref: described, started })
    }

    /// The run's arguments with every source path moved to the git ref (gitref.rs), and its project as engine arguments:
    /// `roots` (the run's) at the ref and its mounts (each `$path` of its Rojo project extracted too)
    pub fn apply_git_ref(&self, args: &Args, roots: &[String], git_ref: &mut crate::gitref::GitRef) -> Result<(Args, Vec<String>), String> {
        let mut result = args.clone();
        result.remove("--git-ref");
        let mut mapped = Vec::new();
        for root in roots {
            mapped.push(git_ref.map_root(Path::new(root))?.display().to_string());
        }
        let mut project = vec!["--roots".to_string(), mapped.join(";")];
        if let Some(file) = self.rojo_file(args) {
            let mapped = git_ref.map(&self.resolve(&file), &[], true)?;
            let mut warnings = Vec::new();
            let mounts = crate::rojo::mounts_with(&mapped, args.value("--rojo-at").as_deref(), &mut warnings, &mut |source| git_ref.map(&source, &[], false))?;
            for warning in warnings {
                crate::term::project_line(&warning);
            }
            project.extend(mounts.iter().flat_map(crate::rojo::Mount::arguments));
        }
        for item in args.list("--mount").iter().flat_map(|value| value.split(';')).filter(|item| !item.is_empty()) {
            match item.split_once('=').filter(|(at, _)| !at.is_empty()) {
                Some((at, value)) => project.extend(["--mount".to_string(), format!("{at}={}", git_ref.map(&self.resolve(value), &[], false)?.display())]),
                None => return Err(format!("--mount {item}: expected Service/Name=<dir|file>")),
            }
        }
        for key in ["--story", "--fixture", "--module", "--model"] {
            if let Some(value) = args.value(key) {
                result.set_value(key, git_ref.map(&self.resolve(&value), &[], false)?.display().to_string());
            }
        }
        Ok((result, project))
    }

    /// Runs a prepared render's child (`rur-native render`); its stderr is echoed as it comes (`echo`) or kept, and
    /// with a `timeout` the child and everything it started are killed when it runs longer
    pub fn run_render(&self, render: &Render, echo: bool, timeout: Option<Duration>) -> Outcome {
        let mut command = Command::new(&self.exe);
        command.args(&render.argv).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        // A checkout's children run in the checkout, as the Node CLI ran them; a release build's in the current
        // directory, its default project
        if !crate::paths::is_standalone() {
            command.current_dir(&self.repo);
        }
        for (name, value) in &self.env {
            match value {
                Some(value) => command.env(name, value),
                None => command.env_remove(name),
            };
        }
        // It downloads: the API key this process took out of its environment at startup goes with it
        crate::download::with_api_key(&mut command);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // CREATE_NO_WINDOW
            command.creation_flags(0x0800_0000);
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            // a process group of its own, so a timeout ends everything it started
            command.process_group(0);
        }
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => return Outcome { code: None, timed_out: false, stdout: String::new(), stderr: format!("{}: {error}", self.exe.display()) },
        };
        let mut stdout_pipe = child.stdout.take().expect("piped stdout");
        let stdout = std::thread::spawn(move || {
            let mut text = Vec::new();
            let _ = stdout_pipe.read_to_end(&mut text);
            String::from_utf8_lossy(&text).into_owned()
        });
        let mut stderr_pipe = child.stderr.take().expect("piped stderr");
        let stderr = std::thread::spawn(move || {
            use std::io::Write;
            let mut text = Vec::new();
            let mut chunk = [0u8; 8192];
            loop {
                match stderr_pipe.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(count) => {
                        if echo {
                            let _ = std::io::stderr().write_all(&chunk[..count]);
                        }
                        text.extend_from_slice(&chunk[..count]);
                    }
                }
            }
            String::from_utf8_lossy(&text).into_owned()
        });
        let deadline = timeout.map(|timeout| Instant::now() + timeout);
        let mut timed_out = false;
        let code = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status.code(),
                Ok(None) => {}
                Err(_) => break None,
            }
            if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                timed_out = true;
                kill_tree(&mut child);
                break child.wait().ok().and_then(|status| status.code());
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        Outcome { code, timed_out, stdout: stdout.join().unwrap_or_default(), stderr: stderr.join().unwrap_or_default() }
    }

    /// A render's result as `rur render` reports it: the child's result (`runtime failed` when it failed), then
    /// summary.json written from the directory
    pub fn finish_render(&self, render: &Render, outcome: &Outcome) -> Result<Json, String> {
        let result = outcome.result().ok_or_else(|| {
            let tail = crate::js::slice_end(&outcome.stderr, 4000);
            format!("runtime failed (exit {}){}", outcome.code.map_or("null".to_string(), |code| code.to_string()), if tail.is_empty() { String::new() } else { format!("\n{tail}") })
        })?;
        let seconds = (render.started.elapsed().as_millis() as f64) / 1000.0;
        let summary = crate::summary::render_summary(&render.out, render.png, &result, seconds, render.git_ref.as_ref())?;
        crate::summary::write_summary(&render.out, &summary)?;
        Ok(summary)
    }
}

/// The roots of the named place (the default place when none is named); none for a place the project does not have
fn place_roots(project: &Path, place: Option<&str>) -> Vec<PathBuf> {
    match place {
        Some(name) => crate::places::place_named(project, name).map(|place| place.roots).unwrap_or_default(),
        None => crate::places::places(project).swap_remove(0).roots,
    }
}

/// `WxH@physical` as its two sizes
fn physical_size(viewport: &str) -> Option<[f64; 2]> {
    let text = viewport.trim();
    let (size, unit) = text.split_once('@')?;
    if !unit.trim().eq_ignore_ascii_case("physical") {
        return None;
    }
    let (width, height) = size.trim().split_once(['x', 'X'])?;
    let number = |text: &str| {
        let text = text.trim();
        let valid = !text.is_empty() && text.chars().all(|c| c.is_ascii_digit() || c == '.') && !text.starts_with('.') && text.matches('.').count() <= 1 && !text.ends_with('.');
        valid.then(|| text.parse::<f64>().ok()).flatten()
    };
    Some([number(width)?, number(height)?])
}

/// `--actions <JSON list | file>` checked as the engine checks it (input/actions.rs), and each action against the
/// capture at `time` (one starting or ending after it would be squeezed onto that moment): a usage error when it can't
/// be used
fn check_actions(actions: &Opt, time: f64) -> Result<(), Failure> {
    let Opt::Value(text) = actions else { return Err(Failure::Usage("--actions needs a JSON list or a JSON file".into())) };
    if text.trim().is_empty() {
        return Err(Failure::Usage("--actions needs a JSON list or a JSON file".into()));
    }
    let inline = text.trim().starts_with('[') || text.trim().starts_with('{');
    let (json, source) = if inline {
        (text.clone(), "--actions".to_string())
    } else {
        let file = resolve(Path::new(""), text);
        let json = std::fs::read_to_string(&file).map_err(|error| {
            let reason = if error.kind() == std::io::ErrorKind::NotFound { "no such file".to_string() } else { error.to_string() };
            Failure::Usage(format!("--actions {}: {reason} (give a JSON list or a JSON file)", file.display()))
        })?;
        (json, format!("--actions {}", file.display()))
    };
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|error| Failure::Usage(format!("{source}: not valid JSON: {error}")))?;
    let late = || {
        for (index, action) in value.as_array()?.iter().enumerate() {
            let kind = action.get("type").and_then(serde_json::Value::as_str);
            let number = |key: &str, default: f64| action.get(key).and_then(serde_json::Value::as_f64).unwrap_or(default);
            let span = match kind {
                Some("click") => number("hold", 0.0),
                Some("drag") => number("duration", 0.25),
                _ => 0.0,
            };
            let end = number("t", 0.0) + span;
            if end > time + 1e-9 {
                let label = format!("--actions[{index}]{}", kind.map(|kind| format!(" ({kind})")).unwrap_or_default());
                let at = crate::glyphs::js_number_string(crate::js::to_fixed(end, 6).parse().unwrap_or(end));
                let what = if span != 0.0 { format!("ends at {at} s") } else { format!("is at {at} s") };
                return Some(format!("{label}: {what}, after the capture at --time {}; raise --time", crate::glyphs::js_number_string(time)));
            }
        }
        None
    };
    crate::engine::input::actions::validate(&value)
        .and_then(|()| late().map_or(Ok(()), Err))
        .map_err(|error| Failure::Usage(if inline { error } else { format!("{error} (in {})", &source["--actions ".len()..]) }))
}

/// A source file's name without its extension: `.story.luau`, `.luau`, `.rbxm` / `.rbxmx` / `.rbx` (and `.json` for a render's
/// own directory)
pub fn source_name(source: &str, json: bool) -> String {
    let name = source.rsplit(['/', '\\']).next().unwrap_or(source);
    for suffix in [".story.luau", ".luau", ".rbxmx", ".rbxm", ".rbx"].into_iter().chain(json.then_some(".json")) {
        if let Some(stem) = name.strip_suffix(suffix) {
            return stem.to_string();
        }
    }
    name.to_string()
}

/// A render ready to run: its output directory, the child's arguments and what its summary adds
pub struct Render {
    pub out: PathBuf,
    pub argv: Vec<String>,
    pub png: bool,
    pub git_ref: Option<Json>,
    pub started: Instant,
}

/// How a render's child ended
pub struct Outcome {
    /// The exit code (None: killed, or it could not start)
    pub code: Option<i32>,
    pub timed_out: bool,
    pub stdout: String,
    pub stderr: String,
}

impl Outcome {
    /// The result line a render prints when it succeeded
    pub fn result(&self) -> Option<Json> {
        if self.code != Some(0) || self.timed_out {
            return None;
        }
        Json::parse(self.stdout.trim().lines().last()?.trim_end_matches('\r')).ok()
    }
}

/// Ends `child` and every process it started
fn kill_tree(child: &mut Child) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = Command::new("taskkill")
            .args(["/pid", &child.id().to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x0800_0000)
            .status();
    }
    #[cfg(unix)]
    {
        let _ = Command::new("kill").args(["-KILL", "--", &format!("-{}", child.id())]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status();
    }
    let _ = child.kill();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|arg| arg.to_string()).collect()
    }

    #[test]
    fn options_parse_as_the_node_cli_parsed_them() {
        let args = Args::parse(&strings(&["sweep", "--mount", "A/B=x", "--mount", "C/D=y", "--json", "--jobs", "4", "--no-png", "--filter", "--viewport", ""]));
        assert_eq!(args.positional, ["sweep"]);
        assert_eq!(args.list("--mount"), ["A/B=x", "C/D=y"]);
        assert_eq!(args.get("--json"), Some(&Opt::Flag));
        assert_eq!(args.number("--jobs"), 4.0);
        assert!(args.truthy("--no-png") && args.truthy("--filter") && !args.truthy("--viewport") && !args.truthy("--missing"));
        assert_eq!(args.value("--filter"), None);
        assert_eq!(args.given("--viewport").as_deref(), Some(""));
        assert_eq!(args.number("--filter"), 1.0);
        assert!(args.number("--missing").is_nan());
        let (rest, env) = global_options(&strings(&["--state-dir=x", "--isolated", "--text-subpixel", "on", "--out", "o"])).unwrap();
        assert_eq!(rest, ["--out", "o"]);
        assert_eq!(env[0].0, "RUR_STATE_DIR");
        assert_eq!(env[2], ("RUR_TEXT_SUBPIXEL", Some("on".to_string())));
        assert!(matches!(global_options(&strings(&["--text-subpixel", "maybe"])), Err(Failure::Usage(_))));
    }

    #[test]
    fn viewports_names_and_output_directories() {
        let (context, _) = Context::new(&strings(&["--repo", "."])).unwrap();
        let viewport = |argv: &[&str]| context.resolve_viewport(&Args::parse(&strings(argv)));
        assert_eq!(viewport(&["--viewport", "1800x1350@physical"]).unwrap().as_deref(), Some("1200x900"));
        assert_eq!(viewport(&["--viewport", "1000x500@physical", "--scale", "2"]).unwrap().as_deref(), Some("500x250"));
        assert_eq!(viewport(&["--viewport", "800x600"]).unwrap().as_deref(), Some("800x600"));
        assert!(viewport(&["--viewport", "1001x900@physical"]).unwrap_err().message().contains("not whole pixels"));
        assert_eq!(source_name("a/b/Card.story.luau", false), "Card");
        assert_eq!(source_name("C:\\x\\Model.rbxmx", false), "Model");
        assert_eq!(source_name("scene.json", true), "scene");
        assert_eq!(source_name("scene.json", false), "scene.json");
        let out = context.render_out_dir(&Args::parse(&strings(&["--out", "somewhere", "--tag", "a/b:c"])), "x");
        assert!(out.ends_with(Path::new("somewhere").join("a_b_c")), "{}", out.display());
        assert_eq!(context.render_out_dir(&Args::default(), "x"), context.out.join("x"));
    }
}
