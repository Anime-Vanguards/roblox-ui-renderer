//! Advice for a story that reads an instance nothing mounted (`Packages is not a valid member of ReplicatedStorage
//! "ReplicatedStorage"`) or whose mount still waits when the run ends: what the Rojo project files and folders on disk
//! say about it, against what the run mounted. A hint names the cause rur checked and the change that fixes it; when
//! rur finds no cause there is no hint.
//!
//! The engine puts the hint in a scene's `diagnostics.hint` (`rur-native render` prints it), and the storybook shows
//! it under a failed or waiting story.
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::rojo;

/// What Roblox raises to require()'s caller when the module it requires raised an error while loading
const REQUIRE_WRAPPER: &str = "Requested module experienced an error while loading";

/// Whether `message` is Luau's generic `require()` wrapper, either on its own or inside a
/// multiline framework wrapper (such as Vide's `error while running root(): ...` traceback).
pub fn is_require_wrapper(message: &str) -> bool {
    message.lines().any(|line| line.trim().ends_with(REQUIRE_WRAPPER))
}

/// A story's failure as a person needs to read it, and as its hint is worked out from: when a required module fails,
/// Roblox logs the module's own error to output and raises a generic wrapper to the caller (ree does the same), so the
/// first logged script error that is not the wrapper stands in for it. `output` is the run's script output (a scene's
/// `diagnostics.output`: entries of `kind` and `message`).
pub fn failure_cause<'a>(failure: &'a str, output: &'a Value) -> &'a str {
    if !is_require_wrapper(failure) {
        return failure;
    }
    output
        .as_array()
        .into_iter()
        .flatten()
        .filter(|entry| entry["kind"] == "error")
        .filter_map(|entry| entry["message"].as_str())
        .find(|message| !is_require_wrapper(message))
        .unwrap_or(failure)
}

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
        if self.mounts.iter().any(|(path, mounted)| path == at && same_path(mounted, source)) {
            return true;
        }
        self.roots.iter().any(|root| root_mounts_path(root, at, source))
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

fn norm_path(path: &Path) -> PathBuf {
    crate::paths::canonical_path(path)
        .or_else(|_| std::path::absolute(path))
        .unwrap_or_else(|_| path.to_path_buf())
}

fn same_path(a: &Path, b: &Path) -> bool {
    a == b || norm_path(a) == norm_path(b)
}

fn path_contains(ancestor: &Path, child: &Path) -> bool {
    let a = norm_path(ancestor);
    let c = norm_path(child);
    c == a || c.starts_with(&a)
}

/// Whether `root` (a `--roots` folder laid out by service) already mounts `(at, source)`,
/// including Rojo script/model files such as `Foo.server.luau` mapped to `ServerScriptService/Foo`.
fn root_mounts_path(root: &Path, at: &str, source: &Path) -> bool {
    let via_root = at.split('/').fold(root.to_path_buf(), |dir, segment| dir.join(segment));
    if same_path(&via_root, source) {
        return true;
    }
    let Some(parent) = via_root.parent() else { return false };
    let Some(expected_name) = via_root.file_name().and_then(|s| s.to_str()) else { return false };
    if !source.parent().is_some_and(|p| same_path(p, parent)) {
        return false;
    }
    let Some(file_name) = source.file_name().and_then(|s| s.to_str()) else { return false };
    let stem = file_name
        .strip_suffix(".server.luau")
        .or_else(|| file_name.strip_suffix(".client.luau"))
        .or_else(|| file_name.strip_suffix(".server.lua"))
        .or_else(|| file_name.strip_suffix(".client.lua"))
        .or_else(|| file_name.strip_suffix(".model.json"))
        .or_else(|| file_name.strip_suffix(".meta.json"))
        .or_else(|| file_name.strip_suffix(".luau"))
        .or_else(|| file_name.strip_suffix(".lua"))
        .or_else(|| file_name.strip_suffix(".json"))
        .or_else(|| file_name.strip_suffix(".rbxm"))
        .or_else(|| file_name.strip_suffix(".rbxmx"))
        .or_else(|| file_name.strip_suffix(".rbx"));
    stem == Some(expected_name)
}

/// What a yielding story mount is waiting on, parsed from engine warnings or the suspended Luau line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WaitCause {
    /// Waiting inside `WaitForChild` (optional receiver/parent path, optional literal child name).
    WaitForChild { parent: Option<String>, child: Option<String> },
    /// Yielded in something other than `WaitForChild` (e.g. `InvokeServer`, `task.wait`, `Event:Wait`).
    Other,
}

