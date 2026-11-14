use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

/// How far one catalog scan may look before it gives up with an error rather than spinning on (a project
/// folder named wrongly, such as a whole drive)
#[derive(Clone, Copy, Debug)]
pub struct Budget {
    pub folders: usize,
    pub time: Duration,
}

impl Default for Budget {
    fn default() -> Self {
        Self { folders: 100_000, time: Duration::from_secs(30) }
    }
}

/// The folders a scan has read so far, for the storybook's progress line
pub type Progress = Arc<AtomicUsize>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoryEntry {
    pub file: PathBuf,
    pub place: String,
    pub key: String,
    pub category: String,
    pub name: String,
}

pub struct Catalog {
    pub entries: Vec<StoryEntry>,
    pub loading: bool,
    pub error: Option<String>,
    pub revision: u64,
    /// Folders read by the scan in flight
    pub folders: Progress,
    receiver: mpsc::Receiver<Result<Vec<StoryEntry>, String>>,
}

impl Catalog {
    pub fn load(project: PathBuf, roots: Option<String>) -> Self {
        let (sender, receiver) = mpsc::channel();
        let folders = Progress::default();
        let progress = folders.clone();
        thread::spawn(move || {
            let started = Instant::now();
            let result = scan_within(&project, roots.as_deref(), Budget::default(), &progress);
            match &result {
                Ok(entries) => eprintln!(
                    "rur: storybook: {} stories in {} ({} folders, {:.1} s)",
                    entries.len(),
                    project.display(),
                    progress.load(Ordering::Relaxed),
                    started.elapsed().as_secs_f64()
                ),
                Err(error) => eprintln!("rur: storybook: {error}"),
            }
            let _ = sender.send(result);
        });
        Self {
            entries: Vec::new(),
            loading: true,
            error: None,
            revision: 0,
            folders,
            receiver,
        }
    }

