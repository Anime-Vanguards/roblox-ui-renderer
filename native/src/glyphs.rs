//! Glyph atlases in process: FreeType 2.14.2 compiled into the crate (build.rs) through
//! glyphs/rur_glyph.c, the C port of tools/glyphs/rasterize.py, so no Python is needed for glyphs.
//! The coverage bitmaps, atlas packing and JSON are byte-identical to the Python pipeline's
//! (cli/rur.mjs buildAtlas).
//!
//! - [`atlas_entries`]: the glyphs a scene draws, a port of cli/rur.mjs `atlasEntries` (same order)
//! - [`Rasterizer`]: one FreeType library with its open faces and a bitmap cache; not shared between
//!   threads (FreeType objects are not thread-safe), each thread can own one
//! - [`Atlas`] / [`write_atlas`]: rasterize.py's shelf-packed RGBA atlas and its glyphs.json
//! - [`truncation_advances`]: tools/glyphs/metrics.py's hinted truncation advances
//!   (engine/text/hinted.rs)

use std::collections::HashMap;
use std::ffi::{c_int, c_long, c_uint, c_void};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;

/// rasterize.py's atlas width and the empty column / row after every glyph
pub const ATLAS_WIDTH: u32 = 2048;
pub const PADDING: u32 = 1;

#[repr(C)]
struct RurGlyph {
    width: c_int,
    rows: c_int,
    left: c_int,
    top: c_int,
    buffer: *mut u8,
}

unsafe extern "C" {
    fn rur_glyph_context_new() -> *mut c_void;
    fn rur_glyph_context_free(context: *mut c_void);
    fn rur_glyph_version(context: *mut c_void, major: *mut c_int, minor: *mut c_int, patch: *mut c_int);
    fn rur_glyph_face_open(context: *mut c_void, bytes: *const u8, length: usize) -> *mut c_void;
    fn rur_glyph_face_close(face: *mut c_void);
    fn rur_glyph_render(
        face: *mut c_void,
        ppem: f64,
        glyph: c_uint,
        stroke_px: f64,
        join: c_int,
        bold: c_long,
        italic: c_int,
        light: c_int,
        shift: c_long,
        out: *mut RurGlyph,
    ) -> c_int;
    fn rur_glyph_free(buffer: *mut u8);
    fn rur_glyph_layers(
        face: *mut c_void,
        glyph: c_uint,
        maximum: c_int,
        glyphs: *mut c_uint,
        rgba: *mut u8,
        foreground: *mut c_int,
    ) -> c_int;
    fn rur_glyph_truncation_advances(face: *mut c_void, maximum: c_int, glyph: *mut c_uint, advances: *mut f64) -> c_int;
}

// ---------------------------------------------------------------------------------------------
// JavaScript number semantics (atlasEntries runs on JSON values with JS coercions)

/// `Number.prototype.toString()` of `value` (shortest round-trip digits, JS exponent rules).
pub fn js_number_string(value: f64) -> String {
    if value.is_nan() {
        return "NaN".into();
    }
    if value == 0.0 {
        return "0".into();
    }
    if value.is_infinite() {
        return if value > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    if value < 0.0 {
        return format!("-{}", js_number_string(-value));
    }
    // Rust's `{:e}` prints the shortest round-trip digits: "d[.ddd]e<exp>"
    let formatted = format!("{value:e}");
    let (mantissa, exponent) = formatted.split_once('e').unwrap();
    let exponent: i32 = exponent.parse().unwrap();
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let k = digits.len() as i32;
    let n = exponent + 1;
    if k <= n && n <= 21 {
        format!("{digits}{}", "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        format!("{}.{}", &digits[..n as usize], &digits[n as usize..])
    } else if -6 < n && n <= 0 {
        format!("0.{}{digits}", "0".repeat((-n) as usize))
    } else {
        let sign = if n - 1 < 0 { '-' } else { '+' };
        let power = (n - 1).abs();
        if k == 1 { format!("{digits}e{sign}{power}") } else { format!("{}.{}e{sign}{power}", &digits[..1], &digits[1..]) }
    }
}

/// A JSON number as `JSON.stringify` writes it: integral values without a fraction.
pub fn js_json_number(value: f64) -> Value {
    if value.fract() == 0.0 && value.abs() < 9_007_199_254_740_992.0 {
        Value::from(value as i64)
    } else {
        serde_json::Number::from_f64(value).map(Value::Number).unwrap_or(Value::Null)
    }
}

/// JS `ToNumber` of a JSON value (`undefined` is `None`: NaN).
fn to_number(value: Option<&Value>) -> f64 {
    match value {
        None => f64::NAN,
        Some(Value::Null) => 0.0,
        Some(Value::Bool(flag)) => f64::from(u8::from(*flag)),
        Some(Value::Number(number)) => number.as_f64().unwrap_or(f64::NAN),
        Some(Value::String(text)) => string_to_number(text),
        Some(Value::Array(items)) => match items.len() {
            0 => 0.0,
            1 => match &items[0] {
                Value::Null => 0.0,
                Value::String(text) => string_to_number(text),
                Value::Number(number) => number.as_f64().unwrap_or(f64::NAN),
                _ => f64::NAN,
            },
            _ => f64::NAN,
        },
        Some(Value::Object(_)) => f64::NAN,
    }
}

fn string_to_number(text: &str) -> f64 {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return 0.0;
    }
    match trimmed {
        "Infinity" | "+Infinity" => return f64::INFINITY,
        "-Infinity" => return f64::NEG_INFINITY,
        _ => {}
    }
    let lower = trimmed.to_ascii_lowercase();
    for (prefix, radix) in [("0x", 16), ("0o", 8), ("0b", 2)] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            return u128::from_str_radix(rest, radix).map(|value| value as f64).unwrap_or(f64::NAN);
        }
    }
    if lower.contains("inf") || lower.contains("nan") {
        return f64::NAN;
    }
    trimmed.parse::<f64>().unwrap_or(f64::NAN)
}

