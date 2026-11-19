//! Advice for a story that reads an instance nothing mounted (`Packages is not a valid member of ReplicatedStorage
//! "ReplicatedStorage"`) or whose mount still waits when the run ends: what the Rojo project files and folders on disk
//! say about it, against what the run mounted. A hint names the cause rur checked and the change that fixes it; when
//! rur finds no cause there is no hint.
//!
//! The engine puts the hint in a scene's `diagnostics.hint` (`rur-native render` prints it), and the storybook shows
//! it under a failed or waiting story.
use std::path::{Path, PathBuf};

use crate::rojo;

/// What a run mounted
#[derive(Clone, Debug, Default)]
pub struct Sources {
    /// `--roots`: folders laid out by service
    pub roots: Vec<PathBuf>,
    /// `--mount Service/Name=<path>`: (instance path `Service/Name`, file or folder)
    pub mounts: Vec<(String, PathBuf)>,
}

impl Sources {
    /// From the engine's `--roots` and `--mount` / `--instance` (instances hold no files)
    pub fn new(roots: &[String], mounts: &[roblox_engine_emulator::project::Mount]) -> Sources {
        use roblox_engine_emulator::project::MountTarget;
        Sources {
            roots: roots.iter().filter(|root| !root.is_empty()).map(PathBuf::from).collect(),
            mounts: mounts
                .iter()
                .filter_map(|mount| match &mount.target {
                    MountTarget::Path(path) => Some((mount.segments.join("/"), PathBuf::from(path))),
                    MountTarget::Class(_) => None,
                })
                .collect(),
        }
    }

    fn mounted(&self, at: &str, source: &Path) -> bool {
        self.mounts.iter().any(|(path, mounted)| path == at && same_path(mounted, source))
    }

    /// Whether the run mounted `file`'s project: every `$path` of it that exists, and at least one
    fn used(&self, file: &Path) -> bool {
        let Ok(mappings) = rojo::mappings(file) else { return false };
        let present: Vec<_> = mappings.iter().filter(|(_, _, exists)| *exists).collect();
        !present.is_empty() && present.iter().all(|(at, source, _)| self.mounted(at, source))
    }

    /// The folder on disk an instance path is, through a mount or a root
    fn folder_of(&self, path: &[String]) -> Option<PathBuf> {
        let through_mounts = self.mounts.iter().filter_map(|(at, source)| {
            let at: Vec<&str> = at.split('/').collect();
            if path.len() < at.len() || !path.iter().zip(&at).all(|(name, mounted)| name == mounted) {
                return None;
            }
            Some((at.len(), path[at.len()..].iter().fold(source.clone(), |dir, name| dir.join(name))))
        });
        let through_roots = self.roots.iter().map(|root| (0, path.iter().fold(root.clone(), |dir, name| dir.join(name))));
        through_mounts.chain(through_roots).filter(|(_, dir)| dir.is_dir()).max_by_key(|(depth, _)| *depth).map(|(_, dir)| dir)
    }

    /// What the run mounted, as a hint lists it
    fn listed(&self) -> String {
        const SHOWN: usize = 6;
        let items: Vec<String> = self
            .roots
            .iter()
            .map(|root| format!("the folder {}", root.display()))
            .chain(self.mounts.iter().map(|(at, source)| format!("{} = {}", rojo::dotted(at), source.display())))
            .collect();
        match items.len() {
            0 => "nothing but the story".into(),
            count if count > SHOWN => format!("{}; and {} more", items[..SHOWN].join("; "), count - SHOWN),
            _ => items.join("; "),
        }
    }
}

fn same_path(a: &Path, b: &Path) -> bool {
    let absolute = |path: &Path| std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    a == b || absolute(a) == absolute(b)
}

/// The services a Rojo project or a root mounts into (a full name starting with one is a path in the DataModel)
const SERVICES: [&str; 16] = [
    "Workspace", "ReplicatedStorage", "ReplicatedFirst", "ServerScriptService", "ServerStorage", "StarterGui", "StarterPack",
    "StarterPlayer", "Lighting", "SoundService", "Players", "Teams", "Chat", "TextChatService", "LocalizationService", "MaterialService",
];

