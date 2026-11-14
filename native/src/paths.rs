//! Where a native host finds its checkout, state directory and project. A build runs in one of two
//! modes, and the mode never changes while the process runs:
//!
//! - **Checkout mode**, for ordinary builds: the checkout Cargo built from (`CARGO_MANIFEST_DIR/..`)
//!   supplies the repo, `.cache`, the default project and the asset helper, as it always has. Copying
//!   the executable elsewhere (the Node CLI keeps a copy in its cache) does not change this.
//! - **Standalone mode**, for release builds: nothing comes from the machine that built the binary.
//!   The state directory is a per-user cache folder and the project is the current directory.
//!
//! The release workflow marks its build with `RUR_RELEASE_BUILD=1`, read at compile time. A marker in
//! the binary is the one signal that the build runner's checkout, or a folder the user happens to
//! keep it in, cannot fake or remove. Run-time variables are no use for this: a batch job replaces
//! the `RUR_*` variables, and a service starts workers that would have to carry the choice along.
//! A build without the marker keeps its checkout defaults even if the baked checkout no longer
//! exists, so the mode never depends on files that can appear or vanish at run time.
//!
//! # Changing the environment
//!
//! `std::env::set_var` is only sound while no other thread reads the environment. Windows guards its
//! environment block, but on Linux and macOS a `setenv` can free memory that another thread's `getenv`
//! (the Vulkan loader's, a GPU driver's) is still reading. So a release build changes its environment
//! once, at the start of `main` before any thread exists ([`initialize`] and
//! `render::gpu::initialize_loader_environment`). Batch jobs with the same environment run in-process;
//! only jobs needing other variables get a child process (`engine::cli::batch_job_process`).
//! Checkout builds keep the code they have always run: in-process batch jobs and temporary Vulkan
//! settings. The Node CLI, the goldens and the checkout tests rely on them, and they are not what a
//! release ships.
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    sync::OnceLock,
};

/// Windows' `canonicalize` returns verbatim paths (`\\?\C:\...`). Paths are compared and handed to
/// Node as plain strings, so the prefix is dropped to match the paths users and the CLI write. A
/// network path (`\\?\UNC\server\share`) becomes `\\server\share`.
#[cfg(windows)]
fn without_verbatim_prefix(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(network_path) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{network_path}"));
    }
    if let Some(local_path) = text.strip_prefix(r"\\?\") {
        return PathBuf::from(local_path);
    }
    path
}

#[cfg(not(windows))]
fn without_verbatim_prefix(path: PathBuf) -> PathBuf {
    path
}

/// `path.canonicalize()` with the Windows verbatim prefix removed (see [`without_verbatim_prefix`]).
pub(crate) fn canonical_path(path: &Path) -> std::io::Result<PathBuf> {
    Ok(without_verbatim_prefix(path.canonicalize()?))
}

/// The checkout this binary was built from, as Cargo saw it at compile time.
fn build_checkout() -> PathBuf {
    let native_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    native_dir.parent().unwrap().to_path_buf()
}

/// True in standalone mode: a build marked `RUR_RELEASE_BUILD=1`. The answer is the same for the
/// whole process, including every batch job and every copy of the executable.
pub fn is_standalone() -> bool {
    option_env!("RUR_RELEASE_BUILD") == Some("1")
}

/// A path from an environment variable. An empty value counts as unset.
fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// The state directory of a standalone build: `RUR_STATE_DIR`, else `RUR_CACHE_DIR`
/// (the names the Node CLI reads), else the per-user cache folder. `env` is a parameter so tests
/// can pass their own variables.
fn standalone_state_with(os: &str, env: impl Fn(&str) -> Option<PathBuf>) -> Option<PathBuf> {
    env("RUR_STATE_DIR")
        .or_else(|| env("RUR_CACHE_DIR"))
        .or_else(|| user_cache_with(os, env))
}

fn standalone_state() -> anyhow::Result<PathBuf> {
    standalone_state_with(std::env::consts::OS, env_path)
        .ok_or_else(|| anyhow::anyhow!("No user cache folder was found. Set RUR_STATE_DIR"))
}

