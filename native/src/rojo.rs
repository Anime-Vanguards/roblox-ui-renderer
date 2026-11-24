//! Rojo project files (`*.project.json`) for the native commands: `--rojo <file>`, `--project <file.project.json>`
//! and, with no project options at all, a `default.project.json` found beside the story ([`resolve_arguments`]).
//! The project becomes engine mounts (`--mount Service/Name=<path>`, `--instance Service/Name=<ClassName>`).
//!
//! The reader is the engine's, `roblox_engine_emulator::rojo` (`ree run --rojo` reads projects with it too),
//! re-exported here: its rules are the Node CLI's cli/rojo.mjs, and tools/tests/cli.mjs compares the two through
//! `rur-native rojo-mounts`. This module adds rur's name to the reader's warnings ([`mounts`]), the
//! `rojo-mounts` command and the choice of a run's project ([`resolve_arguments`]).
use std::path::{Path, PathBuf};

pub(crate) use roblox_engine_emulator::rojo::resolve;
pub use roblox_engine_emulator::rojo::{
    DEFAULT_PROJECT_FILE, Mount, dotted, is_project_file, mappings, missing_path, mounted_list, project_file_in, project_files_in, several_projects, wally_advice,
};

/// The mounts of a project file, in the order cli/rojo.mjs gives them (the engine's reader). `at` is
/// `--rojo-at Service/Name` (for a tree that is not a DataModel). A non-optional `$path` that is missing is skipped
/// and reported in `warnings`, with `rur: ` in front.
pub fn mounts(project_file: &Path, at: Option<&str>, warnings: &mut Vec<String>) -> Result<Vec<Mount>, String> {
    mounts_with(project_file, at, warnings, &mut |path| Ok(path))
}

/// [`mounts`], each `$path` first handed to `source`, which can stand another file or folder in for it (`--git-ref`:
/// the path as it is at a commit, extracted there; gitref.rs)
pub fn mounts_with(
    project_file: &Path,
    at: Option<&str>,
    warnings: &mut Vec<String>,
    source: &mut dyn FnMut(PathBuf) -> Result<PathBuf, String>,
) -> Result<Vec<Mount>, String> {
    let mut missing = Vec::new();
    let mounts = roblox_engine_emulator::rojo::mounts_with(project_file, at, &mut missing, source)?;
    warnings.extend(missing.into_iter().map(|warning| format!("rur: {warning}")));
    Ok(mounts)
}

/// `rur-native rojo-mounts <file.project.json> [--rojo-at Service/Name]`: the project's mounts as a JSON array of
/// `{"kind": "mount" | "instance", "path", "value"}`, as cli/rojo.mjs rojoMounts gives them; returns the exit code
pub fn mounts_cli(argv: &[String]) -> i32 {
    let (mut file, mut at) = (None, None);
    let mut index = 0;
    while index < argv.len() {
        match argv[index].as_str() {
            "--rojo-at" => {
                at = argv.get(index + 1).cloned();
                index += 1;
            }
            other if file.is_none() && !other.starts_with('-') && !other.trim().is_empty() => file = Some(other.to_string()),
            other => {
                eprintln!("rur-native rojo-mounts: unexpected argument {other}");
                return 2;
            }
        }
        index += 1;
    }
    let Some(file) = file else {
        eprintln!("usage: rur-native rojo-mounts <file.project.json> [--rojo-at Service/Name]");
        return 2;
    };
    let mut warnings = Vec::new();
    match mounts(Path::new(&file), at.as_deref(), &mut warnings) {
        Ok(mounts) => {
            for warning in warnings {
                eprintln!("{warning}");
            }
            let listed: Vec<serde_json::Value> = mounts
                .iter()
                .map(|mount| serde_json::json!({ "kind": if mount.instance { "instance" } else { "mount" }, "path": mount.path, "value": mount.value }))
                .collect();
            println!("{}", serde_json::Value::Array(listed));
            0
        }
        Err(error) => {
            eprintln!("rur-native rojo-mounts: {error}");
            1
        }
    }
}

