//! Regression test of the text engine (native/src/engine/text) against golden output: replays
//! `out/golden/text-corpus.json` (85 399 cases recorded from the Luau reference implementation
//! runtime/text, which the text engine ported bit-exactly, before that runtime was retired on
//! 2026-09-27; see notes/rust-engine.md "Text API") and requires every output to be identical
//! (numbers bit-exact). Override the corpus with RUR_TEXT_CORPUS. Skips (with a message) when the
//! corpus is not present, since it needs the local Studio fonts.
//!
//! The expected output depends on the font files, so the corpus is replayed with the frozen font set
//! beside it (`out/golden/fonts`, written by `node tools/engine/golden.mjs --freeze-fonts`; override
//! with RUR_TEXT_CORPUS_FONTS): Studio updates renumber BuilderIcons' glyphs about monthly and a
//! cleared asset cache drops cloud faces (substituted ones then measure differently). Without that set
//! it uses the installed Studio's and the asset cache's fonts and names the recorded ones missing.
//!
//! The text module is compiled into this test directly so the check does not depend on the rest of
//! the engine building.

#[allow(dead_code, unused_imports)]
#[path = "../src/engine/text/mod.rs"]
mod text;

// The text module (compiled here by path) resolves the state directory through the crate root
pub fn state_dir(repo: &std::path::Path) -> std::path::PathBuf {
    rur_native::state_dir(repo)
}
pub fn shared_state_path(repo: &std::path::Path, relative: &str) -> std::path::PathBuf {
    rur_native::shared_state_path(repo, relative)
}
// ... and generates hinted metrics with the crate's FreeType
pub use rur_native::glyphs;
// ... and counts its caches with the crate's counters (a profile's `caches`)
pub use rur_native::cache_counts;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{Map, Value, json};
use text::{FontPaths, LayoutParams, SizeConstraint, TextEngine, TextLayout, TextObject, Truncate, XAlign};

fn number(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => match text.as_str() {
            "inf" => Some(f64::INFINITY),
            "-inf" => Some(f64::NEG_INFINITY),
            "nan" => Some(f64::NAN),
            _ => None,
        },
        _ => None,
    }
}

fn n(value: f64) -> Value {
    if value.is_nan() {
        json!("nan")
    } else if value == f64::INFINITY {
        json!("inf")
    } else if value == f64::NEG_INFINITY {
        json!("-inf")
    } else {
        json!(value)
    }
}

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len()).step_by(2).map(|index| u8::from_str_radix(&text[index..index + 2], 16).unwrap()).collect()
}

/// A text field stored as `key` (UTF-8) or `key + "h"` (hex bytes).
fn text_of(object: &Map<String, Value>, key: &str) -> Vec<u8> {
    if let Some(Value::String(text)) = object.get(key) {
        return text.as_bytes().to_vec();
    }
    if let Some(Value::String(hex)) = object.get(&format!("{key}h")) {
        return unhex(hex);
    }
    Vec::new()
}

fn put_text(object: &mut Map<String, Value>, key: &str, bytes: &[u8]) {
    match std::str::from_utf8(bytes) {
        Ok(text) if !text.contains('\0') => {
            object.insert(key.into(), json!(text));
        }
        _ => {
            let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
            object.insert(format!("{key}h"), json!(hex));
        }
    }
}

fn luau_number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 { format!("{}", value as i64) } else { format!("{value}") }
}

/// Face keys (`path|family|weight|style|substituted`) name font files by path, and the files move:
/// Studio updates install them under a new …/Roblox/Versions/version-<hash>/ folder (and remove the
/// old one), the state dir can move, and a frozen font set has its own copy. Keys are compared with
/// the version folder (or the frozen set's `studio/`) as `<studio>/` and the recorded or current cloud
/// font directory as `<assets>/`.
struct Places {
    /// Lowercase directory prefixes (with a trailing slash) and their placeholders
    prefixes: Vec<(String, &'static str)>,
}

impl Places {
    fn new(recorded_cache: &str, assets: &str, fonts: Option<&str>) -> Places {
        let dir = |path: &str| format!("{}/", path.replace('\\', "/").trim_end_matches('/').to_ascii_lowercase());
        let mut prefixes = vec![(dir(&format!("{recorded_cache}/assets")), "<assets>/"), (dir(assets), "<assets>/")];
        if let Some(fonts) = fonts {
            prefixes.push((dir(&format!("{fonts}/studio")), "<studio>/"));
        }
        Places { prefixes }
    }

