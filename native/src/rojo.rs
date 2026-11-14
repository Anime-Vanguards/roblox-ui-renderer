//! Rojo project files (`*.project.json`) for the native commands: `--rojo <file>`, `--project <file.project.json>`
//! and, with no project options at all, a `default.project.json` found beside the story ([`resolve_arguments`]).
//! The project becomes engine mounts (`--mount Service/Name=<path>`, `--instance Service/Name=<ClassName>`), by the
//! same rules as the Node CLI's cli/rojo.mjs (tools/tests/cli.mjs compares the two through `rur-native rojo-mounts`):
//!
//! - A DataModel tree (`"$className": "DataModel"`): each node with a `$path` mounts that directory or file at its
//!   instance path (ReplicatedStorage/Packages, StarterPlayer/StarterPlayerScripts/...); `$className` nodes without
//!   a `$path` below a service become instances of that class.
//! - Any other tree (a plugin or model project) is mounted at `ReplicatedStorage/<project name>`, or where
//!   `--rojo-at Service/Name` says.
//! - `$path` to another `*.project.json` nests that project's tree; `{ "optional": path }` is skipped when missing;
//!   `$properties`, `$attributes` and `$ignoreUnknownInstances` are ignored.
use std::path::{Component, Path, PathBuf};

use serde::de::{Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};

/// The file name a Rojo project is found by when no project is named
pub const DEFAULT_PROJECT_FILE: &str = "default.project.json";

/// A JSON value with its objects' keys in file order (serde_json's own map sorts them, and the mount order is the
/// order instances are created in, which `GetChildren` shows)
enum Node {
    Object(Vec<(String, Node)>),
    String(String),
    Other,
}

impl<'de> Deserialize<'de> for Node {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Node, D::Error> {
        struct NodeVisitor;
        impl<'de> Visitor<'de> for NodeVisitor {
            type Value = Node;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("JSON")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Node, A::Error> {
                let mut entries: Vec<(String, Node)> = Vec::new();
                while let Some((key, value)) = map.next_entry::<String, Node>()? {
                    // JSON.parse keeps a repeated key at its first place with the last value
                    match entries.iter_mut().find(|(name, _)| *name == key) {
                        Some(entry) => entry.1 = value,
                        None => entries.push((key, value)),
                    }
                }
                Ok(Node::Object(entries))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Node, A::Error> {
                while seq.next_element::<Node>()?.is_some() {}
                Ok(Node::Other)
            }
            fn visit_str<E>(self, value: &str) -> Result<Node, E> {
                Ok(Node::String(value.to_string()))
            }
            fn visit_string<E>(self, value: String) -> Result<Node, E> {
                Ok(Node::String(value))
            }
            fn visit_bool<E>(self, _: bool) -> Result<Node, E> {
                Ok(Node::Other)
            }
            fn visit_i64<E>(self, _: i64) -> Result<Node, E> {
                Ok(Node::Other)
            }
            fn visit_u64<E>(self, _: u64) -> Result<Node, E> {
                Ok(Node::Other)
            }
            fn visit_f64<E>(self, _: f64) -> Result<Node, E> {
                Ok(Node::Other)
            }
            fn visit_unit<E>(self) -> Result<Node, E> {
                Ok(Node::Other)
            }
        }
        deserializer.deserialize_any(NodeVisitor)
    }
}

impl Node {
    fn get(&self, key: &str) -> Option<&Node> {
        match self {
            Node::Object(entries) => entries.iter().find(|(name, _)| name == key).map(|(_, value)| value),
            _ => None,
        }
    }

    fn string(&self, key: &str) -> Option<&str> {
        match self.get(key) {
            Some(Node::String(value)) => Some(value),
            _ => None,
        }
    }

