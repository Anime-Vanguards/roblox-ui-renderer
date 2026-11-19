//! What a render reports about itself (summary.json, and what `rur diff` and `rur sweep` read): the scene's status, its
//! counts, the images and fonts it lacked, and the scene diagnostics, which are what the render left out or could not
//! draw. The Node CLI's `rur render` writes the same summary (cli/rur.mjs render, cli/diagnostics.mjs).
//!
//!   culled nodes     `culled: true` (export.rs): Studio's AutomaticSize culling (an AutomaticSize object whose centre
//!                    is not strictly inside its clip is not drawn, nor its subtree), or a rotated node wholly outside
//!                    its clip
//!   dropped lines    `text.droppedLines`: wrapped lines the box height left out
//!   missing glyphs   glyph id 0 (.notdef, drawn as an empty box): a character in neither the label's font nor any
//!                    fallback font
//!   missing fonts    diagnostics.missingFontAssets (cloud fonts not downloaded) and diagnostics.unavailableFonts
//!                    (families with no face to draw with, not even the Source Sans Pro fallback: no Studio fonts;
//!                    their labels draw no text)
//!   script output    diagnostics.output (print / warn / error)
//!   paths            instance names holding "." (or "[", "]", '"') are written Holder["Item.Name"].Label
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use crate::{engine::text::opentype::Font, js::Json};

/// The fonts whose glyphs a label falls back to, by the start of their file names
const FALLBACK_FILES: [&str; 8] = [
    "sourcesanspro-regular",
    "arimo-regular",
    "robloxemoji",
    "twemojimozilla",
    "notosansthaiui-regular",
    "notonaskharabicui-regular",
    "notosansdevanagariui-regular",
    "123924484074402",
];

/// The scene's per-node notes (by node index), unambiguous paths (by node index) and the summary's diagnostics
pub struct Diagnostics {
    pub notes: HashMap<usize, Vec<String>>,
    pub paths: Vec<String>,
    pub summary: Json,
}

/// A name that a dotted path cannot hold as it is
fn needs_quote(name: &str) -> bool {
    name.is_empty() || name.contains(['.', '[', ']', '"'])
}

fn segment(name: &str, first: bool) -> String {
    if needs_quote(name) {
        format!("[{}]", serde_json::to_string(name).expect("a string"))
    } else if first {
        name.to_string()
    } else {
        format!(".{name}")
    }
}

/// The index of every node by its id (a parent's reference)
fn by_id(nodes: &[Json]) -> HashMap<String, usize> {
    let mut map = HashMap::new();
    for (index, node) in nodes.iter().enumerate() {
        // (a repeated id is the last node's, as a Map built from the list is)
        map.insert(node.get("id").stringify(0), index);
    }
    map
}

fn parent_of(node: &Json, ids: &HashMap<String, usize>) -> Option<usize> {
    let parent = node.get("parent");
    if matches!(parent, Json::Null) && !node.has("parent") {
        return None;
    }
    ids.get(&parent.stringify(0)).copied()
}

/// Unambiguous paths for every node: names holding dots are quoted, the rest as in `node.path`; the engine's prefix
/// above the exported roots (e.g. "CoreGui.") is kept
pub fn node_paths(nodes: &[Json], ids: &HashMap<String, usize>) -> Vec<String> {
    let mut paths: Vec<Option<String>> = vec![None; nodes.len()];
    for start in 0..nodes.len() {
        // The chain of nodes without a path yet, walked up to a known one or a root
        let mut chain = Vec::new();
        let mut at = Some(start);
        while let Some(index) = at {
            if paths[index].is_some() || chain.contains(&index) {
                break;
            }
            chain.push(index);
            at = parent_of(&nodes[index], ids);
        }
        for &index in chain.iter().rev() {
            let node = &nodes[index];
            let name = node.get("name").text();
            let path = match parent_of(node, ids).and_then(|parent| paths[parent].clone()) {
                Some(parent) => parent + &segment(&name, false),
                None => {
                    let path = node.get("path");
                    let raw = if path.truthy() { path.text() } else { name.clone() };
                    let prefix = raw.strip_suffix(name.as_str()).unwrap_or("");
                    if needs_quote(&name) {
                        format!("{}{}", prefix.strip_suffix('.').unwrap_or(prefix), segment(&name, prefix.is_empty()))
                    } else {
                        raw
                    }
                }
            };
            paths[index] = Some(path);
        }
    }
    paths.into_iter().map(Option::unwrap_or_default).collect()
}