/// JS truthiness of an optional JSON value.
fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(flag)) => *flag,
        Some(Value::Number(number)) => number.as_f64().is_some_and(|value| value != 0.0 && !value.is_nan()),
        Some(Value::String(text)) => !text.is_empty(),
        Some(_) => true,
    }
}

/// `value[index]` of a JS array with a JSON number index (undefined unless an integral index in range).
fn index<'a>(items: &'a [Value], position: &Value) -> Option<&'a Value> {
    let position = position.as_f64()?;
    if position < 0.0 || position.fract() != 0.0 {
        return None;
    }
    items.get(position as usize)
}

fn as_array(value: Option<&Value>) -> &[Value] {
    value.and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[])
}

/// JS `Math.round`: nearest integer, ties toward +infinity.
fn js_round(value: f64) -> f64 {
    let floor = value.floor();
    if value - floor >= 0.5 { floor + 1.0 } else { floor }
}

// ---------------------------------------------------------------------------------------------
// atlasEntries

/// One glyph variant a scene draws (cli/rur.mjs atlasEntries' `[file, ppem, glyphId, strokePx,
/// fontIndex, style]`).
#[derive(Clone, Debug, PartialEq)]
pub struct AtlasEntry {
    pub font_file: String,
    pub ppem: f64,
    pub glyph: u32,
    pub stroke_px: f64,
    pub font_index: u32,
    pub style: Option<String>,
}

impl AtlasEntry {
    /// cli/warm.mjs entryKey / the atlas JSON key joined with "|": `fontIndex|ppem|glyphId|strokePx[|style]`
    pub fn key(&self) -> String {
        let mut key = format!(
            "{}|{}|{}|{}",
            self.font_index,
            js_number_string(self.ppem),
            self.glyph,
            js_number_string(self.stroke_px)
        );
        if let Some(style) = &self.style {
            key.push('|');
            key.push_str(style);
        }
        key
    }

    /// The atlas JSON key `[fontIndex, ppem, glyphId, strokePx(, style)]`
    pub fn key_json(&self) -> Value {
        let mut key = vec![
            Value::from(self.font_index),
            js_json_number(self.ppem),
            Value::from(self.glyph),
            js_json_number(self.stroke_px),
        ];
        if let Some(style) = &self.style {
            key.push(Value::from(style.clone()));
        }
        Value::Array(key)
    }

    /// The rasterizer request row `[fontFile, ppem, glyphId, strokePx(, style)]` (rasterize.py's input)
    pub fn request_json(&self) -> Value {
        let mut row = vec![
            Value::from(self.font_file.clone()),
            js_json_number(self.ppem),
            Value::from(self.glyph),
            js_json_number(self.stroke_px),
        ];
        if let Some(style) = &self.style {
            row.push(Value::from(style.clone()));
        }
        Value::Array(row)
    }
}

/// cli/rur.mjs textStrokeRadius: floor(10 t) / 10 physical px truncated to 26.6, t = thickness * scale
/// in float32.
pub fn text_stroke_radius(thickness: f64, scale: f64) -> f64 {
    let fround = |value: f64| value as f32 as f64;
    let tenths = fround(fround(thickness) * fround(scale * 10.0));
    (tenths.floor() / 10.0 * 64.0).floor() / 64.0
}

/// cli/rur.mjs textStrokeJoin: "miter" / "bevel", round (None) otherwise
fn text_stroke_join(stroke: &Value) -> Option<&'static str> {
    let joins = stroke.get("joins");
    let text = match joins {
        Some(Value::String(text)) if !text.is_empty() => text.to_lowercase(),
        Some(Value::Array(items)) if items.len() == 1 => items[0].as_str().map(str::to_lowercase).unwrap_or_default(),
        _ => String::new(),
    };
    match text.as_str() {
        "miter" => Some("miter"),
        "bevel" => Some("bevel"),
        _ => None,
    }
}

/// cli/rur.mjs textBoldStrength: 2 * S (26.6 units) when a weight of 600 or more is asked of a lighter face
fn text_bold_strength(text: &Value, run: Option<&Value>, font: &Value, ppem: f64) -> f64 {
    let requested = match run {
        Some(run) => match run.get("weight") {
            Some(weight) if !weight.is_null() => to_number(Some(weight)),
            _ => {
                if truthy(run.get("bold")) {
                    700.0
                } else {
                    400.0
                }
            }
        },
        None => to_number(text.get("weight")),
    };
    let face_weight = run
        .filter(|run| truthy(Some(run)))
        .and_then(|run| run.get("face"))
        .filter(|face| truthy(Some(face)))
        .and_then(|face| face.get("weight"))
        .filter(|weight| weight.is_number())
        .map(|weight| to_number(Some(weight)))
        .unwrap_or_else(|| to_number(font.get("weight")));
    if !(requested >= 600.0) || !(face_weight < 600.0) {
        return 0.0;
    }
    // the pixel size S of the ppem, as render/text.rs font_size reads it (the two must agree: the
    // strength is part of the glyph's atlas key)
    let size_units = match font.get("sizeUnits") {
        Some(units) if units.is_number() => to_number(Some(units)),
        _ => to_number(font.get("ascender")) - to_number(font.get("descender")),
    };
    let size = js_round(ppem * size_units / to_number(font.get("unitsPerEm"))) - 1.0;
    if size > 0.0 { 2.0 * size } else { 0.0 }
}

/// The quarter-pixel phase of a glyph's pen in a text drawn at quarter pixels (`subpixel`: 4), None in
/// a scene saved before (whole-pixel pens, fully hinted glyphs). render/text.rs reads it the same way.
pub(crate) fn text_quarter(text: &Value, x: Option<&Value>, rotated: bool) -> Option<u8> {
    if text.get("subpixel").and_then(Value::as_f64) != Some(4.0) {
        return None;
    }
    if rotated {
        return Some(0);
    }
    let x = x.and_then(Value::as_f64).unwrap_or(0.0);
    Some((((x - x.floor()) * 4.0 + 0.5).floor() as u8) & 3)
}

