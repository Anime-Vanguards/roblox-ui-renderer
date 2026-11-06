//! Offline text measurement against frozen Studio datasets (data/studio/*.json), ported from the Luau
//! reference tests runtime/tests/text_truncate.luau, text_bounds.luau and text_wrap.luau. Needs the
//! installed Roblox Studio fonts, like the Luau tests, and text_wrap the cloud CJK fallback, downloaded
//! when the asset cache lacks it.
//!
//! text_bounds / text_wrap only reported hit rates in Luau; here the hit counts (and the known misses)
//! are pinned to what the Luau reference produced when it was retired:
//! text-bounds-1.5x.json 3570/3570, sweep-1.5x.json 4785/4785, wrap-1.5x.json 539/540.

use std::collections::BTreeMap;
use std::path::PathBuf;

use rur_native::engine::text::{FontPaths, LayoutParams, TextEngine, Truncate};
use rur_native::resolve::Resolver;
use serde_json::{Value, json};

fn repo() -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    std::fs::canonicalize(&root).unwrap_or(root).display().to_string().trim_start_matches(r"\\?\").to_string()
}

fn engine() -> TextEngine {
    let engine = TextEngine::new(FontPaths::for_repo(&repo()));
    assert!(engine.fonts.content_root().is_some(), "these tests require the installed Roblox Studio fonts");
    engine
}

/// An engine that has the cloud fonts these texts ask for (the CJK fallback): a checkout's asset cache holds
/// only what its renders drew, so they are downloaded as a render downloads them.
fn engine_with_cloud_fonts(texts: &[Value]) -> TextEngine {
    let ask = |engine: &mut TextEngine| {
        let family = family("SourceSansPro");
        for text in texts {
            engine.layout(&LayoutParams::new(text.as_str().unwrap().as_bytes(), &family, 400.0, "normal", 14.0, 1.5), false);
        }
    };
    let mut engine = engine();
    ask(&mut engine);
    if engine.fonts.missing_assets.is_empty() {
        return engine;
    }
    let fetched = Resolver::new(repo(), false).fetch_fonts(&json!({ "missingFontAssets": engine.fonts.missing_assets }));
    let mut fresh = self::engine();
    ask(&mut fresh);
    assert!(fresh.fonts.missing_assets.is_empty(), "cloud fonts {:?} are not cached and did not download ({fetched:?})", fresh.fonts.missing_assets);
    fresh
}

fn read_json(relative: &str) -> Value {
    let path = format!("{}/{relative}", repo());
    serde_json::from_slice(&std::fs::read(&path).unwrap_or_else(|error| panic!("{path}: {error}"))).unwrap()
}

fn family(name: &str) -> String {
    format!("rbxasset://fonts/families/{name}.json")
}

/// Luau `math.round` (half away from zero).
fn round(value: f64) -> f64 {
    value.round()
}

/// runtime/tests/text_truncate.luau: frozen Studio measurements, including sizes where
/// ceil(unhinted advance) errs in both directions.
#[test]
fn text_truncate() {
    let mut engine = engine();
    let data = read_json("data/studio/ellipsis.json");
    let faces: BTreeMap<&str, (&str, f64)> =
        [("SSP", ("SourceSansPro", 400.0)), ("MB", ("Montserrat", 700.0)), ("Fre", ("FredokaOne", 400.0))].into_iter().collect();
    let mut count = 0;
    for (key, expected) in data["samples"].as_object().unwrap() {
        let (name, size) = key.split_once('_').unwrap();
        let (face_name, weight) = faces[name];
        let size: f64 = size.parse().unwrap();
        let expected: Vec<f64> = expected.as_array().unwrap().iter().map(|value| value.as_f64().unwrap()).collect();
        let family = family(face_name);
        // Plain ellipses retain normal shaping; only TextTruncate uses the hinted advance.
        for (index, text) in ["Ab", "\u{2026}", "Ab\u{2026}"].iter().enumerate() {
            let params = LayoutParams::new(text.as_bytes(), &family, weight, "normal", size, 1.5);
            let result = engine.layout(&params, false);
            assert_eq!(result.width, expected[index], "{key}: ordinary text {text}");
        }
        let mut params = LayoutParams::new(b"Ab Cdefghijklmnopqrstuvw", &family, weight, "normal", size, 1.5);
        params.max_width = Some(expected[4]);
        params.truncate = Truncate::AtEnd;
        let result = engine.layout(&params, false);
        assert_eq!(result.width, expected[3], "{key}: expected {}, got {}", expected[3], result.width);
        assert!(!result.fits && result.lines[0].truncated, "{key}: expected truncated line");
        count += 1;
    }
    assert_eq!(count, 159, "ellipsis.json sample count");

    assert!(engine.fonts.diagnostics.is_empty(), "font metrics must be available for regression checks: {:?}", engine.fonts.diagnostics);
    println!("text truncation: {count} Studio samples passed");
}

