//! `--git-ref <ref>`: a run against the project's sources as they are at a git commit, for before/after renders
//! without a second checkout (`rur diff`, `rur render --git-ref`, `rur sweep --git-ref`).
//!
//! The files a run reads (the roots and the project files next to them, the Rojo project and every `$path` it maps,
//! `--mount` sources, the story) are extracted from each repository's `<ref>` into
//! `<state dir>/git-refs/<repo>-<hash>/<commit>/` through a temporary index (`git read-tree` + `checkout-index`; the
//! repository's own index, HEAD and working tree are untouched) and reused by later runs of the same commit.
//! Submodules (e.g. external/ data) are linked to the working tree's checkout. A file the commit does not have is
//! taken from the working tree, with a note: a story or a mount added since, or a folder git ignores that the project
//! maps (a package manager's install folder mapped to ReplicatedStorage.Packages).
//!
//! git runs with explicit arguments (no shell), only `rev-parse`, `read-tree`, `ls-files` and `checkout-index`; the
//! three that write the extracted tree take `-c core.longpaths=true`, so a deeply nested project extracts under a long
//! state directory on Windows (git ignores the setting elsewhere).
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use sha2::{Digest, Sha256};

use crate::js::Json;

/// The environment variables that hold an account's credentials: git is run without them
fn is_credential(name: &str) -> bool {
    ["ROBLOX_ASSET_DELIVERY_COOKIE", "ROBLOSECURITY", "ROBLOX_BROKER_TOKEN"].iter().any(|known| name.eq_ignore_ascii_case(known))
}

/// `git -C <cwd> <args>`: its stdout, or an error naming the command and git's message. `long_paths` adds
/// `-c core.longpaths=true`; `index` runs it on that index file; `input` goes to its stdin.
fn git(cwd: &Path, args: &[&str], long_paths: bool, index: Option<&Path>, input: Option<&[u8]>) -> Result<Vec<u8>, String> {
    let mut command = Command::new("git");
    if long_paths {
        command.args(["-c", "core.longpaths=true"]);
    }
    command.arg("-C").arg(cwd).args(args).stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() }).stdout(Stdio::piped()).stderr(Stdio::piped());
    for (name, _) in std::env::vars_os() {
        if is_credential(&name.to_string_lossy()) {
            command.env_remove(name);
        }
    }
    if let Some(index) = index {
        command.env("GIT_INDEX_FILE", index);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW
        command.creation_flags(0x0800_0000);
    }
    let mut child = command.spawn().map_err(|error| format!("git {}: {error}", args[0]))?;
    if let Some(input) = input {
        use std::io::Write;
        let mut stdin = child.stdin.take().expect("a piped stdin");
        let input = input.to_vec();
        // (written on a thread of its own: git answers while it reads)
        std::thread::spawn(move || stdin.write_all(&input));
    }
    let output = child.wait_with_output().map_err(|error| format!("git {}: {error}", args[0]))?;
    if !output.status.success() {
        return Err(format!("git {} (in {}): {}", args.join(" "), cwd.display(), String::from_utf8_lossy(&output.stderr).trim()));
    }
    Ok(output.stdout)
}

/// `path` with forward slashes
fn slash(path: &str) -> String {
    path.replace('\\', "/")
}

/// `path` as compared: Windows paths ignore case
fn same_case(path: &str) -> String {
    if cfg!(windows) { path.to_lowercase() } else { path.to_string() }
}

/// `child` relative to `parent` (forward slashes, "" for the same path) when it is inside it; Windows compares case
/// insensitively, as Node's path.relative does there
fn inside(parent: &Path, child: &Path) -> Option<String> {
    let mut parent_parts = parent.components();
    let mut child_parts = child.components();
    loop {
        match (parent_parts.next(), child_parts.clone().next()) {
            (None, _) => break,
            (Some(a), Some(b)) if same_case(&a.as_os_str().to_string_lossy()) == same_case(&b.as_os_str().to_string_lossy()) => {
                child_parts.next();
            }
            _ => return None,
        }
    }
    Some(slash(&child_parts.as_path().to_string_lossy()))
}

