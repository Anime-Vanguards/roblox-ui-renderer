//! A project's places (docs/projects.md): named sets of source roots, read from the project's
//! `rur.json`, for runs that give no `--roots` (the desktop storybook's catalog and stories, the render
//! service's story listing and change watching).
//!
//! ```json
//! { "places": [ { "name": "client", "roots": ["shared", "client"] }, { "name": "server", "roots": ["shared", "server"] } ] }
//! ```
//!
//! Roots are relative to the project directory (absolute ones are kept as they are). The engine reads
//! its per-root settings from the same `rur.json` (`placeIds`, `modelAssets`, ...) keyed by the root
//! folder's name, so roots are usually direct children of the project. The first place is the default.
//! A project without `places` (or without a `rur.json`) has one place, `default`, whose root is the
//! project directory itself. cli/places.mjs reads the same file the same way.

use std::path::{Path, PathBuf};

/// The place a project declares without `places`
pub const DEFAULT_PLACE: &str = "default";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Place {
    pub name: String,
    pub roots: Vec<PathBuf>,
}

/// The project's places in the order `rur.json` lists them; never empty
pub fn places(project: &Path) -> Vec<Place> {
    let declared = std::fs::read_to_string(project.join("rur.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(text.trim_start_matches('\u{feff}')).ok())
        .and_then(|config| config.get("places").and_then(|places| places.as_array()).cloned())
        .unwrap_or_default();
    let mut result: Vec<Place> = Vec::new();
    for place in &declared {
        let Some(name) = place.get("name").and_then(|name| name.as_str()) else { continue };
        if name.is_empty() || result.iter().any(|known| known.name == name) {
            continue;
        }
        let roots = place
            .get("roots")
            .and_then(|roots| roots.as_array())
            .map(|roots| roots.iter().filter_map(|root| root.as_str()).filter(|root| !root.is_empty()).map(|root| project.join(root)).collect())
            .unwrap_or_default();
        result.push(Place { name: name.to_string(), roots });
    }
    if result.is_empty() {
        result.push(Place { name: DEFAULT_PLACE.into(), roots: vec![project.to_path_buf()] });
    }
    result
}

/// The place named `name`, if the project has it
pub fn place_named(project: &Path, name: &str) -> Option<Place> {
    places(project).into_iter().find(|place| place.name == name)
}

/// The place a file runs in: the first place with a root that holds it, else the default (first) place
pub fn place_of(project: &Path, file: &Path) -> Place {
    let mut places = places(project);
    let index = places.iter().position(|place| place.roots.iter().any(|root| file.starts_with(root))).unwrap_or(0);
    places.swap_remove(index)
}

/// Every root of the project once, with the first place that lists it (a root two places share
/// belongs to the first)
pub fn roots(project: &Path) -> Vec<(PathBuf, String)> {
    let mut result: Vec<(PathBuf, String)> = Vec::new();
    for place in places(project) {
        for root in place.roots {
            if !result.iter().any(|(known, _)| *known == root) {
                result.push((root, place.name.clone()));
            }
        }
    }
    result
}

/// The roots of `place` joined as `--roots` takes them
pub fn joined(place: &Place) -> String {
    place.roots.iter().map(|root| root.display().to_string()).collect::<Vec<_>>().join(";")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(config: Option<&str>) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "rur-places-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        if let Some(config) = config {
            std::fs::write(directory.join("rur.json"), config).unwrap();
        }
        directory
    }

    #[test]
    fn a_project_without_places_is_one_default_place_at_its_root() {
        let directory = project(None);
        assert_eq!(places(&directory), vec![Place { name: "default".into(), roots: vec![directory.clone()] }]);
        assert_eq!(place_of(&directory, &directory.join("a/b.story.luau")).name, "default");
        std::fs::write(directory.join("rur.json"), r#"{ "placeIds": {} }"#).unwrap();
        assert_eq!(places(&directory).len(), 1);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn places_keep_their_order_and_a_shared_root_belongs_to_the_first() {
        let directory = project(Some(
            "\u{feff}{ \"places\": [ { \"name\": \"client\", \"roots\": [\"shared\", \"client\"] }, { \"name\": \"server\", \"roots\": [\"shared\", \"server\"] }, { \"name\": \"client\", \"roots\": [] } ] }",
        ));
        let all = places(&directory);
        assert_eq!(all.iter().map(|place| place.name.as_str()).collect::<Vec<_>>(), ["client", "server"]);
        assert_eq!(all[1].roots, vec![directory.join("shared"), directory.join("server")]);
        assert_eq!(place_of(&directory, &directory.join("server/X.story.luau")).name, "server");
        assert_eq!(place_of(&directory, &directory.join("shared/X.story.luau")).name, "client");
        assert_eq!(place_of(&directory, &directory.join("elsewhere/X.story.luau")).name, "client");
        assert_eq!(place_named(&directory, "server").unwrap().roots.len(), 2);
        assert!(place_named(&directory, "missing").is_none());
        let roots = roots(&directory);
        assert_eq!(roots.iter().map(|(_, place)| place.as_str()).collect::<Vec<_>>(), ["client", "client", "server"]);
        assert_eq!(joined(&all[0]), format!("{};{}", directory.join("shared").display(), directory.join("client").display()));
        std::fs::remove_dir_all(directory).unwrap();
    }
}