/// `%LOCALAPPDATA%\rur` on Windows, `~/Library/Caches/rur` on macOS, and
/// `$XDG_CACHE_HOME/rur` (else `~/.cache/rur`) elsewhere.
fn user_cache_with(os: &str, env: impl Fn(&str) -> Option<PathBuf>) -> Option<PathBuf> {
    let base = match os {
        "windows" => env("LOCALAPPDATA"),
        "macos" => env("HOME").map(|home| home.join("Library/Caches")),
        _ => env("XDG_CACHE_HOME").or_else(|| env("HOME").map(|home| home.join(".cache"))),
    }?;
    Some(base.join("rur"))
}

/// The engine's "repo", which is where it keeps its caches. Checkout mode: the checkout this
/// binary was built from. Standalone mode: `RUR_REPO`, else the state directory.
#[cfg(not(feature = "web"))]
pub fn default_repo() -> PathBuf {
    if !is_standalone() {
        return build_checkout();
    }
    env_path("RUR_REPO")
        // Without a cache location this is an empty path, but `initialize` has already refused to start
        .or_else(|| standalone_state_with(std::env::consts::OS, env_path))
        .unwrap_or_default()
}

/// The web build runs on the page's own file system.
#[cfg(feature = "web")]
pub fn default_repo() -> PathBuf {
    PathBuf::from("/rur")
}

/// The project when `--project` is not given. Checkout mode: the checkout's project (the example
/// project in the public build), as a path under `repo`. Standalone mode: `RUR_PROJECT`,
/// else the current directory.
pub fn default_project(repo: &Path) -> PathBuf {
    if !is_standalone() {
        return repo.join("examples/project");
    }
    if let Some(project) = env_path("RUR_PROJECT") {
        return std::path::absolute(project).expect("Reading current working directory");
    }
    std::env::current_dir().expect("Reading current working directory")
}

/// Where captures and desktop exports go when `--out` is not given. A checkout keeps `out/native`
/// relative to `--repo`, as before. A standalone binary can be started from any folder, a project
/// among them, so its default goes under the state directory rather than into whatever folder the
/// user ran it in. An output the user names is never moved: a relative one is relative to `--repo`
/// in checkout mode and to the current directory in standalone mode.
pub fn default_output() -> PathBuf {
    if is_standalone() {
        standalone_output("native")
    } else {
        PathBuf::from("out/native")
    }
}

/// `<state dir>/out/<name>`, the standalone default for one kind of output.
pub(crate) fn standalone_output(name: &str) -> PathBuf {
    crate::state_dir(&default_repo()).join("out").join(name)
}

/// The path the desktop Open field names. A standalone binary has no checkout to resolve it
/// against, so a relative path is taken in the selected project; a checkout build still takes
/// it in the checkout.
pub(crate) fn open_path(repo: &Path, project: &Path, path: &Path) -> PathBuf {
    relative_open_path(is_standalone(), repo, project, path)
}

fn relative_open_path(standalone: bool, repo: &Path, project: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else if standalone {
        project.join(path)
    } else {
        repo.join(path)
    }
}

/// The value of `--repo`, written either `--repo <dir>` or `--repo=<dir>`.
fn repo_argument(argv: &[String]) -> Option<PathBuf> {
    argv.iter().enumerate().find_map(|(index, arg)| {
        if let Some(value) = arg.strip_prefix("--repo=") {
            return Some(PathBuf::from(value));
        }
        if arg == "--repo" {
            return argv.get(index + 1).map(PathBuf::from);
        }
        None
    })
}

/// The options that take a path. `--profile` and `--benchmark` are not here: their value is
/// optional, so a bare one is a complete option, and an empty profile kind is already an error.
/// `--controls` and `--context` take a file or inline JSON: both are still accepted, but the value
/// must be present.
const PATH_OPTIONS: &[&str] = &[
    "--repo",
    "--project",
    "--place",
    "--out",
    "--dir",
    "--profile-out",
    "--dump-scene",
    "--dependencies",
    "--snapshot-dir",
    "--snapshot",
    "--story",
    "--scene",
    "--batch",
    "--fixture",
    "--module",
    "--model",
    "--harness",
    "--controls",
    "--context",
    "--actions",
    "--plugin",
    "--load",
    "--animations",
    "--frame-times",
    "--roots",
    "--mount",
    "--rojo",
    "--instance",
];