/// The real path of a file that exists (links resolved), else the path made absolute
fn real(file: &Path) -> PathBuf {
    if file.exists() {
        crate::paths::canonical_path(file).unwrap_or_else(|_| crate::rojo::resolve(Path::new(""), &file.to_string_lossy()))
    } else {
        crate::rojo::resolve(Path::new(""), &file.to_string_lossy())
    }
}

/// A link at `link` to the directory `target`: a junction on Windows (no privilege needed), else a symlink
fn link_directory(target: &Path, link: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        junction::create(target, link)
    }
    #[cfg(not(windows))]
    {
        std::os::unix::fs::symlink(target, link)
    }
}

/// A repository at one commit, extracted on demand
struct Snapshot {
    top: PathBuf,
    commit: String,
    dir: PathBuf,
    state_file: PathBuf,
    index: PathBuf,
    /// The pathspecs extracted so far (also by earlier runs)
    done: Vec<String>,
}

impl Snapshot {
    fn new(top: PathBuf, reference: &str, state: &Path) -> Result<Snapshot, String> {
        let commit = git(&top, &["rev-parse", "--verify", "--quiet", &format!("{reference}^{{commit}}")], false, None, None)?;
        let commit = String::from_utf8_lossy(&commit).trim().to_string();
        let hash = Sha256::digest(same_case(&slash(&top.to_string_lossy())).as_bytes());
        let id: String = hash.iter().take(4).map(|byte| format!("{byte:02x}")).collect();
        let name = top.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
        let dir = state.join("git-refs").join(format!("{name}-{id}")).join(&commit);
        let state_file = dir.join(".rur-git-ref.json");
        let index = PathBuf::from(format!("{}.index", dir.display()));
        let done = Json::read(&state_file).map(|state| state.get("pathspecs").items().iter().filter_map(|spec| spec.as_str().map(str::to_string)).collect()).unwrap_or_default();
        Ok(Snapshot { top, commit, dir, state_file, index, done })
    }

    /// Extracts the files matching `pathspecs` (repository-relative, git pathspec syntax)
    fn extract(&mut self, pathspecs: &[String]) -> Result<(), String> {
        let mut wanted: Vec<String> = Vec::new();
        for spec in pathspecs {
            if !self.done.contains(spec) && !wanted.contains(spec) {
                wanted.push(spec.clone());
            }
        }
        if wanted.is_empty() {
            return Ok(());
        }
        std::fs::create_dir_all(&self.dir).map_err(|error| format!("{}: {error}", self.dir.display()))?;
        if !self.index.exists() {
            git(&self.top, &["read-tree", &self.commit], true, Some(&self.index), None)?;
        }
        let mut list_args = vec!["ls-files", "-s", "-z", "--"];
        list_args.extend(wanted.iter().map(String::as_str));
        let listed = git(&self.top, &list_args, true, Some(&self.index), None)?;
        let mut files: Vec<&[u8]> = Vec::new();
        for line in listed.split(|byte| *byte == 0).filter(|line| !line.is_empty()) {
            let Some(tab) = line.iter().position(|byte| *byte == b'\t') else { continue };
            let (meta, file) = (&line[..tab], &line[tab + 1..]);
            if meta.starts_with(b"160000") {
                // A submodule: a link to the working tree's checkout (the ref's own commit is not fetched)
                let relative = String::from_utf8_lossy(file).into_owned();
                let source = self.top.join(&relative);
                let link = self.dir.join(&relative);
                if source.exists() && !link.exists() {
                    if let Some(parent) = link.parent() {
                        std::fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
                    }
                    link_directory(&source, &link).map_err(|error| format!("linking {} to {}: {error}", link.display(), source.display()))?;
                }
            } else {
                files.push(file);
            }
        }
        if !files.is_empty() {
            let prefix = format!("--prefix={}/", slash(&self.dir.to_string_lossy()));
            git(&self.top, &["checkout-index", "-f", "-z", "--stdin", &prefix], true, Some(&self.index), Some(&files.join(&0u8)))?;
        }
        self.done.extend(wanted);
        let state = Json::object().with("commit", self.commit.as_str()).with("top", self.top.as_path()).with("pathspecs", self.done.clone());
        std::fs::write(&self.state_file, state.stringify(1)).map_err(|error| format!("{}: {error}", self.state_file.display()))
    }
}