    fn path(&self, path: &str) -> String {
        let lower = path.to_ascii_lowercase();
        let neutral = if let Some((prefix, placeholder)) = self.prefixes.iter().find(|(prefix, _)| lower.starts_with(prefix.as_str())) {
            format!("{placeholder}{}", &path[prefix.len()..])
        } else if let Some(start) = lower.find("/roblox/versions/version-") {
            let folder = start + "/roblox/versions/".len();
            let end = path[folder..].find('/').map_or(path.len(), |index| folder + index + 1);
            format!("<studio>/{}", &path[end..])
        } else {
            path.to_string()
        };
        neutral.replace("content//", "content/")
    }

    fn key(&self, key: &str) -> String {
        match key.split_once('|') {
            Some((path, rest)) => format!("{}|{rest}", self.path(path)),
            None => self.path(key),
        }
    }
}

struct Faces {
    places: Places,
    index: HashMap<String, i64>,
    extra: Vec<String>,
}

impl Faces {
    fn id(&mut self, engine: &TextEngine, face: Option<text::FaceId>) -> Value {
        let Some(face) = face else {
            return json!(-1);
        };
        let face = engine.face(face);
        let key = format!("{}|{}|{}|{}|{}", face.path, face.family, luau_number(face.weight), face.style, face.substituted);
        if let Some(&index) = self.index.get(&self.places.key(&key)) {
            return json!(index);
        }
        self.extra.push(key.clone());
        json!(format!("unknown face {key}"))
    }
}

fn canon_layout(engine: &TextEngine, faces: &mut Faces, result: &TextLayout) -> Value {
    let mut lines = Vec::new();
    for line in &result.lines {
        let mut glyphs = Vec::new();
        for glyph in &line.glyphs {
            glyphs.push(faces.id(engine, Some(glyph.face)));
            glyphs.push(json!(glyph.glyph));
            glyphs.push(n(glyph.x));
            glyphs.push(n(glyph.x_offset));
            glyphs.push(n(glyph.advance));
            glyphs.push(json!(glyph.cluster));
            glyphs.push(json!(glyph.code));
            glyphs.push(n(glyph.size));
            glyphs.push(json!(glyph.run));
        }
        lines.push(json!({
            "w": n(line.width), "t": n(line.top), "h": n(line.height), "f": line.first, "l": line.last,
            "pe": line.paragraph_end, "sp": line.split, "tr": line.truncated, "ds": n(line.default_size), "g": glyphs,
        }));
    }
    json!({
        "sz": n(result.size), "w": n(result.width), "h": n(result.height), "lh": n(result.line_height),
        "nw": n(result.natural_width), "fits": result.fits, "br": result.broken, "face": faces.id(engine, result.face),
        "rich": result.rich, "nr": result.runs.as_ref().map_or(-1, |runs| runs.len() as i64), "L": lines,
    })
}

fn canon_color(color: Option<[f64; 3]>) -> Option<Value> {
    color.map(|color| json!([n(color[0]), n(color[1]), n(color[2])]))
}

fn canon_runs(runs: &[text::Run]) -> Value {
    let mut out = Vec::new();
    for run in runs {
        let style = &run.style;
        let mut entry = Map::new();
        entry.insert("b".into(), json!(style.bold));
        entry.insert("i".into(), json!(style.italic));
        entry.insert("u".into(), json!(style.underline));
        entry.insert("s".into(), json!(style.strikethrough));
        entry.insert("uc".into(), json!(style.uppercase));
        entry.insert("sc".into(), json!(style.small_caps));
        if let Some(color) = canon_color(style.color) {
            entry.insert("col".into(), color);
        }
        if let Some(size) = style.size {
            entry.insert("sz".into(), n(size));
        }
        if let Some(family) = &style.family {
            entry.insert("fam".into(), json!(family));
        }
        if let Some(weight) = style.weight {
            entry.insert("wt".into(), n(weight));
        }
        if let Some(weight) = style.face_weight {
            entry.insert("fw".into(), n(weight));
        }
        if let Some(font_style) = &style.font_style {
            entry.insert("fs".into(), json!(font_style));
        }
        if let Some(transparency) = style.transparency {
            entry.insert("tr".into(), n(transparency));
        }
        put_text(&mut entry, "t", &run.text);
        if let Some(stroke) = &style.stroke {
            let mut value = Map::new();
            if let Some(color) = canon_color(stroke.color) {
                value.insert("c".into(), color);
            }
            value.insert("th".into(), n(stroke.thickness));
            value.insert("tr".into(), n(stroke.transparency));
            value.insert("j".into(), json!(stroke.joins));
            entry.insert("st".into(), Value::Object(value));
        }
        if let Some(mark) = &style.mark {
            let mut value = Map::new();
            if let Some(color) = canon_color(mark.color) {
                value.insert("c".into(), color);
            }
            value.insert("tr".into(), n(mark.transparency));
            entry.insert("mk".into(), Value::Object(value));
        }
        out.push(Value::Object(entry));
    }
    Value::Array(out)
}

/// Structural equality with bit-exact numbers; an empty object equals an empty array (Luau tables).
fn same(expected: &Value, actual: &Value) -> bool {
    match (expected, actual) {
        (Value::Object(a), Value::Array(b)) | (Value::Array(b), Value::Object(a)) => a.is_empty() && b.is_empty(),
        (Value::Number(a), Value::Number(b)) => a.as_f64().map(f64::to_bits) == b.as_f64().map(f64::to_bits),
        (Value::Array(a), Value::Array(b)) => a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same(a, b)),
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len() && a.iter().all(|(key, value)| b.get(key).is_some_and(|other| same(value, other)))
        }
        _ => expected == actual,
    }
}