/// Whether `value` can follow the path option `option`. Another flag cannot: it would be taken
/// as a path. A lone `-` is not a flag. A value with a NUL character is no path. An empty or blank
/// value is refused too, except for `--roots`, where an empty list suppresses implicit roots.
fn is_path_value(option: &str, value: &str) -> bool {
    let is_flag = value.starts_with('-') && value != "-";
    let has_nul = value.contains('\0');
    let is_blank = value.trim().is_empty();
    let allows_empty = option == "--roots";
    !is_flag && !has_nul && (allows_empty || !is_blank)
}

/// Checks a path from a request, a query or a batch entry. The service and the batch runner call
/// it before they schedule any work, because an empty path would otherwise become the current
/// directory. Checkout builds keep their original handling and accept everything here.
pub(crate) fn validate_path_value(name: &str, value: &str) -> Result<(), String> {
    if is_standalone() && !is_path_value(name, value) {
        return Err(format!("{name} needs a nonempty value"));
    }
    Ok(())
}

/// The required string field `name` of a request or batch entry, checked as a path.
pub(crate) fn request_path<'a>(
    input: &'a serde_json::Value,
    name: &str,
) -> Result<&'a str, String> {
    let value = input[name]
        .as_str()
        .ok_or_else(|| format!("{name} is required"))?;
    validate_path_value(name, value)?;
    Ok(value)
}

/// Refuses a path option with no value, an empty value or a flag in its value position, in a
/// release binary only. Without a value, a default (the state directory, the current directory)
/// or the engine's own fallback would choose where to write, outside the state directory and the
/// paths the user named; a profiler started from the arguments would already be writing there.
/// A checkout build accepts these inputs, as it always has, and keeps doing so.
///
/// Called before any default is chosen: at startup, by the recorder, by the profile commands, and
/// for batch jobs and service requests, which do not come through `main`.
pub(crate) fn validate_arguments(argv: &[String]) -> Result<(), String> {
    if !is_standalone() {
        return Ok(());
    }
    let argv = crate::engine::launch::split_equals(argv);
    for (index, option) in argv.iter().enumerate() {
        if !PATH_OPTIONS.contains(&option.as_str()) {
            continue;
        }
        let has_value = argv
            .get(index + 1)
            .is_some_and(|value| is_path_value(option, value));
        if !has_value {
            return Err(format!("{option} needs a nonempty value"));
        }
    }
    Ok(())
}

/// `RUR_*` variables by name, as a batch runner has them or a batch job asks for them.
pub(crate) type RurVariables = BTreeMap<OsString, OsString>;

fn is_rur_variable(name: &OsStr) -> bool {
    name.to_string_lossy().starts_with("RUR_")
}

/// This process's `RUR_*` variables.
pub(crate) fn rur_variables() -> RurVariables {
    std::env::vars_os()
        .filter(|(name, _)| is_rur_variable(name))
        .collect()
}

/// The `RUR_*` variables a batch runner was started with, before [`initialize`] adds the state
/// directory and repo it chose. Unset in every other process, and in a runner that could not
/// choose a state directory: then every job runs in a child.
static BATCH_STARTUP_VARIABLES: OnceLock<RurVariables> = OnceLock::new();

/// Whether a release job can run in this batch runner as it is. The job's `RUR_*` variables must be
/// the ones the runner has, and `--repo` must be the repo the runner chose; anything else would
/// need an environment change that only a child process can make safely (see the module comment).
pub(crate) fn batch_environment_matches(job_variables: &RurVariables, argv: &[String]) -> bool {
    let Some(startup_variables) = BATCH_STARTUP_VARIABLES.get() else {
        return false;
    };
    if let Some(repo) = repo_argument(argv) {
        if std::path::absolute(repo).ok() != env_path("RUR_REPO") {
            return false;
        }
    }
    // Startup adds RUR_STATE_DIR and RUR_REPO, so a job need not repeat them. A job may also name
    // exactly the values the runner now has.
    let as_the_runner_started = job_variables == startup_variables;
    let as_the_runner_has_them = *job_variables == rur_variables();
    as_the_runner_started || as_the_runner_has_them
}