/// One `--git-ref`: the ref and the repositories extracted at it so far
pub struct GitRef {
    reference: String,
    state: PathBuf,
    snapshots: Vec<Snapshot>,
}

impl GitRef {
    /// Extracts into `<state>/git-refs`
    pub fn new(reference: &str, state: &Path) -> GitRef {
        GitRef { reference: reference.to_string(), state: state.to_path_buf(), snapshots: Vec::new() }
    }

    /// The snapshot of the repository `file` belongs to (None outside any repository)
    fn snapshot_of(&mut self, file: &Path) -> Result<Option<usize>, String> {
        let mut dir = if file.is_dir() { file.to_path_buf() } else { file.parent().map(Path::to_path_buf).unwrap_or_default() };
        while !dir.exists() {
            match dir.parent() {
                Some(parent) => dir = parent.to_path_buf(),
                None => break,
            }
        }
        let Ok(top) = git(&dir, &["rev-parse", "--show-toplevel"], false, None, None) else { return Ok(None) };
        let top = crate::rojo::resolve(Path::new(""), String::from_utf8_lossy(&top).trim());
        let key = same_case(&top.to_string_lossy());
        if let Some(index) = self.snapshots.iter().position(|snapshot| same_case(&snapshot.top.to_string_lossy()) == key) {
            return Ok(Some(index));
        }
        let snapshot = Snapshot::new(top.clone(), &self.reference, &self.state)
            .map_err(|error| format!("--git-ref {}: not a commit in {} ({})", self.reference, top.display(), error.lines().next().unwrap_or("")))?;
        self.snapshots.push(snapshot);
        Ok(Some(self.snapshots.len() - 1))
    }

    /// `file` (a working-tree path, or a path inside an extracted snapshot) as it is at the ref; `extra` are further
    /// repository-relative pathspecs to extract with it. A file outside any repository is used as it is (`required`:
    /// an error instead), and one the commit does not have is the working tree's, with a note.
    pub fn map(&mut self, file: &Path, extra: &[String], required: bool) -> Result<PathBuf, String> {
        let real = real(file);
        for snapshot in &mut self.snapshots {
            if let Some(relative) = inside(&snapshot.dir, &real) {
                let relative = if relative.is_empty() { ".".to_string() } else { relative };
                snapshot.extract(std::slice::from_ref(&relative))?;
                // A path the project at the ref maps that the commit does not hold (a folder git ignores, such as
                // the one a package manager installs into): the working tree's, as for a story or a mount
                let working = snapshot.top.join(&relative);
                if !real.exists() && working.exists() {
                    eprintln!("rur: --git-ref {}: {relative} does not exist at {}; using the working tree's", self.reference, &snapshot.commit[..10.min(snapshot.commit.len())]);
                    return Ok(working);
                }
                return Ok(real);
            }
        }
        let Some(index) = self.snapshot_of(&real)? else {
            if required {
                return Err(format!("--git-ref: {} is not inside a git repository", file.display()));
            }
            eprintln!("rur: --git-ref {}: {} is not in a git repository; using it as it is", self.reference, file.display());
            return Ok(file.to_path_buf());
        };
        let snapshot = &mut self.snapshots[index];
        let relative = inside(&snapshot.top, &real).unwrap_or_default();
        let mut specs = vec![if relative.is_empty() { ".".to_string() } else { relative.clone() }];
        specs.extend(extra.iter().cloned());
        snapshot.extract(&specs)?;
        let mapped = if relative.is_empty() { snapshot.dir.clone() } else { snapshot.dir.join(&relative) };
        if !mapped.exists() {
            eprintln!("rur: --git-ref {}: {relative} does not exist at {}; using the working tree's", self.reference, &snapshot.commit[..10.min(snapshot.commit.len())]);
            return Ok(file.to_path_buf());
        }
        Ok(mapped)
    }

