//! Change counters for directory trees, so the render service can tell a tree is unchanged without
//! listing it again (serve.rs fingerprint: a project's roots can be thousands of directories, listed
//! twice a render).
//!
//! [`changes`] starts a watch on a tree the first time it is asked about it: on Windows a thread with
//! an overlapped `ReadDirectoryChangesW` on the whole tree, counting the changes that matter to its
//! [`Scope`]. The count is only offered once the watch is registered (changes from then on are seen), and
//! never after the watch failed (a buffer overflow counts as a change and keeps watching; a tree that
//! went away ends the watch). Elsewhere there are no watches and callers list the tree.
use std::path::Path;

/// Which changes under a tree count
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Scope {
    /// Any file or directory created, removed, renamed, written or resized
    Everything,
    /// Entries created, removed or renamed, and writes to the files a project mount reads
    /// ([`mount_read`]): what a preloaded project depends on (scripts and JSON modules are read at
    /// require)
    Structure,
}

/// Whether a write to `name` (a file's name, or its path relative to the tree) changes what a project
/// mount reads. The engine's mount, as `rojo build` 7.6.1 builds a place, reads models (.rbxm, .rbxmx,
/// .model.json), project files, text files (.txt), localization tables (.csv), TOML and YAML modules
/// (.toml, .yaml, .yml) and meta files as it mounts them; scripts and JSON modules it reads at require.
/// Its rules take a suffix in lower case alone (`A.TXT` makes nothing), but it opens meta files, a
/// folder's init.csv and its default.project.json by path, so the file system's case rule decides for
/// those, and they count in any case.
pub fn mount_read(name: &str) -> bool {
    const RULES: [&str; 9] = [".rbxm", ".rbxmx", ".model.json", ".project.json", ".txt", ".csv", ".toml", ".yaml", ".yml"];
    let file = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let lower = file.to_ascii_lowercase();
    RULES.iter().any(|suffix| file.ends_with(suffix)) || lower.ends_with(".meta.json") || lower == "init.csv" || lower == "default.project.json"
}

/// Whether a listing of a project tree passes over the entry named `name` and all below it: node_modules,
/// and a .git, which the engine mounts as an empty Folder whatever it holds. Other names that start with
/// a dot mount as any other (pesde's Packages/.pesde), so changes below them count.
pub fn unlisted(name: &str) -> bool {
    name == ".git" || name == "node_modules"
}

/// The changes counted under `root` since its watch started, or None while it cannot be watched
pub fn changes(root: &Path, scope: Scope) -> Option<u64> {
    imp::changes(root, scope)
}

#[cfg(windows)]
mod imp {
    use super::Scope;
    use std::{
        collections::HashMap,
        ffi::c_void,
        os::windows::ffi::OsStrExt,
        path::{Path, PathBuf},
        sync::{
            Arc, Mutex, OnceLock,
            atomic::{AtomicU8, AtomicU64, Ordering},
        },
        time::{Duration, Instant},
    };

    const STARTING: u8 = 0;
    const WATCHING: u8 = 1;
    const FAILED: u8 = 2;

    struct Watch {
        state: AtomicU8,
        changes: AtomicU64,
    }

    static WATCHES: OnceLock<Mutex<HashMap<(PathBuf, Scope), Arc<Watch>>>> = OnceLock::new();

    pub fn changes(root: &Path, scope: Scope) -> Option<u64> {
        let key = (std::path::absolute(root).ok()?, scope);
        let watch = {
            let mut watches = WATCHES.get_or_init(Default::default).lock().ok()?;
            match watches.get(&key) {
                Some(watch) => watch.clone(),
                None => {
                    if !key.0.is_dir() {
                        return None;
                    }
                    let watch = Arc::new(Watch { state: AtomicU8::new(STARTING), changes: AtomicU64::new(0) });
                    watches.insert(key.clone(), watch.clone());
                    let (path, thread_watch) = (key.0.clone(), watch.clone());
                    if std::thread::Builder::new().name("rur-watch".into()).spawn(move || run(&path, scope, &thread_watch)).is_err() {
                        watch.state.store(FAILED, Ordering::Release);
                    }
                    watch
                }
            }
        };
        // A new watch registers in about a millisecond
        let deadline = Instant::now() + Duration::from_millis(250);
        while watch.state.load(Ordering::Acquire) == STARTING && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        (watch.state.load(Ordering::Acquire) == WATCHING).then(|| watch.changes.load(Ordering::Acquire))
    }