/// Whether a Rojo project's tree is a DataModel: its `"$className"`, or with none a root `$path` to a project file whose
/// tree is one (the reader's rule). Any other tree (a Wally package's or a model's own tree) mounts as one instance.
pub fn is_data_model(file: &Path) -> bool {
    let mut file = file.to_path_buf();
    // (a chain of root `$path`s as deep as the reader follows one)
    for _ in 0..32 {
        let project = std::fs::read_to_string(&file).ok().and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok());
        let Some(tree) = project.as_ref().and_then(|project| project.get("tree")) else { return false };
        match (tree.get("$className").and_then(serde_json::Value::as_str), tree.get("$path").and_then(serde_json::Value::as_str)) {
            (Some(class), _) => return class == "DataModel",
            (None, Some(path)) if is_project_file(path) => file = resolve(file.parent().unwrap_or(Path::new("")), path),
            _ => return false,
        }
    }
    false
}

/// Whether project `file` mounts `target` (a file or folder): one of its `$path`s that exists is `target` or holds it
fn mounts_path(file: &Path, target: &Path) -> bool {
    let real = |path: &Path| crate::paths::canonical_path(path).unwrap_or_else(|_| path.to_path_buf());
    let target = real(target);
    mappings(file).is_ok_and(|mappings| mappings.iter().any(|(_, source, exists)| *exists && target.starts_with(real(source))))
}

/// The DataModel projects beside `package`, a project whose tree is not a DataModel, that mount `source` (a story or
/// module file), or with no source every file and folder `package` maps. A Wally package's code finds its dependencies
/// as its siblings (`script:FindFirstAncestor(<name>).Parent` is the Packages folder of Wally's link modules), and only
/// such a DataModel dev project has them: the one people serve into Studio to run the package's stories.
pub fn data_models_beside(package: &Path, source: Option<&Path>) -> Vec<PathBuf> {
    let targets: Vec<PathBuf> = match source {
        Some(source) => vec![source.to_path_buf()],
        None => mappings(package).unwrap_or_default().into_iter().filter(|(_, _, exists)| *exists).map(|(_, path, _)| path).collect(),
    };
    if targets.is_empty() || is_data_model(package) {
        return Vec::new();
    }
    let dir = package.parent().unwrap_or(Path::new(""));
    project_files_in(dir)
        .into_iter()
        .filter(|file| file.file_name() != package.file_name() && is_data_model(file) && targets.iter().all(|target| mounts_path(file, target)))
        .collect()
}

/// What a run mounts, as the `rur: Rojo project` line and a hint name it: the source file, or the package
pub fn source_name(source: Option<&Path>) -> String {
    source.and_then(Path::file_name).map_or_else(|| "the package".to_string(), |name| name.to_string_lossy().into_owned())
}

/// The names of project files, as a line lists them
pub fn file_names(files: &[PathBuf]) -> String {
    files.iter().filter_map(|file| file.file_name()).map(|name| name.to_string_lossy().into_owned()).collect::<Vec<_>>().join(", ")
}

/// The project a run without project options mounts from folder `dir` ([`project_file_in`]) for `source` (the story or
/// module file it runs; None for the storybook, which runs the stories the project maps), and why when that needs
/// saying: a project whose tree is not a DataModel gives way to the one DataModel project beside it that mounts the
/// source ([`data_models_beside`]); with several such projects it stays, and the reason names them. `Err` lists the
/// folder's project files when there are several and none is default.project.json.
pub fn project_for_source(dir: &Path, source: Option<&Path>) -> Result<Option<(PathBuf, Option<String>)>, Vec<PathBuf>> {
    let Some(found) = project_file_in(dir)? else { return Ok(None) };
    let mut beside = data_models_beside(&found, source);
    let name = found.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    let what = source_name(source);
    Ok(Some(match beside.len() {
        0 => (found, None),
        1 => (beside.remove(0), Some(format!("{name} beside it is the package's own tree, not a DataModel, and this is the one DataModel project there that mounts {what}"))),
        _ => (found, Some(format!("this is the package's own tree, not a DataModel, and several DataModel projects beside it mount {what} ({})", file_names(&beside)))),
    }))
}

/// How a run's Rojo project was chosen
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Chosen {
    /// `--rojo <file>` or `--project <file.project.json>`
    Named,
    /// No project options: the project file nearest the story (or in the current directory), and why it was taken when
    /// that needs saying ([`project_for_source`])
    Found { why: Option<String> },
}