/// The style token of a glyph rasterized without hinting: the glyphs under a text shadow's glow (a
/// UIShadow with Mode = Text and a BlurRadius), which Studio draws from the unhinted outline
pub const UNHINTED: &str = "u";
/// `style` with the unhinted token
pub fn unhinted_style(style: Option<&str>) -> String {
    match style.filter(|style| !style.is_empty()) {
        Some(style) => format!("{style}+{UNHINTED}"),
        None => UNHINTED.to_string(),
    }
}
/// Whether a node draws a text shadow's glow (render/text.rs text_shadow)
pub fn text_glow(node: &Value) -> bool {
    as_array(node.get("shadows")).iter().any(|shadow| {
        shadow.get("mode").and_then(Value::as_str) == Some("Text")
            && shadow.get("blur").and_then(Value::as_f64).is_some_and(|blur| blur > 0.0)
    })
}

/// cli/rur.mjs textGlyphStyle
fn text_glyph_style(join: Option<&str>, bold: f64, quarter: Option<u8>) -> Option<String> {
    let bold = (bold != 0.0 && !bold.is_nan()).then(|| format!("b{}", js_number_string(bold)));
    let quarter = quarter.map(|quarter| format!("q{quarter}"));
    let parts: Vec<String> = join.map(str::to_string).into_iter().chain(bold).chain(quarter).collect();
    if parts.is_empty() { None } else { Some(parts.join("+")) }
}

/// The glyphs a scene draws, in cli/rur.mjs atlasEntries' order (a JS Map: the first insertion of a
/// key keeps its position). Stroked variants: every Contextual UIStroke for the glyphs of runs
/// without a `<stroke>` tag, the run's own `<stroke>` otherwise.
///
/// Where the JS throws (a stroked glyph in a scene without `viewport`, a null stroke) this skips the
/// stroked variant instead. Glyph rows whose font index, glyph id or ppem is not a number are
/// skipped (the Python rasterizer rejects them).
pub fn atlas_entries(scene: &Value) -> Vec<AtlasEntry> {
    let fonts = as_array(scene.get("fonts"));
    let scale = to_number(scene.get("viewport").and_then(|viewport| viewport.get("scale")));
    let mut entries: Vec<AtlasEntry> = Vec::new();
    let mut positions: HashMap<String, usize> = HashMap::new();
    let mut want = |entry: AtlasEntry| {
        let key = entry.key();
        match positions.get(&key) {
            Some(&position) => entries[position] = entry,
            None => {
                positions.insert(key, entries.len());
                entries.push(entry);
            }
        }
    };
    for node in as_array(scene.get("nodes")) {
        let Some(text) = node.get("text").filter(|text| truthy(Some(text))) else {
            continue;
        };
        let strokes: Vec<&Value> = as_array(node.get("strokes"))
            .iter()
            .filter(|stroke| stroke.get("mode").and_then(Value::as_str) == Some("Contextual"))
            .collect();
        let runs = as_array(text.get("runs"));
        let glow = text_glow(node);
        for line in as_array(text.get("lines")) {
            let copies = as_array(line.get("copies"));
            for (row_index, row) in as_array(line.get("glyphs")).iter().enumerate() {
                let Some(row) = row.as_array() else {
                    continue;
                };
                let (Some(font_index), Some(glyph), Some(ppem)) = (row.first(), row.get(1), row.get(3)) else {
                    continue;
                };
                let Some(font) = index(fonts, font_index).filter(|font| font.is_object()) else {
                    continue;
                };
                let Some(file) = font.get("file").and_then(Value::as_str).filter(|file| !file.is_empty()) else {
                    continue;
                };
                let (Some(font_index), Some(glyph), Some(ppem)) = (
                    font_index.as_u64().and_then(|value| u32::try_from(value).ok()),
                    glyph.as_u64().and_then(|value| u32::try_from(value).ok()),
                    ppem.as_f64(),
                ) else {
                    continue;
                };
                let run = row.get(4).and_then(|position| index(runs, position)).filter(|run| truthy(Some(run)));
                let bold = text_bold_strength(text, run, font, ppem);
                let quarter = text_quarter(text, row.get(2), node.get("matrix").is_some_and(Value::is_array));
                want(AtlasEntry {
                    font_file: file.to_string(),
                    ppem,
                    glyph,
                    stroke_px: 0.0,
                    font_index,
                    style: text_glyph_style(None, bold, quarter),
                });
                if glow {
                    want(AtlasEntry {
                        font_file: file.to_string(),
                        ppem,
                        glyph,
                        stroke_px: 0.0,
                        font_index,
                        style: Some(unhinted_style(text_glyph_style(None, bold, quarter).as_deref())),
                    });
                }
                // The legacy TextStroke copies of a tied pen that round another way (export.rs `tie_step`)
                for copy in copies.iter().filter(|copy| copy.get(0).and_then(Value::as_u64) == Some(row_index as u64)) {
                    for side in 1..=2 {
                        let quarter = text_quarter(text, copy.get(side), false);
                        want(AtlasEntry {
                            font_file: file.to_string(),
                            ppem,
                            glyph,
                            stroke_px: 0.0,
                            font_index,
                            style: text_glyph_style(None, bold, quarter),
                        });
                    }
                }
                let tag = run.and_then(|run| run.get("stroke")).filter(|tag| truthy(Some(tag)));
                let chosen: Vec<&Value> = match tag {
                    Some(tag) => vec![tag],
                    None => strokes.clone(),
                };
                for stroke in chosen {
                    let px = text_stroke_radius(to_number(stroke.get("thickness")), scale);
                    if px > 0.0 {
                        want(AtlasEntry {
                            font_file: file.to_string(),
                            ppem,
                            glyph,
                            stroke_px: px,
                            font_index,
                            style: text_glyph_style(text_stroke_join(stroke), bold, quarter),
                        });
                    }
                }
            }
        }
    }
    entries
}

// ---------------------------------------------------------------------------------------------
// Rasterizer

/// One glyph's coverage (8-bit, `width * rows`, tightly packed) and its bitmap origin. A colour glyph
/// (COLR layers: the emoji fonts) has no coverage of its own but `layers`, bottom first.
#[derive(Debug)]
pub struct GlyphBitmap {
    pub width: u32,
    pub rows: u32,
    pub left: i32,
    pub top: i32,
    pub coverage: Box<[u8]>,
    pub layers: Vec<GlyphLayer>,
}