/// runtime/tests/text_bounds.luau over one dataset: (hits, total, per-font hits/total, first 25 misses).
fn text_bounds(engine: &mut TextEngine, file: &str) -> (usize, usize, BTreeMap<String, (usize, usize)>, Vec<String>) {
    let weights: BTreeMap<&str, f64> = [
        ("Thin", 100.0),
        ("ExtraLight", 200.0),
        ("Light", 300.0),
        ("Regular", 400.0),
        ("Medium", 500.0),
        ("SemiBold", 600.0),
        ("Bold", 700.0),
        ("ExtraBold", 800.0),
        ("Heavy", 900.0),
    ]
    .into_iter()
    .collect();
    let data = read_json(file);
    let scale = 1.5;
    let strings = data["strings"].as_array().unwrap();
    let (mut hits, mut total) = (0, 0);
    let mut per_font: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut misses = Vec::new();
    for row in data["rows"].as_array().unwrap() {
        let weight_name = row.get("weight").and_then(Value::as_str).unwrap_or("Regular");
        let weight = weights[weight_name];
        let font = row["font"].as_str().unwrap();
        let studio_size = row.get("S").and_then(Value::as_f64);
        let text_size = match studio_size {
            Some(s) => ((s + 0.25) / scale).floor(),
            None => row["size"].as_f64().unwrap(),
        };
        let family = family(font);
        let widths = row["widths"].as_array().unwrap();
        for (index, text) in strings.iter().enumerate() {
            let text = text.as_str().unwrap();
            let width = widths[index].as_f64().unwrap();
            let target = if studio_size.is_some() { width } else { round(width * scale) };
            let params = LayoutParams::new(text.as_bytes(), &family, weight, "normal", text_size, scale);
            let result = engine.layout(&params, false);
            let key = format!("{font}-{weight_name}");
            let stat = per_font.entry(key.clone()).or_default();
            stat.1 += 1;
            total += 1;
            if result.width == target {
                hits += 1;
                stat.0 += 1;
            } else if misses.len() < 25 {
                misses.push(format!("{key} TS={text_size} \"{text}\" studio={target} ours={}", result.width));
            }
        }
    }
    println!("{file}: matched {hits}/{total}");
    for (key, (hit, count)) in &per_font {
        println!("  {key}: {hit}/{count}");
    }
    for miss in &misses {
        println!("  miss {miss}");
    }
    (hits, total, per_font, misses)
}

#[test]
fn text_bounds_dataset() {
    let mut engine = engine();
    let (hits, total, per_font, misses) = text_bounds(&mut engine, "data/studio/text-bounds-1.5x.json");
    assert_eq!((hits, total), (3570, 3570), "text-bounds-1.5x.json misses: {misses:#?}");
    assert_eq!(per_font.len(), 14);
    assert!(per_font.values().all(|&(hit, count)| hit == 255 && count == 255), "{per_font:?}");
}