/// Called once by `main` before any thread starts. Checkout mode leaves the engine's defaults
/// alone. Standalone mode publishes its state directory and repo, in the environment and to the
/// pinned engine, so the engine, the asset helper and any worker the process starts agree.
///
/// `engine-batch` records its starting variables for jobs that need no environment change. If it
/// has no usable state directory, it still starts: jobs that name their own run in children.
///
/// # Safety
/// Must run before any thread starts, so that nothing reads the environment while it changes (see
/// the module comment).
pub unsafe fn initialize() -> anyhow::Result<()> {
    if !is_standalone() {
        return Ok(());
    }
    let argv: Vec<String> = std::env::args().collect();
    // Before the state directory is created or any default is chosen
    validate_arguments(&argv).map_err(anyhow::Error::msg)?;
    // The profile commands also take paths without an option name. They run from the desktop
    // build only, and they are checked here too, so that an empty input is refused rather than
    // choosing the profile in the current folder.
    #[cfg(feature = "desktop")]
    if argv.get(1).is_some_and(|arg| arg == "profile") {
        if let Some(command) = argv.get(2) {
            crate::profile_cli::validate_arguments(command, &argv[3..])
                .map_err(anyhow::Error::msg)?;
        }
    }
    let is_batch_runner = argv.get(1).is_some_and(|arg| arg == "engine-batch");
    let startup_variables = is_batch_runner.then(rur_variables);
    let state = match standalone_state() {
        Ok(state) => std::path::absolute(state)?,
        // Without a state directory a runner can still serve jobs that name their own
        Err(_) if is_batch_runner => return Ok(()),
        Err(error) => return Err(error),
    };
    let repo = std::path::absolute(repo_argument(&argv).unwrap_or_else(|| state.clone()))?;
    match std::fs::create_dir_all(&state) {
        Ok(()) => {}
        Err(_) if is_batch_runner => return Ok(()),
        Err(error) => return Err(error.into()),
    }
    // SAFETY: the caller guarantees that no other thread exists yet
    unsafe {
        std::env::set_var("RUR_STATE_DIR", &state);
        std::env::set_var("RUR_REPO", &repo);
    }
    // SAFETY: the caller guarantees that no other thread exists yet
    #[cfg(feature = "desktop")]
    unsafe {
        default_environment_profile_output(&argv);
    }
    roblox_engine_emulator::set_default_repo(repo);
    if let Some(startup_variables) = startup_variables {
        let _ = BATCH_STARTUP_VARIABLES.set(startup_variables);
    }
    Ok(())
}

/// Gives `rur-native record` a standalone output for a profile that `REE_PROFILE` turns on.
///
/// Such a profile needs its own default, apart from the `--profile-out` that [`record_arguments`]
/// appends. When the environment enables a profile, the pinned recorder takes the output from
/// `REE_PROFILE_OUT` and ignores `--profile-out`, so with no variable it writes `ree-profile` into
/// the current directory. Setting the variable here, once, gives the engine the output the
/// command line implies: `--profile-out`, else `profile` below the recording output, else below the
/// default record output. A nonempty `REE_PROFILE_OUT` is the caller's explicit choice and wins
/// over all of these, so it is left alone.
///
/// The environment changes at startup because nothing else may do it: changing it later races with
/// the threads the process starts. Other commands already pass their own default output, which
/// the engine reads when `REE_PROFILE_OUT` is unset, so they need nothing here.
///
/// # Safety
/// Must run before any thread starts, so that nothing reads the environment while it changes (see
/// the module comment).
#[cfg(feature = "desktop")]
unsafe fn default_environment_profile_output(argv: &[String]) {
    let is_recording = argv.get(1).is_some_and(|arg| arg == "record");
    let profile_enabled = std::env::var("REE_PROFILE").is_ok_and(|value| !value.trim().is_empty());
    if !is_recording || !profile_enabled || env_path("REE_PROFILE_OUT").is_some() {
        return;
    }
    let recorder_arguments = crate::engine::launch::split_equals(&argv[2..]);
    let output = option_value(&recorder_arguments, "--profile-out")
        .map(PathBuf::from)
        .unwrap_or_else(|| record_profile_directory(&recorder_arguments));
    // SAFETY: the caller guarantees that no other thread exists yet
    unsafe { std::env::set_var("REE_PROFILE_OUT", output) };
}