/// What [`resolve_arguments`] did: the arguments for the engine, and the project it mounted
#[derive(Debug)]
pub struct Resolved {
    pub arguments: Vec<String>,
    pub project: Option<(PathBuf, Chosen, Vec<Mount>)>,
    /// `$path`s of the project that do not exist, or the project files rur would not choose between
    pub warnings: Vec<String>,
}

impl Resolved {
    /// The stderr line that says which project was mounted where (None without a project)
    pub fn note(&self) -> Option<String> {
        let (file, chosen, mounts) = self.project.as_ref()?;
        let how = match chosen {
            Chosen::Named => String::new(),
            Chosen::Found { why } => format!(" (found beside the story; {}--rojo picks another)", why.as_ref().map(|why| format!("{why}; ")).unwrap_or_default()),
        };
        Some(format!("rur: Rojo project {}{how}: mounted {}", file.display(), mounted_list(mounts)))
    }
}

/// The value after `key`, split from `--key=value` too
fn option<'a>(argv: &'a [String], key: &str) -> Option<&'a String> {
    argv.iter().position(|arg| arg == key).and_then(|index| argv.get(index + 1))
}

/// The options that choose a run's project: with any of them nothing is detected
const PROJECT_OPTIONS: [&str; 6] = ["--roots", "--mount", "--instance", "--project", "--place", "--rojo-at"];

/// The command line of a native story command (`render`, `sequence`, `engine`, `record`) with its Rojo project as
/// mounts: `--rojo <file>` [`--rojo-at Service/Name`], or `--project <file.project.json>` (then `--project` names the
/// project's folder, for content paths relative to it). Without any project option (`--roots`, `--mount`,
/// `--instance`, `--project`, `--place`) and without RUR_PROJECT, a run of a `--story` or `--module` mounts the
/// project file nearest the source (its folder or a parent of it: [`project_file_in`]), else the current directory's, unless the
/// folder rur would take as the project already describes the run: a checkout build's default project holding the
/// source, or a `rur.json` that declares places in the current directory (a release build's project).
/// The project's mounts come before the `--mount`s of the command line, as in cli/rojo.mjs.
pub fn resolve_arguments(argv: &[String]) -> Result<Resolved, String> {
    // `--key=value` as two arguments, for the keys read here
    let argv: Vec<String> = argv
        .iter()
        .flat_map(|arg| match arg.split_once('=') {
            Some((key @ ("--rojo" | "--rojo-at" | "--project"), value)) => vec![key.to_string(), value.to_string()],
            _ => vec![arg.clone()],
        })
        .collect();
    let mut rest = Vec::with_capacity(argv.len());
    let mut rojo = None;
    let mut at = None;
    let mut project_dir = None;
    let mut index = 0;
    while index < argv.len() {
        let key = argv[index].as_str();
        let value = argv.get(index + 1);
        let need = || value.cloned().filter(|value| !value.is_empty()).ok_or_else(|| format!("{key} needs a value"));
        match key {
            "--rojo" => rojo = Some(need()?),
            "--rojo-at" => at = Some(need()?),
            "--project" if value.is_some_and(|value| is_project_file(value)) => {
                let file = need()?;
                project_dir = resolve(Path::new(""), &file).parent().map(Path::to_path_buf);
                rojo = Some(file);
            }
            _ => {
                rest.push(argv[index].clone());
                index += 1;
                continue;
            }
        }
        index += 2;
    }
    let (file, chosen) = match rojo {
        Some(file) => (Some(resolve(Path::new(""), &file)), Chosen::Named),
        None if at.is_some() => return Err("--rojo-at places a Rojo project: it needs --rojo <file.project.json>".into()),
        None => match detect(&rest) {
            Ok(Some((file, why))) => (Some(file), Chosen::Found { why }),
            Ok(None) => (None, Chosen::Found { why: None }),
            Err(several) => return Ok(Resolved { arguments: rest, project: None, warnings: vec![format!("rur: {}", several_projects(&several))] }),
        },
    };
    let Some(file) = file else {
        return Ok(Resolved { arguments: rest, project: None, warnings: Vec::new() });
    };
    if !file.is_file() {
        return Err(format!("Rojo project {}: no such file", file.display()));
    }
    let mut warnings = Vec::new();
    let mounts = mounts(&file, at.as_deref(), &mut warnings)?;
    let mut arguments: Vec<String> = mounts.iter().flat_map(Mount::arguments).collect();
    // No project roots beside the project's mounts (as the Node CLI passes them), even when it mounts nothing
    if option(&rest, "--roots").is_none() {
        arguments.extend(["--roots".into(), String::new()]);
    }
    // The project's folder for content paths relative to it, unless the command names one
    if option(&rest, "--project").is_none() {
        let dir = project_dir.or_else(|| file.parent().map(Path::to_path_buf));
        if let Some(dir) = dir {
            arguments.extend(["--project".into(), dir.to_string_lossy().into_owned()]);
        }
    }
    arguments.extend(rest);
    Ok(Resolved { arguments, project: Some((file, chosen, mounts)), warnings })
}