/// Extracts the first quoted string literal (`"..."` or `'...'`) from `text`.
fn first_quoted_literal(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let q = bytes[i];
        if q == b'"' || q == b'\'' {
            let start = i + 1;
            let mut j = start;
            while j < bytes.len() {
                if bytes[j] == b'\\' {
                    j += 2;
                    continue;
                }
                if bytes[j] == q {
                    return Some(text[start..j].to_string());
                }
                j += 1;
            }
            return None;
        }
        i += 1;
    }
    None
}

/// Extracts a Roblox service or identifier path immediately preceding `:WaitForChild` / `.WaitForChild`.
fn wait_receiver_parent(prefix: &str) -> Option<String> {
    let trimmed = prefix.trim_end();
    // Handle `game:GetService("ReplicatedStorage")`
    if let Some(after_gs) = trimmed.rsplit("GetService").next() {
        if trimmed.contains("GetService") {
            if let Some(svc) = first_quoted_literal(after_gs) {
                return Some(svc);
            }
        }
    }
    let ident: String = trimmed
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '.')
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let ident = ident.trim_matches('.');
    (!ident.is_empty()).then(|| ident.to_string())
}

/// Parses what a yielding thread is waiting on from `"Infinite yield possible"` warnings or its suspended source line.
pub fn parse_wait_cause(warnings: &[&str], source_line: Option<&str>) -> Option<WaitCause> {
    for w in warnings {
        const PREFIX: &str = "Infinite yield possible on '";
        if let Some(rest) = w.strip_prefix(PREFIX) {
            if let Some((parent, after)) = rest.split_once(":WaitForChild(") {
                let child = first_quoted_literal(after);
                return Some(WaitCause::WaitForChild {
                    parent: Some(parent.to_string()),
                    child,
                });
            }
        }
    }
    let line = source_line?.trim();
    if line.is_empty() {
        return None;
    }
    if let Some(pos) = line.find(":WaitForChild(").or_else(|| line.find(".WaitForChild(")) {
        let prefix = &line[..pos];
        let after = &line[pos + ":WaitForChild(".len()..];
        let parent = wait_receiver_parent(prefix);
        let child = first_quoted_literal(after);
        return Some(WaitCause::WaitForChild { parent, child });
    }
    Some(WaitCause::Other)
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

/// Whether `rur --rojo <file>` accepts this project file instead of exiting with an error
/// (for example, a DataModel project with non-service top-level entries that `--rojo` rejects).
fn rojo_mountable(file: &Path) -> bool {
    let Ok(mounts) = rojo::mounts(file, None, &mut Vec::new()) else {
        return false;
    };
    mounts
        .iter()
        .all(|m| m.path.split('/').next().is_some_and(|svc| SERVICES.contains(&svc)))
}

/// Whether `path` (a file or service folder mounted by `--roots`) is covered by `present` Rojo mappings
/// under `service`. A directory is covered either directly or when every non-hidden child entry in it
/// is covered by `present`.
fn dir_covered_by_mappings(path: &Path, service: &str, present: &[(&str, &Path)], depth: u8) -> bool {
    if present.iter().any(|(at, src)| at.split('/').next() == Some(service) && path_contains(src, path)) {
        return true;
    }
    if depth == 0 || !path.is_dir() {
        return false;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return false;
    };
    let mut saw_entry = false;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_s = name.to_string_lossy();
        if name_s.starts_with('.') {
            continue;
        }
        saw_entry = true;
        if !dir_covered_by_mappings(&entry.path(), service, present, depth - 1) {
            return false;
        }
    }
    saw_entry
}