/// The instance a message says is missing: its parent's path and its name
fn missing_member(message: &str) -> Option<(Vec<String>, String)> {
    const MARKER: &str = " is not a valid member of ";
    let at = message.find(MARKER)?;
    // (the name is the last word before the marker: after a chunk's `:12: `, or a line React's error boundary adds)
    let child = message[..at].rsplit(|c: char| c == ':' || c.is_whitespace()).next().filter(|child| !child.is_empty())?;
    let (_class, rest) = message[at + MARKER.len()..].split_once(' ')?;
    let full_name = rest.strip_prefix('"')?.split('"').next()?;
    let parent: Vec<String> = full_name.split('.').map(str::to_string).collect();
    SERVICES.contains(&parent[0].as_str()).then(|| (parent, child.to_string()))
}

/// The project files in the story's folder and above it, and above each folder the run mounted: nearest first
fn nearby_project_files(sources: &Sources, story: Option<&Path>) -> Vec<PathBuf> {
    let anchors = story
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .into_iter()
        .chain(sources.roots.iter().cloned())
        .chain(sources.mounts.iter().filter_map(|(_, source)| source.parent().map(Path::to_path_buf)));
    let mut dirs: Vec<PathBuf> = Vec::new();
    for anchor in anchors {
        for dir in anchor.ancestors() {
            if !dirs.iter().any(|seen| seen == dir) {
                dirs.push(dir.to_path_buf());
            }
        }
    }
    dirs.iter().flat_map(|dir| rojo::project_files_in(dir)).collect()
}

/// Why the run did not mount `file`, when rur can tell
fn why_unused(file: &Path, sources: &Sources, projects: &[PathBuf]) -> String {
    let dir = file.parent().unwrap_or(Path::new(""));
    if let Some(other) = projects.iter().find(|other| other.as_path() != file && sources.used(other)) {
        return format!("this run used {}", other.display());
    }
    if rojo::declares_places(dir) {
        return "this run used the places in rur.json beside it".into();
    }
    if let Err(several) = rojo::project_file_in(dir) {
        return format!("rur picks none of the {} project files there by itself", several.len());
    }
    "this run didn't mount it".into()
}

fn file_name(file: &Path) -> String {
    file.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default()
}

/// The hint for a failure that reads an instance nothing mounted, from the story at `story`
pub fn missing_child_hint(message: &str, sources: &Sources, story: Option<&Path>) -> Option<String> {
    let (parent, child) = missing_member(message)?;
    let wanted = format!("{}/{child}", parent.join("/"));
    let projects = nearby_project_files(sources, story);
    // A project file maps the instance: to a path that doesn't exist, or the run didn't use the file
    let maps = |file: &PathBuf| rojo::mappings(file).ok().and_then(|mappings| mappings.into_iter().find(|(at, _, _)| *at == wanted));
    for file in &projects {
        let Some((_, source, exists)) = maps(file) else { continue };
        if !exists {
            return Some(rojo::missing_path(file, &wanted, &source));
        }
        if sources.mounted(&wanted, &source) {
            continue;
        }
        // Several project files beside each other that map it, and the run used none of them
        let dir = file.parent().unwrap_or(Path::new(""));
        let beside = projects.iter().filter(|other| other.parent() == Some(dir) && maps(other).is_some()).count();
        if let (true, Err(several)) = (beside > 1, rojo::project_file_in(dir)) {
            return Some(rojo::several_projects(&several));
        }
        return Some(format!("{} maps {}, but {}: pass --rojo {}", file_name(file), rojo::dotted(&wanted), why_unused(file, sources, &projects), file.display()));
    }
    // Inside a folder the run mounted: that folder has no file or folder of the name
    if let Some(dir) = sources.folder_of(&parent) {
        if has_entry(&dir, &child) {
            return None;
        }
        let wally = wally_lists(&dir, &child).map(|advice| format!(": {advice}")).unwrap_or_default();
        return Some(format!("{} is the folder {}, which has no {child}{wally}", parent.join("."), dir.display()));
    }
    // A child of a service nothing maps
    let [service] = parent.as_slice() else { return None };
    let nearest = projects.first();
    let base = nearest.and_then(|file| file.parent()).or_else(|| story.and_then(Path::parent));
    let folder = base.map(|dir| dir.join(&child)).filter(|folder| folder.exists());
    Some(match (nearest, folder) {
        (Some(file), Some(folder)) => format!(
            "{} has no {service}.{child}, but the folder {} exists: add \"{child}\": {{ \"$path\": \"{}\" }} under \"{service}\" in its tree",
            file_name(file),
            folder.display(),
            folder.strip_prefix(file.parent().unwrap_or(Path::new(""))).unwrap_or(&folder).display().to_string().replace('\\', "/")
        ),
        (None, Some(folder)) => format!("No Rojo project file maps {service}.{child}, but the folder {} exists: pass --mount {service}/{child}={}", folder.display(), folder.display()),
        (Some(file), None) => {
            let under: Vec<String> = rojo::mappings(file)
                .unwrap_or_default()
                .into_iter()
                .filter_map(|(at, _, _)| at.strip_prefix(&format!("{service}/")).map(|rest| rest.split('/').next().unwrap_or(rest).to_string()))
                .fold(Vec::new(), |mut names, name| {
                    if !names.contains(&name) {
                        names.push(name);
                    }
                    names
                });
            if under.is_empty() {
                format!("{} maps nothing under {service}", file.display())
            } else {
                format!("{} has no {service}.{child}; under {service} it maps {}", file.display(), under.join(", "))
            }
        }
        (None, None) => format!("No Rojo project file is beside the story or above it, and this run mounted {}", sources.listed()),
    })
}