/// Why a culled node is not drawn: (whether it is the culled root, the reason)
fn cull_reason(node: &Json, nodes: &[Json], ids: &HashMap<String, usize>) -> Option<(bool, &'static str)> {
    if !node.get("culled").truthy() {
        return None;
    }
    if parent_of(node, ids).is_some_and(|parent| nodes[parent].get("culled").truthy()) {
        return Some((false, "inside a culled ancestor"));
    }
    if node.get("rotation").truthy() {
        return Some((true, "rotated, wholly outside its clip"));
    }
    let rect = node.get("rect").items();
    let clip = node.get("clip");
    if clip.truthy() && !rect.is_empty() {
        let number = |value: Option<&Json>| value.and_then(Json::as_f64).unwrap_or(f64::NAN);
        let [x, y, w, h] = [0, 1, 2, 3].map(|index| number(rect.get(index)));
        let [cx0, cy0, cw, ch] = [0, 1, 2, 3].map(|index| number(clip.items().get(index)));
        let (cx, cy) = (x + w / 2.0, y + h / 2.0);
        let inside = cx > cx0 && cx < cx0 + cw && cy > cy0 && cy < cy0 + ch;
        if !inside {
            return Some((true, "AutomaticSize, centre outside clip"));
        }
    }
    // A text object's own AutomaticSize culling hides its descendants but still draws its text
    Some((true, "AutomaticSize culling of its parent"))
}

/// A font file's name without its extension, as a label names its font
fn font_label(entry: &Json) -> String {
    let file = entry.get("file");
    if !file.truthy() {
        return "unknown font".into();
    }
    let file = file.text();
    let name = file.rsplit(['/', '\\']).next().unwrap_or(&file);
    let lower = name.to_ascii_lowercase();
    match [".ttf", ".otf", ".ttc"].iter().find(|extension| lower.ends_with(*extension)) {
        Some(extension) => name[..name.len() - extension.len()].to_string(),
        None => name.to_string(),
    }
}

/// The fonts read so far, by file (None: a file that could not be read or parsed, such as a collection)
#[derive(Default)]
struct FontCache(HashMap<String, Option<Font>>);

impl FontCache {
    fn font(&mut self, file: &str) -> Option<&Font> {
        self.0
            .entry(file.to_string())
            .or_insert_with(|| {
                let data = std::fs::read(file).ok()?;
                // (a font collection is not read: it names its faces in a header of its own)
                if data.starts_with(b"ttcf") {
                    return None;
                }
                Font::parse(data, file).ok()
            })
            .as_ref()
    }
}

/// Whether `font` maps `c` to a glyph (glyph 0 is .notdef)
fn has_char(font: &Font, c: char) -> bool {
    font.cmap.get(&(c as u32)).is_some_and(|glyph| *glyph > 0)
}