/// The project file a run without project options mounts ([`resolve_arguments`]), and why when that needs saying
/// ([`project_for_source`]); `Err` lists the project files of the nearest folder that has several and no
/// default.project.json (rur mounts none of them then)
fn detect(argv: &[String]) -> Result<Option<(PathBuf, Option<String>)>, Vec<PathBuf>> {
    if PROJECT_OPTIONS.iter().any(|key| argv.iter().any(|arg| arg == key)) {
        return Ok(None);
    }
    if std::env::var_os("RUR_PROJECT").is_some_and(|value| !value.is_empty()) {
        return Ok(None);
    }
    let Some(source) = option(argv, "--story").or_else(|| option(argv, "--module")) else { return Ok(None) };
    let source = resolve(Path::new(""), source);
    let source = crate::paths::canonical_path(&source).unwrap_or(source);
    let Ok(cwd) = std::env::current_dir() else { return Ok(None) };
    if crate::paths::is_standalone() {
        // A release build's project is the current directory: its rur.json places, when it declares them, are the run's
        if declares_places(&cwd) {
            return Ok(None);
        }
    } else {
        // A checkout build's default project mounts the stories inside it
        let repo = option(argv, "--repo").map(PathBuf::from).unwrap_or_else(crate::engine::default_repo);
        let project = crate::paths::default_project(&repo);
        let project = crate::paths::canonical_path(&project).unwrap_or(project);
        if source.starts_with(&project) {
            return Ok(None);
        }
    }
    for dir in source.ancestors().skip(1).chain([cwd.as_path()]) {
        if let Some(found) = project_for_source(dir, Some(&source))? {
            return Ok(Some(found));
        }
    }
    Ok(None)
}