/// The hint for a story whose mount still waits when the run ends (a WaitForChild on something nothing mounted, say):
/// the nearest project file's `$path`s that don't exist, or that the run didn't mount
pub fn waiting_hint(sources: &Sources, story: Option<&Path>) -> Option<String> {
    let projects = nearby_project_files(sources, story);
    let dir = projects.first()?.parent()?;
    let here: Vec<&PathBuf> = projects.iter().filter(|file| file.parent() == Some(dir)).collect();
    let nearest = match here.iter().find(|file| sources.used(file)) {
        Some(used) => *used,
        None if here.len() > 1 => return Some(rojo::several_projects(&here.into_iter().cloned().collect::<Vec<_>>())),
        None => here[0],
    };
    let mappings = rojo::mappings(nearest).ok()?;
    if let Some((at, source, _)) = mappings.iter().find(|(_, _, exists)| !exists) {
        return Some(rojo::missing_path(nearest, at, source));
    }
    let unmounted: Vec<String> = mappings.iter().filter(|(at, source, _)| !sources.mounted(at, source)).map(|(at, _, _)| rojo::dotted(at)).collect();
    if unmounted.is_empty() {
        return None;
    }
    const SHOWN: usize = 4;
    let listed = if unmounted.len() > SHOWN { format!("{} and {} more", unmounted[..SHOWN].join(", "), unmounted.len() - SHOWN) } else { unmounted.join(", ") };
    Some(format!("{} maps {listed}, but {}: pass --rojo {}", file_name(nearest), why_unused(nearest, sources, &projects), nearest.display()))
}

/// A traceback frame (`<chunk>:<line> [function <name>]`) as a person finds it: the story's own chunk as its file name
/// (`ReplicatedStorage.RURFixtures.Button.story:5` is `Button.story.luau:5`), other chunks as they are
pub fn story_frame(frame: &str, story: Option<&Path>) -> String {
    let location = frame.split(' ').next().unwrap_or(frame);
    let name = story.and_then(Path::file_name).map(|name| name.to_string_lossy().into_owned());
    if let (Some(name), Some((chunk, line))) = (name, location.rsplit_once(':')) {
        let stem = name.strip_suffix(".luau").or_else(|| name.strip_suffix(".lua")).unwrap_or(&name);
        if chunk == stem || chunk.ends_with(&format!(".{stem}")) {
            return format!("{name}:{line}");
        }
    }
    location.to_string()
}

/// Whether `dir` has a file or folder that mounts as an instance named `name` (`name`, `name.luau`, `name.rbxm`...)
fn has_entry(dir: &Path, name: &str) -> bool {
    let dotted = format!("{name}.");
    std::fs::read_dir(dir).into_iter().flatten().flatten().any(|entry| {
        let entry = entry.file_name().to_string_lossy().into_owned();
        entry == name || entry.starts_with(&dotted)
    })
}