/// A colour glyph's layer: a plain glyph's coverage drawn in its palette colour (straight RGBA), or in the
/// text colour when `foreground`
#[derive(Debug)]
pub struct GlyphLayer {
    pub bitmap: Arc<GlyphBitmap>,
    pub color: [u8; 4],
    pub foreground: bool,
}

/// A colour glyph's layers: (layer glyph, RGBA, foreground)
type Layers = Arc<[(u32, [u8; 4], bool)]>;

/// rasterize.py parse_style: (join, bold strength, italic) of "miter+b108+i"
fn parse_style(style: Option<&str>) -> (c_int, i64, bool, Option<u8>) {
    let (mut join, mut bold, mut italic, mut quarter) = (0, 0i64, false, None);
    for token in style.unwrap_or("").split('+') {
        match token {
            "round" => join = 0,
            "miter" => join = 1,
            "bevel" => join = 2,
            "i" => italic = true,
            "q0" => quarter = Some(0),
            "q1" => quarter = Some(1),
            "q2" => quarter = Some(2),
            "q3" => quarter = Some(3),
            _ => {
                if let Some(digits) = token.strip_prefix('b') {
                    if !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()) {
                        bold = digits.parse().unwrap_or(i64::MAX);
                    }
                }
            }
        }
    }
    (join, bold, italic, quarter)
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Stamp {
    size: u64,
    modified: Option<SystemTime>,
}

fn stamp(path: &str) -> Option<Stamp> {
    let metadata = std::fs::metadata(path).ok()?;
    Some(Stamp { size: metadata.len(), modified: metadata.modified().ok() })
}

/// (ppem bits, glyph, stroke bits, style) within one font file
type BitmapKey = (u64, u32, u64, Option<String>);

struct Font {
    face: *mut c_void,
    stamp: Option<Stamp>,
    bitmaps: HashMap<BitmapKey, Arc<GlyphBitmap>>,
    layers: HashMap<u32, Layers>,
}

/// Rasterized glyph bitmaps are kept up to this many coverage bytes (then the cache starts over)
const BITMAP_CACHE_BYTES: usize = 256 * 1024 * 1024;

/// FreeType with its open faces and a bitmap cache keyed like the physical atlas key (file, ppem,
/// glyph, strokePx, style). A font's faces and bitmaps are dropped when its file's size or
/// modification time changes. Not `Sync`: FreeType objects belong to one thread at a time.
pub struct Rasterizer {
    context: *mut c_void,
    fonts: HashMap<String, Font>,
    bytes: usize,
    version: String,
}

// The library and its faces are only used through &mut self
unsafe impl Send for Rasterizer {}

impl Rasterizer {
    pub fn new() -> Result<Rasterizer> {
        let context = unsafe { rur_glyph_context_new() };
        if context.is_null() {
            bail!("FreeType initialization failed");
        }
        let (mut major, mut minor, mut patch) = (0, 0, 0);
        unsafe { rur_glyph_version(context, &mut major, &mut minor, &mut patch) };
        Ok(Rasterizer { context, fonts: HashMap::new(), bytes: 0, version: format!("{major}.{minor}.{patch}") })
    }

    /// The FreeType version compiled in ("2.14.2")
    pub fn freetype_version(&self) -> &str {
        &self.version
    }

    fn close_font(&mut self, file: &str) {
        if let Some(font) = self.fonts.remove(file) {
            self.bytes -= font.bitmaps.values().map(|bitmap| bitmap.coverage.len()).sum::<usize>();
            unsafe { rur_glyph_face_close(font.face) };
        }
    }

    /// Drops a font's face and bitmaps when its file changed since it was opened.
    pub fn refresh(&mut self, file: &str) {
        if let Some(font) = self.fonts.get(file) {
            if stamp(file) != font.stamp {
                self.close_font(file);
            }
        }
    }

    /// Drops every open face and cached bitmap.
    pub fn clear(&mut self) {
        let files: Vec<String> = self.fonts.keys().cloned().collect();
        for file in files {
            self.close_font(&file);
        }
    }

    fn font(&mut self, file: &str) -> Result<&mut Font> {
        if !self.fonts.contains_key(file) {
            let before = stamp(file);
            let bytes = std::fs::read(file).with_context(|| format!("cannot open font {file}"))?;
            let face = unsafe { rur_glyph_face_open(self.context, bytes.as_ptr(), bytes.len()) };
            if face.is_null() {
                bail!("FreeType cannot open font {file}");
            }
            self.fonts.insert(file.to_string(), Font { face, stamp: before, bitmaps: HashMap::new(), layers: HashMap::new() });
        }
        Ok(self.fonts.get_mut(file).unwrap())
    }

    /// A glyph's COLR layers (empty when it is not a colour glyph)
    fn color_layers(&mut self, file: &str, glyph: u32) -> Result<Layers> {
        let font = self.font(file)?;
        if let Some(layers) = font.layers.get(&glyph) {
            return Ok(layers.clone());
        }
        let count = unsafe { rur_glyph_layers(font.face, glyph, 0, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut()) };
        let count = count.max(0) as usize;
        let mut glyphs = vec![0 as c_uint; count];
        let mut rgba = vec![0u8; count * 4];
        let mut foreground = vec![0 as c_int; count];
        if count > 0 {
            unsafe {
                rur_glyph_layers(font.face, glyph, count as c_int, glyphs.as_mut_ptr(), rgba.as_mut_ptr(), foreground.as_mut_ptr())
            };
        }
        let layers: Layers = (0..count)
            .map(|index| (glyphs[index], [rgba[index * 4], rgba[index * 4 + 1], rgba[index * 4 + 2], rgba[index * 4 + 3]], foreground[index] != 0))
            .collect();
        font.layers.insert(glyph, layers.clone());
        Ok(layers)
    }