    #[repr(C)]
    struct Overlapped {
        internal: usize,
        internal_high: usize,
        offset: u32,
        offset_high: u32,
        event: *mut c_void,
    }

    unsafe extern "system" {
        fn CreateFileW(name: *const u16, access: u32, share: u32, security: *mut c_void, disposition: u32, flags: u32, template: *mut c_void) -> *mut c_void;
        fn CreateEventW(security: *mut c_void, manual: i32, initial: i32, name: *const u16) -> *mut c_void;
        fn ReadDirectoryChangesW(directory: *mut c_void, buffer: *mut c_void, length: u32, subtree: i32, filter: u32, returned: *mut u32, overlapped: *mut Overlapped, completion: *mut c_void) -> i32;
        fn GetOverlappedResult(file: *mut c_void, overlapped: *mut Overlapped, transferred: *mut u32, wait: i32) -> i32;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }

    const FILE_LIST_DIRECTORY: u32 = 0x1;
    const SHARE_ALL: u32 = 0x1 | 0x2 | 0x4;
    const OPEN_EXISTING: u32 = 3;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OVERLAPPED: u32 = 0x4000_0000;
    const NOTIFY: u32 = 0x1 | 0x2 | 0x8 | 0x10 | 0x40; // file name, dir name, size, last write, creation
    const ACTION_MODIFIED: u32 = 3;

    fn run(path: &Path, scope: Scope, watch: &Watch) {
        let fail = || watch.state.store(FAILED, Ordering::Release);
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
        let directory = unsafe { CreateFileW(wide.as_ptr(), FILE_LIST_DIRECTORY, SHARE_ALL, std::ptr::null_mut(), OPEN_EXISTING, FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OVERLAPPED, std::ptr::null_mut()) };
        if directory.is_null() || directory as isize == -1 {
            return fail();
        }
        let event = unsafe { CreateEventW(std::ptr::null_mut(), 0, 0, std::ptr::null()) };
        if event.is_null() {
            unsafe { CloseHandle(directory) };
            return fail();
        }
        // DWORD-aligned, at most 64 KB (the limit for a tree on a network share)
        let mut buffer = vec![0u32; 16 * 1024];
        loop {
            let mut overlapped = Overlapped { internal: 0, internal_high: 0, offset: 0, offset_high: 0, event };
            let issued = unsafe { ReadDirectoryChangesW(directory, buffer.as_mut_ptr() as *mut c_void, (buffer.len() * 4) as u32, 1, NOTIFY, std::ptr::null_mut(), &mut overlapped, std::ptr::null_mut()) };
            if issued == 0 {
                break;
            }
            // Registered: changes from here on are reported
            watch.state.store(WATCHING, Ordering::Release);
            let mut length = 0u32;
            if unsafe { GetOverlappedResult(directory, &mut overlapped, &mut length, 1) } == 0 {
                break;
            }
            // No entries: the buffer overflowed, so some changes are unknown
            if length == 0 || counts(&buffer, length as usize, scope) {
                watch.changes.fetch_add(1, Ordering::AcqRel);
            }
        }
        unsafe {
            CloseHandle(event);
            CloseHandle(directory);
        }
        fail();
    }

