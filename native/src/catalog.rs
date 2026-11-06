use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
};

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
    receiver: mpsc::Receiver<Result<Vec<StoryEntry>, String>>,
}

impl Catalog {
    pub fn load(project: PathBuf, roots: Option<String>) -> Self {
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let _ = sender.send(scan(&project, roots.as_deref()));
        });
        Self {
            entries: Vec::new(),
            loading: true,
            error: None,
            revision: 0,
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
    // Explicit roots belong to the project's default place; else every root of every place (src/places.rs)
    let roots: Vec<(PathBuf, String)> = match roots {
        Some(roots) => {
            let place = crate::places::places(project).swap_remove(0).name;
            roots.split(';').filter(|root| !root.is_empty()).map(|root| (PathBuf::from(root), place.clone())).collect()
        }
        None => crate::places::roots(project),
    };
    let mut files = BTreeMap::new();
    for (root, place) in roots {
        if root.exists() {
            walk(&root, &place, &mut files)?;
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

fn walk(
    directory: &Path,
    place: &str,
    files: &mut BTreeMap<PathBuf, String>,
) -> Result<(), String> {
    for entry in
        fs::read_dir(directory).map_err(|error| format!("{}: {error}", directory.display()))?
    {
        let entry = entry.map_err(|error| error.to_string())?;
        let kind = entry.file_type().map_err(|error| error.to_string())?;
        if kind.is_dir() {
            // An unreadable subdirectory holds no story we could load anyway; skipping it keeps the rest
            // of the catalog (Windows cannot open synced folders whose names end in "." by path).
            let _ = walk(&entry.path(), place, files);
        } else if kind.is_file() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.ends_with(".story.luau") || name.ends_with(".story.lua") {
                files
                    .entry(entry.path())
                    .or_insert_with(|| place.to_string());
            }
        }
    }
    Ok(())
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
}