#[test]
fn text_bounds_sweep() {
    let mut engine = engine();
    let (hits, total, per_font, misses) = text_bounds(&mut engine, "data/studio/sweep-1.5x.json");
    assert_eq!((hits, total), (4785, 4785), "sweep-1.5x.json misses: {misses:#?}");
    let expected: BTreeMap<String, (usize, usize)> = ["SourceSansPro-Regular", "FredokaOne-Regular", "BuilderSans-Regular"]
        .into_iter()
        .map(|key| (key.to_string(), (1595, 1595)))
        .collect();
    assert_eq!(per_font, expected);
}

/// runtime/tests/text_wrap.luau: wrapped layout against data/studio/wrap-1.5x.json.
#[test]
fn text_wrap() {
    let data = read_json("data/studio/wrap-1.5x.json");
    let scale = 1.5;
    let texts = data["texts"].as_array().unwrap();
    let mut engine = engine_with_cloud_fonts(texts);
    let (mut hits, mut total) = (0, 0);
    let mut misses = Vec::new();
    for row in data["wrap"].as_array().unwrap() {
        let number = |key: &str| row[key].as_f64().unwrap();
        // Luau: Data.texts[Row.t] (1-based)
        let text = texts[number("t") as usize - 1].as_str().unwrap();
        let family = family(row["f"].as_str().unwrap());
        let mut params = LayoutParams::new(text.as_bytes(), &family, 400.0, "normal", number("s"), scale);
        params.wrapped = true;
        params.max_width = Some(number("w") * scale);
        params.max_height = Some(500.0 * scale);
        let result = engine.layout(&params, false);
        let studio_x = round(number("bx") * scale * 100.0) / 100.0;
        let studio_y = round(number("by") * scale * 100.0) / 100.0;
        total += 1;
        if result.width == studio_x && result.height == studio_y {
            hits += 1;
        } else if misses.len() < 40 {
            let prefix: String = String::from_utf8_lossy(&text.as_bytes()[..text.len().min(24)]).into_owned();
            misses.push(format!(
                "{} s={} w={} t={} \"{prefix}\" studio=({studio_x},{studio_y}) ours=({},{}) lines={}",
                row["f"].as_str().unwrap(),
                number("s"),
                number("w"),
                number("t"),
                result.width,
                result.height,
                result.lines.len()
            ));
        }
    }
    println!("wrap matched {hits}/{total}");
    for miss in &misses {
        println!("  {miss}");
    }
    assert_eq!((hits, total), (539, 540), "wrap-1.5x.json misses: {misses:#?}");
    // The one known miss, identical to the Luau reference's.
    assert_eq!(misses, vec!["SourceSansPro s=14 w=30 t=3 \"a  b   c    d\" studio=(10,42) ours=(34,42) lines=2".to_string()]);
}

/// Emoji sequences (fixtures/text-probe-emoji.luau, Studio TextBounds at 1.5x, FredokaOne 32): a variation
/// selector and a zero width space measure nothing ("A🏜\u{FE0F}B" = "A🏜B", "A\u{200B}B" = "AB"), a skin
/// tone and a ZWJ family are one Twemoji glyph ("👍🏽" = "👍", "A👨‍👩‍👦B" = "A🏜B").
#[test]
fn emoji_sequences() {
    let mut engine = engine();
    let family = family("FredokaOne");
    let mut layout = |text: &str| {
        let params = LayoutParams::new(text.as_bytes(), &family, 400.0, "normal", 32.0, 1.5);
        let result = engine.layout(&params, false);
        (result.width, result.lines.iter().map(|line| line.glyphs.len()).sum::<usize>())
    };
    assert_eq!(layout("A\u{1F3DC}\u{FE0F}B").0, layout("A\u{1F3DC}B").0);
    assert_eq!(layout("\u{1F3DC}\u{FE0F}").0, layout("\u{1F3DC}").0);
    assert_eq!(layout("A\u{200B}B").0, layout("AB").0);
    assert_eq!(layout("\u{1F44D}\u{1F3FD}"), layout("\u{1F44D}"));
    assert_eq!(layout("\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F466}"), layout("\u{1F3DC}"));
    assert_eq!(layout("A\u{1F468}\u{200D}\u{2764}\u{FE0F}\u{200D}\u{1F468}B"), layout("A\u{1F3DC}B"));
}