/// Whether replacing the run's current roots/mounts with `--rojo <file>` keeps everything the run
/// mounted (every service directory under every `--roots` folder, every `--mount`, and the story
/// itself when the story came from the run's project).
fn rojo_covers_run(file: &Path, sources: &Sources, story: Option<&Path>) -> bool {
    if !rojo_mountable(file) {
        return false;
    }
    let Ok(mappings) = rojo::mappings(file) else {
        return false;
    };
    let present: Vec<(&str, &Path)> = mappings
        .iter()
        .filter(|(_, _, exists)| *exists)
        .map(|(at, src, _)| (at.as_str(), src.as_path()))
        .collect();
    if present.is_empty() {
        return false;
    }
    for root in &sources.roots {
        for svc in SERVICES {
            let svc_dir = root.join(svc);
            if svc_dir.is_dir() && !dir_covered_by_mappings(&svc_dir, svc, &present, 2) {
                return false;
            }
        }
    }
    for (at, mounted_path) in &sources.mounts {
        let svc = at.split('/').next().unwrap_or("");
        if !present
            .iter()
            .any(|(m_at, m_src)| m_at.split('/').next() == Some(svc) && path_contains(m_src, mounted_path))
        {
            return false;
        }
    }
    if let Some(story_path) = story {
        let story_in_sources = sources.roots.iter().any(|r| path_contains(r, story_path))
            || sources.mounts.iter().any(|(_, m)| path_contains(m, story_path));
        if story_in_sources && !present.iter().any(|(_, m_src)| path_contains(m_src, story_path)) {
            return false;
        }
    }
    true
}