    /// The children a tree node names: its keys without `$`, with object values
    fn children(&self) -> impl Iterator<Item = (&String, &Node)> {
        let entries: &[(String, Node)] = match self {
            Node::Object(entries) => entries,
            _ => &[],
        };
        entries.iter().filter(|(name, value)| !name.starts_with('$') && matches!(value, Node::Object(_))).map(|(name, value)| (name, value))
    }
}

/// One mount of a project: `--mount path=value` (a directory or file) or `--instance path=value` (a class name)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mount {
    pub instance: bool,
    /// The instance path below the DataModel, `/`-separated, starting with a service
    pub path: String,
    pub value: String,
}

impl Mount {
    /// The engine arguments of this mount
    pub fn arguments(&self) -> [String; 2] {
        [if self.instance { "--instance" } else { "--mount" }.into(), format!("{}={}", self.path, self.value)]
    }
}

/// Whether `file` names a Rojo project (`*.project.json`)
pub fn is_project_file(file: &str) -> bool {
    file.to_ascii_lowercase().ends_with(".project.json")
}

/// `path` made absolute against the current directory with `.` and `..` removed, as Node's `path.resolve` does
fn resolve(base: &Path, path: &str) -> PathBuf {
    let joined = base.join(path);
    let joined = std::path::absolute(&joined).unwrap_or(joined);
    let mut out = PathBuf::new();
    for component in joined.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                // (never above the root, as path.resolve)
                if !matches!(out.components().next_back(), Some(Component::RootDir | Component::Prefix(_)) | None) {
                    out.pop();
                }
            }
            other => out.push(other),
        }
    }
    out
}

fn read_project(file: &Path) -> Result<Node, String> {
    let text = std::fs::read_to_string(file).map_err(|error| format!("{}: {error}", file.display()))?;
    serde_json::from_str::<Node>(&text).map_err(|error| format!("{}: {error}", file.display()))
}

/// A top-level name of a DataModel tree that can be a service (the engine checks the rest)
fn is_service(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_uppercase())
}

/// The mounts of a project file, in the order cli/rojo.mjs gives them. `at` is `--rojo-at Service/Name` (for a tree
/// that is not a DataModel). A non-optional `$path` that is missing is skipped and reported in `warnings`.
pub fn mounts(project_file: &Path, at: Option<&str>, warnings: &mut Vec<String>) -> Result<Vec<Mount>, String> {
    let file = resolve(Path::new(""), &project_file.to_string_lossy());
    let project = read_project(&file)?;
    let Some(tree @ Node::Object(_)) = project.get("tree") else {
        return Err(format!("{}: not a Rojo project (no \"tree\")", file.display()));
    };
    let file_name = file.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    let mut walk = Walk { file: &file, file_name, mounts: Vec::new(), warnings };
    let base = file.parent().map(Path::to_path_buf).unwrap_or_default();
    if tree.string("$className") == Some("DataModel") {
        for (name, child) in tree.children() {
            if !is_service(name) {
                return Err(format!("{}: {name} at the top of a DataModel tree is not a service", file.display()));
            }
            walk.visit(child, &mut vec![name.clone()], &base, 0)?;
        }
    } else {
        let mut place: Vec<String> = match at {
            Some(at) => at.split(['/', '\\']).filter(|part| !part.is_empty()).map(str::to_string).collect(),
            None => vec!["ReplicatedStorage".into(), project.string("name").filter(|name| !name.is_empty()).unwrap_or("Project").to_string()],
        };
        if place.len() < 2 {
            return Err(format!("--rojo-at {}: needs Service/Name", at.unwrap_or_default()));
        }
        walk.visit(tree, &mut place, &base, 0)?;
    }
    Ok(walk.mounts)
}

struct Walk<'a> {
    file: &'a Path,
    file_name: String,
    mounts: Vec<Mount>,
    warnings: &'a mut Vec<String>,
}