/// Standalone mode only: gives the engine arguments a project, which is `--project`, else the
/// default one. The project's roots come from the source's place (or `--place`) unless roots or
/// mounts are given, and the checkout's Studio snapshots do not apply.
///
/// `roots_given` says whether `--roots` was among the arguments. The engine's list of roots drops
/// empty entries, so `--roots ""` leaves it as empty as no `--roots` does. The two differ: omitted
/// roots come from the place, while an empty list means no implicit roots, as in a checkout build.
#[cfg(feature = "desktop")]
pub fn prepare_project(
    args: &mut crate::engine::launch::Args,
    repo: &Path,
    source: Option<&Path>,
    roots_given: bool,
) -> Result<(), String> {
    if !is_standalone() {
        return Ok(());
    }
    let project = args
        .project
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| default_project(repo));
    let project = std::path::absolute(project).map_err(|error| error.to_string())?;
    if !roots_given && args.roots.is_empty() && args.mounts.is_empty() {
        // The place name is kept next to its roots: the arguments, and the service's standby key
        // made from them, then say which place was chosen, not only what it holds.
        let place = selected_place(&project, args.place.as_deref(), source)?;
        args.roots = place
            .roots
            .iter()
            .map(|root| root.to_string_lossy().into_owned())
            .collect();
        args.place = Some(place.name);
    }
    args.project = Some(project.to_string_lossy().into_owned());
    args.no_snapshots = true;
    Ok(())
}

/// The first place with a root that holds `source`, else the first place. Both sides are canonical
/// paths, so a link to the project or to the story (a symlink, a junction) still finds its place;
/// `places::place_of` compares the paths as written.
#[cfg(feature = "desktop")]
fn place_containing(project: &Path, source: &Path) -> Result<crate::places::Place, String> {
    let source = canonical_path(source).map_err(|error| format!("source: {error}"))?;
    let mut places = crate::places::places(project);
    let root_holds_source =
        |root: &PathBuf| canonical_path(root).is_ok_and(|root| source.starts_with(root));
    let index = places
        .iter()
        .position(|place| place.roots.iter().any(root_holds_source))
        .unwrap_or(0);
    Ok(places.swap_remove(index))
}

/// The named place, else the source's place, else the first place.
#[cfg(feature = "desktop")]
pub(crate) fn selected_place(
    project: &Path,
    name: Option<&str>,
    source: Option<&Path>,
) -> Result<crate::places::Place, String> {
    Ok(match name {
        Some(name) => crate::places::place_named(project, name)
            .ok_or_else(|| format!("unknown place: {name}"))?,
        None => place_containing(project, source.unwrap_or(project))?,
    })
}

/// The engine's options among `arguments`, as the engine parses them, and whether `--roots` was one
/// of them (see `prepare_project` for why that matters). Other arguments are left to their owner.
#[cfg(feature = "desktop")]
fn parse_engine_arguments(
    arguments: &[String],
) -> Result<(crate::engine::launch::Args, bool), String> {
    let mut engine_args = crate::engine::launch::Args::default();
    let mut roots_given = false;
    let mut index = 0;
    while index < arguments.len() {
        let key = &arguments[index];
        // `accept` gives how many arguments the option used, or `None` when it is not an engine option
        let used = engine_args.accept(key, arguments.get(index + 1))?;
        roots_given |= used.is_some() && key == "--roots";
        index += used.unwrap_or(1);
    }
    Ok((engine_args, roots_given))
}