    /// A run's root at the ref, with the project files beside it (rur.json, the Studio hierarchy snapshots,
    /// submodule data in external/)
    pub fn map_root(&mut self, root: &Path) -> Result<PathBuf, String> {
        let real = crate::paths::canonical_path(root).map_err(|error| format!("{}: {error}", root.display()))?;
        let Some(index) = self.snapshot_of(&real)? else { return Ok(root.to_path_buf()) };
        let relative = inside(&self.snapshots[index].top, &real).unwrap_or_default();
        let parent = match relative.rsplit_once('/') {
            Some((parent, _)) => parent.to_string(),
            None => ".".to_string(),
        };
        let prefix = if parent == "." { String::new() } else { format!("{parent}/") };
        let extra = [format!(":(glob){prefix}*"), format!("{prefix}external")];
        self.map(root, &extra, false)
    }

    /// What a summary says of the ref: `{ref, commits: [{repo, commit, dir}]}`
    pub fn describe(&self) -> Json {
        let commits: Vec<Json> = self
            .snapshots
            .iter()
            .map(|snapshot| Json::object().with("repo", snapshot.top.as_path()).with("commit", snapshot.commit.as_str()).with("dir", snapshot.dir.as_path()))
            .collect();
        Json::object().with("ref", self.reference.as_str()).with("commits", commits)
    }
}

/// `rur-native git-ref --git-ref <ref> [--roots <dir;dir>] [--rojo <file.project.json> [--rojo-at Service/Name]]
/// [--mount Service/Name=<path>]... [--story | --module | --model | --fixture <file>]`: a run's sources extracted at the
/// ref, for the Node CLI's `rur render --git-ref` (cli/gitref.mjs). Prints `{"project": [the engine's --roots and
/// mounts], "sources": {"story": <file at the ref>, ...}, "gitRef": {ref, commits}}`; `--roots` given (even empty) are
/// the run's roots, else they come from the project as for a render. Returns the exit code.
pub fn cli(argv: &[String]) -> i32 {
    use crate::story_args::{Context, Failure};
    let run = || -> Result<Json, Failure> {
        let (context, args) = Context::new(argv)?;
        let reference = args.value("--git-ref").ok_or_else(|| Failure::Usage("git-ref needs --git-ref <ref>".into()))?;
        let roots = match args.given("--roots") {
            Some(roots) => roots.split(';').filter(|root| !root.is_empty()).map(|root| context.resolve(root).display().to_string()).collect(),
            None => context.project_roots(&args),
        };
        let mut git_ref = GitRef::new(&reference, &context.state);
        let (mapped, project) = context.apply_git_ref(&args, &roots, &mut git_ref)?;
        let mut sources = Json::object();
        for key in ["--story", "--module", "--model", "--fixture"] {
            if let Some(value) = mapped.value(key) {
                sources.set(&key[2..], value);
            }
        }
        Ok(Json::object().with("project", project).with("sources", sources).with("gitRef", git_ref.describe()))
    };
    match run() {
        Ok(result) => {
            println!("{}", result.stringify(0));
            0
        }
        Err(failure) => {
            eprintln!("rur-native git-ref: {}", failure.message());
            failure.code()
        }
    }
}

/// A directory junction on Windows, which needs no privilege (a directory symlink does): the reparse point
/// `FSCTL_SET_REPARSE_POINT` writes on an empty directory, as `mklink /J` makes it
#[cfg(windows)]
mod junction {
    use std::{os::windows::ffi::OsStrExt, os::windows::fs::OpenOptionsExt, os::windows::io::AsRawHandle, path::Path};