impl Walk<'_> {
    fn visit(&mut self, node: &Node, segments: &mut Vec<String>, base: &Path, depth: u32) -> Result<(), String> {
        if depth > 32 {
            return Err(format!("{}: project nesting too deep", self.file.display()));
        }
        let (path_value, optional) = match node.get("$path") {
            Some(Node::String(path)) => (Some(path.as_str()), false),
            Some(optional @ Node::Object(_)) => (optional.string("optional"), true),
            _ => (None, false),
        };
        if let Some(path_value) = path_value {
            let source = resolve(base, path_value);
            let source_text = source.to_string_lossy().into_owned();
            if !source.exists() {
                if !optional {
                    let at = if segments.is_empty() { "tree".to_string() } else { segments.join("/") };
                    self.warnings.push(format!("rur: {}: {at} $path {source_text} does not exist", self.file_name));
                }
            } else if is_project_file(&source_text) {
                if let Some(nested_tree @ Node::Object(_)) = read_project(&source)?.get("tree") {
                    let nested_base = source.parent().map(Path::to_path_buf).unwrap_or_default();
                    self.visit(nested_tree, segments, &nested_base, depth + 1)?;
                }
            } else if !segments.is_empty() {
                self.mounts.push(Mount { instance: false, path: segments.join("/"), value: source_text });
            }
        } else if let (Some(class), true) = (node.string("$className"), segments.len() > 1) {
            self.mounts.push(Mount { instance: true, path: segments.join("/"), value: class.to_string() });
        }
        for (name, child) in node.children() {
            segments.push(name.clone());
            let visited = self.visit(child, segments, base, depth);
            segments.pop();
            visited?;
        }
        Ok(())
    }
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

/// How a run's Rojo project was chosen
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Chosen {
    /// `--rojo <file>` or `--project <file.project.json>`
    Named,
    /// No project options: the `default.project.json` nearest the story (or in the current directory)
    Found,
}

/// What [`resolve_arguments`] did: the arguments for the engine, and the project it mounted
#[derive(Debug)]
pub struct Resolved {
    pub arguments: Vec<String>,
    pub project: Option<(PathBuf, Chosen, Vec<Mount>)>,
    /// `$path`s of the project that do not exist
    pub warnings: Vec<String>,
}