    /// One glyph's coverage, as rasterize.py renders `[file, ppem, glyph, strokePx, style]`. A glyph
    /// FreeType cannot load or stroke is empty; a font that cannot be opened or sized is an error.
    /// A colour glyph's fill is its layers, each rasterized as a plain glyph without the style's synthetic
    /// bold or slant (Studio draws an emoji the same under a bold weight, fixtures/text-probe-emoji.luau
    /// "Bold"); its stroked variants are the base glyph's outline, which a COLR font leaves empty, so a
    /// UIStroke draws nothing around it (as in Studio, "Stroke").
    pub fn glyph(&mut self, file: &str, ppem: f64, glyph: u32, stroke_px: f64, style: Option<&str>) -> Result<Arc<GlyphBitmap>> {
        let key: BitmapKey = (ppem.to_bits(), glyph, stroke_px.to_bits(), style.map(str::to_string));
        if let Some(found) = self.fonts.get(file).and_then(|font| font.bitmaps.get(&key)) {
            return Ok(found.clone());
        }
        if stroke_px == 0.0 {
            let layers = self.color_layers(file, glyph)?;
            if !layers.is_empty() {
                let mut out = Vec::with_capacity(layers.len());
                for &(layer, color, foreground) in layers.iter() {
                    // A layer is an outline (FreeType loads layers without FT_LOAD_COLOR too)
                    let quarter = style.and_then(|style| style.split('+').find(|token| token.starts_with('q') && token.len() == 2));
                    out.push(GlyphLayer { bitmap: self.rasterize(file, ppem, layer, 0.0, quarter)?, color, foreground });
                }
                let bitmap = Arc::new(GlyphBitmap { width: 0, rows: 0, left: 0, top: 0, coverage: Box::new([]), layers: out });
                self.fonts.get_mut(file).unwrap().bitmaps.insert(key, bitmap.clone());
                return Ok(bitmap);
            }
        }
        self.rasterize(file, ppem, glyph, stroke_px, style)
    }

    /// glyph()'s FreeType rendering of one outline (cached)
    fn rasterize(&mut self, file: &str, ppem: f64, glyph: u32, stroke_px: f64, style: Option<&str>) -> Result<Arc<GlyphBitmap>> {
        let key: BitmapKey = (ppem.to_bits(), glyph, stroke_px.to_bits(), style.map(str::to_string));
        if let Some(found) = self.fonts.get(file).and_then(|font| font.bitmaps.get(&key)) {
            if found.layers.is_empty() {
                return Ok(found.clone());
            }
        }
        let (join, bold, italic, quarter) = parse_style(style);
        let unhinted = style.is_some_and(|style| style.split('+').any(|token| token == UNHINTED));
        let font = self.font(file)?;
        let mut out = RurGlyph { width: 0, rows: 0, left: 0, top: 0, buffer: std::ptr::null_mut() };
        let status = unsafe {
            rur_glyph_render(
                font.face,
                ppem,
                glyph,
                stroke_px,
                join,
                bold.clamp(c_long::MIN as i64, c_long::MAX as i64) as c_long,
                c_int::from(italic),
                if unhinted { 2 } else { c_int::from(quarter.is_some()) },
                c_long::from(quarter.unwrap_or(0)) * 16,
                &mut out,
            )
        };
        if status != 0 {
            bail!("FreeType cannot size {file} for ppem {} (error {status})", js_number_string(ppem));
        }
        let length = out.width.max(0) as usize * out.rows.max(0) as usize;
        let coverage: Box<[u8]> = if out.buffer.is_null() || length == 0 {
            Box::new([])
        } else {
            let copy = unsafe { std::slice::from_raw_parts(out.buffer, length) }.into();
            unsafe { rur_glyph_free(out.buffer) };
            copy
        };
        let bitmap = Arc::new(GlyphBitmap {
            width: if length == 0 { 0 } else { out.width as u32 },
            rows: if length == 0 { 0 } else { out.rows as u32 },
            left: out.left,
            top: out.top,
            coverage,
            layers: Vec::new(),
        });
        if self.bytes + bitmap.coverage.len() > BITMAP_CACHE_BYTES {
            for font in self.fonts.values_mut() {
                font.bitmaps.clear();
            }
            self.bytes = 0;
        }
        self.bytes += bitmap.coverage.len();
        self.fonts.get_mut(file).unwrap().bitmaps.insert(key, bitmap.clone());
        Ok(bitmap)
    }

    /// rasterize.py: every entry's bitmap shelf-packed into one RGBA atlas (white, alpha = coverage); a
    /// colour glyph's layers are packed after its own (empty) bitmap, in its row's `layers`.
    /// Fonts whose files changed since they were opened are reloaded first.
    pub fn build_atlas(&mut self, entries: &[AtlasEntry]) -> Result<Atlas> {
        let mut checked = std::collections::HashSet::new();
        for entry in entries {
            if checked.insert(entry.font_file.as_str()) {
                self.refresh(&entry.font_file);
            }
        }
        let mut bitmaps = Vec::with_capacity(entries.len());
        for entry in entries {
            bitmaps.push(self.glyph(&entry.font_file, entry.ppem, entry.glyph, entry.stroke_px, entry.style.as_deref())?);
        }
        // Every bitmap to pack, in order: each entry's own, then its layers'
        let packed: Vec<&GlyphBitmap> = bitmaps
            .iter()
            .flat_map(|bitmap| std::iter::once(bitmap.as_ref()).chain(bitmap.layers.iter().map(|layer| layer.bitmap.as_ref())))
            .collect();
        let mut placements = Vec::with_capacity(packed.len());
        let (mut x, mut y, mut shelf) = (0u32, 0u32, 0u32);
        for bitmap in &packed {
            let (w, h) = (bitmap.width, bitmap.rows);
            if x + w + PADDING > ATLAS_WIDTH {
                x = 0;
                y += shelf + PADDING;
                shelf = 0;
            }
            if w > ATLAS_WIDTH {
                bail!("a glyph bitmap ({w} px) is wider than the atlas");
            }
            placements.push((x, y));
            x += w + PADDING;
            shelf = shelf.max(h);
        }
        let height = (y + shelf + PADDING).max(1);
        let stride = ATLAS_WIDTH as usize * 4;
        let mut rgba = vec![255u8; stride * height as usize];
        for alpha in rgba.iter_mut().skip(3).step_by(4) {
            *alpha = 0;
        }
        for (bitmap, &(x, y)) in packed.iter().zip(&placements) {
            for row in 0..bitmap.rows as usize {
                let source = &bitmap.coverage[row * bitmap.width as usize..(row + 1) * bitmap.width as usize];
                let start = (y as usize + row) * stride + x as usize * 4;
                for (column, value) in source.iter().enumerate() {
                    rgba[start + column * 4 + 3] = *value;
                }
            }
        }
        let mut glyphs = Vec::with_capacity(entries.len());
        let mut placed = placements.into_iter();
        for (entry, bitmap) in entries.iter().zip(&bitmaps) {
            let (x, y) = placed.next().unwrap();
            let layers = bitmap
                .layers
                .iter()
                .map(|layer| {
                    let (x, y) = placed.next().unwrap();
                    let b = &layer.bitmap;
                    AtlasLayer { x, y, w: b.width, h: b.rows, left: b.left, top: b.top, color: layer.color, foreground: layer.foreground }
                })
                .collect();
            glyphs.push(AtlasGlyph { key: entry.key_json(), x, y, w: bitmap.width, h: bitmap.rows, left: bitmap.left, top: bitmap.top, layers });
        }
        Ok(Atlas { width: ATLAS_WIDTH, height, rgba, glyphs, freetype: self.version.clone() })
    }