/// `wally install` when `dir` is a Wally package folder whose wally.toml lists `name`
fn wally_lists(dir: &Path, name: &str) -> Option<String> {
    let project = dir.parent()?;
    let manifest = std::fs::read_to_string(project.join("wally.toml")).ok()?;
    let listed = manifest.lines().any(|line| line.split_once('=').is_some_and(|(key, _)| key.trim() == name));
    (listed && rojo::wally_advice(project, dir).is_some()).then(|| format!("wally.toml lists {name}; run `wally install` in {}", project.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rur-hints-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    const PROJECT: &str = r#"{"name":"g","tree":{"$className":"DataModel","ReplicatedStorage":{"Packages":{"$path":"Packages"},"Shared":{"$path":"src/shared"}}}}"#;
    const MISSING: &str = r#"Packages is not a valid member of ReplicatedStorage "ReplicatedStorage""#;

    /// A game folder: a Rojo project mapping Packages and Shared, a wally.toml, a story under src/shared
    fn game(name: &str, project_file: &str) -> (PathBuf, PathBuf) {
        let dir = temp_dir(name);
        write(&dir.join(project_file), PROJECT);
        write(&dir.join("wally.toml"), "[package]\nname = \"me/game\"\n\n[dependencies]\nFusion = \"elttob/fusion@0.3.0\"\n");
        write(&dir.join("Packages/Fusion.lua"), "return {}");
        let story = dir.join("src/shared/Button.story.luau");
        write(&story, "return {}");
        (dir, story)
    }

    fn mounted(dir: &Path, project_file: &str) -> Sources {
        let mounts = rojo::mounts(&dir.join(project_file), None, &mut Vec::new()).unwrap();
        Sources { roots: Vec::new(), mounts: mounts.into_iter().map(|mount| (mount.path, PathBuf::from(mount.value))).collect() }
    }

    #[test]
    fn a_wally_folder_that_was_never_installed_says_to_run_wally_install() {
        let (dir, story) = game("wally", "default.project.json");
        std::fs::remove_dir_all(dir.join("Packages")).unwrap();
        let sources = mounted(&dir, "default.project.json");
        let hint = missing_child_hint(MISSING, &sources, Some(&story)).unwrap();
        assert_eq!(hint, format!("default.project.json maps ReplicatedStorage.Packages to Packages, which doesn't exist: run `wally install` in {}", dir.display()));
        assert_eq!(waiting_hint(&sources, Some(&story)).unwrap(), hint);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_project_file_the_run_did_not_use_is_named_with_the_reason() {
        let (dir, story) = game("unused", "default.project.json");
        let roots = Sources { roots: vec![dir.clone()], mounts: Vec::new() };
        let hint = missing_child_hint(MISSING, &roots, Some(&story)).unwrap();
        assert_eq!(hint, format!("default.project.json maps ReplicatedStorage.Packages, but this run didn't mount it: pass --rojo {}", dir.join("default.project.json").display()));
        write(&dir.join("rur.json"), r#"{"places":[{"name":"client","roots":["src"]}]}"#);
        assert!(missing_child_hint(MISSING, &roots, Some(&story)).unwrap().contains("but this run used the places in rur.json beside it: pass --rojo"));
        let waiting = waiting_hint(&roots, Some(&story)).unwrap();
        assert!(waiting.starts_with("default.project.json maps ReplicatedStorage.Packages, ReplicatedStorage.Shared, but this run used the places"), "{waiting}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn several_project_files_name_the_ones_rur_would_not_choose_between() {
        let (dir, story) = game("several", "place.project.json");
        write(&dir.join("dev.project.json"), PROJECT);
        let none = Sources::default();
        let hint = missing_child_hint(MISSING, &none, Some(&story)).unwrap();
        assert_eq!(hint, format!("{} has several Rojo projects (dev.project.json, place.project.json), so none was mounted: pass --rojo <file> to mount one", dir.display()));
        assert_eq!(waiting_hint(&none, Some(&story)).unwrap(), hint);
        // Only one of them maps it: that one, and why the run didn't use it
        write(&dir.join("dev.project.json"), r#"{"name":"g","tree":{"$className":"DataModel","ReplicatedStorage":{"Shared":{"$path":"src/shared"}}}}"#);
        let hint = missing_child_hint(MISSING, &none, Some(&story)).unwrap();
        assert!(hint.starts_with("place.project.json maps ReplicatedStorage.Packages, but rur picks none of the 2 project files there by itself: pass --rojo"), "{hint}");
        // The one the run used is named instead
        let used = mounted(&dir, "place.project.json");
        assert!(waiting_hint(&used, Some(&story)).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_child_inside_a_mounted_folder_names_that_folder() {
        let (dir, story) = game("inside", "default.project.json");
        let sources = mounted(&dir, "default.project.json");
        let packages = dir.join("Packages");
        let hint = missing_child_hint(r#"Fusion is not a valid member of Folder "ReplicatedStorage.Packages""#, &sources, Some(&story));
        assert!(hint.is_none(), "Fusion.lua is there: {hint:?}");
        let hint = missing_child_hint(r#"Signal is not a valid member of Folder "ReplicatedStorage.Packages""#, &sources, Some(&story)).unwrap();
        assert_eq!(hint, format!("ReplicatedStorage.Packages is the folder {}, which has no Signal", packages.display()));
        std::fs::remove_file(packages.join("Fusion.lua")).unwrap();
        let hint = missing_child_hint(r#"Fusion is not a valid member of Folder "ReplicatedStorage.Packages""#, &sources, Some(&story)).unwrap();
        assert!(hint.ends_with(&format!("which has no Fusion: wally.toml lists Fusion; run `wally install` in {}", dir.display())), "{hint}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_service_child_no_project_maps_says_what_the_project_has() {
        let (dir, story) = game("unmapped", "default.project.json");
        write(&dir.join("default.project.json"), r#"{"name":"g","tree":{"$className":"DataModel","ReplicatedStorage":{"Shared":{"$path":"src/shared"}}}}"#);
        let sources = mounted(&dir, "default.project.json");
        let hint = missing_child_hint(MISSING, &sources, Some(&story)).unwrap();
        assert_eq!(hint, format!("default.project.json has no ReplicatedStorage.Packages, but the folder {} exists: add \"Packages\": {{ \"$path\": \"Packages\" }} under \"ReplicatedStorage\" in its tree", dir.join("Packages").display()));
        std::fs::remove_dir_all(dir.join("Packages")).unwrap();
        let hint = missing_child_hint(MISSING, &sources, Some(&story)).unwrap();
        assert_eq!(hint, format!("{} has no ReplicatedStorage.Packages; under ReplicatedStorage it maps Shared", dir.join("default.project.json").display()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn without_a_project_file_the_hint_lists_what_the_run_mounted() {
        let dir = temp_dir("bare");
        let story = dir.join("Button.story.luau");
        write(&story, "return {}");
        let hint = missing_child_hint(MISSING, &Sources { roots: vec![dir.clone()], mounts: Vec::new() }, Some(&story)).unwrap();
        assert_eq!(hint, format!("No Rojo project file is beside the story or above it, and this run mounted the folder {}", dir.display()));
        write(&dir.join("Packages/Fusion.lua"), "return {}");
        let hint = missing_child_hint(MISSING, &Sources::default(), Some(&story)).unwrap();
        assert!(hint.starts_with("No Rojo project file maps ReplicatedStorage.Packages, but the folder") && hint.contains("pass --mount ReplicatedStorage/Packages="), "{hint}");
        assert_eq!(story_frame("ReplicatedStorage.RURFixtures.Button.story:5", Some(&story)), "Button.story.luau:5");
        assert_eq!(story_frame("ReplicatedStorage.Shared.Theme:3 function Get", Some(&story)), "ReplicatedStorage.Shared.Theme:3");
        // The name after a line of its own (React's "------ Error caught by React ------")
        let caught = "------ Error caught by React ------\nPackages is not a valid member of ReplicatedStorage \"ReplicatedStorage\"";
        assert!(missing_child_hint(caught, &Sources::default(), Some(&story)).unwrap().contains("maps ReplicatedStorage.Packages"));
        // Not a missing member, or not a service's descendant: no hint
        assert!(missing_child_hint("attempt to index nil with 'Packages'", &Sources::default(), Some(&story)).is_none());
        assert!(missing_child_hint(r#"Foo is not a valid member of Frame "Frame""#, &Sources::default(), Some(&story)).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
