//! Port of runtime/tests/hinted.luau: the hinted truncation-metric cache (cold / warm / corrupt-cache
//! recovery, RUR_HINTED_MAX_SIZE range diagnostics, an unwritable cache, concurrent writers) plus the
//! glyph raster regression that tools/glyphs/test_rasterize.py ran (frozen Studio i/j crops, now on the
//! in-process rasterizer, crate::glyphs). Needs the installed Studio fonts; no Python.
//!
//! RUR_HINTED_MAX_SIZE is process-global (the text engine reads it from the environment), so this file
//! is its own test binary with a single test. The cache directory is passed through `FontPaths::cache`
//! (a fresh `out/selftest/hinted-<token>`), away from the normal cache. A new `FontRegistry` stands in
//! for each `Hinted.Preload` (the Rust port prepares fonts lazily and keeps the loaded metrics in memory).

use std::io::Read;
use std::path::PathBuf;

use rur_native::engine::text::hinted::sha256_hex;
use rur_native::engine::text::{FaceId, FontPaths, FontRegistry};
use rur_native::glyphs::{AtlasEntry, Rasterizer};
use serde_json::Value;

fn repo() -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let root = std::fs::canonicalize(&root).unwrap_or(root).display().to_string();
    root.trim_start_matches(r"\\?\").replace('\\', "/")
}

fn entries(directory: &str) -> Vec<String> {
    let mut names: Vec<String> =
        std::fs::read_dir(directory).unwrap().map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    names
}

struct Setup {
    registry: FontRegistry,
    face: FaceId,
    path: String,
    glyph: u32,
}

/// One `Hinted.Preload`: a fresh registry over the test cache, and SourceSansPro-Regular's ellipsis.
fn preload(paths: &FontPaths) -> Setup {
    let mut registry = FontRegistry::new(paths.clone());
    assert!(registry.content_root().is_some(), "hinted regression requires installed Studio fonts");
    let face = registry.get_face("rbxasset://fonts/families/SourceSansPro.json", 400.0, "normal").expect("SourceSansPro face");
    let resolved = registry.face(face).clone();
    let path = resolved.path.to_string();
    assert!(path.ends_with("/fonts/SourceSansPro-Regular.ttf"), "unexpected face {path}");
    let glyph = *resolved.font.cmap.get(&0x2026).expect("SourceSansPro has U+2026");
    Setup { registry, face, path, glyph }
}

/// PIL's `Image.alpha_composite` alpha channel for a white-on-transparent source over `destination`
fn composite_alpha(destination: u8, source: u8) -> u8 {
    let blend = u32::from(destination) * (255 - u32::from(source));
    let alpha = u32::from(source) * 255 + blend + 0x80;
    (((alpha >> 8) + alpha) >> 8) as u8
}

/// tools/glyphs/test_rasterize.py: the production rasterization of i/j glyphs against frozen Studio
/// pixels (data/studio/dotted-glyphs.json), within its tolerance.
fn dotted_glyphs(root: &str, fonts: &str) {
    let data: Value = serde_json::from_slice(&std::fs::read(format!("{root}/data/studio/dotted-glyphs.json")).unwrap()).unwrap();
    let tolerance = data["tolerance"].as_u64().unwrap() as u8;
    let cases = data["cases"].as_array().unwrap();
    let mut requests = Vec::new();
    for case in cases {
        for glyph in case["glyphs"].as_array().unwrap() {
            requests.push(AtlasEntry {
                font_file: format!("{fonts}/{}", case["font"].as_str().unwrap()),
                ppem: glyph[1].as_f64().unwrap(),
                glyph: glyph[0].as_u64().unwrap() as u32,
                stroke_px: 0.0,
                font_index: 0,
                style: None,
            });
        }
    }
    let atlas = Rasterizer::new().unwrap().build_atlas(&requests).unwrap();
    let mut placed = atlas.glyphs.iter();
    let mut failures = Vec::new();
    for case in cases {
        let crop = case["crop"].as_array().unwrap();
        let (width, height) = (crop[2].as_u64().unwrap() as i64, crop[3].as_u64().unwrap() as i64);
        let mut actual = vec![0u8; (width * height) as usize];
        for glyph in case["glyphs"].as_array().unwrap() {
            let entry = placed.next().unwrap();
            let (x, baseline) = (glyph[2].as_i64().unwrap(), glyph[3].as_i64().unwrap());
            for row in 0..i64::from(entry.h) {
                for column in 0..i64::from(entry.w) {
                    let (tx, ty) = (x + i64::from(entry.left) + column, baseline - i64::from(entry.top) + row);
                    if tx < 0 || ty < 0 || tx >= width || ty >= height {
                        continue;
                    }
                    let source = atlas.rgba[((i64::from(entry.y) + row) as usize * atlas.width as usize
                        + (i64::from(entry.x) + column) as usize)
                        * 4
                        + 3];
                    let target = &mut actual[(ty * width + tx) as usize];
                    *target = composite_alpha(*target, source);
                }
            }
        }
        let encoded = case["alphaZlibBase64"].as_str().unwrap();
        let mut expected = Vec::new();
        flate2::read::ZlibDecoder::new(&base64(encoded)[..]).read_to_end(&mut expected).unwrap();
        assert_eq!(expected.len(), actual.len());
        let bad = actual.iter().zip(&expected).filter(|(a, b)| a.abs_diff(**b) > tolerance).count();
        if bad > 0 {
            let worst = actual.iter().zip(&expected).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
            failures.push(format!("{}: {bad} pixels exceed {tolerance}, max {worst}", case["name"].as_str().unwrap()));
        }
    }
    assert!(failures.is_empty(), "Studio dotted-glyph regression:\n{}", failures.join("\n"));
    println!("rasterize: {} frozen Studio i/j crops passed (FreeType {})", cases.len(), atlas.freetype);
}