/// The text a rich-text label shows: tags removed and the five escapes read
fn plain_rich_text(text: &str) -> String {
    let mut plain = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('<') {
        plain.push_str(&rest[..open]);
        match rest[open..].find('>') {
            Some(close) => rest = &rest[open + close + 1..],
            None => {
                plain.push_str(&rest[open..]);
                rest = "";
            }
        }
    }
    plain.push_str(rest);
    plain.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

/// One missing character of a label: (char, `U+XXXX`, font label), or (None, None, font label) with the .notdef count
/// when the font could not be read or names none of them
type Missing = (Option<char>, Option<String>, String, usize);

/// The characters a text node draws as .notdef; empty when it draws none
fn node_missing_glyphs(node: &Json, fonts: &[Json], cache: &mut FontCache) -> Vec<Missing> {
    let text = node.get("text");
    let notdef: Vec<&Json> = text
        .get("lines")
        .items()
        .iter()
        .flat_map(|line| line.get("glyphs").items())
        .filter(|glyph| glyph.items().get(1) == Some(&Json::Number(0.0)))
        .map(|glyph| glyph.items().first().unwrap_or(&Json::Null))
        .collect();
    if notdef.is_empty() {
        return Vec::new();
    }
    let primary = notdef[0].as_f64().filter(|index| *index >= 0.0 && index.fract() == 0.0).and_then(|index| fonts.get(index as usize)).unwrap_or(&Json::Null);
    let label = font_label(primary);
    let shown = node.get("props").get("text");
    let mut string = if matches!(shown, Json::Null) { String::new() } else { shown.text() };
    if text.get("rich").truthy() {
        string = plain_rich_text(&string);
    }
    let fallback_files: Vec<String> = fonts
        .iter()
        .filter(|entry| entry.get("file").truthy())
        .map(|entry| entry.get("file").text())
        .filter(|file| {
            let name = file.rsplit(['/', '\\']).next().unwrap_or(file).to_lowercase();
            FALLBACK_FILES.iter().any(|fallback| name.contains(fallback))
        })
        .collect();
    let primary_file = primary.get("file");
    let mut missing = Vec::new();
    if primary_file.truthy() && cache.font(&primary_file.text()).is_some() {
        let mut seen = std::collections::HashSet::new();
        for c in string.chars() {
            if crate::js::is_space(c) || !seen.insert(c) {
                continue;
            }
            if has_char(cache.font(&primary_file.text()).unwrap(), c) {
                continue;
            }
            if fallback_files.iter().any(|file| cache.font(file).is_some_and(|font| has_char(font, c))) {
                continue;
            }
            missing.push((Some(c), Some(format!("U+{:04X}", c as u32)), label.clone(), 0));
        }
    }
    if missing.is_empty() {
        missing.push((None, None, label, notdef.len()));
    }
    missing
}

/// The summary's `missingFonts` from a scene's diagnostics (cli/diagnostics.mjs missingFonts): the cloud font assets
/// that are not downloaded, then the font families whose text is not drawn at all (no face of them nor the Source
/// Sans Pro fallback could be read, as on a machine without Studio's fonts)
pub fn missing_fonts(diagnostics: &Json) -> Vec<Json> {
    let mut fonts = diagnostics.get("missingFontAssets").items().to_vec();
    for family in diagnostics.get("unavailableFonts").items() {
        if !fonts.contains(family) {
            fonts.push(family.clone());
        }
    }
    fonts
}

/// The scene's diagnostics: per-node notes for `inspect`-style descriptions, unambiguous paths, and the summary's
/// `culled`, `textLinesDropped`, `missingGlyphs` and `output` (cli/diagnostics.mjs sceneDiagnostics)
pub fn scene_diagnostics(scene: &Json) -> Diagnostics {
    const EXAMPLES: usize = 5;
    const OUTPUT_LINES: usize = 20;
    let nodes = scene.get("nodes").items();
    let ids = by_id(nodes);
    let paths = node_paths(nodes, &ids);
    let fonts = scene.get("fonts").items();
    // (a label with text and no lines draws nothing when some family had no face to draw with)
    let unavailable = scene.get("diagnostics").get("unavailableFonts").items();
    let mut notes: HashMap<usize, Vec<String>> = HashMap::new();
    let mut culled_roots = Vec::new();
    let mut culled_nodes = 0;
    let mut dropped = Vec::new();
    // Missing glyphs by "code|font": (char, code, font, labels, example paths)
    let mut glyphs: Vec<(String, Json, Json, String, usize, Vec<String>)> = Vec::new();
    let mut cache = FontCache::default();
    for (index, node) in nodes.iter().enumerate() {
        let mut note = |text: String| notes.entry(index).or_default().push(text);
        if let Some((root, reason)) = cull_reason(node, nodes, &ids) {
            culled_nodes += 1;
            note(format!("culled, not drawn: {reason}"));
            if root {
                culled_roots.push(format!("{} ({reason})", paths[index]));
            }
        }
        let text = node.get("text");
        let dropped_lines = text.get("droppedLines").as_f64().unwrap_or(0.0);
        if dropped_lines > 0.0 {
            let shown = text.get("lines").items().len();
            let count = crate::glyphs::js_number_string(dropped_lines);
            note(format!("{count} wrapped line{} dropped ({shown} shown; box too short)", if dropped_lines == 1.0 { "" } else { "s" }));
            dropped.push(format!("{} ({count} of {} lines dropped)", paths[index], crate::glyphs::js_number_string(shown as f64 + dropped_lines)));
        }
        if text.truthy() && !unavailable.is_empty() && text.get("lines").items().is_empty() && dropped_lines == 0.0 {
            let shown = node.get("props").get("text");
            if !matches!(shown, Json::Null) && !shown.text().chars().all(crate::js::is_space) {
                note(format!("text not drawn: no face of its font to draw with (missingFonts: {})", unavailable.iter().map(Json::text).collect::<Vec<_>>().join(", ")));
            }
        }
        if text.truthy() {
            for (c, code, font, count) in node_missing_glyphs(node, fonts, &mut cache) {
                note(match (c, &code) {
                    (Some(c), Some(code)) => format!("missing glyph '{c}' {code} in {font}"),
                    _ => format!("{count} missing glyph(s) in {font}"),
                });
                let key = format!("{}|{font}", code.as_deref().unwrap_or("null"));
                let position = match glyphs.iter().position(|group| group.0 == key) {
                    Some(position) => position,
                    None => {
                        glyphs.push((key, c.map(String::from).into(), code.into(), font, 0, Vec::new()));
                        glyphs.len() - 1
                    }
                };
                let group = &mut glyphs[position];
                group.4 += 1;
                if group.5.len() < EXAMPLES {
                    group.5.push(paths[index].clone());
                }
            }
        }
    }
    let output = scene.get("diagnostics").get("output").items();
    let lines: Vec<Json> = output
        .iter()
        .take(OUTPUT_LINES)
        .map(|entry| {
            let message = entry.get("message");
            let message = if matches!(message, Json::Null) { String::new() } else { message.text() };
            let first = message.split('\n').next().unwrap_or("");
            let first = first.strip_suffix('\r').unwrap_or(first);
            Json::from(format!("{}: {}", entry.get("kind").text(), crate::js::slice_start(first, 300)))
        })
        .collect();
    let summary = Json::object()
        .with("culled", Json::object().with("count", culled_roots.len()).with("nodes", culled_nodes as usize).with("examples", culled_roots.iter().take(EXAMPLES).cloned().collect::<Vec<_>>()))
        .with("textLinesDropped", Json::object().with("count", dropped.len()).with("examples", dropped.iter().take(EXAMPLES).cloned().collect::<Vec<_>>()))
        .with(
            "missingGlyphs",
            glyphs
                .into_iter()
                .map(|(_, c, code, font, labels, paths)| Json::object().with("char", c).with("code", code).with("font", font).with("labels", labels).with("paths", paths))
                .collect::<Vec<_>>(),
        )
        .with("output", Json::object().with("total", output.len()).with("lines", lines));
    Diagnostics { notes, paths, summary }
}

/// What a render wrote, as summary.json holds it (cli/rur.mjs render): `result` is what `rur-native render` printed,
/// `seconds` the render's time; `git_ref` and the result's profile are added when there are any
pub fn render_summary(out: &Path, png: bool, result: &Json, seconds: f64, git_ref: Option<&Json>) -> Result<Json, String> {
    let file = out.join("scene.json");
    let scene = Json::read(&file).ok_or_else(|| format!("{}: no scene", file.display()))?;
    let manifest = Json::read(&out.join("manifest.json")).unwrap_or_else(|| Json::object().with("images", Json::object()));
    let missing_images: Vec<String> = manifest
        .get("images")
        .entries()
        .iter()
        .filter(|(_, entry)| entry.get("error").truthy())
        .map(|(uri, entry)| format!("{uri}: {}", entry.get("error").text()))
        .collect();
    let diagnostics = scene.get("diagnostics");
    let output = diagnostics.get("output").items();
    let count = |kind: &str| output.iter().filter(|entry| entry.get("kind").as_str() == Some(kind)).count();
    let mut summary = Json::object();
    if scene.has("status") {
        summary.set("status", scene.get("status").clone());
    }
    let failure = scene.get("failure");
    summary.set("failure", if failure.truthy() { failure.clone() } else { Json::Null });
    // What to mount when the failure reads a missing child of a service (hints.rs)
    if diagnostics.get("hint").truthy() {
        summary.set("hint", diagnostics.get("hint").clone());
    }
    summary.set("out", out);
    summary.set("png", png.then(|| out.join("render.png").display().to_string()));
    summary.set("nodes", scene.get("nodes").items().len());
    if scene.has("viewport") {
        summary.set("viewport", scene.get("viewport").clone());
    }
    summary.set("scriptErrors", count("error"));
    summary.set("warnings", count("warn"));
    summary.set("unemulated", diagnostics.get("unemulated").items().iter().map(|entry| entry.get("member").clone()).collect::<Vec<_>>());
    summary.set("missingImages", missing_images);
    summary.set("missingFonts", missing_fonts(diagnostics));
    for (key, value) in scene_diagnostics(&scene).summary.entries() {
        summary.set(key, value.clone());
    }
    summary.set("renderErrors", result.get("renderErrors").items().to_vec());
    summary.set("seconds", seconds);
    let phases = result.get("phases");
    summary.set("phases", if phases.truthy() { phases.clone() } else { Json::object() });
    if let Some(git_ref) = git_ref {
        summary.set("gitRef", git_ref.clone());
    }
    if result.get("profile").truthy() {
        summary.set("profile", result.get("profile").clone());
    }
    Ok(summary)
}

/// Writes summary.json into the render's directory (two-space indentation, as `JSON.stringify(summary, null, 2)`)
pub fn write_summary(out: &Path, summary: &Json) -> Result<PathBuf, String> {
    let file = out.join("summary.json");
    std::fs::write(&file, summary.stringify(2)).map_err(|error| format!("{}: {error}", file.display()))?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostics_name_culled_nodes_dropped_lines_and_quoted_paths() {
        let scene = Json::parse(
            r#"{"nodes": [
                {"id": 1, "name": "Gui", "path": "CoreGui.Gui", "rect": [0, 0, 100, 100]},
                {"id": 2, "parent": 1, "name": "Item.Name", "path": "CoreGui.Gui.Item.Name", "rect": [0, 0, 10, 10]},
                {"id": 3, "parent": 2, "name": "Row", "culled": true, "rect": [200, 0, 10, 10], "clip": [0, 0, 100, 100]},
                {"id": 4, "parent": 3, "name": "Inner", "culled": true, "rect": [200, 0, 10, 10]},
                {"id": 5, "parent": 1, "name": "Label", "rect": [0, 0, 10, 10], "text": {"droppedLines": 2, "lines": [{"glyphs": []}]}},
                {"id": 6, "name": "Odd.Root", "path": "CoreGui.Odd.Root", "rect": [0, 0, 1, 1]}
            ], "diagnostics": {"output": [{"kind": "print", "message": "hello\nsecond"}, {"kind": "warn", "message": "careful"}]}}"#,
        )
        .unwrap();
        let diagnostics = scene_diagnostics(&scene);
        assert_eq!(diagnostics.paths, ["CoreGui.Gui", "CoreGui.Gui[\"Item.Name\"]", "CoreGui.Gui[\"Item.Name\"].Row", "CoreGui.Gui[\"Item.Name\"].Row.Inner", "CoreGui.Gui.Label", "CoreGui[\"Odd.Root\"]"]);
        assert_eq!(
            diagnostics.summary.stringify(0),
            concat!(
                r#"{"culled":{"count":1,"nodes":2,"examples":["CoreGui.Gui[\"Item.Name\"].Row (AutomaticSize, centre outside clip)"]},"#,
                r#""textLinesDropped":{"count":1,"examples":["CoreGui.Gui.Label (2 of 3 lines dropped)"]},"missingGlyphs":[],"#,
                r#""output":{"total":2,"lines":["print: hello","warn: careful"]}}"#
            )
        );
        assert_eq!(diagnostics.notes[&3], ["culled, not drawn: inside a culled ancestor"]);
        assert_eq!(diagnostics.notes[&4], ["2 wrapped lines dropped (1 shown; box too short)"]);
    }

    #[test]
    fn a_label_without_a_readable_font_counts_its_missing_glyphs() {
        let scene = Json::parse(
            r#"{"fonts": [{"file": "missing/Font-Regular.ttf"}], "nodes": [
                {"id": 1, "name": "Label", "rect": [0, 0, 1, 1], "props": {"text": "<b>a&amp;b</b>"}, "text": {"rich": true, "lines": [{"glyphs": [[0, 0], [0, 5], [0, 0]]}]}}
            ]}"#,
        )
        .unwrap();
        assert_eq!(scene_diagnostics(&scene).summary.get("missingGlyphs").stringify(0), r#"[{"char":null,"code":null,"font":"Font-Regular","labels":1,"paths":["Label"]}]"#);
        assert_eq!(plain_rich_text("<b>a&amp;lt;</b> &lt;c&gt;"), "a&lt; <c>");
    }

    /// A family with no face to draw with (no Studio fonts) is a missing font, after the cloud assets, and a label in it
    /// that has text but no lines says why it draws nothing
    #[test]
    fn fonts_with_no_face_are_missing_fonts_and_their_labels_say_so() {
        let diagnostics = Json::parse(r#"{"missingFontAssets": ["123"], "unavailableFonts": ["rbxasset://fonts/families/Ubuntu.json", "123"]}"#).unwrap();
        assert_eq!(Json::Array(missing_fonts(&diagnostics)).stringify(0), r#"["123","rbxasset://fonts/families/Ubuntu.json"]"#);
        assert_eq!(missing_fonts(&Json::object()).len(), 0);
        let scene = Json::parse(
            r#"{"nodes": [
                {"id": 1, "name": "Button", "rect": [0, 0, 1, 1], "props": {"text": "Play"}, "text": {"lines": {}}},
                {"id": 2, "name": "Empty", "rect": [0, 0, 1, 1], "props": {"text": " "}, "text": {"lines": {}}}
            ], "diagnostics": {"unavailableFonts": ["rbxasset://fonts/families/Ubuntu.json"]}}"#,
        )
        .unwrap();
        let notes = scene_diagnostics(&scene).notes;
        assert_eq!(notes[&0], ["text not drawn: no face of its font to draw with (missingFonts: rbxasset://fonts/families/Ubuntu.json)"]);
        assert!(!notes.contains_key(&1), "{notes:?}");
    }
}