/// The arguments for `rur-native record`. In standalone mode the pinned recorder would choose its
/// own project, which could be a checkout's, so the project choice of the engine CLI
/// (`prepare_project`) is made here and appended to the caller's arguments. The recorder then
/// mounts the roots and reads their settings as it always does.
#[cfg(feature = "desktop")]
pub fn record_arguments(argv: &[String]) -> Result<Vec<String>, String> {
    if !is_standalone() {
        return Ok(argv.to_vec());
    }
    let mut recorder_arguments = crate::engine::launch::split_equals(argv);
    // Before the recorder appends its own arguments, which would hide an incomplete one
    validate_arguments(&recorder_arguments)?;
    let profile_out = default_profile_out(&recorder_arguments);

    let (mut engine_args, roots_given) = parse_engine_arguments(&recorder_arguments)?;
    let repo = engine_args
        .repo
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(default_repo);
    prepare_project(&mut engine_args, &repo, None, roots_given)?;

    if let Some(out) = profile_out {
        recorder_arguments.extend(["--profile-out".into(), out.to_string_lossy().into_owned()]);
    }
    let project = engine_args
        .project
        .expect("prepare_project sets the project");
    // `--roots` is passed even when empty: an explicit empty list or `--mount` arguments suppress
    // the place's roots, and the recorder must not pick roots of its own then.
    recorder_arguments.extend([
        "--project".into(),
        project,
        "--roots".into(),
        engine_args.roots.join(";"),
    ]);
    if let Some(place) = engine_args.place {
        recorder_arguments.extend(["--place".into(), place]);
    }
    recorder_arguments.push("--no-snapshots".into());
    Ok(recorder_arguments)
}

/// The value after `key` in `argv`, once `--key=value` has been split into two arguments.
fn option_value<'a>(argv: &'a [String], key: &str) -> Option<&'a String> {
    let index = argv.iter().position(|arg| arg == key)?;
    argv.get(index + 1)
}

/// The `--profile-out` that a standalone recorder is given, or `None` when the caller already
/// chose the profile output: with `--profile-out`, or with `RUR_BOUNDARY_PROFILE` while no engine
/// profile is running.
///
/// The pinned recorder starts its profiler before it validates `--out`, and its `--dump-scene`
/// alias has no profile default of its own, so left alone either would write a fallback folder
/// beside the caller. The default is `profile` below the output directory (`--dir`, `--out` or
/// the `--dump-scene` alias), else below the default record output.
///
/// An engine profile (`caches`, `phases` and the other kinds the pinned engine defines; not the
/// `boundary` profile) can be turned on by `--profile` or by `REE_PROFILE`. The recorder prefers
/// `--profile-out` to `REE_PROFILE_OUT` when the command line turns the profile on, so an appended
/// default would hide an output the caller exported. That explicit `REE_PROFILE_OUT` is passed on as
/// the `--profile-out` instead, and so wins over the default. `RUR_BOUNDARY_PROFILE` is read only
/// by the boundary profiler. Left set while an engine profile runs, it must not suppress the
/// engine profile's default.
fn default_profile_out(argv: &[String]) -> Option<PathBuf> {
    use roblox_engine_emulator::profiler;
    if option_value(argv, "--profile-out").is_some() {
        return None;
    }
    let boundary_profile = profiler::profile_value(argv).as_deref() == Some("boundary");
    let engine_profile = profiler::requested(argv) && !boundary_profile;
    if !engine_profile && env_path("RUR_BOUNDARY_PROFILE").is_some() {
        return None;
    }
    let environment_output = if engine_profile {
        env_path("REE_PROFILE_OUT")
    } else {
        None
    };
    Some(environment_output.unwrap_or_else(|| record_profile_directory(argv)))
}

/// The recorder's unnamed profile output, used by both the command line and the environment: the
/// `profile` folder below the recording output, else below the default record output.
fn record_profile_directory(argv: &[String]) -> PathBuf {
    let recording_output = option_value(argv, "--dir")
        .or_else(|| option_value(argv, "--out"))
        .or_else(|| option_value(argv, "--dump-scene"))
        .map(PathBuf::from)
        .unwrap_or_else(|| standalone_output("record"));
    recording_output.join("profile")
}

/// Said wherever an asset needs a download and there is no Node helper to do it.
pub const HELPER_REQUIRED: &str =
    "needs the rur CLI from a checkout with Node.js; set RUR_ASSET_REPO to that checkout";

/// The Node helper that downloads assets, or the reason there is none. Checkout mode always uses
/// `repo`'s helper. Standalone mode looks in `RUR_ASSET_REPO`, else `repo`, and needs the file to
/// exist. Any folder holding `cli/native-assets.mjs` works, which lets the asset tests point
/// `repo` at a stand-in.
pub fn asset_helper(repo: &Path) -> Result<PathBuf, String> {
    if !is_standalone() {
        return Ok(repo.join("cli/native-assets.mjs"));
    }
    let repo = env_path("RUR_ASSET_REPO").unwrap_or_else(|| repo.to_path_buf());
    let helper = repo.join("cli/native-assets.mjs");
    if !helper.is_file() {
        return Err(HELPER_REQUIRED.into());
    }
    // Node resolves the links in a script's path but reports the command-line path as written, and
    // the helper only starts when the two name the same file. The real path also makes a relative
    // `RUR_ASSET_REPO` independent of the working directory.
    canonical_path(&helper).map_err(|error| error.to_string())
}