    pub fn poll(&mut self) {
        match self.receiver.try_recv() {
            Ok(Ok(entries)) => {
                self.entries = entries;
                self.revision = self.revision.wrapping_add(1);
                self.loading = false;
            }
            Ok(Err(error)) => {
                self.error = Some(error);
                self.loading = false;
            }
            Err(mpsc::TryRecvError::Disconnected) if self.loading => {
                self.error = Some("Story discovery stopped unexpectedly".into());
                self.loading = false;
            }
            _ => {}
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CatalogRow {
    Category(usize),
    Story(usize),
}

#[derive(Default)]
pub(crate) struct CatalogView {
    revision: Option<u64>,
    search: String,
    place: String,
    searchable: Vec<String>,
    pub rows: Vec<CatalogRow>,
    pub story_count: usize,
}

impl CatalogView {
    /// An unchanged catalog/filter takes no scan or string allocation on a display frame.
    pub fn update(
        &mut self,
        entries: &[StoryEntry],
        revision: u64,
        search: &str,
        place: &str,
    ) -> bool {
        if self.revision == Some(revision) && self.search == search && self.place == place {
            return false;
        }
        if self.revision != Some(revision) {
            self.searchable = entries
                .iter()
                .map(|entry| entry.key.to_lowercase())
                .collect();
        }
        self.rows.clear();
        self.story_count = 0;
        let query = search.to_lowercase();
        let mut category = None;
        for (index, entry) in entries.iter().enumerate() {
            if (place != "all" && place != entry.place) || !self.searchable[index].contains(&query)
            {
                continue;
            }
            if category != Some(&entry.category) {
                self.rows.push(CatalogRow::Category(index));
                category = Some(&entry.category);
            }
            self.rows.push(CatalogRow::Story(index));
            self.story_count += 1;
        }
        self.revision = Some(revision);
        self.search.clear();
        self.search.push_str(search);
        self.place.clear();
        self.place.push_str(place);
        true
    }
}

pub fn scan(project: &Path, roots: Option<&str>) -> Result<Vec<StoryEntry>, String> {
    scan_within(project, roots, Budget::default(), &Progress::default())
}

/// [`scan`], giving up with an error once it has read `budget.folders` folders or spent `budget.time`
pub fn scan_within(project: &Path, roots: Option<&str>, budget: Budget, progress: &Progress) -> Result<Vec<StoryEntry>, String> {
    // Explicit roots belong to the project's default place; else every root of every place (src/places.rs)
    let roots: Vec<(PathBuf, String)> = match roots {
        Some(roots) => {
            let place = crate::places::places(project).swap_remove(0).name;
            roots.split(';').filter(|root| !root.is_empty()).map(|root| (PathBuf::from(root), place.clone())).collect()
        }
        None => crate::places::roots(project),
    };
    let mut files = BTreeMap::new();
    let mut walk = Walk { budget, progress, started: Instant::now(), files: &mut files, stopped: None };
    for (root, place) in roots {
        if root.exists() {
            walk.directory(&root, &root, &place)?;
        }
    }
    let mut entries: Vec<_> = files
        .into_iter()
        .map(|(file, place)| {
            let key = file
                .strip_prefix(project)
                .unwrap_or(&file)
                .to_string_lossy()
                .replace('\\', "/");
            let name = file
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .trim_end_matches(".luau")
                .trim_end_matches(".lua")
                .trim_end_matches(".story")
                .to_string();
            let category = key
                .rsplit_once('/')
                .map(|(dir, _)| dir.to_owned())
                .unwrap_or_else(|| place.clone());
            StoryEntry {
                file,
                place,
                key,
                name,
                category,
            }
        })
        .collect();
    entries.sort_by(|left, right| left.key.to_lowercase().cmp(&right.key.to_lowercase()));
    Ok(entries)
}

struct Walk<'a> {
    budget: Budget,
    progress: &'a Progress,
    started: Instant,
    files: &'a mut BTreeMap<PathBuf, String>,
    /// Set once the budget runs out: the whole scan stops with this error
    stopped: Option<String>,
}

impl Walk<'_> {
    fn directory(&mut self, root: &Path, directory: &Path, place: &str) -> Result<(), String> {
        let read = self.progress.fetch_add(1, Ordering::Relaxed) + 1;
        if read > self.budget.folders || self.started.elapsed() > self.budget.time {
            let error = format!(
                "Stopped looking for stories after {read} folders and {:.0} s under {}. Is that the project folder? Open the folder that holds your stories or its default.project.json (rur-native <project folder>)",
                self.started.elapsed().as_secs_f64(),
                root.display()
            );
            self.stopped = Some(error.clone());
            return Err(error);
        }
        for entry in fs::read_dir(directory).map_err(|error| format!("{}: {error}", directory.display()))? {
            let entry = entry.map_err(|error| error.to_string())?;
            let kind = entry.file_type().map_err(|error| error.to_string())?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if kind.is_dir() {
                // Dot folders (.git, editor and tool state) and node_modules hold no stories, as `rur stories` skips them
                // in a Rojo project
                if name.starts_with('.') || name == "node_modules" {
                    continue;
                }
                // An unreadable subdirectory holds no story we could load anyway; skipping it keeps the rest
                // of the catalog (Windows cannot open synced folders whose names end in "." by path). Running out of
                // budget stops the whole scan.
                let _ = self.directory(root, &entry.path(), place);
                if let Some(error) = &self.stopped {
                    return Err(error.clone());
                }
            } else if kind.is_file() && (name.ends_with(".story.luau") || name.ends_with(".story.lua")) {
                self.files.entry(entry.path()).or_insert_with(|| place.to_string());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn story(place: &str, category: &str, name: &str) -> StoryEntry {
        let key = format!("{category}/{name}.story.luau");
        StoryEntry {
            file: PathBuf::from(&key),
            place: place.into(),
            key,
            category: category.into(),
            name: name.into(),
        }
    }

    #[test]
    fn filtered_catalog_keeps_matching_categories_and_original_story_indices() {
        let entries = vec![
            story("game", "game/Combat", "Damage"),
            story("lobby", "lobby/Inventory", "Cards"),
            story("lobby", "lobby/Inventory", "Voyage"),
            story("lobby", "shared/Widgets", "VOYAGE Button"),
        ];
        let mut view = CatalogView::default();
        assert!(view.update(&entries, 1, "voyage", "lobby"));
        assert_eq!(view.story_count, 2);
        assert_eq!(
            view.rows,
            [
                CatalogRow::Category(2),
                CatalogRow::Story(2),
                CatalogRow::Category(3),
                CatalogRow::Story(3)
            ]
        );
        assert!(!view.update(&entries, 1, "voyage", "lobby"));
        assert!(view.update(&entries, 1, "", "game"));
        assert_eq!(view.rows, [CatalogRow::Category(0), CatalogRow::Story(0)]);
        assert!(view.update(&entries, 1, "inventory", "all"));
        assert_eq!(
            view.rows,
            [
                CatalogRow::Category(1),
                CatalogRow::Story(1),
                CatalogRow::Story(2)
            ]
        );
    }

    #[test]
    fn catalog_revision_invalidates_cached_search_even_when_entry_count_is_unchanged() {
        let mut entries = vec![story("lobby", "UI", "Voyage")];
        let mut view = CatalogView::default();
        view.update(&entries, 1, "voyage", "all");
        entries[0] = story("lobby", "UI", "Inventory");
        assert!(view.update(&entries, 2, "voyage", "all"));
        assert!(view.rows.is_empty());
        assert_eq!(view.story_count, 0);
        assert!(view.update(&entries, 2, "inventory", "all"));
        assert_eq!(view.story_count, 1);
    }

    fn project() -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "rur-catalog-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        for dir in ["src/Stories", ".git/hooks", "node_modules/pkg", "Packages/Lib"] {
            fs::create_dir_all(directory.join(dir)).unwrap();
        }
        for file in ["src/Stories/Button.story.luau", ".git/hooks/Hook.story.luau", "node_modules/pkg/Dep.story.luau", "Packages/Lib/Lib.story.lua"] {
            fs::write(directory.join(file), "return {}").unwrap();
        }
        directory
    }

    #[test]
    fn a_project_lists_its_stories_without_dot_folders_or_node_modules() {
        let directory = project();
        let entries = scan(&directory, None).unwrap();
        let keys: Vec<&str> = entries.iter().map(|entry| entry.key.as_str()).collect();
        assert_eq!(keys, ["Packages/Lib/Lib.story.lua", "src/Stories/Button.story.luau"]);
        fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn a_scan_that_runs_out_of_folders_stops_with_an_error_instead_of_spinning_on() {
        let directory = project();
        let progress = Progress::default();
        let budget = Budget { folders: 3, time: Duration::from_secs(60) };
        let error = scan_within(&directory, None, budget, &progress).unwrap_err();
        assert!(error.starts_with("Stopped looking for stories after 4 folders"), "{error}");
        assert!(error.contains("rur-native <project folder>"), "{error}");
        assert_eq!(progress.load(Ordering::Relaxed), 4);
        let error = scan_within(&directory, None, Budget { folders: 1000, time: Duration::ZERO }, &Progress::default()).unwrap_err();
        assert!(error.starts_with("Stopped looking for stories"), "{error}");
        fs::remove_dir_all(&directory).unwrap();
    }
}