impl Resolved {
    /// The stderr line that says which project was mounted where (None without a project)
    pub fn note(&self) -> Option<String> {
        let (file, chosen, mounts) = self.project.as_ref()?;
        let how = match chosen {
            Chosen::Named => "",
            Chosen::Found => " (found beside the story; --rojo, --project or --roots picks another)",
        };
        let places: Vec<&str> = mounts.iter().map(|mount| mount.path.as_str()).collect();
        let listed = if places.is_empty() { "nothing".to_string() } else { places.join(", ") };
        Some(format!("rur: Rojo project {}{how}: mounted {listed}", file.display()))
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
/// `default.project.json` nearest the source (its folder or a parent of it), else the current directory's, unless the
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
        None => (detect(&rest), Chosen::Found),
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

/// The `default.project.json` a run without project options mounts ([`resolve_arguments`])
fn detect(argv: &[String]) -> Option<PathBuf> {
    if PROJECT_OPTIONS.iter().any(|key| argv.iter().any(|arg| arg == key)) {
        return None;
    }
    if std::env::var_os("RUR_PROJECT").is_some_and(|value| !value.is_empty()) {
        return None;
    }
    let source = option(argv, "--story").or_else(|| option(argv, "--module"))?;
    let source = resolve(Path::new(""), source);
    let source = crate::paths::canonical_path(&source).unwrap_or(source);
    let cwd = std::env::current_dir().ok()?;
    if crate::paths::is_standalone() {
        // A release build's project is the current directory: its rur.json places, when it declares them, are the run's
        if declares_places(&cwd) {
            return None;
        }
    } else {
        // A checkout build's default project mounts the stories inside it
        let repo = option(argv, "--repo").map(PathBuf::from).unwrap_or_else(crate::engine::default_repo);
        let project = crate::paths::default_project(&repo);
        let project = crate::paths::canonical_path(&project).unwrap_or(project);
        if source.starts_with(&project) {
            return None;
        }
    }
    source.ancestors().skip(1).chain([cwd.as_path()]).map(|dir| dir.join(DEFAULT_PROJECT_FILE)).find(|file| file.is_file())
}

/// Whether `dir`'s rur.json declares places
pub fn declares_places(dir: &Path) -> bool {
    std::fs::read_to_string(dir.join("rur.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .is_some_and(|config| config.get("places").is_some_and(|places| places.as_array().is_some_and(|places| !places.is_empty())))
}

/// A hint for a failure that reads a missing child of a service (`Packages is not a valid member of ReplicatedStorage
/// "ReplicatedStorage"`): what the run mounted, and how to mount the rest. `sources` lists the run's roots and mounts.
pub fn missing_child_hint(message: &str, sources: &[String]) -> Option<String> {
    const MARKER: &str = " is not a valid member of ";
    let at = message.find(MARKER)?;
    let child = message[..at].rsplit([':', ' ']).next().filter(|child| !child.is_empty())?;
    let after = &message[at + MARKER.len()..];
    let (class, rest) = after.split_once(' ')?;
    let full_name = rest.strip_prefix('"')?.split('"').next()?;
    // Only a child of a service: game.<Service>, whose full name is the service's own
    if full_name != class || !is_service(class) {
        return None;
    }
    let mounted = if sources.is_empty() {
        "nothing but the story".to_string()
    } else {
        const SHOWN: usize = 8;
        let mut listed = sources.iter().take(SHOWN).cloned().collect::<Vec<_>>().join("; ");
        if sources.len() > SHOWN {
            listed.push_str(&format!("; and {} more", sources.len() - SHOWN));
        }
        listed
    };
    Some(format!(
        "{class}.{child} was not mounted. The run mounted {mounted}. If {child} comes from a Rojo project (Wally packages, say), pass --rojo <default.project.json> or run beside it; else mount it with --mount {class}/{child}=<dir>"
    ))
}

/// The roots and mounts of a run as [`missing_child_hint`] lists them
pub fn describe_sources(roots: &[String], mounts: &[roblox_engine_emulator::project::Mount]) -> Vec<String> {
    use roblox_engine_emulator::project::MountTarget;
    let roots = roots.iter().map(|root| format!("root {root}"));
    let mounts = mounts.iter().map(|mount| match &mount.target {
        MountTarget::Path(path) => format!("{} = {path}", mount.segments.join("/")),
        MountTarget::Class(class) => format!("{} ({class})", mount.segments.join("/")),
    });
    roots.chain(mounts).collect()
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

    /// A DataModel tree mounts its `$path`s at their instance paths in file order, `$className` nodes without a path
    /// as instances, nested projects in place; missing paths are skipped (reported unless optional)
    #[test]
    fn a_datamodel_tree_becomes_mounts_in_file_order() {
        let dir = temp_dir("datamodel");
        write(&dir.join("src/shared/init.luau"), "return {}");
        write(&dir.join("Packages/Fusion.luau"), "return {}");
        write(&dir.join("nested/inner.project.json"), r#"{"name":"inner","tree":{"$path":"lib"}}"#);
        write(&dir.join("nested/lib/a.luau"), "return 1");
        write(
            &dir.join("default.project.json"),
            r#"{"name":"game","tree":{"$className":"DataModel",
                "ReplicatedStorage":{"$className":"ReplicatedStorage","Shared":{"$path":"src/shared"},"Packages":{"$path":"Packages"},
                    "Inner":{"$path":"nested/inner.project.json"},"Remotes":{"$className":"Folder"}},
                "ServerStorage":{"Gone":{"$path":"missing"},"Maybe":{"$path":{"optional":"missing-too"}}}}}"#,
        );
        let mut warnings = Vec::new();
        let mounts = mounts(&dir.join("default.project.json"), None, &mut warnings).unwrap();
        let listed: Vec<(bool, &str, String)> = mounts
            .iter()
            .map(|mount| (mount.instance, mount.path.as_str(), if mount.instance { mount.value.clone() } else { Path::new(&mount.value).strip_prefix(&dir).unwrap().to_string_lossy().replace('\\', "/") }))
            .collect();
        assert_eq!(
            listed,
            vec![
                (false, "ReplicatedStorage/Shared", "src/shared".into()),
                (false, "ReplicatedStorage/Packages", "Packages".into()),
                (false, "ReplicatedStorage/Inner", "nested/lib".into()),
                (true, "ReplicatedStorage/Remotes", "Folder".into()),
            ]
        );
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("ServerStorage/Gone $path") && warnings[0].contains("does not exist"), "{warnings:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A tree that is not a DataModel mounts at ReplicatedStorage/<name>, or at --rojo-at
    #[test]
    fn a_plugin_tree_mounts_under_its_name_or_rojo_at() {
        let dir = temp_dir("plugin");
        write(&dir.join("src/init.luau"), "return {}");
        write(&dir.join("plugin.project.json"), r#"{"name":"MyPlugin","tree":{"$path":"src"}}"#);
        let file = dir.join("plugin.project.json");
        let mut warnings = Vec::new();
        assert_eq!(mounts(&file, None, &mut warnings).unwrap()[0].path, "ReplicatedStorage/MyPlugin");
        assert_eq!(mounts(&file, Some("ServerStorage/Tools"), &mut warnings).unwrap()[0].path, "ServerStorage/Tools");
        assert!(mounts(&file, Some("ServerStorage"), &mut warnings).unwrap_err().contains("needs Service/Name"));
        write(&dir.join("bad.project.json"), r#"{"name":"x","tree":{"$className":"DataModel","lower":{}}}"#);
        assert!(mounts(&dir.join("bad.project.json"), None, &mut warnings).unwrap_err().contains("is not a service"));
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
            assert!(resolved.note().unwrap().contains("mounted ReplicatedStorage/Packages"));
        }
        assert!(resolve_arguments(&strings(&["--rojo-at", "ServerStorage/X"])).unwrap_err().contains("needs --rojo"));
        assert!(resolve_arguments(&strings(&["--rojo", &dir.join("nope.project.json").to_string_lossy()])).unwrap_err().contains("no such file"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A story with no project options finds the default.project.json above it; any project option turns that off
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
            assert_eq!((crate::paths::canonical_path(file).unwrap(), chosen), (crate::paths::canonical_path(&dir.join("default.project.json")).unwrap(), &Chosen::Found));
            assert!(resolved.arguments.iter().any(|arg| arg.starts_with("ReplicatedStorage/Packages=")));
            assert!(resolved.note().unwrap().contains("found beside the story"));
        }
        for option in [["--roots", ""], ["--mount", "Workspace/X=y"], ["--place", "client"]] {
            let resolved = resolve_arguments(&strings(&["--story", &story, option[0], option[1]])).unwrap();
            assert!(resolved.project.is_none(), "{option:?}");
        }
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

    #[test]
    fn a_missing_child_of_a_service_gets_a_hint() {
        let sources = vec!["root C:/game/src".to_string()];
        let hint = missing_child_hint(r#"Packages is not a valid member of ReplicatedStorage "ReplicatedStorage""#, &sources).unwrap();
        assert!(hint.starts_with("ReplicatedStorage.Packages was not mounted. The run mounted root C:/game/src."), "{hint}");
        assert!(hint.contains("--rojo") && hint.contains("--mount ReplicatedStorage/Packages=<dir>"));
        let traced = missing_child_hint(r#"ReplicatedStorage.Story:3: Packages is not a valid member of ReplicatedStorage "ReplicatedStorage""#, &[]).unwrap();
        assert!(traced.contains("mounted nothing but the story"), "{traced}");
        assert!(missing_child_hint(r#"Fusion is not a valid member of Folder "ReplicatedStorage.Packages""#, &sources).is_none());
        assert!(missing_child_hint("attempt to index nil", &sources).is_none());
    }
}