/// True when downloads are possible. A standalone build without the helper works on local content
/// only.
pub fn has_asset_helper(repo: &Path) -> bool {
    asset_helper(repo).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_open_uses_the_selected_project_only_in_standalone_mode() {
        let base = std::env::temp_dir();
        let repo = base.join("state");
        let project = base.join("project");
        let relative = Path::new("src/Example.story.luau");
        assert_eq!(
            relative_open_path(true, &repo, &project, relative),
            project.join(relative)
        );
        assert_eq!(
            relative_open_path(false, &repo, &project, relative),
            repo.join(relative)
        );
        let absolute = base.join("other/Example.story.luau");
        for standalone in [false, true] {
            assert_eq!(
                relative_open_path(standalone, &repo, &project, &absolute),
                absolute
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn canonical_paths_keep_network_shares_and_drop_local_verbatim_prefixes() {
        assert_eq!(
            without_verbatim_prefix(PathBuf::from(r"\\?\UNC\server\share\project")),
            PathBuf::from(r"\\server\share\project")
        );
        assert_eq!(
            without_verbatim_prefix(PathBuf::from(r"\\?\C:\project")),
            PathBuf::from(r"C:\project")
        );
    }

    #[test]
    fn standalone_cache_uses_only_redirected_user_locations() {
        let temp = std::env::temp_dir().join("rur-path-tests");
        let env = |key: &str| match key {
            "HOME" => Some(temp.join("home")),
            "LOCALAPPDATA" => Some(temp.join("local")),
            "XDG_CACHE_HOME" => Some(temp.join("xdg")),
            _ => None,
        };
        assert_eq!(
            user_cache_with("windows", env),
            Some(temp.join("local/rur"))
        );
        assert_eq!(
            user_cache_with("macos", env),
            Some(temp.join("home/Library/Caches/rur"))
        );
        assert_eq!(user_cache_with("linux", env), Some(temp.join("xdg/rur")));
        // Linux without XDG_CACHE_HOME falls back to ~/.cache
        assert_eq!(
            user_cache_with("linux", |key| (key == "HOME").then(|| temp.join("home"))),
            Some(temp.join("home/.cache/rur"))
        );
        assert_eq!(user_cache_with("linux", |_| None), None);
        // RUR_STATE_DIR wins over RUR_CACHE_DIR, and both win over the user cache
        assert_eq!(
            standalone_state_with("linux", |key| match key {
                "RUR_STATE_DIR" => Some(temp.join("state")),
                "RUR_CACHE_DIR" => Some(temp.join("alias")),
                "XDG_CACHE_HOME" => Some(temp.join("xdg")),
                _ => None,
            }),
            Some(temp.join("state"))
        );
        assert_eq!(
            standalone_state_with("linux", |key| (key == "RUR_CACHE_DIR")
                .then(|| temp.join("alias"))),
            Some(temp.join("alias"))
        );
    }

    #[test]
    fn mode_depends_only_on_the_compiled_release_marker() {
        assert_eq!(
            is_standalone(),
            option_env!("RUR_RELEASE_BUILD") == Some("1")
        );
    }

    #[test]
    fn repo_argument_reads_both_spellings() {
        let argv = |words: &[&str]| {
            words
                .iter()
                .map(|word| word.to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            repo_argument(&argv(&["rur-native", "--repo", "a"])),
            Some(PathBuf::from("a"))
        );
        assert_eq!(
            repo_argument(&argv(&["rur-native", "--repo=b", "--repo", "a"])),
            Some(PathBuf::from("b"))
        );
        assert_eq!(repo_argument(&argv(&["rur-native", "--repo"])), None);
        assert_eq!(repo_argument(&argv(&["rur-native", "render"])), None);
    }
}