    /// metrics.py: the truncation glyph (U+2026, else ".") and its hinted advances at physical sizes
    /// 1..=maximum. Uses a face of its own (the atlas faces' state is not touched).
    pub fn truncation_advances(&mut self, bytes: &[u8], maximum: u32) -> Result<(u32, Vec<f64>)> {
        truncation_advances_in(self.context, bytes, maximum)
    }
}

impl Drop for Rasterizer {
    fn drop(&mut self) {
        for (_, font) in self.fonts.drain() {
            unsafe { rur_glyph_face_close(font.face) };
        }
        unsafe { rur_glyph_context_free(self.context) };
    }
}

fn truncation_advances_in(context: *mut c_void, bytes: &[u8], maximum: u32) -> Result<(u32, Vec<f64>)> {
    if maximum == 0 || maximum > c_int::MAX as u32 {
        bail!("maxSize must be a positive integer");
    }
    let face = unsafe { rur_glyph_face_open(context, bytes.as_ptr(), bytes.len()) };
    if face.is_null() {
        bail!("FreeType cannot open the font");
    }
    let mut glyph: c_uint = 0;
    let mut advances = vec![0.0f64; maximum as usize];
    let status = unsafe { rur_glyph_truncation_advances(face, maximum as c_int, &mut glyph, advances.as_mut_ptr()) };
    unsafe { rur_glyph_face_close(face) };
    if status != 0 {
        bail!("FreeType failed to load the truncation glyph (error {status})");
    }
    Ok((glyph, advances))
}

/// metrics.py generate: `(glyph, advances)` of a font file's bytes, with a FreeType library of its own
/// (any thread).
pub fn truncation_advances(bytes: &[u8], maximum: u32) -> Result<(u32, Vec<f64>)> {
    let context = unsafe { rur_glyph_context_new() };
    if context.is_null() {
        bail!("FreeType initialization failed");
    }
    let result = truncation_advances_in(context, bytes, maximum);
    unsafe { rur_glyph_context_free(context) };
    result
}

/// The FreeType version compiled into the crate ("2.14.2")
pub fn freetype_version() -> &'static str {
    static VERSION: OnceLock<String> = OnceLock::new();
    VERSION.get_or_init(|| Rasterizer::new().map(|rasterizer| rasterizer.version.clone()).unwrap_or_default())
}

/// A process-wide rasterizer for callers without one of their own (serialized by its mutex).
pub fn shared() -> &'static Mutex<Rasterizer> {
    static SHARED: OnceLock<Mutex<Rasterizer>> = OnceLock::new();
    SHARED.get_or_init(|| Mutex::new(Rasterizer::new().expect("FreeType initialization")))
}

/// [`Rasterizer::build_atlas`] on the [`shared`] rasterizer
pub fn build_atlas(entries: &[AtlasEntry]) -> Result<Atlas> {
    shared().lock().map_err(|_| anyhow!("glyph rasterizer poisoned"))?.build_atlas(entries)
}

// ---------------------------------------------------------------------------------------------
// Atlas

#[derive(Clone, Debug, PartialEq)]
pub struct AtlasGlyph {
    /// `[fontIndex, ppem, glyphId, strokePx(, style)]`
    pub key: Value,
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub left: i32,
    pub top: i32,
    /// A colour glyph's layers, bottom first (JSON `layers`, written only for colour glyphs)
    pub layers: Vec<AtlasLayer>,
}

/// One layer of a colour glyph in the atlas: its coverage's placement and bearings, its palette colour
/// (straight RGBA) or the text colour (`foreground`; JSON `"foreground": true` instead of `color`)
#[derive(Clone, Debug, PartialEq)]
pub struct AtlasLayer {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub left: i32,
    pub top: i32,
    pub color: [u8; 4],
    pub foreground: bool,
}

impl AtlasLayer {
    pub fn json(&self) -> Value {
        let mut row = serde_json::Map::new();
        row.insert("x".into(), self.x.into());
        row.insert("y".into(), self.y.into());
        row.insert("w".into(), self.w.into());
        row.insert("h".into(), self.h.into());
        row.insert("left".into(), self.left.into());
        row.insert("top".into(), self.top.into());
        if self.foreground {
            row.insert("foreground".into(), true.into());
        } else {
            row.insert("color".into(), Value::from(self.color.to_vec()));
        }
        Value::Object(row)
    }
}

pub struct Atlas {
    pub width: u32,
    pub height: u32,
    /// Straight RGBA8, `width * height * 4`: white with alpha = coverage (rasterize.py's image)
    pub rgba: Vec<u8>,
    pub glyphs: Vec<AtlasGlyph>,
    pub freetype: String,
}