/// The differing paths (at most a few), for the report.
fn difference(expected: &Value, actual: &Value, path: &str) -> String {
    let mut out = Vec::new();
    differences(expected, actual, path, &mut out);
    out.join("\n    ")
}

fn differences(expected: &Value, actual: &Value, path: &str, out: &mut Vec<String>) {
    if out.len() >= 6 {
        return;
    }
    match (expected, actual) {
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
            for (index, (a, b)) in a.iter().zip(b).enumerate() {
                if !same(a, b) {
                    differences(a, b, &format!("{path}[{index}]"), out);
                }
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            for (key, value) in a {
                match b.get(key) {
                    Some(other) if same(value, other) => {}
                    Some(other) => differences(value, other, &format!("{path}.{key}"), out),
                    None => out.push(format!("{path}.{key}: missing in Rust (Luau {value})")),
                }
            }
            for key in b.keys() {
                if !a.contains_key(key) {
                    out.push(format!("{path}.{key}: extra in Rust ({})", b[key]));
                }
            }
        }
        _ => {
            let clip = |value: &Value| {
                let text = value.to_string();
                if text.len() > 300 { format!("{}...", &text[..300]) } else { text }
            };
            out.push(format!("{path}: Luau {} vs Rust {}", clip(expected), clip(actual)));
        }
    }
}

fn opt(object: &Map<String, Value>, key: &str) -> Option<f64> {
    number(object.get(key))
}

fn string<'a>(object: &'a Map<String, Value>, key: &str) -> &'a str {
    object.get(key).and_then(Value::as_str).unwrap_or("")
}