fn base64(text: &str) -> Vec<u8> {
    let value = |byte: u8| match byte {
        b'A'..=b'Z' => byte - b'A',
        b'a'..=b'z' => byte - b'a' + 26,
        b'0'..=b'9' => byte - b'0' + 52,
        b'+' => 62,
        b'/' => 63,
        _ => panic!("invalid base64"),
    };
    let bytes: Vec<u8> = text.bytes().filter(|byte| *byte != b'=' && !byte.is_ascii_whitespace()).map(value).collect();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for chunk in bytes.chunks(4) {
        let mut buffer = 0u32;
        for (position, sextet) in chunk.iter().enumerate() {
            buffer |= u32::from(*sextet) << (18 - 6 * position);
        }
        for position in 0..chunk.len().saturating_sub(1) {
            out.push((buffer >> (16 - 8 * position)) as u8);
        }
    }
    out
}

#[test]
fn hinted_cache() {
    let root = repo();
    let token = &sha256_hex(format!("{:?}:{}", std::time::SystemTime::now(), std::process::id()).as_bytes())[..16];
    let cache = format!("{root}/out/selftest/hinted-{token}");
    // Keep fault injection away from the normal cache. Single test in this binary: no other thread reads
    // the environment while it changes.
    unsafe { std::env::set_var("RUR_HINTED_MAX_SIZE", "32") };
    let mut paths = FontPaths::for_repo(&root);
    paths.cache = cache.clone();
    let hinted = format!("{cache}/hinted");

    // Cold
    let mut setup = preload(&paths);
    assert_eq!(setup.registry.hinted_advance(setup.face, 30.0, setup.glyph), Some(24.0), "cold hinted advance: {:?}", setup.registry.diagnostics);
    let names = entries(&hinted);
    assert_eq!(names.len(), 1, "one font cache and no leftover temporary files: {names:?}");
    let cached_file = format!("{hinted}/{}", names[0]);
    let contents = std::fs::read(&cached_file).unwrap();
    let modified = std::fs::metadata(&cached_file).unwrap().modified().unwrap();
    let written: Value = serde_json::from_slice(&contents).unwrap();
    assert_eq!(written["freetype"], "2.14.2");
    assert_eq!(written["advances"].as_array().map(Vec::len), Some(32));

    // Warm
    let mut setup = preload(&paths);
    assert_eq!(setup.registry.hinted_advance(setup.face, 30.0, setup.glyph), Some(24.0), "warm hinted advance: {:?}", setup.registry.diagnostics);
    assert!(
        std::fs::read(&cached_file).unwrap() == contents && std::fs::metadata(&cached_file).unwrap().modified().unwrap() == modified,
        "warm cache rewritten"
    );

    // Corrupt cache regenerates
    std::fs::write(&cached_file, "{broken").unwrap();
    let mut setup = preload(&paths);
    assert_eq!(setup.registry.hinted_advance(setup.face, 30.0, setup.glyph), Some(24.0), "corrupt cache did not regenerate: {:?}", setup.registry.diagnostics);
    let regenerated: Value = serde_json::from_slice(&std::fs::read(&cached_file).unwrap()).unwrap();
    assert_eq!(regenerated["maxSize"].as_f64(), Some(32.0));
    assert_eq!(std::fs::read(&cached_file).unwrap(), contents, "regeneration is deterministic");

    // Outside RUR_HINTED_MAX_SIZE
    assert_eq!(setup.registry.hinted_advance(setup.face, 33.0, setup.glyph), None);
    let diagnostic = setup.registry.diagnostics.get(&format!("hinted:{}", setup.path)).cloned().unwrap_or_default();
    assert!(diagnostic.contains("outside hinted metric range"), "{diagnostic}");
    setup.registry.diagnostics.clear();

    // An unwritable cache still measures (in memory)
    let blocked = format!("{cache}/blocked");
    std::fs::write(&blocked, "not a directory").unwrap();
    let mut unwritable = paths.clone();
    unwritable.cache = blocked.clone();
    let mut setup = preload(&unwritable);
    assert_eq!(setup.registry.hinted_advance(setup.face, 30.0, setup.glyph), Some(24.0), "{:?}", setup.registry.diagnostics);

    // Concurrent writers of one cache leave one valid file and no temporary files
    std::fs::remove_file(&cached_file).unwrap();
    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| {
                let mut setup = preload(&paths);
                assert_eq!(setup.registry.hinted_advance(setup.face, 30.0, setup.glyph), Some(24.0));
            });
        }
    });
    assert_eq!(entries(&hinted), names, "concurrent writes left temporary files");
    assert_eq!(std::fs::read(&cached_file).unwrap(), contents);

    // A face without U+2026 or "." measures glyph zero, like metrics.py
    let content = setup.registry.content_root().unwrap().to_string();
    let fonts = format!("{content}/fonts");
    let emoji = std::fs::read(format!("{fonts}/TwemojiMozilla.ttf"));
    if let Ok(bytes) = emoji {
        let (glyph, advances) = rur_native::glyphs::truncation_advances(&bytes, 4).unwrap();
        assert_eq!((glyph, advances.len()), (0, 4));
    }

    dotted_glyphs(&root, &fonts);
    println!("hinted cache: cold/warm, corrupt-cache recovery, unwritable cache and range diagnostics passed");
    let _ = std::fs::remove_dir_all(&cache);
}