impl Atlas {
    fn glyphs_json(&self) -> Value {
        Value::Array(
            self.glyphs
                .iter()
                .map(|glyph| {
                    let mut row = serde_json::Map::new();
                    row.insert("key".into(), glyph.key.clone());
                    row.insert("x".into(), glyph.x.into());
                    row.insert("y".into(), glyph.y.into());
                    row.insert("w".into(), glyph.w.into());
                    row.insert("h".into(), glyph.h.into());
                    row.insert("left".into(), glyph.left.into());
                    row.insert("top".into(), glyph.top.into());
                    if !glyph.layers.is_empty() {
                        row.insert("layers".into(), Value::Array(glyph.layers.iter().map(AtlasLayer::json).collect()));
                    }
                    Value::Object(row)
                })
                .collect(),
        )
    }

    /// glyphs.json's text: `{"width","height","freetype","glyphs":[{"key","x","y","w","h","left","top"(,"layers")}]}`
    /// in cli/rur.mjs buildAtlas' field order
    pub fn json_text(&self) -> String {
        let mut out = format!(
            "{{\"width\":{},\"height\":{},\"freetype\":{},\"glyphs\":[",
            self.width,
            self.height,
            Value::from(self.freetype.clone())
        );
        for (position, glyph) in self.glyphs.iter().enumerate() {
            if position > 0 {
                out.push(',');
            }
            out.push_str(&format!(
                "{{\"key\":{},\"x\":{},\"y\":{},\"w\":{},\"h\":{},\"left\":{},\"top\":{}",
                glyph.key, glyph.x, glyph.y, glyph.w, glyph.h, glyph.left, glyph.top
            ));
            if !glyph.layers.is_empty() {
                let layers: Vec<Value> = glyph.layers.iter().map(AtlasLayer::json).collect();
                out.push_str(&format!(",\"layers\":{}", Value::Array(layers)));
            }
            out.push('}');
        }
        out.push_str("]}");
        out
    }

    /// The scene manifest's `atlas` value `{src: "glyphs.png", width, height, glyphs}`
    pub fn manifest(&self) -> Value {
        let mut atlas = serde_json::Map::new();
        atlas.insert("src".into(), "glyphs.png".into());
        atlas.insert("width".into(), self.width.into());
        atlas.insert("height".into(), self.height.into());
        atlas.insert("glyphs".into(), self.glyphs_json());
        Value::Object(atlas)
    }

    /// The atlas as an (uncompressed-fast) RGBA PNG
    pub fn png(&self) -> Result<Vec<u8>> {
        use image::ImageEncoder;
        let mut bytes = Vec::new();
        image::codecs::png::PngEncoder::new_with_quality(
            &mut bytes,
            image::codecs::png::CompressionType::Fast,
            image::codecs::png::FilterType::Sub,
        )
        .write_image(&self.rgba, self.width, self.height, image::ExtendedColorType::Rgba8)?;
        Ok(bytes)
    }
}

/// Writes glyphs.png and glyphs.json (cli/rur.mjs buildAtlas' files) and returns the manifest's
/// `atlas` value.
pub fn write_atlas(atlas: &Atlas, png_path: &Path, json_path: &Path) -> Result<Value> {
    std::fs::write(png_path, atlas.png()?).with_context(|| format!("writing {}", png_path.display()))?;
    std::fs::write(json_path, atlas.json_text()).with_context(|| format!("writing {}", json_path.display()))?;
    Ok(atlas.manifest())
}

