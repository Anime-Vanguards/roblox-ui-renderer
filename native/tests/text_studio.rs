//! Offline text measurement against frozen Studio datasets (data/studio/*.json), ported from the Luau
//! reference tests runtime/tests/text_truncate.luau, text_bounds.luau and text_wrap.luau. Needs the
//! installed Roblox Studio fonts, like the Luau tests, and text_wrap the cloud CJK fallback, downloaded
//! when the asset cache lacks it.
//!
//! text_bounds / text_wrap only reported hit rates in Luau; here the hit counts (and the known misses)
//! are pinned. The datasets were re-measured in Studio 0.746 (2026-11-09) with the same probes
//! (tools/studio/measure-text.luau, measure-sweep.luau, measure-wrap.luau, layout-probe-ellipsis3.luau):
//! text-bounds-1.5x.json 3570/3570, sweep-1.5x.json 4785/4785, wrap-1.5x.json 538/540. The faces with a line
//! gap were measured on 2026-11-10 (tools/studio/measure-linegap.luau): text-bounds-linegap-1.5x.json
//! 3059/3060 and text-bounds-linegap-1x.json 3047/3060.

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
    engine_with(|engine| {
        let family = family("SourceSansPro");
        for text in texts {
            engine.layout(&LayoutParams::new(text.as_str().unwrap().as_bytes(), &family, 400.0, "normal", 14.0, 1.5), false);
        }
    })
}

/// An engine that has the cloud fonts the layouts of `ask` reach for (a cloud family's file arrives first,
/// then its faces).
fn engine_with(ask: impl Fn(&mut TextEngine)) -> TextEngine {
    let mut fetched = Vec::new();
    for _ in 0..4 {
        let mut engine = engine();
        ask(&mut engine);
        if engine.fonts.missing_assets.is_empty() {
            return engine;
        }
        fetched.push(Resolver::new(repo(), false).fetch_fonts(&json!({ "missingFontAssets": engine.fonts.missing_assets })));
    }
    let mut engine = engine();
    ask(&mut engine);
    panic!("cloud fonts {:?} are not cached and did not download ({fetched:?})", engine.fonts.missing_assets);
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
    text_bounds_at(engine, file, 1.5)
}

/// A measure-text.luau dataset taken at Studio's DPI `scale` (a row may name its `family`, a cloud one).
fn text_bounds_at(engine: &mut TextEngine, file: &str, scale: f64) -> (usize, usize, BTreeMap<String, (usize, usize)>, Vec<String>) {
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
        let family = row.get("family").and_then(Value::as_str).map_or_else(|| family(font), str::to_string);
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
    // (3557 before a face with a line gap had its size request cut to 26.6: 13 Arimo and LegacyArial
    // strings were 1 px wide)
    assert_eq!((hits, total), (3570, 3570), "text-bounds-1.5x.json misses: {misses:#?}");
    assert_eq!(per_font.len(), 14);
    assert!(per_font.values().all(|&(hit, count)| hit == 255 && count == 255), "{per_font:?}");
}

/// Every Studio face with an hhea line gap (Arimo, Ubuntu, Permanent Marker, AccanthisADFStd, Guru) and
/// Poppins, a cloud family with a larger one: Studio 0.746 TextBounds at DPI 1.5 and at DPI 1.
#[test]
fn line_gap_faces() {
    let rows = read_json("data/studio/text-bounds-linegap-1x.json");
    let weight = |name: &Value| match name.as_str().unwrap() {
        "Regular" => 400.0,
        "Medium" => 500.0,
        "SemiBold" => 600.0,
        _ => 700.0,
    };
    let mut engine = engine_with(|engine| {
        for row in rows["rows"].as_array().unwrap() {
            let family = row.get("family").and_then(Value::as_str).map_or_else(|| family(row["font"].as_str().unwrap()), str::to_string);
            engine.layout(&LayoutParams::new(b"a", &family, weight(&row["weight"]), "normal", 20.0, 1.5), false);
        }
    });
    // Known misses: one Permanent Marker string at DPI 1.5 (1 px), and at DPI 1 the two trailing spaces of
    // "Hello  " in Guru, which Studio measures narrower at 13 of the 17 sizes
    for (file, scale, known) in [
        ("data/studio/text-bounds-linegap-1.5x.json", 1.5, ("PermanentMarker-Regular", 255)),
        ("data/studio/text-bounds-linegap-1x.json", 1.0, ("Guru-Regular", 242)),
    ] {
        let (_, total, per_font, misses) = text_bounds_at(&mut engine, file, scale);
        assert_eq!((per_font.len(), total), (12, 3060), "{file}");
        for (key, &(hit, count)) in &per_font {
            let expected = if key == known.0 { known.1 } else { 255 };
            assert_eq!((hit, count), (expected, 255), "{file} {key}: {misses:#?}");
        }
    }
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
    assert_eq!((hits, total), (538, 540), "wrap-1.5x.json misses: {misses:#?}");
    // The known misses: runs of spaces at a break (the Luau reference missed the same kind of case).
    assert_eq!(
        misses,
        vec![
            "SourceSansPro s=20 w=30 t=3 \"a  b   c    d\" studio=(14,60) ours=(44,60) lines=2".to_string(),
            "FredokaOne s=14 w=30 t=3 \"a  b   c    d\" studio=(11,42) ours=(38,42) lines=2".to_string(),
        ]
    );
}

/// The pen adds exact advances and rounds once (Studio 0.746 TextBounds at 1.5x, Montserrat Bold at 18 px:
/// rich TextSize 16 under UIScale 0.8, and plain TextSize 12). Rounding each advance made "1 Egg" 42 and
/// put "E" a pixel closer to the "1" than Studio draws it.
#[test]
fn pen_rounding() {
    let mut engine = engine();
    let family = family("Montserrat");
    let mut width = |text: &str, rich: bool, text_size: f64, scale: Option<f64>| {
        let mut params = LayoutParams::new(text.as_bytes(), &family, 700.0, "normal", text_size, 1.5);
        params.scale = scale;
        engine.layout(&params, rich).width
    };
    for (text, expected) in [("50 Eggs", 65.0), ("10 Eggs", 62.0), ("3 Eggs", 54.0), ("1 Egg", 43.0), ("1 E", 21.0), ("1E", 17.0), ("0 E", 25.0)] {
        assert_eq!(width(text, true, 16.0, Some(0.8)), expected, "rich {text} under UIScale 0.8");
        assert_eq!(width(text, false, 12.0, None), expected, "{text}");
    }
    for (text, expected) in [("11111", 31.0), ("00000", 53.0), ("iiiii", 23.0)] {
        assert_eq!(width(text, false, 12.0, None), expected, "{text}");
    }
    // The pen runs on across rich text runs
    assert_eq!(width("1<font color='#ff0000'>1</font>111", true, 12.0, None), 31.0);
    assert_eq!(width("0<font color='#f00'>0</font><font color='#0f0'>0</font>00", true, 12.0, None), 53.0);
    assert_eq!(width("i<font color='#f00'>i</font>i<font color='#0f0'>i</font>i", true, 12.0, None), 23.0);
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