    /// Whether the notifications in `buffer` include one `scope` counts
    fn counts(buffer: &[u32], length: usize, scope: Scope) -> bool {
        let bytes = unsafe { std::slice::from_raw_parts(buffer.as_ptr() as *const u8, length.min(buffer.len() * 4)) };
        let mut offset = 0usize;
        loop {
            let Some(header) = bytes.get(offset..offset + 12) else { return false };
            let field = |at: usize| u32::from_le_bytes(header[at..at + 4].try_into().unwrap());
            let (next, action, name_bytes) = (field(0) as usize, field(4), field(8) as usize);
            let name: Vec<u16> = bytes.get(offset + 12..offset + 12 + name_bytes).unwrap_or_default().chunks_exact(2).map(|pair| u16::from_le_bytes([pair[0], pair[1]])).collect();
            let name = String::from_utf16_lossy(&name);
            // What a listing of the tree skips
            let skipped = name.split(['\\', '/']).any(super::unlisted);
            if !skipped && (scope == Scope::Everything || action != ACTION_MODIFIED || super::mount_read(&name)) {
                return true;
            }
            if next == 0 {
                return false;
            }
            offset += next;
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::Scope;
    use std::path::Path;

    pub fn changes(_root: &Path, _scope: Scope) -> Option<u64> {
        None
    }
}

#[cfg(test)]
mod rule_tests {
    use super::mount_read;

    /// The files the engine reads as it mounts them count, each by its rule's lower-case suffix, and meta
    /// files, init.csv and default.project.json in any case; scripts and JSON modules (read at require) and
    /// suffixes no rule takes do not
    #[test]
    fn mount_read_takes_the_files_the_mount_reads() {
        for name in [
            "ReplicatedStorage\\Shared\\Greeting.txt",
            "a.b.txt",
            "Locale/en.csv",
            "Data/config.toml",
            "Data/table.yaml",
            "Data/table.yml",
            "Spawn.model.json",
            "Map.rbxm",
            "Map.rbxmx",
            "Nested/default.project.json",
            "Other.project.json",
            "Panel.meta.json",
            "init.meta.json",
            "INIT.META.JSON",
            "Up.Meta.json",
            "Folder/Init.csv",
            "Default.Project.json",
        ] {
            assert!(mount_read(name), "{name}");
        }
        for name in ["Module.luau", "Run.server.lua", "Data.json", "Notes.TXT", "Map.RBXMX", "Spawn.Model.json", "c.TOML", "Other.Project.json", "README.md", "csv"] {
            assert!(!mount_read(name), "{name}");
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn settles(root: &Path, scope: Scope, from: u64) -> u64 {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let now = changes(root, scope).unwrap();
            if now != from || Instant::now() > deadline {
                return now;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn structure_counts_entries_and_mount_files_not_scripts() {
        let root = std::env::temp_dir().join(format!("rur-watch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("a")).unwrap();
        std::fs::write(root.join("a/module.luau"), "return 1").unwrap();
        std::fs::write(root.join("a/init.meta.json"), "{}").unwrap();
        std::fs::write(root.join("a/greeting.txt"), "hello").unwrap();
        let start = changes(&root, Scope::Structure).expect("watched");
        let everything = changes(&root, Scope::Everything).expect("watched");
        // A script edit: Everything counts it, Structure does not
        std::fs::write(root.join("a/module.luau"), "return 2").unwrap();
        assert!(settles(&root, Scope::Everything, everything) > everything);
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(changes(&root, Scope::Structure), Some(start));
        // A meta file edit and a new directory count
        std::fs::write(root.join("a/init.meta.json"), "{\"className\":\"Folder\"}").unwrap();
        let after_meta = settles(&root, Scope::Structure, start);
        assert!(after_meta > start);
        // A text file's edit counts: the mount reads it into a StringValue
        std::fs::write(root.join("a/greeting.txt"), "hello again").unwrap();
        let after_text = settles(&root, Scope::Structure, after_meta);
        assert!(after_text > after_meta);
        std::fs::create_dir_all(root.join("a/b")).unwrap();
        assert!(settles(&root, Scope::Structure, after_text) > after_text);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn changes_below_a_dot_folder_count_but_not_below_git() {
        let root = std::env::temp_dir().join(format!("rur-watch-dots-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("Packages/.pesde")).unwrap();
        std::fs::create_dir_all(root.join(".git/objects")).unwrap();
        let start = changes(&root, Scope::Structure).expect("watched");
        // Git's own files: the engine lists nothing in a .git
        std::fs::create_dir_all(root.join(".git/objects/ab")).unwrap();
        std::fs::write(root.join(".git/objects/ab/cdef"), "x").unwrap();
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(changes(&root, Scope::Structure), Some(start));
        // A package pesde installs mounts
        std::fs::create_dir_all(root.join("Packages/.pesde/pkg")).unwrap();
        assert!(settles(&root, Scope::Structure, start) > start);
        let _ = std::fs::remove_dir_all(&root);
    }
}