/// cli/rur.mjs buildAtlas: the scene's atlas written to `out_dir`/glyphs.png + glyphs.json, or None
/// when the scene draws no glyphs.
pub fn build_scene_atlas(rasterizer: &mut Rasterizer, scene: &Value, out_dir: &Path) -> Result<Option<Value>> {
    let entries = atlas_entries(scene);
    if entries.is_empty() {
        return Ok(None);
    }
    let atlas = rasterizer.build_atlas(&entries)?;
    let png: PathBuf = out_dir.join("glyphs.png");
    write_atlas(&atlas, &png, &out_dir.join("glyphs.json")).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A quarter-pixel text's glyph takes the bitmap of its pen's quarter, half up; rotated text the
    /// unshifted one at every pen; a scene saved before quarter pens has no phase.
    #[test]
    fn quarter_phase_of_a_pen() {
        let text = json!({ "subpixel": 4 });
        let phase = |x: f64, rotated: bool| text_quarter(&text, Some(&json!(x)), rotated);
        assert_eq!([phase(12.0, false), phase(12.25, false), phase(12.5, false), phase(12.75, false)], [Some(0), Some(1), Some(2), Some(3)]);
        assert_eq!(phase(-0.25, false), Some(3));
        assert_eq!(phase(33.078125, true), Some(0));
        assert_eq!(phase(105.796875, true), Some(0));
        assert_eq!(text_quarter(&json!({}), Some(&json!(12.25)), false), None);
        assert_eq!(text_glyph_style(Some("round"), 0.0, Some(3)).as_deref(), Some("round+q3"));
    }

    /// The synthetic bold strength is 2 S for the pixel size S of the ppem, whichever units the scene's
    /// font spreads over S + 1 px (render/text.rs font_size reads the same size).
    #[test]
    fn bold_strength_reads_the_size_units() {
        let text = json!({ "weight": 700 });
        let sized = json!({"ascender":1854,"descender":-434,"unitsPerEm":2048,"lineGap":67,"sizeUnits":2355,"weight":400});
        let saved = json!({"ascender":1854,"descender":-434,"unitsPerEm":2048,"lineGap":67,"weight":400});
        assert_eq!(text_bold_strength(&text, None, &sized, 55.0 * 2048.0 / 2355.0), 108.0);
        assert_eq!(text_bold_strength(&text, None, &saved, 55.0 * 2048.0 / 2288.0), 108.0);
    }

    #[test]
    fn js_numbers_format_like_javascript() {
        for (value, text) in [
            (0.0, "0"),
            (-0.0, "0"),
            (18.0, "18"),
            (1.5, "1.5"),
            (0.1, "0.1"),
            (1e-7, "1e-7"),
            (1.25e-7, "1.25e-7"),
            (0.000001, "0.000001"),
            (1e21, "1e+21"),
            (123456789012345680000.0, "123456789012345680000"),
            (2.015625, "2.015625"),
            (-3.5, "-3.5"),
            (1.0 / 3.0, "0.3333333333333333"),
        ] {
            assert_eq!(js_number_string(value), text, "{value:e}");
        }
    }

    /// A Twemoji glyph is drawn from its COLR layers: the atlas row holds no coverage of its own but each
    /// layer's, with its palette colour; its stroke is empty and a bold style does not change it.
    #[test]
    fn colour_glyphs_rasterize_as_layers() {
        let Some(root) = crate::engine::text::fonts::discover_content_root() else {
            eprintln!("colour glyph checks skipped (no Studio install)");
            return;
        };
        let file = format!("{}/fonts/TwemojiMozilla.ttf", root.trim_end_matches(['/', '\\']));
        if !Path::new(&file).is_file() {
            eprintln!("colour glyph checks skipped ({file} missing)");
            return;
        }
        let mut rasterizer = Rasterizer::new().unwrap();
        let glyph = (1..4000).find(|&glyph| !rasterizer.color_layers(&file, glyph).unwrap().is_empty()).expect("a COLR glyph");
        let entry = |stroke_px: f64, style: Option<&str>| AtlasEntry {
            font_file: file.clone(),
            ppem: 48.0,
            glyph,
            stroke_px,
            font_index: 0,
            style: style.map(str::to_string),
        };
        let atlas = rasterizer.build_atlas(&[entry(0.0, None), entry(3.0, None), entry(0.0, Some("b42"))]).unwrap();
        let [plain, stroked, bold] = &atlas.glyphs[..] else { panic!("three rows") };
        assert_eq!((plain.w, plain.h), (0, 0));
        assert!(!plain.layers.is_empty() && plain.layers.iter().any(|layer| layer.w > 0 && layer.h > 0));
        assert!(plain.layers.iter().all(|layer| !layer.foreground && layer.color[3] == 255));
        assert!(stroked.layers.is_empty() && stroked.w == 0, "a UIStroke draws nothing around an emoji");
        let sizes = |row: &AtlasGlyph| row.layers.iter().map(|layer| (layer.w, layer.h, layer.left, layer.top, layer.color)).collect::<Vec<_>>();
        assert_eq!(sizes(bold), sizes(plain), "no synthetic bold on colour glyphs");
        let json: Value = serde_json::from_str(&atlas.json_text()).unwrap();
        assert_eq!(json["glyphs"][0]["layers"].as_array().unwrap().len(), plain.layers.len());
        assert!(json["glyphs"][1].get("layers").is_none());
    }

    #[test]
    fn stroke_radius_uses_float32_tenths() {
        assert_eq!(text_stroke_radius(1.0, 1.5), 1.5);
        assert_eq!(text_stroke_radius(2.0, 1.0), 2.0);
        // f32(0.7) * f32(15) rounds just below 10.5 tenths
        assert_eq!(text_stroke_radius(0.7, 1.5), 1.0);
        assert_eq!(text_stroke_radius(0.0, 1.5), 0.0);
        assert!(text_stroke_radius(f64::NAN, 1.5).is_nan());
    }

    /// A label under a text shadow with a blur also wants each glyph unhinted (the glow's outlines); a
    /// hard text shadow and a frame shadow want nothing more
    #[test]
    fn a_text_glow_wants_unhinted_glyphs() {
        let scene = |shadow: Value| {
            json!({
                "viewport": {"scale": 1.5},
                "fonts": [{"file": "a.ttf", "weight": 400, "ascender": 900, "descender": -300, "unitsPerEm": 1000}],
                "nodes": [{"text": {"subpixel": 4, "lines": [{"glyphs": [[0, 5, 0.25, 18]]}]}, "shadows": [shadow]}]
            })
        };
        let keys = |shadow: Value| atlas_entries(&scene(shadow)).iter().map(AtlasEntry::key).collect::<Vec<String>>();
        assert_eq!(keys(json!({"mode": "Text", "blur": 4})), ["0|18|5|0|q1", "0|18|5|0|q1+u"]);
        assert_eq!(keys(json!({"mode": "Text", "blur": 0})), ["0|18|5|0|q1"]);
        assert_eq!(keys(json!({"mode": "Shape", "blur": 4})), ["0|18|5|0|q1"]);
        assert_eq!(unhinted_style(None), "u");
        assert_eq!(unhinted_style(Some("b42+q3")), "b42+q3+u");
    }
    #[test]
    fn entries_keep_map_order_and_variants() {
        let scene = json!({
            "viewport": {"scale": 1.5},
            "fonts": [{"file": "a.ttf", "weight": 400, "ascender": 900, "descender": -300, "unitsPerEm": 1000}],
            "nodes": [
                {"text": {"weight": 700, "lines": [{"glyphs": [[0, 5, 0, 18], [0, 6, 0, 18]]}]},
                 "strokes": [{"mode": "Contextual", "thickness": 1, "joins": "Miter"}, {"mode": "Border", "thickness": 2}]},
                {"text": {"weight": 400, "runs": [{"stroke": {"thickness": 2, "joins": "bevel"}}, {"bold": true}],
                  "lines": [{"glyphs": [[0, 5, 0, 18, 0], [0, 6, 0, 18, 1], [0, 5, 0, 18]]}]}},
                {"text": {"lines": [{"glyphs": [[1, 5, 0, 18], [0, 7, 0, 18.5]]}]}}
            ]
        });
        let keys: Vec<String> = atlas_entries(&scene).iter().map(AtlasEntry::key).collect();
        // bold: round(18 * 1200 / 1000) - 1 = 21 -> b42
        assert_eq!(
            keys,
            [
                "0|18|5|0|b42",
                "0|18|5|1.5|miter+b42",
                "0|18|6|0|b42",
                "0|18|6|1.5|miter+b42",
                "0|18|5|0",
                "0|18|5|3|bevel",
                "0|18.5|7|0",
            ]
        );
    }
}