/// Whether a wait cause has to do with an unmounted Rojo mapping `at` (e.g. `"ReplicatedStorage/Packages"`).
fn wait_matches_mapping(cause: Option<&WaitCause>, at: &str) -> bool {
    let Some(WaitCause::WaitForChild { parent, child }) = cause else {
        return false;
    };
    let segments: Vec<&str> = at.split('/').collect();
    if let Some(child_name) = child.as_deref() {
        if !segments.iter().any(|seg| *seg == child_name) {
            return false;
        }
        if let Some(parent_path) = parent.as_deref() {
            let root = parent_path.split('.').next().unwrap_or(parent_path);
            if (SERVICES.contains(&root) || root == "PlayerGui") && segments.first().copied() != Some(root) {
                return false;
            }
        }
        true
    } else if let Some(parent_path) = parent.as_deref() {
        let norm = parent_path.replace('.', "/");
        at == norm || at.starts_with(&format!("{norm}/"))
    } else {
        false
    }
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
        if !rojo_covers_run(file, sources, story) {
            continue;
        }
        // Several project files beside each other that map it, and the run used none of them
        let dir = file.parent().unwrap_or(Path::new(""));
        let beside = projects
            .iter()
            .filter(|other| other.parent() == Some(dir) && maps(other).is_some() && rojo_covers_run(other, sources, story))
            .count();
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
    // The run mounted a package's own tree (not a DataModel), whose code finds its dependencies as siblings that only
    // a DataModel project beside it has
    if let Some(hint) = projects.iter().filter(|file| sources.used(file)).find_map(|package| data_model_hint(package, &wanted, story)) {
        return Some(hint);
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

/// The hint for a run that mounted `package`, a project whose tree is not a DataModel, when DataModel projects beside it
/// mount the story ([`rojo::data_models_beside`]): the instance path `wanted` exists only in them
fn data_model_hint(package: &Path, wanted: &str, story: Option<&Path>) -> Option<String> {
    let beside: Vec<PathBuf> = rojo::data_models_beside(package, story)
        .into_iter()
        .filter(|file| rojo_mountable(file))
        .collect();
    let mounted = format!("this run mounted {}, the package's own tree, not a DataModel, so nothing mounts {}", file_name(package), rojo::dotted(wanted));
    let what = rojo::source_name(story);
    match beside.as_slice() {
        [] => None,
        [file] => Some(format!("{mounted}; {} beside it is a DataModel project that mounts {what}: pass --rojo {}", file_name(file), file.display())),
        several => Some(format!("{mounted}; DataModel projects beside it mount {what} ({}): pass --rojo <file> to mount one", rojo::file_names(several))),
    }
}

/// The hint for a story whose mount still waits when the run ends (a WaitForChild on something nothing mounted, say):
/// the nearest project file's `$path`s that don't exist, or a `--rojo` hint only when the named project covers
/// everything the run mounted and the wait matches an unmounted path that project maps.
pub fn waiting_hint(sources: &Sources, story: Option<&Path>, cause: Option<&WaitCause>) -> Option<String> {
    if matches!(cause, Some(WaitCause::Other)) {
        return None;
    }
    let projects = nearby_project_files(sources, story);
    let dir = projects.first()?.parent()?;
    let here: Vec<&PathBuf> = projects.iter().filter(|file| file.parent() == Some(dir)).collect();
    let nearest = match here.iter().find(|file| sources.used(file)) {
        Some(used) => *used,
        None if here.len() > 1 => {
            let matching: Vec<PathBuf> = here
                .into_iter()
                .filter(|f| {
                    rojo_covers_run(f, sources, story)
                        && rojo::mappings(f).is_ok_and(|ms| {
                            ms.iter()
                                .any(|(at, src, exists)| *exists && !sources.mounted(at, src) && wait_matches_mapping(cause, at))
                        })
                })
                .cloned()
                .collect();
            return (!matching.is_empty()).then(|| rojo::several_projects(&matching));
        }
        None => here[0],
    };
    let mappings = rojo::mappings(nearest).ok()?;
    if let Some((at, source, _)) = mappings.iter().find(|(at, _, exists)| {
        !exists && (cause.is_none() || wait_matches_mapping(cause, at))
    }) {
        return Some(rojo::missing_path(nearest, at, source));
    }
    if !rojo_covers_run(nearest, sources, story) {
        return None;
    }
    let unmounted: Vec<String> = mappings
        .iter()
        .filter(|(at, source, _)| !sources.mounted(at, source) && wait_matches_mapping(cause, at))
        .map(|(at, _, _)| rojo::dotted(at))
        .collect();
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
        assert_eq!(waiting_hint(&sources, Some(&story), None).unwrap(), hint);
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
        let wait_packages = WaitCause::WaitForChild {
            parent: Some("ReplicatedStorage".into()),
            child: Some("Packages".into()),
        };
        let waiting = waiting_hint(&roots, Some(&story), Some(&wait_packages)).unwrap();
        assert!(waiting.starts_with("default.project.json maps ReplicatedStorage.Packages, but this run used the places"), "{waiting}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn several_project_files_name_the_ones_rur_would_not_choose_between() {
        let (dir, story) = game("several", "place.project.json");
        write(&dir.join("dev.project.json"), PROJECT);
        let none = Sources::default();
        let hint = missing_child_hint(MISSING, &none, Some(&story)).unwrap();
        assert_eq!(hint, format!("{} has several Rojo projects (dev.project.json, place.project.json), so none was mounted: pass --rojo <file> to mount one", dir.display()));
        let wait_packages = WaitCause::WaitForChild {
            parent: Some("ReplicatedStorage".into()),
            child: Some("Packages".into()),
        };
        assert_eq!(waiting_hint(&none, Some(&story), Some(&wait_packages)).unwrap(), hint);
        // Only one of them maps it: that one, and why the run didn't use it
        write(&dir.join("dev.project.json"), r#"{"name":"g","tree":{"$className":"DataModel","ReplicatedStorage":{"Shared":{"$path":"src/shared"}}}}"#);
        let hint = missing_child_hint(MISSING, &none, Some(&story)).unwrap();
        assert!(hint.starts_with("place.project.json maps ReplicatedStorage.Packages, but rur picks none of the 2 project files there by itself: pass --rojo"), "{hint}");
        // The one the run used is named instead
        let used = mounted(&dir, "place.project.json");
        assert!(waiting_hint(&used, Some(&story), Some(&wait_packages)).is_none());
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

    /// A Wally package mounted as its own tree (default.project.json, not a DataModel): its code reads its dependency as
    /// a sibling, ReplicatedStorage.Signal, which only the DataModel dev project beside it has, so the hint names it
    #[test]
    fn a_package_tree_run_names_the_data_model_project_that_mounts_the_story() {
        let dir = temp_dir("package");
        write(&dir.join("default.project.json"), r#"{"name":"Kit","tree":{"$path":"src"}}"#);
        write(&dir.join("dev.project.json"), r#"{"name":"dev","tree":{"$className":"DataModel","ReplicatedStorage":{"Packages":{"$path":"Packages","Kit":{"$path":"src"}}}}}"#);
        write(&dir.join("Packages/Signal.lua"), "return {}");
        let story = dir.join("src/Components/Thing.story.lua");
        write(&story, "return {}");
        let sources = mounted(&dir, "default.project.json");
        let missing = r#"Signal is not a valid member of ReplicatedStorage "ReplicatedStorage""#;
        let hint = missing_child_hint(missing, &sources, Some(&story)).unwrap();
        assert_eq!(
            hint,
            format!(
                "this run mounted default.project.json, the package's own tree, not a DataModel, so nothing mounts ReplicatedStorage.Signal; dev.project.json beside it is a DataModel project that mounts Thing.story.lua: pass --rojo {}",
                dir.join("dev.project.json").display()
            )
        );
        // Two of them: both named
        write(&dir.join("play.project.json"), r#"{"name":"play","tree":{"$className":"DataModel","ReplicatedStorage":{"Packages":{"$path":"Packages","Kit":{"$path":"src"}}}}}"#);
        let hint = missing_child_hint(missing, &sources, Some(&story)).unwrap();
        assert!(hint.ends_with("; DataModel projects beside it mount Thing.story.lua (dev.project.json, play.project.json): pass --rojo <file> to mount one"), "{hint}");
        // A member missing inside the package's own folder is that folder's
        let hint = missing_child_hint(r#"Gone is not a valid member of Folder "ReplicatedStorage.Kit.Components""#, &sources, Some(&story)).unwrap();
        assert_eq!(hint, format!("ReplicatedStorage.Kit.Components is the folder {}, which has no Gone", dir.join("src").join("Components").display()));
        // The run that mounted the DataModel project gets no such hint
        let dev = mounted(&dir, "dev.project.json");
        assert!(missing_child_hint(missing, &dev, Some(&story)).is_none_or(|hint| !hint.contains("DataModel")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// When require() fails, the module's own error is logged before require() raises its wrapper: that error is the
    /// failure people read and the one hints come from
    #[test]
    fn a_failed_require_reads_as_the_error_its_module_logged() {
        let wrapper = "Requested module experienced an error while loading";
        let output = serde_json::json!([
            { "kind": "warn", "message": "slow" },
            { "kind": "error", "message": "Packages is not a valid member of ReplicatedStorage \"ReplicatedStorage\"" },
            { "kind": "error", "message": wrapper }
        ]);
        assert_eq!(failure_cause(wrapper, &output), "Packages is not a valid member of ReplicatedStorage \"ReplicatedStorage\"");
        assert_eq!(failure_cause(&format!("ReplicatedStorage.Stories.A.story:1: {wrapper}"), &output), "Packages is not a valid member of ReplicatedStorage \"ReplicatedStorage\"");
        let multiline_wrapped = format!(
            "error while running root():\n\neffect error:\nerror while running branch():\n\n{wrapper}\nReplicatedStorage.ui.vide.branch:10 function traceback\nReplicatedStorage.Stories.A.story:96 function Panel\n"
        );
        let output_with_wrapped = serde_json::json!([
            { "kind": "error", "message": "ReplicatedStorage.BrokenMod:4: missing config" },
            { "kind": "error", "message": wrapper },
            { "kind": "error", "message": multiline_wrapped }
        ]);
        assert_eq!(failure_cause(&multiline_wrapped, &output_with_wrapped), "ReplicatedStorage.BrokenMod:4: missing config");
        assert_eq!(failure_cause("boom", &output), "boom");
        assert_eq!(failure_cause(wrapper, &serde_json::json!([{ "kind": "error", "message": wrapper }])), wrapper);
        assert_eq!(failure_cause(wrapper, &Value::Null), wrapper);
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

    #[test]
    fn waiting_hint_skips_mounted_roots_and_unmountable_rojo_projects() {
        // 1. A project whose service folders are already mounted via --roots needs no --rojo hint
        let dir = temp_dir("roots-mounted");
        write(
            &dir.join("default.project.json"),
            r#"{"name":"g","tree":{"$className":"DataModel","ReplicatedFirst":{"$path":"ReplicatedFirst"},"ReplicatedStorage":{"$path":"ReplicatedStorage"}}}"#,
        );
        write(&dir.join("ReplicatedFirst/Init.luau"), "return {}");
        let story = dir.join("ReplicatedStorage/Ui.story.luau");
        write(&story, "return {}");
        let via_roots = Sources { roots: vec![dir.clone()], mounts: Vec::new() };
        let wait_init = WaitCause::WaitForChild { parent: Some("ReplicatedFirst".into()), child: Some("Init".into()) };
        assert!(waiting_hint(&via_roots, Some(&story), Some(&wait_init)).is_none(), "folders mounted through --roots must not trigger a --rojo hint");
        let _ = std::fs::remove_dir_all(&dir);

        // 2. A DataModel project with a non-service top-level key (which `rur --rojo` rejects) must never be recommended
        let bad = temp_dir("unmountable-rojo");
        write(
            &bad.join("default.project.json"),
            r#"{"name":"g","tree":{"$className":"DataModel","ReplicatedFirst":{"$path":"rf"},"CustomTop":{"$path":"custom"}}}"#,
        );
        write(&bad.join("rf/Init.luau"), "return {}");
        write(&bad.join("custom/A.luau"), "return {}");
        let story = bad.join("custom/Ui.story.luau");
        write(&story, "return {}");
        let empty = Sources::default();
        assert!(!rojo_mountable(&bad.join("default.project.json")));
        assert!(waiting_hint(&empty, Some(&story), Some(&wait_init)).is_none(), "must not recommend --rojo on a project that --rojo rejects");
        let missing = r#"Init is not a valid member of ReplicatedFirst "ReplicatedFirst""#;
        let hint = missing_child_hint(missing, &empty, Some(&story));
        assert!(hint.as_deref().is_none_or(|h| !h.contains("pass --rojo")), "must not suggest broken --rojo: {hint:?}");
        let _ = std::fs::remove_dir_all(&bad);
    }

    #[test]
    fn rojo_hint_requires_covering_mounted_roots_and_matching_wait() {
        // 1. Multi-root run where default.project.json only covers the second root (dropping shared_root)
        let dir = temp_dir("partial-roots");
        let shared = dir.join("shared");
        let place = dir.join("place");
        write(&shared.join("ReplicatedStorage/Core/Lib.luau"), "return {}");
        write(&place.join("ReplicatedStorage/App.luau"), "return {}");
        write(&place.join("Extra/Net.luau"), "return {}");
        let story = place.join("StarterPlayer/Stories/Card.story.luau");
        write(&story, "return {}");
        write(
            &place.join("default.project.json"),
            r#"{"name":"Place","tree":{"$className":"DataModel","ReplicatedStorage":{"App":{"$path":"ReplicatedStorage/App.luau"},"Extra":{"$path":"Extra"}}}}"#,
        );
        let multi_roots = Sources { roots: vec![shared.clone(), place.clone()], mounts: Vec::new() };
        let wait_extra = WaitCause::WaitForChild {
            parent: Some("ReplicatedStorage".into()),
            child: Some("Extra".into()),
        };
        assert!(
            waiting_hint(&multi_roots, Some(&story), Some(&wait_extra)).is_none(),
            "must not suggest --rojo when project drops another mounted root"
        );
        let missing_extra = r#"Extra is not a valid member of ReplicatedStorage "ReplicatedStorage""#;
        let mhint = missing_child_hint(missing_extra, &multi_roots, Some(&story));
        assert!(
            mhint.as_deref().is_none_or(|h| !h.contains("pass --rojo")),
            "missing_child_hint must not suggest partial --rojo: {mhint:?}"
        );

        // 2. Project DOES cover roots, but wait is unrelated (InvokeServer or WaitForChild on another path)
        let full = temp_dir("unrelated-wait");
        write(&full.join("ReplicatedStorage/App.luau"), "return {}");
        write(&full.join("Packages/Kit.luau"), "return {}");
        let full_story = full.join("ReplicatedStorage/Card.story.luau");
        write(&full_story, "return {}");
        write(
            &full.join("default.project.json"),
            r#"{"name":"Full","tree":{"$className":"DataModel","ReplicatedStorage":{"$path":"ReplicatedStorage","Packages":{"$path":"Packages"}}}}"#,
        );
        let single_root = Sources { roots: vec![full.clone()], mounts: Vec::new() };
        assert_eq!(parse_wait_cause(&[], Some("rf:InvokeServer()")), Some(WaitCause::Other));
        assert!(
            waiting_hint(&single_root, Some(&full_story), Some(&WaitCause::Other)).is_none(),
            "InvokeServer() wait must not print a --rojo hint"
        );
        let wait_unrelated = parse_wait_cause(&[], Some("local x = PlayerGui:WaitForChild(\"Hotbar\")"));
        assert!(
            waiting_hint(&single_root, Some(&full_story), wait_unrelated.as_ref()).is_none(),
            "unrelated WaitForChild must not print a --rojo hint"
        );
        assert!(
            waiting_hint(&single_root, Some(&full_story), None).is_none(),
            "unknown wait must not guess a --rojo hint"
        );
        // Matching WaitForChild on a covering project DOES emit the --rojo hint
        let wait_pkg = parse_wait_cause(&[], Some("local p = ReplicatedStorage:WaitForChild(\"Packages\")"));
        let ok_hint = waiting_hint(&single_root, Some(&full_story), wait_pkg.as_ref()).unwrap();
        assert!(ok_hint.contains("pass --rojo"), "{ok_hint}");

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&full);
    }
}