    const FSCTL_SET_REPARSE_POINT: u32 = 0x0009_00A4;
    const IO_REPARSE_TAG_MOUNT_POINT: u32 = 0xA000_0003;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn DeviceIoControl(
            device: *mut core::ffi::c_void,
            code: u32,
            input: *const core::ffi::c_void,
            input_size: u32,
            output: *mut core::ffi::c_void,
            output_size: u32,
            returned: *mut u32,
            overlapped: *mut core::ffi::c_void,
        ) -> i32;
    }

    pub fn create(target: &Path, link: &Path) -> std::io::Result<()> {
        let target = std::path::absolute(target)?;
        // The NT path the junction points at, and the path it shows
        let substitute: Vec<u16> = std::ffi::OsStr::new(r"\??\").encode_wide().chain(target.as_os_str().encode_wide()).collect();
        let print: Vec<u16> = target.as_os_str().encode_wide().collect();
        let names = (substitute.len() + 1 + print.len() + 1) * 2;
        let data_length = 8 + names;
        if data_length > 16 * 1024 - 8 {
            return Err(std::io::Error::other("the junction's target path is too long"));
        }
        let mut buffer: Vec<u8> = Vec::with_capacity(8 + data_length);
        buffer.extend(IO_REPARSE_TAG_MOUNT_POINT.to_le_bytes());
        buffer.extend((data_length as u16).to_le_bytes());
        buffer.extend(0u16.to_le_bytes());
        // substitute name: offset, length; print name: offset, length (bytes, without the terminating nulls)
        buffer.extend(0u16.to_le_bytes());
        buffer.extend(((substitute.len() * 2) as u16).to_le_bytes());
        buffer.extend((((substitute.len() + 1) * 2) as u16).to_le_bytes());
        buffer.extend(((print.len() * 2) as u16).to_le_bytes());
        for unit in substitute.iter().chain([&0]).chain(print.iter()).chain([&0]) {
            buffer.extend(unit.to_le_bytes());
        }
        std::fs::create_dir(link)?;
        let made = (|| {
            let directory = std::fs::OpenOptions::new().write(true).custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS).open(link)?;
            let mut returned = 0u32;
            // SAFETY: the handle is an open directory, the buffer a whole REPARSE_DATA_BUFFER of the size given, and
            // the call is synchronous (no OVERLAPPED)
            let done = unsafe {
                DeviceIoControl(
                    directory.as_raw_handle(),
                    FSCTL_SET_REPARSE_POINT,
                    buffer.as_ptr().cast(),
                    buffer.len() as u32,
                    std::ptr::null_mut(),
                    0,
                    &mut returned,
                    std::ptr::null_mut(),
                )
            };
            if done == 0 { Err(std::io::Error::last_os_error()) } else { Ok(()) }
        })();
        if made.is_err() {
            let _ = std::fs::remove_dir(link);
        }
        made
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(dir: &Path, args: &[&str]) {
        let status = Command::new("git").arg("-C").arg(dir).args(["-c", "user.name=rur", "-c", "user.email=rur@localhost", "-c", "core.autocrlf=false"]).args(args).stdout(Stdio::null()).stderr(Stdio::null()).status().unwrap();
        assert!(status.success(), "git {args:?}");
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rur-gitref-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        crate::paths::canonical_path(&dir).unwrap()
    }

    /// A committed file is read as committed while the working tree has another version, a file the commit lacks is
    /// the working tree's, and a folder git ignores (and the commit so lacks) is the working tree's when the project
    /// at the ref maps it; the repository's index and working tree are untouched
    #[test]
    fn files_come_from_the_commit_and_ignored_folders_from_the_working_tree() {
        let repo = temp("repo");
        let state = temp("state");
        std::fs::write(repo.join(".gitignore"), "Packages/\n").unwrap();
        std::fs::create_dir_all(repo.join("src")).unwrap();
        std::fs::write(repo.join("src/App.luau"), "return 'committed'").unwrap();
        run(&repo, &["init", "-q"]);
        run(&repo, &["add", "-A"]);
        run(&repo, &["commit", "-qm", "base"]);
        std::fs::write(repo.join("src/App.luau"), "return 'edited'").unwrap();
        std::fs::write(repo.join("src/New.luau"), "return 'new'").unwrap();
        std::fs::create_dir_all(repo.join("Packages")).unwrap();
        std::fs::write(repo.join("Packages/Lib.luau"), "return 'package'").unwrap();

        let mut reference = GitRef::new("HEAD", &state);
        let app = reference.map(&repo.join("src/App.luau"), &[], true).unwrap();
        assert!(app.starts_with(state.join("git-refs")), "{}", app.display());
        assert_eq!(std::fs::read_to_string(&app).unwrap(), "return 'committed'");
        assert_eq!(reference.map(&repo.join("src/New.luau"), &[], true).unwrap(), repo.join("src/New.luau"));
        // (the project file at the ref sits in the snapshot; its $paths resolve there)
        let snapshot = reference.snapshots[0].dir.clone();
        assert_eq!(reference.map(&snapshot.join("src"), &[], false).unwrap(), snapshot.join("src"));
        assert_eq!(reference.map(&snapshot.join("Packages"), &[], false).unwrap(), repo.join("Packages"));
        assert_eq!(reference.map(&snapshot.join("Missing"), &[], false).unwrap(), snapshot.join("Missing"));
        let described = reference.describe();
        assert_eq!(described.get("ref").as_str(), Some("HEAD"));
        assert_eq!(described.get("commits").items().len(), 1);
        let status = Command::new("git").arg("-C").arg(&repo).args(["status", "--short", "--untracked-files=no"]).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&status.stdout).trim(), "M src/App.luau");
        let _ = std::fs::remove_dir_all(&repo);
        let _ = std::fs::remove_dir_all(&state);
    }

    /// A deeply nested file extracts into a state directory whose paths pass Windows' 260 characters (git needs
    /// core.longpaths for that); elsewhere there is no such limit and the test checks the same extraction
    #[test]
    fn deeply_nested_files_extract_past_the_windows_path_limit() {
        let repo = temp("deep");
        let state = temp("deep-state").join("a-state-directory-with-a-long-name-like-a-user-temp-folder-has");
        let nested: PathBuf = (0..6).map(|level| format!("nested-folder-{level}-with-a-long-name")).collect();
        let file = repo.join(&nested).join("Story.story.luau");
        let relative = nested.join("Story.story.luau");
        // (git writes the long path itself; the test's own write goes through Rust, which handles long paths)
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "return 'deep'").unwrap();
        run(&repo, &["-c", "core.longpaths=true", "init", "-q"]);
        run(&repo, &["-c", "core.longpaths=true", "add", "-A"]);
        run(&repo, &["-c", "core.longpaths=true", "commit", "-qm", "deep"]);
        let mut reference = GitRef::new("HEAD", &state);
        let mapped = reference.map(&file, &[], true).unwrap();
        assert!(mapped.starts_with(&state), "{}", mapped.display());
        assert!(mapped.ends_with(&relative));
        if cfg!(windows) {
            assert!(mapped.as_os_str().len() > 260, "{} characters", mapped.as_os_str().len());
        }
        assert_eq!(std::fs::read_to_string(&mapped).unwrap(), "return 'deep'");
        let _ = std::fs::remove_dir_all(&repo);
        let _ = std::fs::remove_dir_all(state.parent().unwrap());
    }

    #[cfg(windows)]
    #[test]
    fn a_junction_links_a_directory() {
        let dir = temp("junction");
        std::fs::create_dir_all(dir.join("target")).unwrap();
        std::fs::write(dir.join("target/file.txt"), "linked").unwrap();
        junction::create(&dir.join("target"), &dir.join("link")).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("link/file.txt")).unwrap(), "linked");
        assert!(std::fs::symlink_metadata(dir.join("link")).unwrap().file_type().is_symlink() || std::fs::symlink_metadata(dir.join("link")).unwrap().is_dir());
        std::fs::remove_dir(dir.join("link")).unwrap();
        assert!(dir.join("target/file.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