/// Whether `dir`'s rur.json declares places
pub fn declares_places(dir: &Path) -> bool {
    std::fs::read_to_string(dir.join("rur.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .is_some_and(|config| config.get("places").is_some_and(|places| places.as_array().is_some_and(|places| !places.is_empty())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rur-rojo-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// The engine's reader warns about a missing `$path` without a name; rur's commands put theirs in front
    #[test]
    fn missing_paths_are_reported_with_rurs_name() {
        let dir = temp_dir("missing");
        write(&dir.join("game.project.json"), r#"{"name":"g","tree":{"$className":"DataModel","ReplicatedStorage":{"Packages":{"$path":"Packages"}}}}"#);
        let file = dir.join("game.project.json");
        let mut warnings = Vec::new();
        assert!(mounts(&file, None, &mut warnings).unwrap().is_empty());
        mounts_with(&file, None, &mut warnings, &mut |path| Ok(path)).unwrap();
        assert_eq!(warnings, ["rur: game.project.json maps ReplicatedStorage.Packages to Packages, which doesn't exist"; 2]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `--rojo`, `--rojo=<file>` and `--project <file.project.json>` become mounts ahead of the command's own, with the
    /// project's folder as `--project`; `--rojo-at` alone is refused
    #[test]
    fn rojo_options_become_engine_mounts() {
        let dir = temp_dir("arguments");
        write(&dir.join("Packages/Fusion.luau"), "return {}");
        write(&dir.join("default.project.json"), r#"{"name":"g","tree":{"$className":"DataModel","ReplicatedStorage":{"Packages":{"$path":"Packages"}}}}"#);
        let file = dir.join("default.project.json").to_string_lossy().into_owned();
        let strings = |args: &[&str]| args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>();
        for argv in [strings(&["--rojo", &file, "--mount", "Workspace/X=y"]), strings(&[&format!("--rojo={file}"), "--mount", "Workspace/X=y"]), strings(&["--project", &file, "--mount", "Workspace/X=y"])] {
            let resolved = resolve_arguments(&argv).unwrap();
            let arguments = &resolved.arguments;
            assert_eq!(arguments[0], "--mount");
            assert!(arguments[1].starts_with("ReplicatedStorage/Packages="), "{arguments:?}");
            assert_eq!((arguments[2].as_str(), arguments[3].as_str()), ("--roots", ""));
            assert_eq!((arguments[4].as_str(), arguments[5].as_str()), ("--project", dir.to_string_lossy().as_ref()));
            assert_eq!(&arguments[6..], &strings(&["--mount", "Workspace/X=y"])[..]);
            assert!(resolved.note().unwrap().ends_with(": mounted ReplicatedStorage.Packages"), "{:?}", resolved.note());
        }
        assert!(resolve_arguments(&strings(&["--rojo-at", "ServerStorage/X"])).unwrap_err().contains("needs --rojo"));
        assert!(resolve_arguments(&strings(&["--rojo", &dir.join("nope.project.json").to_string_lossy()])).unwrap_err().contains("no such file"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A story with no project options finds the project file above it; any project option turns that off
    #[test]
    fn a_story_beside_a_rojo_project_mounts_it() {
        let dir = temp_dir("detect");
        write(&dir.join("Packages/Fusion.luau"), "return {}");
        write(&dir.join("src/Stories/A.story.luau"), "return {}");
        write(&dir.join("default.project.json"), r#"{"name":"g","tree":{"$className":"DataModel","ReplicatedStorage":{"Packages":{"$path":"Packages"}}}}"#);
        let story = dir.join("src/Stories/A.story.luau").to_string_lossy().into_owned();
        let strings = |args: &[&str]| args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>();
        if std::env::var_os("RUR_PROJECT").is_none() {
            let resolved = resolve_arguments(&strings(&["--story", &story])).unwrap();
            let (file, chosen, _) = resolved.project.as_ref().unwrap();
            assert_eq!((crate::paths::canonical_path(file).unwrap(), chosen), (crate::paths::canonical_path(&dir.join("default.project.json")).unwrap(), &Chosen::Found { why: None }));
            assert!(resolved.arguments.iter().any(|arg| arg.starts_with("ReplicatedStorage/Packages=")));
            assert!(resolved.note().unwrap().contains("found beside the story"));
        }
        for option in [["--roots", ""], ["--mount", "Workspace/X=y"], ["--place", "client"]] {
            let resolved = resolve_arguments(&strings(&["--story", &story, option[0], option[1]])).unwrap();
            assert!(resolved.project.is_none(), "{option:?}");
        }
        if std::env::var_os("RUR_PROJECT").is_none() {
            // A lone place.project.json is found too; with two, neither is mounted and the note names them
            std::fs::rename(dir.join("default.project.json"), dir.join("place.project.json")).unwrap();
            let resolved = resolve_arguments(&strings(&["--story", &story])).unwrap();
            assert!(resolved.project.as_ref().unwrap().0.ends_with("place.project.json"));
            std::fs::copy(dir.join("place.project.json"), dir.join("dev.project.json")).unwrap();
            let resolved = resolve_arguments(&strings(&["--story", &story])).unwrap();
            assert!(resolved.project.is_none());
            assert!(resolved.warnings[0].contains("has several Rojo projects (dev.project.json, place.project.json), so none was mounted"), "{:?}", resolved.warnings);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A Wally package: default.project.json is the package's own tree, and the DataModel dev project beside it (the
    /// Packages folder with the package inside) is what its stories run in. Detection takes the one DataModel project
    /// that mounts the story and says why; with two it keeps default.project.json and names them; one that doesn't
    /// mount the story, or a default.project.json that is a DataModel, changes nothing.
    #[test]
    fn a_package_tree_gives_way_to_the_data_model_project_beside_it() {
        if std::env::var_os("RUR_PROJECT").is_some() {
            return;
        }
        let dir = temp_dir("package");
        write(&dir.join("default.project.json"), r#"{"name":"Kit","tree":{"$path":"src"}}"#);
        write(&dir.join("dev.project.json"), r#"{"name":"dev","tree":{"$className":"DataModel","ReplicatedStorage":{"Packages":{"$path":"Packages","Kit":{"$path":"src"}}}}}"#);
        write(&dir.join("Packages/Signal.lua"), "return {}");
        write(&dir.join("src/init.lua"), "return {}");
        write(&dir.join("src/Components/Thing.story.lua"), "return {}");
        let story = dir.join("src/Components/Thing.story.lua").to_string_lossy().into_owned();
        let strings = |args: &[&str]| args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>();
        let resolved = resolve_arguments(&strings(&["--story", &story])).unwrap();
        assert!(resolved.project.as_ref().unwrap().0.ends_with("dev.project.json"), "{:?}", resolved.project);
        assert!(resolved.arguments.iter().any(|arg| arg.starts_with("ReplicatedStorage/Packages/Kit=")), "{:?}", resolved.arguments);
        let note = resolved.note().unwrap();
        assert!(
            note.contains("(found beside the story; default.project.json beside it is the package's own tree, not a DataModel, and this is the one DataModel project there that mounts Thing.story.lua; --rojo picks another): mounted ReplicatedStorage.Packages, ReplicatedStorage.Packages.Kit"),
            "{note}"
        );
        // The storybook (no story): the DataModel project mounts everything the package maps
        assert_eq!(project_for_source(&dir, None).unwrap().unwrap().0, dir.join("dev.project.json"));
        // A second DataModel project that mounts the story: default.project.json stays, and the line names both
        write(&dir.join("play.project.json"), r#"{"name":"play","tree":{"$className":"DataModel","ReplicatedStorage":{"Packages":{"$path":"Packages","Kit":{"$path":"src"}}}}}"#);
        let resolved = resolve_arguments(&strings(&["--story", &story])).unwrap();
        assert!(resolved.project.as_ref().unwrap().0.ends_with("default.project.json"));
        let note = resolved.note().unwrap();
        assert!(note.contains("several DataModel projects beside it mount Thing.story.lua (dev.project.json, play.project.json); --rojo picks another)"), "{note}");
        // A DataModel project that maps only the Packages folder doesn't mount the story
        write(&dir.join("play.project.json"), r#"{"name":"play","tree":{"$className":"DataModel","ReplicatedStorage":{"Packages":{"$path":"Packages"}}}}"#);
        assert!(resolve_arguments(&strings(&["--story", &story])).unwrap().project.unwrap().0.ends_with("dev.project.json"));
        // A default.project.json that is a DataModel (through a root $path to one) is the run's, as before
        write(&dir.join("default.project.json"), r#"{"name":"Kit","tree":{"$path":"dev.project.json"}}"#);
        assert!(is_data_model(&dir.join("default.project.json")) && !is_data_model(&dir.join("missing.project.json")));
        let resolved = resolve_arguments(&strings(&["--story", &story])).unwrap();
        assert!(resolved.project.as_ref().unwrap().0.ends_with("default.project.json"));
        assert!(resolved.note().unwrap().contains("(found beside the story; --rojo picks another)"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The commands people type refuse what the engine parser does not know, where it used to skip it
    #[test]
    fn typed_commands_refuse_unknown_arguments() {
        let strings = |args: &[&str]| args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>();
        let parse = |args: &[&str]| crate::engine::cli::parse_command(&strings(args));
        assert!(parse(&["engine", "--story", "a.luau", "--viewport", "800x450"]).is_ok());
        assert_eq!(parse(&["--rojoo", "x.project.json", "--story", "a.luau"]).unwrap_err(), "unknown option --rojoo");
        assert!(parse(&["--story", "a.luau", "b.luau"]).unwrap_err().contains("unexpected argument \"b.luau\""));
        // (the requests the Node CLI builds still parse leniently)
        assert_eq!(crate::engine::cli::parse(&strings(&["--rojoo", "x"])).unwrap().unknown.as_deref(), Some("--rojoo"));
    }
}