fn boolean(object: &Map<String, Value>, key: &str) -> bool {
    object.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn run_case(engine: &mut TextEngine, faces: &mut Faces, case: &Map<String, Value>) -> Value {
    let kind = string(case, "k");
    let empty = Map::new();
    let params = case.get("p").and_then(Value::as_object).unwrap_or(&empty);
    match kind {
        "layout" => {
            let text = text_of(params, "t");
            let family = string(params, "f").to_string();
            let style = string(params, "s").to_string();
            let layout = LayoutParams {
                text: &text,
                family: &family,
                weight: opt(params, "w").unwrap(),
                style: &style,
                text_size: opt(params, "ts").unwrap(),
                pixel_scale: opt(params, "ps").unwrap(),
                wrapped: boolean(params, "wr"),
                max_width: opt(params, "mw"),
                max_height: opt(params, "mh"),
                line_height: opt(params, "lh"),
                truncate: Truncate::from_name(string(params, "tr")),
                scale: opt(params, "sc"),
                pixel_size: opt(params, "px"),
                effective_size: opt(params, "es"),
            };
            let result = engine.layout(&layout, boolean(case, "rich"));
            canon_layout(engine, faces, &result)
        }
        "text" => {
            let text = text_of(params, "t");
            let family = string(params, "f").to_string();
            let style = string(params, "s").to_string();
            let constraint = match (opt(params, "cmin"), opt(params, "cmax")) {
                (Some(min_text_size), Some(max_text_size)) => Some(SizeConstraint { min_text_size, max_text_size }),
                _ => None,
            };
            let object = TextObject {
                text: &text,
                rich: boolean(params, "rich"),
                family: &family,
                weight: opt(params, "w").unwrap(),
                style: &style,
                wrapped: boolean(params, "wr"),
                scaled: boolean(params, "scaled"),
                text_size: opt(params, "ts").unwrap(),
                line_height: opt(params, "lh").unwrap(),
                truncate: Truncate::from_name(string(params, "tr")),
                constraint,
            };
            let info = engine.layout_text(
                &object,
                opt(params, "bw").unwrap(),
                opt(params, "bh").unwrap(),
                opt(params, "sc").unwrap(),
                opt(params, "dpi").unwrap(),
                boolean(params, "auto"),
            );
            let mut out = Map::new();
            out.insert("ts".into(), n(info.text_size));
            out.insert("ps".into(), n(info.pixel_scale));
            out.insert("ax".into(), n(info.absolute_x));
            out.insert("ay".into(), n(info.absolute_y));
            out.insert("bx".into(), n(info.bounds_x));
            out.insert("by".into(), n(info.bounds_y));
            if let Some(quad) = info.quad_scale {
                out.insert("q".into(), n(quad));
            }
            out.insert("L".into(), canon_layout(engine, faces, &info.layout));
            Value::Object(out)
        }
        "pen" => {
            let pen = text::line_pen(
                XAlign::from_name(string(params, "a")),
                opt(params, "x").unwrap(),
                opt(params, "w").unwrap(),
                opt(params, "lw").unwrap(),
                boolean(params, "rich"),
                boolean(params, "exact"),
                // (the Luau engine's rule was the 1.5x one: widths not a multiple of 3 px)
                opt(params, "ps").unwrap_or(1.5),
            );
            n(pen)
        }
        "size" => {
            let family = string(params, "f");
            let (text_size, dpi, scale, effective) = (opt(params, "ts").unwrap(), opt(params, "ps").unwrap(), opt(params, "sc"), opt(params, "es"));
            let (integer, size) = text::size_for(text_size, dpi, Some(family), scale, effective);
            let (rich, scaled) = text::rich_size_for(text_size, dpi, Some(family), scale, effective);
            let quad = text::quad_size_for(text_size, dpi, Some(family), scale, effective);
            let pixels = text::rich_scaled_pixels((text_size * dpi).floor(), dpi, Some(family), scale);
            json!([n(integer), n(size), n(rich), n(scaled), n(quad), n(pixels)])
        }
        "parse" => {
            let source = text_of(case, "t");
            let (runs, valid) = engine.parse_rich(&source);
            let mut out = Map::new();
            out.insert("v".into(), json!(valid));
            out.insert("r".into(), canon_runs(&runs));
            let content = engine.content_text(&source);
            put_text(&mut out, "c", &content);
            Value::Object(out)
        }
        other => panic!("unknown case kind {other}"),
    }
}

/// runtime/tests/hinted.luau's check: SourceSans Pro Regular's "…" has a hinted advance of 24 px at
/// 30 px (the hinted-metric cache, generated in process when missing). The truncation ellipsis no longer
/// uses it: Studio 0.746 measures it 23 px (data/studio/ellipsis.json SSP_20), its rounded 26.6 advance.
#[test]
fn hinted_ellipsis_advance() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let root = std::fs::canonicalize(&root).unwrap().to_string_lossy().trim_start_matches(r"\\?\").replace('\\', "/");
    let mut engine = TextEngine::new(FontPaths::for_repo(&root));
    let Some(content) = engine.fonts.font_content_root().map(str::to_string) else {
        eprintln!("skipping: no Studio content root");
        return;
    };
    let face = engine.fonts.get_face("rbxasset://fonts/families/SourceSansPro.json", 400.0, "normal").unwrap();
    assert_eq!(&*engine.face(face).path, format!("{content}/fonts/SourceSansPro-Regular.ttf"));
    let glyph = *engine.face(face).font.cmap.get(&0x2026).unwrap();
    assert_eq!(engine.fonts.hinted_advance(face, 30.0, glyph), Some(24.0));
    assert_eq!(engine.fonts.hinted_advance(face, 30.5, glyph), None);
    let ellipsis = engine.ellipsis(face, 30.0);
    assert_eq!(ellipsis.1, 23.0);
}

/// A face recorded under one Studio version folder and asset cache is the same face read from another
/// install or from a frozen font set.
#[test]
fn face_keys_ignore_where_fonts_live() {
    let places = Places::new("C:/r/.cache", "D:/golden/fonts/assets", Some("D:/golden/fonts"));
    let key = |path: &str| places.key(&format!("{path}|rbxasset://fonts/families/Montserrat.json|100|normal|false"));
    let studio = key("C:/Users/u/AppData/Local/Roblox/Versions/version-c792f79abddd41bd/content//fonts/Montserrat-Regular.ttf");
    assert_eq!(studio, key("C:/Users/u/AppData/Local/Roblox/Versions/version-9b554450a0fc4e65/content//fonts/Montserrat-Regular.ttf"));
    assert_eq!(studio, key("D:/golden/fonts/studio/content/fonts/Montserrat-Regular.ttf"));
    assert_eq!(studio, "<studio>/content/fonts/Montserrat-Regular.ttf|rbxasset://fonts/families/Montserrat.json|100|normal|false");
    let icons = "ExtraContent/LuaPackages/Packages/_Index/BuilderIcons/BuilderIcons/Font/BuilderIcons-Regular.ttf";
    assert_eq!(places.path(&format!("C:/AppData/Roblox/Versions/version-76e1a02649ad4f35/{icons}")), places.path(&format!("D:/golden/fonts/studio/{icons}")));
    assert_eq!(key("C:/r/.cache/assets/11598116985.ttf"), key("d:/golden/fonts/assets/11598116985.ttf"));
    assert_eq!(places.path("C:/r/.cache/assets/11598116985.ttf"), "<assets>/11598116985.ttf");
    assert_eq!(places.path("C:/r/.cache/assetsX/1.ttf"), "C:/r/.cache/assetsX/1.ttf");
}

fn corpus_path() -> PathBuf {
    if let Ok(path) = std::env::var("RUR_TEXT_CORPUS") {
        return PathBuf::from(path);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../out/golden/text-corpus.json")
}

/// The frozen font set: RUR_TEXT_CORPUS_FONTS (empty: none), else `fonts/` beside the corpus when it
/// has a manifest (tools/engine/golden.mjs --freeze-fonts).
fn frozen_fonts(corpus: &std::path::Path) -> Option<String> {
    let dir = match std::env::var("RUR_TEXT_CORPUS_FONTS") {
        Ok(dir) if dir.is_empty() => return None,
        Ok(dir) => PathBuf::from(dir),
        Err(_) => corpus.parent()?.join("fonts"),
    };
    let dir = std::fs::canonicalize(dir.join("manifest.json")).ok()?.parent()?.to_path_buf();
    Some(dir.to_string_lossy().trim_start_matches(r"\\?\").replace('\\', "/"))
}

/// The recorded face files the engine cannot find where it now reads fonts (a cleared asset cache, a
/// frozen set that lacks them).
fn missing_font_files(corpus: &Value, places: &Places, engine: &TextEngine) -> Vec<String> {
    let studio = engine.fonts.font_content_root().map(|content| {
        let content = content.trim_end_matches('/');
        content.strip_suffix("/content").unwrap_or(content).to_string()
    });
    let mut missing: Vec<String> = Vec::new();
    for face in corpus["faces"].as_array().into_iter().flatten().filter_map(Value::as_str) {
        let recorded = face.split('|').next().unwrap_or(face);
        let neutral = places.path(recorded);
        let now = if let Some(name) = neutral.strip_prefix("<assets>/") {
            format!("{}/{name}", engine.fonts.assets_dir())
        } else if let (Some(relative), Some(studio)) = (neutral.strip_prefix("<studio>/"), &studio) {
            format!("{studio}/{relative}")
        } else {
            recorded.to_string()
        };
        if !std::path::Path::new(&now).is_file() && !missing.contains(&now) {
            missing.push(now);
        }
    }
    missing
}

#[test]
fn text_engine_matches_luau_corpus() {
    let path = corpus_path();
    let Ok(bytes) = std::fs::read(&path) else {
        eprintln!("skipping: no text corpus at {} (the frozen corpus of out/golden; notes/rust-engine.md, Text API)", path.display());
        return;
    };
    let corpus: Value = serde_json::from_slice(&bytes).expect("corpus JSON");
    let paths = corpus["paths"].as_object().expect("paths");
    let fonts = frozen_fonts(&path);
    let font_paths = FontPaths {
        root: paths["root"].as_str().unwrap().to_string(),
        cache: paths["cache"].as_str().unwrap().to_string(),
        // the recorded Studio install, or the current one once an update has removed it (the engine
        // discovers the install when this is None); only used for fonts without a frozen set
        content_root: paths.get("content").and_then(Value::as_str).filter(|dir| std::path::Path::new(dir).is_dir()).map(str::to_string),
        fonts: fonts.clone(),
    };
    let mut engine = TextEngine::new(font_paths.clone());
    let places = Places::new(paths["cache"].as_str().unwrap(), engine.fonts.assets_dir(), fonts.as_deref());
    let mut faces = Faces { index: HashMap::new(), extra: Vec::new(), places };
    for (index, face) in corpus["faces"].as_array().unwrap().iter().enumerate() {
        let key = faces.places.key(face.as_str().unwrap());
        faces.index.insert(key, index as i64);
    }
    // Cloud fonts downloaded after the corpus was recorded (a family's Thin face, say) would change
    // which face resolves: keep only the ones it used
    let assets = engine.fonts.assets_dir().to_string();
    let recorded: std::collections::HashSet<String> = corpus["faces"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|face| face.as_str()?.split('|').next())
        .filter_map(|file| faces.places.path(file).strip_prefix("<assets>/").map(|name| format!("{assets}/{name}")))
        .collect();
    engine.fonts.restrict_cached_assets(&recorded);
    let missing = missing_font_files(&corpus, &faces.places, &engine);
    match &fonts {
        Some(fonts) => println!("fonts: the frozen set {fonts}"),
        None => println!("fonts: the installed Studio's and the asset cache's (no frozen set: node tools/engine/golden.mjs --freeze-fonts)"),
    }
    for file in &missing {
        println!("  recorded font file missing: {file}");
    }

    // RUR_TEXT_CORPUS_UPDATE_Q=<file>: writes the corpus to <file> with the expected output of every
    // "text" case that differs only in its quad scale `q` replaced by the current one (Studio-verified
    // quad-scale rule changes, notes/text.md), for review before it replaces the frozen corpus.
    let update_q = std::env::var("RUR_TEXT_CORPUS_UPDATE_Q").ok();
    // RUR_TEXT_CORPUS_UPDATE_RICHSCALED=<file>: the same for every differing "text" case of rich TextScaled
    // text (Studio-verified changes of its size search; any other difference still fails the test).
    let update_rich_scaled = std::env::var("RUR_TEXT_CORPUS_UPDATE_RICHSCALED").ok();
    let rich_scaled = |case: &Map<String, Value>| -> bool {
        case.get("p").and_then(Value::as_object).is_some_and(|params| boolean(params, "rich") && boolean(params, "scaled"))
    };
    let mut updated: Vec<(usize, Value)> = Vec::new();
    let without_q = |value: &Value| -> Value {
        let mut value = value.clone();
        if let Some(object) = value.as_object_mut() {
            object.remove("q");
        }
        value
    };

    let cases = corpus["cases"].as_array().expect("cases");
    let started = std::time::Instant::now();
    let mut per_kind: HashMap<String, (usize, usize)> = HashMap::new();
    let mut failures = Vec::new();
    for (index, case) in cases.iter().enumerate() {
        let case = case.as_object().unwrap();
        let kind = string(case, "k").to_string();
        let actual = run_case(&mut engine, &mut faces, case);
        let expected = &case["o"];
        let entry = per_kind.entry(kind.clone()).or_default();
        entry.0 += 1;
        if same(expected, &actual) {
            entry.1 += 1;
        } else if (update_q.is_some() && kind == "text" && same(&without_q(expected), &without_q(&actual)))
            || (update_rich_scaled.is_some() && kind == "text" && rich_scaled(case) && !actual.to_string().contains("unknown face"))
        {
            updated.push((index, actual.clone()));
        } else if failures.len() < 40 {
            failures.push(format!("case {index} ({kind}) {}\n    {}", Value::Object(case.get("p").and_then(Value::as_object).cloned().unwrap_or_default()), difference(expected, &actual, "o")));
        }
    }
    let elapsed = started.elapsed();
    if let Some(out) = update_q.as_ref().or(update_rich_scaled.as_ref()) {
        let mut corpus = corpus.clone();
        let cases = corpus["cases"].as_array_mut().unwrap();
        for (index, actual) in &updated {
            cases[*index].as_object_mut().unwrap().insert("o".into(), actual.clone());
        }
        std::fs::write(out, serde_json::to_vec(&corpus).unwrap()).expect("write the updated corpus");
        println!("{} text cases updated (quad scale / rich TextScaled): written with the current output to {out}", updated.len());
    }

    // Cached results are shared: equal inputs give the identical Arc (the Luau identity rule).
    let family = "rbxasset://fonts/families/SourceSansPro.json";
    let params = LayoutParams::new(b"identity check", family, 400.0, "normal", 14.0, 1.5);
    let first = engine.layout(&params, false);
    let second = engine.layout(&params, false);
    assert!(Arc::ptr_eq(&first, &second), "cached layouts are shared");

    // Timing of the layout slice alone (no canonicalization): a fresh engine (fonts already parsed by a
    // warm-up pass is not assumed: cold includes font loading), then the same calls again (cache hits).
    let layout_cases: Vec<&Map<String, Value>> =
        cases.iter().filter_map(Value::as_object).filter(|case| string(case, "k") == "layout").collect();
    let inputs: Vec<(Vec<u8>, String, String, &Map<String, Value>, bool)> = layout_cases
        .iter()
        .map(|case| {
            let params = case["p"].as_object().unwrap();
            (text_of(params, "t"), string(params, "f").to_string(), string(params, "s").to_string(), params, boolean(case, "rich"))
        })
        .collect();
    let prepared: Vec<(LayoutParams, bool)> = inputs
        .iter()
        .map(|(text, family, style, params, rich)| {
            let layout = LayoutParams {
                text,
                family,
                weight: opt(params, "w").unwrap(),
                style,
                text_size: opt(params, "ts").unwrap(),
                pixel_scale: opt(params, "ps").unwrap(),
                wrapped: boolean(params, "wr"),
                max_width: opt(params, "mw"),
                max_height: opt(params, "mh"),
                line_height: opt(params, "lh"),
                truncate: Truncate::from_name(string(params, "tr")),
                scale: opt(params, "sc"),
                pixel_size: opt(params, "px"),
                effective_size: opt(params, "es"),
            };
            (layout, *rich)
        })
        .collect();
    let run_all = |engine: &mut TextEngine, count: usize| {
        for (layout, rich) in prepared.iter().take(count) {
            std::hint::black_box(engine.layout(layout, *rich));
        }
    };
    let mut fresh = TextEngine::new(font_paths);
    let cold = std::time::Instant::now();
    run_all(&mut fresh, inputs.len());
    let cold = cold.elapsed();
    // Cache hits: a slice that fits the measure cache (it clears itself past 20 000 entries, like Luau).
    let hits = inputs.len().min(4000);
    run_all(&mut fresh, hits);
    let warm = std::time::Instant::now();
    run_all(&mut fresh, hits);
    let warm = warm.elapsed();
    println!(
        "layout timing: {} calls cold {:.1} ms ({:.2} us/call incl. font loading); {hits} cached calls {:.2} ms ({:.3} us/call)",
        inputs.len(),
        cold.as_secs_f64() * 1e3,
        cold.as_secs_f64() * 1e6 / inputs.len() as f64,
        warm.as_secs_f64() * 1e3,
        warm.as_secs_f64() * 1e6 / hits as f64
    );

    let mut kinds: Vec<_> = per_kind.into_iter().collect();
    kinds.sort();
    let total: usize = kinds.iter().map(|(_, (count, _))| count).sum();
    let identical: usize = kinds.iter().map(|(_, (_, same))| same).sum();
    println!("text corpus {}: {identical}/{total} identical in {:.2} s", path.display(), elapsed.as_secs_f64());
    for (kind, (count, same)) in &kinds {
        println!("  {kind}: {same}/{count}");
    }
    for failure in &failures {
        println!("  {failure}");
    }
    let cause = match missing.len() {
        0 => String::new(),
        count => format!(
            "; {count} font files the corpus was recorded with are missing ({}): {}",
            missing.iter().take(6).map(String::as_str).collect::<Vec<_>>().join(", "),
            if fonts.is_some() { "the frozen font set is incomplete" } else { "restore them (a render that uses a cloud font fetches it) or freeze a font set that has them" }
        ),
    };
    assert_eq!(identical, total, "{} of {total} corpus cases differ from the Luau text engine{cause}", total - identical);
}
