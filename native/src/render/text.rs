//! Exported, FreeType-shaped text. No native font substitution or reshaping occurs here.
//! Arithmetic and draw order follow Studio's measured text pipeline (notes/text.md).
use crate::assets::{AssetStore, GlyphKey};
use serde_json::Value;
use std::{fmt, sync::Arc};

#[derive(Clone, Debug)]
pub struct TextQuad {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<[u8]>,
    /// Applied after filtering, as Canvas globalAlpha.
    pub opacity: f64,
    /// Editing backgrounds/caret use axis-aligned target rectangles.
    pub translation_only: bool,
    /// 0 = unrestricted, 1 = outside all rects, 2 = inside any rect.
    pub selection_mode: u8,
    /// Device-local [left, top, right, bottom]; add node translation only, like buildTextEditing.
    pub selection_rects: Vec<[f64; 4]>,
}

#[derive(Debug)]
pub struct TextError(pub String);
impl fmt::Display for TextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for TextError {}

#[derive(Clone)]
struct Placement {
    font: u32,
    glyph: u32,
    ppem: f64,
    run: Option<usize>,
    bold: u32,
    /// Synthetic italic: italic requested on a face without an italic file (its quads skew, Quad::skew).
    italic: bool,
    scale: f64,
    /// Rotated text: scaled quad corners stay unrounded (fixtures/text-probe-fracsize.luau)
    unsnapped: bool,
    /// Drawn: false for glyphs MaxVisibleGraphemes hides (the line's `shown` / `ellipsis`); they still
    /// place the baseline and are still atlas requirements
    shown: bool,
    /// The pen's quarter-pixel phase (a text with `subpixel`: 4), None for whole-pixel text
    quarter: Option<u8>,
    x: f64,
    baseline: f64,
}
struct Line {
    ox: f64,
    baseline: f64,
    runs: Vec<Value>,
    main: Placement,
    placements: Vec<Placement>,
}
#[derive(Clone)]
struct Quad {
    x: f64,
    y: f64,
    w: usize,
    h: usize,
    coverage: Arc<[u8]>,
    bounds: [f64; 4],
    scaled: bool,
    /// Synthetic italic (TEXT_ITALIC_SKEW): the quad's top slides right by skew x its height,
    /// pivoting on its bottom edge
    skew: f64,
    color: [f64; 3],
    alpha: f64,
    shade: Option<Shade>,
    /// How far the snapped corners of a scaled quad were from a snapping boundary (snap_device's floor
    /// argument's distance to the nearest integer; infinite when nothing was snapped): a quad moved by
    /// whole px snaps to the moved corners when this is not tiny (TextLayerCache node entries)
    margin: f64,
}
/// Synthetic italic slope, applied to a face without an italic file (fixtures/text-probe-synth.luau
/// FaceItalic18 / 36 and Rich72: 0 px > 2 levels for 0.2197..0.2199, least absolute error at 0.21985)
const TEXT_ITALIC_SKEW: f64 = 0.21985;
impl Quad {
    /// The coverage box: the quad's box, widened on the right by ceil(skew x height)
    fn cover(&self) -> [f64; 4] {
        let [l, t, r, b] = self.bounds;
        [l, t, r + (self.skew * (b - t)).ceil(), b]
    }
    fn resampled(&self) -> bool {
        self.scaled || self.skew != 0.0
    }
}
type Shade = super::gradients::GradientMesh;
fn arr(v: &Value) -> &[Value] {
    v.as_array().map(Vec::as_slice).unwrap_or(&[])
}
fn num(v: &Value, default: f64) -> f64 {
    v.as_f64().filter(|n| n.is_finite()).unwrap_or(default)
}
fn n(v: &Value, key: &str, default: f64) -> f64 {
    num(&v[key], default)
}
fn snap(v: f64) -> f64 {
    (v + 0.5).floor()
}
fn byte(v: f64) -> f64 {
    (v * 256.0).floor().clamp(0.0, 255.0)
}
fn gpu8(v: f64) -> f64 {
    (((v * 4096.0).floor() * 255.0 + 2047.0) / 4096.0)
        .floor()
        .clamp(0.0, 255.0)
}
fn alpha(v: &Value) -> f64 {
    byte(1.0 - n(v, "transparency", 0.0))
}
fn color(v: &Value, fallback: [f64; 3]) -> [f64; 3] {
    if v.is_array() {
        std::array::from_fn(|i| byte(num(&v[i], 0.0)))
    } else {
        fallback
    }
}
fn premul(c: f64, a: f64) -> u8 {
    ((c * a + 127.0) / 255.0).floor().clamp(0.0, 255.0) as u8
}
/// The font units a glyph's ppem spreads over S + 1 px: the scene font's `sizeUnits` (a face with a line
/// gap, sized with the gap: engine/text/layout.rs height_units), else ascender - descender (every other
/// face, and scenes saved before faces with a line gap were sized with it).
pub(crate) fn size_units(font: &Value) -> f64 {
    n(font, "sizeUnits", n(font, "ascender", 0.0) - n(font, "descender", 0.0))
}
/// The pixel size S of a glyph's ppem.
fn font_size(font: &Value, ppem: f64) -> f64 {
    snap(ppem * size_units(font) / n(font, "unitsPerEm", 1.0)) - 1.0
}
/// The baseline below the line top: the ascender's share of the S + 1 px line, line gap included
/// (text-baseline).
fn ascent(font: &Value, ppem: f64) -> f64 {
    let span = n(font, "ascender", 0.0) - n(font, "descender", 0.0);
    snap(
        n(font, "ascender", 0.0) * ppem / n(font, "unitsPerEm", 1.0) * size_units(font)
            / (span + n(font, "lineGap", 0.0)),
    )
}

fn placements(text: &Value, fonts: &Value, rotated: bool) -> (Vec<Placement>, Vec<Line>) {
    let mut out = Vec::new();
    let mut lines = Vec::new();
    for line in arr(&text["lines"]) {
        let px = n(line, "px", snap(n(line, "x", 0.0)));
        let ox = if rotated { n(line, "ux", px) } else { px };
        let top = if rotated {
            n(line, "top", 0.0)
        } else {
            snap(n(line, "top", 0.0))
        };
        let mut local = Vec::new();
        let mut largest = f64::NEG_INFINITY;
        let mut asc = 0.0;
        let mut main = None;
        let glyphs = arr(&line["glyphs"]);
        // MaxVisibleGraphemes (export.rs reveal_line): the first `shown` entries and the last `ellipsis` are drawn
        let shown = line["shown"].as_u64().map_or(glyphs.len(), |v| v as usize);
        let tail = glyphs.len() - line["ellipsis"].as_u64().map_or(0, |v| v as usize).min(glyphs.len());
        for (index, glyph) in glyphs.iter().enumerate() {
            let fi = num(&glyph[0], 0.0) as usize;
            let font = &fonts[fi];
            if font.is_null() {
                continue;
            }
            let ppem = num(&glyph[3], 0.0);
            let run = glyph.get(4).and_then(Value::as_u64).map(|v| v as usize);
            let info = run.map(|i| &text["runs"][i]);
            let requested = info
                .map(|r| {
                    n(
                        r,
                        "weight",
                        if r["bold"].as_bool() == Some(true) {
                            700.0
                        } else {
                            400.0
                        },
                    )
                })
                .unwrap_or(n(text, "weight", 0.0));
            let face = info
                .map(|r| n(&r["face"], "weight", n(font, "weight", 400.0)))
                .unwrap_or(n(font, "weight", 400.0));
            // cli/rur.mjs textSyntheticItalic
            let italic_requested =
                info.is_some_and(|r| r["italic"] == true) || text["style"] == "italic";
            let face_italic = info
                .and_then(|r| r["face"]["italic"].as_bool())
                .unwrap_or(font["style"] == "italic");
            let size = font_size(font, ppem);
            let base = n(text, "quadScale", 1.0);
            let base = if base > 0.0 { base } else { 1.0 };
            let scale = info
                .map(|r| n(r, "quadScale", base))
                .filter(|r| *r > 0.0)
                .unwrap_or(base);
            let p = Placement {
                font: fi as u32,
                glyph: num(&glyph[1], 0.0) as u32,
                ppem,
                run,
                bold: if requested >= 600.0 && face < 600.0 && size > 0.0 {
                    (2.0 * size) as u32
                } else {
                    0
                },
                italic: italic_requested && !face_italic,
                scale,
                unsnapped: rotated,
                shown: index < shown || index >= tail,
                quarter: crate::glyphs::text_quarter(text, glyph.get(2), rotated),
                // Quarter-pixel text: the whole pixel under the pen (its bitmap carries the quarter); rotated,
                // the exact pen (the unshifted bitmap, resampled)
                x: ox + match (text["subpixel"] == 4, rotated) {
                    (true, true) => num(&glyph[2], 0.0),
                    (true, false) => (num(&glyph[2], 0.0) + 0.125).floor(),
                    (false, _) => snap(num(&glyph[2], 0.0)),
                },
                baseline: 0.0,
            };
            if size > largest {
                largest = size;
                asc = ascent(font, ppem);
                main = Some(p.clone());
            }
            local.push(p);
        }
        for p in &mut local {
            p.baseline = top + asc;
        }
        if let Some(main) = main {
            if !arr(&line["runs"]).is_empty() {
                lines.push(Line {
                    ox,
                    baseline: top + asc,
                    runs: arr(&line["runs"]).to_vec(),
                    main,
                    placements: local.clone(),
                });
            }
        }
        out.extend(local);
    }
    (out, lines)
}

/// The text's UIGradient over the node's rect: the fill's gradient (gradients::GradientMesh), multiplying the
/// text's colour and opacity
fn triangles(gradient: &Value, rect: [f64; 4], bounds: [f64; 4], color: [f64; 3], alpha: f64) -> Shade {
    super::gradients::gradient_mesh(gradient, rect, color, alpha, bounds)
}
fn shading(q: &Quad, x: f64, y: f64, coverage: f64) -> [f64; 4] {
    if let Some(shade) = &q.shade {
        // The gradient at the place the texel is drawn, dithered by that place's pixel
        let c = shade.at([x, y]);
        return [c[0], c[1], c[2], gpu8(coverage / 255.0 * (c[3] / 255.0))];
    }
    [
        q.color[0],
        q.color[1],
        q.color[2],
        gpu8(coverage / 255.0 * (q.alpha / 255.0)),
    ]
}

/// UIStroke radius: tenths of a device px as floor(f32(thickness x f32(DPI x 10))), then truncated to 1/64:
/// thickness 1.8 at 1.5x is 2.7 px, not the 2.6 of f32(f32(thickness x DPI) x 10) (a "Play" button
/// 224 -> 0 px > 2, notes/text.md).
fn stroke_radius(stroke: &Value, scale: f64) -> f64 {
    let tenths = (n(stroke, "thickness", 0.0) as f32) * ((scale * 10.0) as f32);
    ((tenths.floor() as f64 / 10.0) * 64.0).floor() / 64.0
}
fn glyph_keys(p: &Placement, stroke: f64, join: Option<&str>) -> Vec<GlyphKey> {
    let mut style = join.unwrap_or("").to_string();
    if p.bold > 0 {
        if !style.is_empty() {
            style.push('+');
        }
        style.push_str(&format!("b{}", p.bold));
    }
    let quarter = p.quarter.map(|quarter| format!("q{quarter}"));
    let with_quarter = |style: &str| match &quarter {
        Some(quarter) if style.is_empty() => quarter.clone(),
        Some(quarter) => format!("{style}+{quarter}"),
        None => style.to_string(),
    };
    let style = with_quarter(&style);
    let mut keys = vec![GlyphKey::new(
        p.font,
        p.ppem,
        p.glyph,
        stroke,
        if style.is_empty() { None } else { Some(&style) },
    )];
    if p.bold > 0 {
        let plain = with_quarter(join.unwrap_or(""));
        keys.push(GlyphKey::new(p.font, p.ppem, p.glyph, stroke, if plain.is_empty() { None } else { Some(&plain) }));
    }
    if p.italic {
        // Atlases built when italic was an atlas variant ("…+i", already slanted: drawn without the skew)
        let legacy = if style.is_empty() { "i".to_string() } else { format!("{style}+i") };
        keys.push(GlyphKey::new(p.font, p.ppem, p.glyph, stroke, Some(&legacy)));
    }
    keys
}
/// Whether `key` is the legacy slanted italic variant of glyph_keys
fn legacy_italic(key: &GlyphKey) -> bool {
    key.0.ends_with("|i") || key.0.ends_with("+i")
}

struct GlyphDraw<'a> {
    placement: &'a Placement,
    stroke: f64,
    join: Option<&'static str>,
    offset: [f64; 2],
    color: [f64; 3],
    alpha: f64,
    gradient: &'a Value,
}

/// Shared draw planning keeps manifest readiness and actual glyph lookup in lockstep.
fn glyph_draws<'a>(
    node: &'a Value,
    placements: &'a [Placement],
    scale: f64,
    intrinsic: bool,
) -> Vec<GlyphDraw<'a>> {
    let text = &node["text"];
    let gradient = &node["gradient"];
    let mut draws = Vec::new();
    // Glyphs MaxVisibleGraphemes hides draw nothing (fill, strokes, legacy stroke), but stay requirements
    let placements: Vec<&'a Placement> = placements.iter().filter(|p| intrinsic || p.shown).collect();
    let tag = |p: &Placement| {
        p.run
            .map(|i| &text["runs"][i]["stroke"])
            .filter(|v| v.is_object())
    };
    let join = |s: &Value| match s["joins"]
        .as_str()
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "miter" => Some("miter"),
        "bevel" => Some("bevel"),
        _ => None,
    };
    for stroke in arr(&node["strokes"])
        .iter()
        .filter(|s| s["mode"].as_str() == Some("Contextual"))
    {
        let radius = stroke_radius(stroke, scale);
        if radius <= 0.0 || (!intrinsic && alpha(stroke) <= 0.0) {
            continue;
        }
        let grad = if stroke["gradient"].is_object() {
            &stroke["gradient"]
        } else {
            gradient
        };
        for &p in &placements {
            if tag(p).is_some() {
                continue;
            }
            draws.push(GlyphDraw {
                placement: p,
                stroke: radius,
                join: join(stroke),
                offset: [0.0; 2],
                color: color(&stroke["color"], [0.0; 3]),
                alpha: alpha(stroke),
                gradient: grad,
            });
        }
    }
    for &p in &placements {
        if let Some(stroke) = tag(p) {
            let radius = stroke_radius(stroke, scale);
            if radius > 0.0 && (intrinsic || alpha(stroke) > 0.0) {
                draws.push(GlyphDraw {
                    placement: p,
                    stroke: radius,
                    join: join(stroke),
                    offset: [0.0; 2],
                    color: color(&stroke["color"], [0.0; 3]),
                    alpha: alpha(stroke),
                    gradient,
                });
            }
        }
    }
    if text["stroke"].is_object() {
        let a = byte(
            (1.0 - n(&text["stroke"], "transparency", 0.0)) * (1.0 - n(text, "transparency", 0.0)),
        );
        if a > 0.0 && !intrinsic {
            for offset in [[-1.0, -1.0], [1.0, -1.0], [-1.0, 1.0], [1.0, 1.0]] {
                for &p in &placements {
                    draws.push(GlyphDraw {
                        placement: p,
                        stroke: 0.0,
                        join: None,
                        offset,
                        color: color(&text["stroke"]["color"], [0.0; 3]),
                        alpha: a,
                        gradient,
                    });
                }
            }
        }
    }
    for &p in &placements {
        let (c, a) = run_paint(text, p.run.map(|i| &text["runs"][i]));
        if intrinsic || a > 0.0 {
            draws.push(GlyphDraw {
                placement: p,
                stroke: 0.0,
                join: None,
                offset: [0.0; 2],
                color: c,
                alpha: a,
                gradient,
            });
        }
    }
    draws
}

/// Each row is one required glyph variant, with acceptable atlas keys in lookup order.
/// The optional second key is precisely the renderer's pre-synthetic-bold atlas fallback.
/// This examines exported layout only; it never copies coverage or rasterizes a glyph.
/// As in cli/rur.mjs atlasEntries, intrinsic requirements ignore opacity and visibility.
pub fn glyph_requirements(node: &Value, fonts: &Value, scale: f64) -> Vec<Vec<GlyphKey>> {
    let (placements, _) = placements(&node["text"], fonts, node["matrix"].is_array());
    let mut seen = std::collections::HashSet::new();
    let mut requirements = Vec::new();
    for draw in glyph_draws(node, &placements, scale, true) {
        let keys = glyph_keys(draw.placement, draw.stroke, draw.join);
        if seen.insert(keys.clone()) {
            requirements.push(keys);
        }
    }
    requirements
}
/// A glyph's atlas bitmap as glyph_quad uses it: coverage bytes (alpha of the atlas rows), size,
/// bearings and whether it came from the legacy slanted italic variant
#[derive(Clone)]
struct GlyphBitmap {
    coverage: Arc<[u8]>,
    w: u32,
    h: u32,
    left: i32,
    top: i32,
    slanted: bool,
    /// A colour glyph's layers (the atlas row's `layers`), bottom first; its own coverage is empty
    layers: Vec<GlyphLayer>,
}
/// A colour glyph's layer: a coverage bitmap and its palette colour (straight RGBA), or the text colour
#[derive(Clone)]
struct GlyphLayer {
    bitmap: GlyphBitmap,
    color: [u8; 4],
    foreground: bool,
}
/// The coverage (alpha) of a page's w x h texels at (x, y)
fn page_coverage(page: &crate::assets::TextureAsset, x: u32, y: u32, w: u32, h: u32) -> Result<Arc<[u8]>, TextError> {
    if x + w > page.width || y + h > page.height {
        return Err(TextError("glyph lies outside atlas page".into()));
    }
    let mut coverage = Vec::with_capacity((w * h) as usize);
    for y in y..y + h {
        for x in x..x + w {
            coverage.push(page.rgba[((y * page.width + x) * 4 + 3) as usize]);
        }
    }
    Ok(coverage.into())
}
/// Glyph bitmaps by their lookup inputs, valid while the glyph atlases are (TextLayerCache::begin_frame
/// clears it when the assets change): a glyph is looked up and copied out of its page once
type GlyphMemo = rustc_hash::FxHashMap<(u32, u64, u32, u64, u32, bool, u8, Option<u8>), Option<GlyphBitmap>>;
fn glyph_bitmap(
    p: &Placement,
    stroke: f64,
    join: Option<&str>,
    assets: &AssetStore,
) -> Result<Option<GlyphBitmap>, TextError> {
    let keys = glyph_keys(p, stroke, join);
    let found = keys.iter().find_map(|key| assets.glyph(key).map(|entry| (entry, legacy_italic(key))));
    let (entry, slanted) = found.ok_or_else(|| TextError(format!("missing shaped glyph {}", keys[0].0)))?;
    let drawn = |w: u32, h: u32| w > 0 && h > 0;
    if !drawn(entry.w, entry.h) && !entry.layers.iter().any(|layer| drawn(layer.w, layer.h)) {
        return Ok(None);
    }
    let page = assets
        .texture(&entry.page)
        .ok_or_else(|| TextError(format!("missing glyph atlas page {:?}", entry.page)))?;
    let mut layers = Vec::with_capacity(entry.layers.len());
    for layer in entry.layers.iter().filter(|layer| drawn(layer.w, layer.h)) {
        layers.push(GlyphLayer {
            bitmap: GlyphBitmap {
                coverage: page_coverage(page, layer.x, layer.y, layer.w, layer.h)?,
                w: layer.w,
                h: layer.h,
                left: layer.left,
                top: layer.top,
                slanted,
                layers: Vec::new(),
            },
            color: layer.color,
            foreground: layer.foreground,
        });
    }
    Ok(Some(GlyphBitmap {
        coverage: page_coverage(page, entry.x, entry.y, entry.w, entry.h)?,
        w: entry.w,
        h: entry.h,
        left: entry.left,
        top: entry.top,
        slanted,
        layers,
    }))
}
fn glyph_quad(
    p: &Placement,
    stroke: f64,
    join: Option<&str>,
    offset: [f64; 2],
    color: [f64; 3],
    alpha: f64,
    gradient: &Value,
    rect: [f64; 4],
    assets: &AssetStore,
    dpi: f64,
    memo: Option<&mut GlyphMemo>,
    out: &mut Vec<Quad>,
) -> Result<(), TextError> {
    let bitmap = match memo {
        Some(memo) => {
            let join_code = match join {
                None => 0,
                Some("miter") => 1,
                Some("bevel") => 2,
                Some(_) => 3,
            };
            let key = (p.font, p.ppem.to_bits(), p.glyph, stroke.to_bits(), p.bold, p.italic, join_code, p.quarter);
            match memo.get(&key) {
                Some(bitmap) if join_code != 3 => bitmap.clone(),
                _ => {
                    let bitmap = glyph_bitmap(p, stroke, join, assets)?;
                    if join_code != 3 {
                        memo.insert(key, bitmap.clone());
                    }
                    bitmap
                }
            }
        }
        None => glyph_bitmap(p, stroke, join, assets)?,
    };
    let Some(bitmap) = bitmap else {
        return Ok(());
    };
    if bitmap.layers.is_empty() {
        out.push(placed_quad(p, bitmap, offset, color, alpha, gradient, rect, dpi));
        return Ok(());
    }
    // A colour glyph (emoji): each COLR layer is a glyph quad of its own in its palette colour, blended on
    // its own (as Studio draws them: the "Half" eye at TextTransparency 0.5). TextColor3 and a rich-text
    // colour tint only `foreground` layers; transparency (label, run) and the UIGradient apply to every
    // layer; the legacy TextStroke's offset copies are not drawn around it. Measured:
    // fixtures/text-probe-emoji.luau (Red, Black, Rich, Legacy, Gradient, GradientFade, Half),
    // notes/text.md "Colour emoji".
    if offset != [0.0; 2] || stroke != 0.0 {
        return Ok(());
    }
    for layer in bitmap.layers {
        let (layer_color, layer_alpha) = if layer.foreground {
            (color, alpha)
        } else {
            (
                [layer.color[0] as f64, layer.color[1] as f64, layer.color[2] as f64],
                alpha * layer.color[3] as f64 / 255.0,
            )
        };
        out.push(placed_quad(p, layer.bitmap, offset, layer_color, layer_alpha, gradient, rect, dpi));
    }
    Ok(())
}
/// A glyph bitmap's quad at the placement's pen
fn placed_quad(
    p: &Placement,
    bitmap: GlyphBitmap,
    offset: [f64; 2],
    color: [f64; 3],
    alpha: f64,
    gradient: &Value,
    rect: [f64; 4],
    dpi: f64,
) -> Quad {
    let GlyphBitmap { coverage, w, h, left, top, slanted, .. } = bitmap;
    let x = p.x + left as f64 + offset[0];
    let y = p.baseline - top as f64 + offset[1];
    let w = w as usize;
    let h = h as usize;
    let s = p.scale;
    // Scaled quads (text.quadScale): corners rounded to whole px, except under a rotation, where Studio keeps
    // them unrounded (fixtures/text-probe-fracsize.luau RotPlain51 / RotFPlain37: 0 px > 2 unrounded, 1461-1676
    // rounded; a rotated "Mythic" label 2218 -> 0 px in the glyph quads). Rounded like frame edges (snap_device: up to
    // 1/960 logical px below k + 0.5 still lands on k + 1; button "Review & craft" "v" right edge 796.4994 ->
    // 797 at 1.5x, fixtures/text-probe-quadties.luau, notes/text.md Round 8)
    let mut margin = f64::INFINITY;
    let mut corner = |v: f64| {
        if p.unsnapped {
            return v;
        }
        let t = v * 256. + dpi * 4. / 15.;
        margin = margin.min(t - t.floor()).min(t.floor() + 1. - t);
        crate::render::geometry::snap_device(v, dpi)
    };
    let bounds = if s == 1.0 {
        [x - 1.0, y - 1.0, x + w as f64 + 1.0, y + h as f64 + 1.0]
    } else {
        [
            corner(p.x + offset[0] + (left as f64 - 1.0) * s),
            corner(p.baseline + offset[1] - (top as f64 + 1.0) * s),
            corner(p.x + offset[0] + (left as f64 + w as f64 + 1.0) * s),
            corner(p.baseline + offset[1] + (h as f64 - top as f64 + 1.0) * s),
        ]
    };
    Quad {
        x,
        y,
        w,
        h,
        coverage,
        bounds,
        scaled: s != 1.0,
        skew: if p.italic && !slanted { TEXT_ITALIC_SKEW } else { 0.0 },
        color,
        alpha,
        shade: if gradient.is_object() {
            Some(triangles(gradient, rect, bounds, color, alpha))
        } else {
            None
        },
        margin,
    }
}
fn rect_quad(
    bounds: [f64; 4],
    color: [f64; 3],
    alpha: f64,
    gradient: &Value,
    rect: [f64; 4],
) -> Quad {
    let w = (bounds[2] - bounds[0]).max(0.0) as usize;
    let h = (bounds[3] - bounds[1]).max(0.0) as usize;
    Quad {
        x: bounds[0],
        y: bounds[1],
        w,
        h,
        coverage: vec![255; w * h].into(),
        bounds,
        scaled: false,
        skew: 0.0,
        color,
        alpha,
        shade: if gradient.is_object() {
            Some(triangles(gradient, rect, bounds, color, alpha))
        } else {
            None
        },
        margin: f64::INFINITY,
    }
}
fn run_paint(text: &Value, run: Option<&Value>) -> ([f64; 3], f64) {
    let c = color(&text["color"], [255.0; 3]);
    let a = alpha(text);
    if let Some(r) = run {
        (
            color(&r["color"], c),
            if r["transparency"].is_number() {
                byte((1.0 - n(r, "transparency", 0.0)) * (1.0 - n(text, "transparency", 0.0)))
            } else {
                a
            },
        )
    } else {
        (c, a)
    }
}
fn quads(
    node: &Value,
    fonts: &Value,
    assets: &AssetStore,
    scale: f64,
) -> Result<Vec<Quad>, TextError> {
    quads_memo(node, fonts, assets, scale, None)
}
fn quads_memo(
    node: &Value,
    fonts: &Value,
    assets: &AssetStore,
    scale: f64,
    mut memo: Option<&mut GlyphMemo>,
) -> Result<Vec<Quad>, TextError> {
    let text = &node["text"];
    let (placements, lines) = placements(text, fonts, node["matrix"].is_array());
    let gradient = &node["gradient"];
    let rect =
        std::array::from_fn(|i| ((num(&node["rect"][i], 0.0) as f32) * (scale as f32)) as f64);
    let mut marks = Vec::new();
    let mut decor = Vec::new();
    for line in &lines {
        for segment in &line.runs {
            let ri = num(&segment[0], 0.0) as usize;
            let run = &text["runs"][ri];
            let rx0 = num(&segment[1], 0.0);
            let rx1 = num(&segment[2], 0.0);
            if rx1 <= rx0 || run.is_null() {
                continue;
            }
            let p = line
                .placements
                .iter()
                .find(|p| p.run == Some(ri))
                .unwrap_or(&line.main);
            let font = &fonts[p.font as usize];
            let size = font_size(font, p.ppem);
            let top = line.baseline - ascent(font, p.ppem);
            let t = (size / scale / 20.0 - 1e-9).ceil() * scale;
            let l = line.ox + rx0;
            let r = line.ox + rx1;
            if run["mark"].is_object() && alpha(&run["mark"]) > 0.0 {
                marks.push(rect_quad(
                    [l, top, r, top + size],
                    color(&run["mark"]["color"], [0.0; 3]),
                    alpha(&run["mark"]),
                    gradient,
                    rect,
                ));
            }
            let (c, a) = run_paint(text, Some(run));
            if a <= 0.0 {
                continue;
            }
            if run["underline"].as_bool() == Some(true) {
                decor.push(rect_quad(
                    [l, top + size - t.floor().max(1.0), r, top + size],
                    c,
                    a,
                    gradient,
                    rect,
                ));
            }
            if run["strike"].as_bool() == Some(true) {
                let y = top + (size / 2.0).ceil() + 1.0;
                decor.push(rect_quad(
                    [l, y, r, y + t.ceil().max(1.0)],
                    c,
                    a,
                    gradient,
                    rect,
                ));
            }
        }
    }
    let mut out = marks;
    for draw in glyph_draws(node, &placements, scale, false) {
        glyph_quad(
            draw.placement,
            draw.stroke,
            draw.join,
            draw.offset,
            draw.color,
            draw.alpha,
            draw.gradient,
            rect,
            assets,
            scale,
            memo.as_deref_mut(),
            &mut out,
        )?;
    }
    out.extend(decor);
    Ok(out)
}

/// A quad's coverage at pixel (i, j) of its cover box: the bitmap itself, or (scaled / skewed) the padded
/// bitmap resampled over the box of `width` x `height` (bilinear, 8-bit weights; text-probe-uiscale Rich885 99.2%)
fn coverage_at(q: &Quad, i: usize, j: usize, width: usize, height: usize) -> f64 {
    if !q.resampled() {
        return q.coverage[j * q.w + i] as f64;
    }
    let texel = |x: i64, y: i64| {
        if x < 1 || y < 1 || x > q.w as i64 || y > q.h as i64 {
            0.0
        } else {
            q.coverage[(y as usize - 1) * q.w + x as usize - 1] as f64
        }
    };
    let shift = q.skew * (height as f64 - (j as f64 + 0.5));
    let u = (i as f64 + 0.5 - shift) * ((q.w + 2) as f64 / width as f64) - 0.5;
    let v = (j as f64 + 0.5) * ((q.h + 2) as f64 / height as f64) - 0.5;
    let x = u.floor() as i64;
    let y = v.floor() as i64;
    let a = snap((u - x as f64) * 256.0) / 256.0;
    let b = snap((v - y as f64) * 256.0) / 256.0;
    (texel(x, y) * (1.0 - a) + texel(x + 1, y) * a) * (1.0 - b)
        + (texel(x, y + 1) * (1.0 - a) + texel(x + 1, y + 1) * a) * b
}
fn image(
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    width: usize,
    height: usize,
    rgba: Vec<u8>,
    opacity: f64,
) -> TextQuad {
    TextQuad {
        x,
        y,
        w,
        h,
        width: width as u32,
        height: height as u32,
        rgba: rgba.into(),
        opacity,
        translation_only: false,
        selection_mode: 0,
        selection_rects: Vec::new(),
    }
}
fn composite(quads: &[Quad]) -> Result<Vec<TextQuad>, TextError> {
    let Some(layout) = composite_layout(quads)? else {
        return Ok(Vec::new());
    };
    let rgba = composite_pixels(quads, layout);
    Ok(vec![image(
        layout.x,
        layout.y,
        layout.w,
        layout.h,
        layout.width as usize,
        layout.height as usize,
        rgba,
        1.0,
    )])
}
/// composite()'s layer box: the quads' covers' union; None when empty
fn composite_layout(quads: &[Quad]) -> Result<Option<TextLayout>, TextError> {
    if quads.is_empty() {
        return Ok(None);
    }
    let mut bounds = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for q in quads {
        let cover = q.cover();
        bounds[0] = bounds[0].min(cover[0]);
        bounds[1] = bounds[1].min(cover[1]);
        bounds[2] = bounds[2].max(cover[2]);
        bounds[3] = bounds[3].max(cover[3]);
    }
    let [x, y, r, b] = bounds;
    let w = (r - x) as usize;
    let h = (b - y) as usize;
    if w == 0 || h == 0 {
        return Ok(None);
    }
    if w.checked_mul(h)
        .filter(|p| *p <= 64 * 1024 * 1024)
        .is_none()
    {
        return Err(TextError("text layer exceeds allocation limit".into()));
    }
    Ok(Some(TextLayout {
        x,
        y,
        w: r - x,
        h: b - y,
        width: w as u32,
        height: h as u32,
    }))
}
/// composite()'s premultiplied pixels over `layout` (composite_layout).
/// The quads blend in order into a double-precision layer, straight alpha per quad (colour c A / 255 +
/// layer (1 - A / 255), alpha A + layer alpha (1 - A / 255)); the layer is then stored as putImageData stores
/// it: alpha round(La), straight colour min(255, round(min(round(Lc), a) * 255 / a)), premultiplied
/// round(c a / 255) (notes/gpu.md: text as a CPU layer).
fn composite_pixels(quads: &[Quad], layout: TextLayout) -> Vec<u8> {
    let (x, y) = (layout.x, layout.y);
    let (w, h) = (layout.width as usize, layout.height as usize);
    let mut layer = vec![0.0; w * h * 4];
    for q in quads {
        let box_w = (q.bounds[2] - q.bounds[0]) as usize;
        let (qx, qy, qw, qh) = if q.resampled() {
            let cover = q.cover();
            (
                cover[0],
                cover[1],
                (cover[2] - cover[0]) as usize,
                (cover[3] - cover[1]) as usize,
            )
        } else {
            (q.x, q.y, q.w, q.h)
        };
        for j in 0..qh {
            for i in 0..qw {
                let c = coverage_at(q, i, j, box_w, qh);
                if c <= 0.0 {
                    continue;
                }
                let sx = (qx + i as f64 - x) as usize;
                let sy = (qy + j as f64 - y) as usize;
                if sx >= w || sy >= h {
                    continue;
                }
                let p = shading(q, qx + i as f64 + 0.5, qy + j as f64 + 0.5, c);
                let k = (sy * w + sx) * 4;
                let a = p[3] / 255.0;
                for channel in 0..3 {
                    layer[k + channel] = p[channel] * a + layer[k + channel] * (1.0 - a);
                }
                layer[k + 3] = p[3] + layer[k + 3] * (1.0 - a);
            }
        }
    }
    let mut rgba = vec![0; layer.len()];
    for k in (0..rgba.len()).step_by(4) {
        let a = snap(layer[k + 3]).clamp(0.0, 255.0);
        rgba[k + 3] = a as u8;
        if a > 0.0 {
            for c in 0..3 {
                let straight = snap(snap(layer[k + c]).min(a) * 255.0 / a).min(255.0);
                rgba[k + c] = premul(straight, a);
            }
        }
    }
    rgba
}
/// A gradient quad under the node's rotation `m` (device <- node-local device px),
/// shaded per device pixel: the padded bitmap sampled bilinearly (8-bit weights) where the pixel centre maps into the
/// quad, and the vertex colour and alpha interpolated there. Baking the colour per texel and resampling the image
/// mixed neighbouring texels' colours by their coverage (a rotated "Mythic" label: 39 px off by 3-4 levels, notes/text.md
/// Round 8). A translation-only layer: x, y are device px minus m's translation.
fn rotated_shaded(q: &Quad, m: [f64; 6]) -> Option<TextQuad> {
    let [bl, bt, br, bb] = q.bounds;
    let (bw, bh) = (br - bl, bb - bt);
    if q.shade.is_none() || !(bw > 0.0 && bh > 0.0) {
        return None;
    }
    let cover = q.cover();
    let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for [px, py] in [[cover[0], cover[1]], [cover[2], cover[1]], [cover[0], cover[3]], [cover[2], cover[3]]] {
        let dx = m[0] * px + m[2] * py + m[4];
        let dy = m[1] * px + m[3] * py + m[5];
        x0 = x0.min(dx);
        y0 = y0.min(dy);
        x1 = x1.max(dx);
        y1 = y1.max(dy);
    }
    let (x0, y0) = (x0.floor(), y0.floor());
    let (w, h) = (x1.ceil() - x0, y1.ceil() - y0);
    if !(w > 0.0 && h > 0.0) {
        return None;
    }
    let det = m[0] * m[3] - m[1] * m[2];
    if det == 0.0 {
        return None;
    }
    let (ia, ib, ic, id) = (m[3] / det, -m[1] / det, -m[2] / det, m[0] / det);
    let ie = -(ia * m[4] + ic * m[5]);
    let jf = -(ib * m[4] + id * m[5]);
    let texel = |x: i64, y: i64| {
        if x < 1 || y < 1 || x > q.w as i64 || y > q.h as i64 {
            0.0
        } else {
            q.coverage[(y as usize - 1) * q.w + x as usize - 1] as f64
        }
    };
    let su = (q.w + 2) as f64 / bw;
    let sv = (q.h + 2) as f64 / bh;
    let (width, height) = (w as usize, h as usize);
    let mut rgba = vec![0u8; width * height * 4];
    let mut any = false;
    for j in 0..height {
        for i in 0..width {
            let px = x0 + i as f64 + 0.5;
            let py = y0 + j as f64 + 0.5;
            let x = ia * px + ic * py + ie;
            let y = ib * px + id * py + jf;
            let by = y - bt;
            let bx = x - bl - q.skew * (bh - by);
            if by < 0.0 || by >= bh || bx < 0.0 || bx >= bw {
                continue;
            }
            let u = bx * su - 0.5;
            let v = by * sv - 0.5;
            let (tx, ty) = (u.floor(), v.floor());
            let wa = snap((u - tx) * 256.0) / 256.0;
            let wb = snap((v - ty) * 256.0) / 256.0;
            let (tx, ty) = (tx as i64, ty as i64);
            let c = (texel(tx, ty) * (1.0 - wa) + texel(tx + 1, ty) * wa) * (1.0 - wb)
                + (texel(tx, ty + 1) * (1.0 - wa) + texel(tx + 1, ty + 1) * wa) * wb;
            if c <= 0.0 {
                continue;
            }
            let p = shading(q, x, y, c);
            if p[3] <= 0.0 {
                continue;
            }
            let k = (j * width + i) * 4;
            rgba[k + 3] = p[3] as u8;
            for e in 0..3 {
                rgba[k + e] = premul(p[e], p[3]);
            }
            any = true;
        }
    }
    if !any {
        return None;
    }
    let mut out = image(x0 - m[4], y0 - m[5], w, h, width, height, rgba, 1.0);
    out.translation_only = true;
    Some(out)
}
/// Rotated text's quads as images drawn under the node's transform; with the node's device matrix, gradient
/// quads become device-space layers shaded per pixel (rotated_shaded).
fn rotated(quads: &[Quad], matrix: Option<[f64; 6]>) -> Vec<TextQuad> {
    quads
        .iter()
        .filter(|q| q.w > 0 && q.h > 0)
        .filter_map(|q| {
            if let (Some(m), true) = (matrix, q.shade.is_some()) {
                return rotated_shaded(q, m);
            }
            if q.skew != 0.0 {
                // Synthetic italic under a rotation: the skewed coverage resampled over its cover box, so the
                // image is resampled twice (skewing the draw's transform instead would resample it once)
                let cover = q.cover();
                let width = (cover[2] - cover[0]) as usize;
                let height = (cover[3] - cover[1]) as usize;
                let box_w = (q.bounds[2] - q.bounds[0]) as usize;
                let mut rgba = vec![0; width * height * 4];
                for j in 0..height {
                    for i in 0..width {
                        let coverage = coverage_at(q, i, j, box_w, height);
                        if coverage <= 0.0 {
                            continue;
                        }
                        let p = if q.shade.is_some() {
                            shading(q, cover[0] + i as f64 + 0.5, cover[1] + j as f64 + 0.5, coverage)
                        } else {
                            [q.color[0], q.color[1], q.color[2], coverage]
                        };
                        let k = (j * width + i) * 4;
                        let a = snap(p[3]).clamp(0.0, 255.0);
                        rgba[k + 3] = a as u8;
                        for c in 0..3 {
                            rgba[k + c] = premul(p[c], a);
                        }
                    }
                }
                return Some(image(
                    cover[0],
                    cover[1],
                    cover[2] - cover[0],
                    cover[3] - cover[1],
                    width,
                    height,
                    rgba,
                    if q.shade.is_some() { 1.0 } else { q.alpha / 255.0 },
                ));
            }
            let width = q.w + 2;
            let height = q.h + 2;
            // Where texel (i, j) lands: on a scaled quad its centre stretched over the box, so the
            // gradient is evaluated where the texel is drawn
            let (sx, sy) = if q.scaled {
                (
                    (q.bounds[2] - q.bounds[0]) / width as f64,
                    (q.bounds[3] - q.bounds[1]) / height as f64,
                )
            } else {
                (1.0, 1.0)
            };
            let (ox, oy) = if q.scaled { (q.bounds[0], q.bounds[1]) } else { (q.x - 1.0, q.y - 1.0) };
            let mut rgba = vec![0; width * height * 4];
            for j in 0..q.h {
                for i in 0..q.w {
                    let coverage = q.coverage[j * q.w + i] as f64;
                    let p = if q.shade.is_some() {
                        shading(q, ox + (i as f64 + 1.5) * sx, oy + (j as f64 + 1.5) * sy, coverage)
                    } else {
                        [q.color[0], q.color[1], q.color[2], coverage]
                    };
                    let k = ((j + 1) * width + i + 1) * 4;
                    rgba[k + 3] = p[3] as u8;
                    for c in 0..3 {
                        rgba[k + c] = premul(p[c], p[3]);
                    }
                }
            }
            let [x, y, r, b] = if q.scaled {
                q.bounds
            } else {
                [
                    q.x - 1.0,
                    q.y - 1.0,
                    q.x + q.w as f64 + 1.0,
                    q.y + q.h as f64 + 1.0,
                ]
            };
            Some(image(
                x,
                y,
                r - x,
                b - y,
                width,
                height,
                rgba,
                if q.shade.is_some() {
                    1.0
                } else {
                    q.alpha / 255.0
                },
            ))
        })
        .collect()
}
/// A text quad's layer in a resampled CanvasGroup texture: straight RGBA over texels [x, x + w) x [y, y + h).
pub struct TextureTextLayer {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<u8>,
}

/// An unrotated text node's glyph quads drawn straight into a resampled CanvasGroup
/// texture (`l`, `t`: the group's device origin, `n`: texture size, `s`: texels per device px), one layer per quad
/// in draw order: the quad's screen box scaled into the texture covers the texels whose centre is inside, and the
/// padded glyph bitmap is sampled once, bilinearly with 8-bit weights, at the texel centre mapped back over the box
/// (the gradient evaluated there). Measured: a list's hint "Select an emote to play..." (16 px glyphs at quad
/// scale 1.03125 in a 244 -> 245 texel group) 174 -> 0 px > 2, fixtures/text-probe-grouptext.luau (notes/text.md
/// Round 8). `clip`: the node's text clip in device px. None: the node needs the screen path (selection, caret,
/// border strokes).
pub fn texture_text_layers(
    node: &Value,
    fonts: &Value,
    assets: &AssetStore,
    scale: f64,
    origin: [f64; 2],
    n: [u32; 2],
    s: [f64; 2],
    clip: Option<[f64; 4]>,
) -> Result<Option<Vec<TextureTextLayer>>, TextError> {
    if node["matrix"].is_array()
        || node["selection"].is_object()
        || node["caret"].is_object()
        || arr(&node["strokes"]).iter().any(|st| st["mode"].as_str() != Some("Contextual"))
    {
        return Ok(None);
    }
    let [l, t] = origin;
    let [sx, sy] = s;
    let [cx0, cy0, cx1, cy1] = clip.unwrap_or([f64::NEG_INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::INFINITY]);
    let texel_x = |e: f64| ((e - l) * sx - 0.5).ceil();
    let texel_y = |e: f64| ((e - t) * sy - 0.5).ceil();
    let mut out = Vec::new();
    for q in quads(node, fonts, assets, scale)? {
        let [bl, bt, br, bb] = q.bounds;
        let (bw, bh) = (br - bl, bb - bt);
        if !(bw > 0.0 && bh > 0.0) || q.w == 0 || q.h == 0 {
            continue;
        }
        let cover = q.cover();
        let x0 = texel_x(cover[0].max(cx0)).max(0.0);
        let x1 = texel_x(cover[2].min(cx1)).min(n[0] as f64);
        let y0 = texel_y(cover[1].max(cy0)).max(0.0);
        let y1 = texel_y(cover[3].min(cy1)).min(n[1] as f64);
        if x1 <= x0 || y1 <= y0 {
            continue;
        }
        let (w, h) = ((x1 - x0) as usize, (y1 - y0) as usize);
        // Decoration rects fill their box; glyph bitmaps carry a transparent 1 px border
        let pad: i64 = if !q.scaled && q.bounds[0] == q.x && q.bounds[1] == q.y { 0 } else { 1 };
        let texel = |x: i64, y: i64| {
            if x < pad || y < pad || x >= q.w as i64 + pad || y >= q.h as i64 + pad {
                0.0
            } else {
                q.coverage[(y - pad) as usize * q.w + (x - pad) as usize] as f64
            }
        };
        let su = (q.w as f64 + 2.0 * pad as f64) / bw;
        let sv = (q.h as f64 + 2.0 * pad as f64) / bh;
        let mut rgba = vec![0u8; w * h * 4];
        let mut any = false;
        for j in 0..h {
            let y = t + (y0 + j as f64 + 0.5) / sy;
            let by = y - bt;
            let v = by * sv - 0.5;
            let ty = v.floor();
            let wb = snap((v - ty) * 256.0) / 256.0;
            let shift = q.skew * (bh - by);
            for i in 0..w {
                let x = l + (x0 + i as f64 + 0.5) / sx;
                let bx = x - bl - shift;
                if by < 0.0 || by >= bh || bx < 0.0 || bx >= bw {
                    continue;
                }
                let u = bx * su - 0.5;
                let tx = u.floor();
                let wa = snap((u - tx) * 256.0) / 256.0;
                let (tx, tyi) = (tx as i64, ty as i64);
                let c = (texel(tx, tyi) * (1.0 - wa) + texel(tx + 1, tyi) * wa) * (1.0 - wb)
                    + (texel(tx, tyi + 1) * (1.0 - wa) + texel(tx + 1, tyi + 1) * wa) * wb;
                if c <= 0.0 {
                    continue;
                }
                let p = shading(&q, x, y, c);
                if p[3] <= 0.0 {
                    continue;
                }
                let k = (j * w + i) * 4;
                rgba[k] = p[0] as u8;
                rgba[k + 1] = p[1] as u8;
                rgba[k + 2] = p[2] as u8;
                rgba[k + 3] = p[3] as u8;
                any = true;
            }
        }
        if any {
            out.push(TextureTextLayer { x: x0 as u32, y: y0 as u32, w: w as u32, h: h as u32, rgba });
        }
    }
    Ok(Some(out))
}

fn solid(bounds: [f64; 4], c: [u8; 4]) -> Option<TextQuad> {
    let [x, y, r, b] = bounds;
    let w = (r - x).max(0.0) as usize;
    let h = (b - y).max(0.0) as usize;
    if w == 0 || h == 0 {
        return None;
    }
    let rgba = c.repeat(w * h);
    let mut quad = image(x, y, r - x, b - y, w, h, rgba, 1.0);
    quad.translation_only = true;
    Some(quad)
}
fn snapped_rect(v: &Value) -> [f64; 4] {
    let x = num(&v[0], 0.0);
    let y = num(&v[1], 0.0);
    [
        snap(x),
        snap(y),
        snap(x + num(&v[2], 0.0)),
        snap(y + num(&v[3], 0.0)),
    ]
}

/// Composited text layers kept for reuse, on two levels:
/// - by the exact description of their quads (what composite() reads), so a text node prepared again
///   (re-exported, or equal to another) reuses its layer instead of blending it again. Quads whose
///   positions and boxes are all whole device px and that have no gradient blend the same wherever they
///   are (composite() reads only integer offsets from the layer origin, and shading() then ignores the
///   position): those are keyed relative to the layer origin. Any other layer is keyed absolutely.
/// - by the node's text itself with its line pens relative to the first line's (node_key), for text
///   laid out on whole px without gradients, whose quads then only translate with the pens (scaled
///   quads' corners too, unless one sat on a snapping boundary): a moved label (a scrolled row) skips
///   building its quads too. A node entry made while its layer was not drawn (clipped out) keeps the
///   quads, and is composited when it is drawn.
/// Keys are compared in full, never by hash only. Layers missing from both are composited with the
/// frame's other deferred rasters (TextPlan::Composite) and stored once done (store()). Glyph bitmaps
/// are looked up once per atlas state (GlyphMemo).
#[derive(Default)]
pub struct TextLayerCache {
    layers: rustc_hash::FxHashMap<u64, Vec<LayerEntry>>,
    nodes: rustc_hash::FxHashMap<u64, Vec<NodeEntry>>,
    glyphs: GlyphMemo,
    /// The device scale node entries were made at
    node_scale: u64,
    bytes: usize,
    clock: u64,
    serial: u64,
    pub hits: u64,
    pub node_hits: u64,
    pub misses: u64,
}
/// A composited layer's placement: device box (x, y, w, h) and pixel size
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextLayout {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub width: u32,
    pub height: u32,
}
struct LayerEntry {
    key: Vec<u64>,
    /// The layout relative to the quads' least cover corner (absolute for absolute keys)
    layout: TextLayout,
    rgba: Arc<[u8]>,
    used: u64,
}
enum NodeLayer {
    /// The text draws nothing
    Empty,
    Pixels(TextLayout, Arc<[u8]>),
    /// Not composited yet
    Quads(TextLayout, TextPixels),
}
struct NodeEntry {
    serial: u64,
    text: Value,
    strokes: Value,
    /// The first line's pen (x, top) the entry was made at
    anchor: [f64; 2],
    /// Layouts relative to the anchor
    layer: NodeLayer,
    used: u64,
}
/// A layer's quads and the layout they composite over (as placed where they were built: composite()
/// reads positions relative to the layout only)
#[derive(Clone)]
pub struct TextPixels {
    quads: Arc<Vec<Quad>>,
    layout: TextLayout,
}
impl TextPixels {
    /// composite()'s pixels, for the deferred raster queue
    pub fn job(&self) -> Box<dyn FnOnce() -> Vec<u8> + Send> {
        let (quads, layout) = (self.quads.clone(), self.layout);
        Box::new(move || composite_pixels(&quads, layout))
    }
}
/// Where a deferred composite's pixels go once rasterized (store()) or when it is not drawn (defer())
pub struct TextStore {
    /// Layer key: hash, key, least cover corner (computed from the quads when first needed)
    layer: Option<(u64, Vec<u64>, [f64; 2])>,
    /// A node entry to make: hash, anchor, text, strokes
    node: Option<(u64, [f64; 2], Value, Value)>,
    /// A node entry to complete: hash, serial
    existing: Option<(u64, u64)>,
    /// The layer's layout where it is drawn this time
    layout: TextLayout,
}
pub enum TextPlan {
    /// Ready layers (cache hits, or text drawn as prep_text draws it)
    Layers(Vec<TextQuad>),
    /// One layer to composite: its placement now, its pixels from `pixels`
    Composite {
        layout: TextLayout,
        pixels: TextPixels,
        store: TextStore,
    },
}
const TEXT_CACHE_BYTES: usize = 64 << 20;
pub(crate) fn layer_quad(layout: TextLayout, rgba: Arc<[u8]>) -> TextQuad {
    TextQuad {
        x: layout.x,
        y: layout.y,
        w: layout.w,
        h: layout.h,
        width: layout.width,
        height: layout.height,
        rgba,
        opacity: 1.0,
        translation_only: false,
        selection_mode: 0,
        selection_rects: Vec::new(),
    }
}
fn moved(layout: TextLayout, by: [f64; 2]) -> TextLayout {
    TextLayout {
        x: layout.x + by[0],
        y: layout.y + by[1],
        ..layout
    }
}

/// A line's pen as placements() computes it for unrotated text: x = px (or snap(x)), top = snap(top)
fn line_pen(line: &Value) -> [f64; 2] {
    [n(line, "px", snap(n(line, "x", 0.0))), snap(n(line, "top", 0.0))]
}
fn hash_value(value: &Value, hasher: &mut rustc_hash::FxHasher) {
    use std::hash::{Hash, Hasher};
    match value {
        Value::Null => hasher.write_u8(0),
        Value::Bool(b) => {
            hasher.write_u8(1);
            b.hash(hasher);
        }
        Value::Number(number) => {
            hasher.write_u8(2);
            hasher.write_u64(number.as_f64().map_or(0, f64::to_bits));
        }
        Value::String(s) => {
            hasher.write_u8(3);
            s.hash(hasher);
        }
        Value::Array(items) => {
            hasher.write_u8(4);
            hasher.write_usize(items.len());
            for item in items {
                hash_value(item, hasher);
            }
        }
        Value::Object(map) => {
            hasher.write_u8(5);
            hasher.write_usize(map.len());
            for (key, item) in map {
                key.hash(hasher);
                hash_value(item, hasher);
            }
        }
    }
}
/// Keys of a text object / line whose values move with the text (compared as relative pens instead)
fn moving_text_key(key: &str) -> bool {
    key == "content" || key == "lines"
}
fn moving_line_key(key: &str) -> bool {
    matches!(key, "x" | "px" | "ux" | "top")
}
/// The node-level key: hash and first pen, for text whose quads only translate with its pens (unrotated,
/// no gradient on the node or its strokes, no selection or caret, whole px pens; entries are only made
/// when the quads also came out on whole px, away from snapping boundaries); None otherwise
fn node_key(node: &Value, scale: f64) -> Option<(u64, [f64; 2])> {
    use std::hash::Hasher;
    let text = &node["text"];
    if node["matrix"].is_array()
        || node["gradient"].is_object()
        || !arr(&node["selection"]["rects"]).is_empty()
        || node["caret"]["visible"].as_bool() == Some(true)
        || arr(&node["strokes"]).iter().any(|s| s["gradient"].is_object())
        || !text.is_object()
    {
        return None;
    }
    let lines = arr(&text["lines"]);
    let anchor = line_pen(lines.first()?);
    let whole = |v: f64| v.fract() == 0.0 && v.abs() < 1e9;
    let mut hasher = rustc_hash::FxHasher::default();
    hasher.write_u64(scale.to_bits());
    hash_value(&node["strokes"], &mut hasher);
    for (key, value) in text.as_object()? {
        if !moving_text_key(key) {
            std::hash::Hash::hash(key, &mut hasher);
            hash_value(value, &mut hasher);
        }
    }
    hasher.write_usize(lines.len());
    for line in lines {
        let pen = line_pen(line);
        if !pen.iter().all(|v| whole(*v)) {
            return None;
        }
        hasher.write_u64((pen[0] - anchor[0]).to_bits());
        hasher.write_u64((pen[1] - anchor[1]).to_bits());
        for (key, value) in line.as_object()? {
            if !moving_line_key(key) {
                std::hash::Hash::hash(key, &mut hasher);
                hash_value(value, &mut hasher);
            }
        }
    }
    Some((hasher.finish(), anchor))
}
/// Whether two objects are equal apart from their `moving` keys
fn same_fixed(a: &Value, b: &Value, moving: fn(&str) -> bool) -> bool {
    let (Some(a), Some(b)) = (a.as_object(), b.as_object()) else {
        return false;
    };
    let mut a = a.iter().filter(|(key, _)| !moving(key));
    let mut b = b.iter().filter(|(key, _)| !moving(key));
    loop {
        match (a.next(), b.next()) {
            (None, None) => return true,
            (Some((ka, va)), Some((kb, vb))) if ka == kb && va == vb => {}
            _ => return false,
        }
    }
}
/// Whether two texts lay out the same quads up to the translation between their anchors
fn same_text(a: &Value, a_anchor: [f64; 2], b: &Value, b_anchor: [f64; 2]) -> bool {
    if !same_fixed(a, b, moving_text_key) {
        return false;
    }
    let (a_lines, b_lines) = (arr(&a["lines"]), arr(&b["lines"]));
    a_lines.len() == b_lines.len()
        && a_lines.iter().zip(b_lines).all(|(x, y)| {
            let (px, py) = (line_pen(x), line_pen(y));
            px[0] - a_anchor[0] == py[0] - b_anchor[0]
                && px[1] - a_anchor[1] == py[1] - b_anchor[1]
                && same_fixed(x, y, moving_line_key)
        })
}
/// The quads' description for the layer key, relative to their least cover corner when they blend the
/// same anywhere; with that corner and whether the key is relative
fn quads_key(quads: &[Quad]) -> (u64, Vec<u64>, [f64; 2], bool) {
    use std::hash::Hasher;
    let mut origin = [f64::INFINITY; 2];
    for q in quads {
        let cover = q.cover();
        origin[0] = origin[0].min(cover[0]);
        origin[1] = origin[1].min(cover[1]);
    }
    let whole = |v: f64| v.fract() == 0.0 && v.abs() < 1e9;
    let relative = quads.iter().all(|q| {
        q.shade.is_none() && whole(q.x) && whole(q.y) && q.bounds.iter().all(|v| whole(*v))
    }) && origin.iter().all(|v| whole(*v));
    let [ox, oy] = if relative { origin } else { [0.0; 2] };
    let mut key: Vec<u64> = Vec::with_capacity(quads.len() * 24);
    key.push(relative as u64);
    for q in quads {
        key.extend([
            (q.x - ox).to_bits(),
            (q.y - oy).to_bits(),
            q.w as u64,
            q.h as u64,
            (q.bounds[0] - ox).to_bits(),
            (q.bounds[1] - oy).to_bits(),
            (q.bounds[2] - ox).to_bits(),
            (q.bounds[3] - oy).to_bits(),
            q.scaled as u64,
            q.skew.to_bits(),
            q.color[0].to_bits(),
            q.color[1].to_bits(),
            q.color[2].to_bits(),
            q.alpha.to_bits(),
        ]);
        match &q.shade {
            None => key.push(u64::MAX),
            Some(shade) => {
                let bits = shade.bits();
                key.push(bits.len() as u64);
                key.extend(bits);
            }
        }
        key.push(q.coverage.len() as u64);
        key.extend(q.coverage.chunks(8).map(|c| {
            let mut word = [0u8; 8];
            word[..c.len()].copy_from_slice(c);
            u64::from_le_bytes(word)
        }));
    }
    let mut hasher = rustc_hash::FxHasher::default();
    for word in &key {
        hasher.write_u64(*word);
    }
    (hasher.finish(), key, [ox, oy], relative)
}
impl TextLayerCache {
    pub fn clear(&mut self) {
        self.layers.clear();
        self.nodes.clear();
        self.glyphs.clear();
        self.bytes = 0;
    }
    pub fn bytes(&self) -> usize {
        self.bytes
    }
    fn drop_nodes(&mut self) {
        let dropped: usize = self.nodes.values().flatten().map(Self::node_bytes).sum();
        self.bytes -= dropped;
        self.nodes.clear();
    }
    /// Node entries and glyph bitmaps describe pixels only under the fonts and glyph atlases they were
    /// made with
    pub fn begin_frame(&mut self, assets_changed: bool) {
        if assets_changed {
            self.drop_nodes();
            self.glyphs.clear();
        }
    }
    fn node_bytes(entry: &NodeEntry) -> usize {
        512 + match &entry.layer {
            NodeLayer::Empty => 0,
            NodeLayer::Pixels(_, rgba) => rgba.len(),
            NodeLayer::Quads(_, pixels) => pixels.quads.iter().map(|q| q.coverage.len() + 128).sum(),
        }
    }
    fn layer_bytes(entry: &LayerEntry) -> usize {
        entry.rgba.len() + entry.key.len() * 8 + 128
    }
    /// Drops the least recently used entries until `extra` more bytes fit
    fn make_room(&mut self, extra: usize) {
        if self.bytes + extra <= TEXT_CACHE_BYTES {
            return;
        }
        let mut order: Vec<(u64, bool, u64)> = self
            .layers
            .iter()
            .flat_map(|(h, list)| list.iter().map(move |e| (e.used, true, *h)))
            .chain(
                self.nodes
                    .iter()
                    .flat_map(|(h, list)| list.iter().map(move |e| (e.used, false, *h))),
            )
            .collect();
        order.sort_unstable();
        for (used, layer, hash) in order {
            if self.bytes + extra <= TEXT_CACHE_BYTES / 8 * 7 {
                break;
            }
            if layer {
                let list = self.layers.get_mut(&hash).unwrap();
                if let Some(i) = list.iter().position(|e| e.used == used) {
                    let removed = list.swap_remove(i);
                    self.bytes -= Self::layer_bytes(&removed);
                }
                if list.is_empty() {
                    self.layers.remove(&hash);
                }
            } else {
                let list = self.nodes.get_mut(&hash).unwrap();
                if let Some(i) = list.iter().position(|e| e.used == used) {
                    let removed = list.swap_remove(i);
                    self.bytes -= Self::node_bytes(&removed);
                }
                if list.is_empty() {
                    self.nodes.remove(&hash);
                }
            }
        }
    }
    /// Makes a node entry (`layer` at this node's placement) or completes an existing one
    fn store_node(&mut self, store: &mut TextStore, layer: NodeLayer) {
        self.clock += 1;
        let used = self.clock;
        let relative = |layer: NodeLayer, anchor: [f64; 2]| {
            let by = [-anchor[0], -anchor[1]];
            match layer {
                NodeLayer::Empty => NodeLayer::Empty,
                NodeLayer::Pixels(layout, rgba) => NodeLayer::Pixels(moved(layout, by), rgba),
                NodeLayer::Quads(layout, pixels) => NodeLayer::Quads(moved(layout, by), pixels),
            }
        };
        if let Some((hash, serial)) = store.existing.take() {
            // A deferred entry composited now: its pixels replace its quads (its layout stays)
            let Some(entry) = self
                .nodes
                .get_mut(&hash)
                .and_then(|list| list.iter_mut().find(|e| e.serial == serial))
            else {
                return;
            };
            if let (NodeLayer::Quads(layout, _), NodeLayer::Pixels(_, rgba)) = (&entry.layer, layer) {
                let before = Self::node_bytes(entry);
                entry.layer = NodeLayer::Pixels(*layout, rgba);
                entry.used = used;
                let after = Self::node_bytes(entry);
                self.bytes = self.bytes + after - before;
            }
            return;
        }
        let Some((hash, anchor, text, strokes)) = store.node.take() else {
            return;
        };
        self.serial += 1;
        let entry = NodeEntry {
            serial: self.serial,
            text,
            strokes,
            anchor,
            layer: relative(layer, anchor),
            used,
        };
        let bytes = Self::node_bytes(&entry);
        self.make_room(bytes);
        self.bytes += bytes;
        self.nodes.entry(hash).or_default().push(entry);
    }
    /// Plans an unrotated text node's layers (the others as prep_text draws them)
    pub fn plan(
        &mut self,
        node: &Value,
        fonts: &Value,
        assets: &AssetStore,
        scale: f64,
    ) -> Result<TextPlan, TextError> {
        if !scale.is_finite() || scale <= 0.0 {
            return Err(TextError("invalid text device scale".into()));
        }
        if node["matrix"].is_array()
            || !arr(&node["selection"]["rects"]).is_empty()
            || node["caret"]["visible"].as_bool() == Some(true)
        {
            return Ok(TextPlan::Layers(prep_text(node, fonts, assets, scale)?));
        }
        if scale.to_bits() != self.node_scale {
            self.drop_nodes();
            self.node_scale = scale.to_bits();
        }
        self.clock += 1;
        let clock = self.clock;
        let node_key = node_key(node, scale);
        if let Some((hash, anchor)) = node_key {
            let text = &node["text"];
            let strokes = &node["strokes"];
            if let Some(entry) = self.nodes.get_mut(&hash).and_then(|list| {
                list.iter_mut().find(|e| {
                    e.strokes == *strokes && same_text(&e.text, e.anchor, text, anchor)
                })
            }) {
                entry.used = clock;
                self.node_hits += 1;
                return Ok(match &entry.layer {
                    NodeLayer::Empty => TextPlan::Layers(Vec::new()),
                    NodeLayer::Pixels(layout, rgba) => {
                        TextPlan::Layers(vec![layer_quad(moved(*layout, anchor), rgba.clone())])
                    }
                    NodeLayer::Quads(layout, pixels) => {
                        let layout = moved(*layout, anchor);
                        TextPlan::Composite {
                            layout,
                            pixels: pixels.clone(),
                            store: TextStore {
                                layer: None,
                                node: None,
                                existing: Some((hash, entry.serial)),
                                layout,
                            },
                        }
                    }
                });
            }
        }
        let quads = quads_memo(node, fonts, assets, scale, Some(&mut self.glyphs))?;
        // Scaled quads' corners snap the same moved by whole px unless one sat on a snapping boundary
        let settled = quads.iter().all(|q| q.margin > 1e-6);
        let node_entry = |relative: bool| {
            node_key
                .filter(|_| relative && settled)
                .map(|(hash, anchor)| (hash, anchor, node["text"].clone(), node["strokes"].clone()))
        };
        let empty_layout = TextLayout { x: 0.0, y: 0.0, w: 0.0, h: 0.0, width: 0, height: 0 };
        if quads.is_empty() {
            let mut store = TextStore { layer: None, node: node_entry(true), existing: None, layout: empty_layout };
            self.store_node(&mut store, NodeLayer::Empty);
            return Ok(TextPlan::Layers(Vec::new()));
        }
        let (hash, key, origin, relative) = quads_key(&quads);
        if let Some(entry) = self
            .layers
            .get_mut(&hash)
            .and_then(|list| list.iter_mut().find(|e| e.key == key))
        {
            entry.used = clock;
            self.hits += 1;
            let layout = moved(entry.layout, origin);
            let rgba = entry.rgba.clone();
            let mut store = TextStore { layer: None, node: node_entry(relative), existing: None, layout };
            self.store_node(&mut store, NodeLayer::Pixels(layout, rgba.clone()));
            return Ok(TextPlan::Layers(vec![layer_quad(layout, rgba)]));
        }
        self.misses += 1;
        let Some(layout) = composite_layout(&quads)? else {
            let mut store = TextStore { layer: None, node: node_entry(relative), existing: None, layout: empty_layout };
            self.store_node(&mut store, NodeLayer::Empty);
            return Ok(TextPlan::Layers(Vec::new()));
        };
        Ok(TextPlan::Composite {
            layout,
            pixels: TextPixels {
                quads: Arc::new(quads),
                layout,
            },
            store: TextStore {
                layer: Some((hash, key, origin)),
                node: node_entry(relative),
                existing: None,
                layout,
            },
        })
    }
    /// A deferred composite's pixels are identified by its quads key (same key, same pixels)
    pub fn pending_key<'a>(store: &'a mut TextStore, pixels: &TextPixels) -> (u64, &'a [u64]) {
        let layer = store.layer.get_or_insert_with(|| {
            let (hash, key, origin, _) = quads_key(&pixels.quads);
            // Placed at the stored quads' corner: the key and layout below are relative to it
            (hash, key, origin)
        });
        (layer.0, &layer.1)
    }
    /// The layer key of a queued composite (pending_key computed it)
    pub fn queued_key(store: &TextStore) -> Option<(u64, &[u64])> {
        store.layer.as_ref().map(|layer| (layer.0, layer.1.as_slice()))
    }
    /// A composite that is not drawn this time (clipped out): its node entry keeps the quads
    pub fn defer(&mut self, mut store: TextStore, pixels: TextPixels) {
        if store.node.is_some() {
            let layout = store.layout;
            self.store_node(&mut store, NodeLayer::Quads(layout, pixels));
        }
    }
    /// Keeps a deferred composite's pixels (TextPlan::Composite) under its keys
    pub fn store(&mut self, mut store: TextStore, pixels: &TextPixels, rgba: Arc<[u8]>) {
        let layout = store.layout;
        self.store_node(&mut store, NodeLayer::Pixels(layout, rgba.clone()));
        let Some((hash, key, origin)) = store.layer.take() else {
            return;
        };
        if self.layers.get(&hash).is_some_and(|list| list.iter().any(|e| e.key == key)) {
            return;
        }
        self.clock += 1;
        // Relative to the quads' own corner, as the pixels were composited over pixels.layout
        let entry = LayerEntry {
            key,
            layout: moved(pixels.layout, [-origin[0], -origin[1]]),
            rgba,
            used: self.clock,
        };
        let bytes = Self::layer_bytes(&entry);
        if bytes > TEXT_CACHE_BYTES / 4 {
            return;
        }
        self.make_room(bytes);
        self.bytes += bytes;
        self.layers.entry(hash).or_default().push(entry);
    }
}

/// Prepare ordered premultiplied layers, already laid out in device coordinates.
/// Glyph atlas misses are explicit errors; callers can retry after loading the requested atlas.
pub fn prep_text(
    node: &Value,
    fonts: &Value,
    assets: &AssetStore,
    scale: f64,
) -> Result<Vec<TextQuad>, TextError> {
    if !scale.is_finite() || scale <= 0.0 {
        return Err(TextError("invalid text device scale".into()));
    }
    let prepare = |node: &Value| -> Result<Vec<TextQuad>, TextError> {
        let q = quads(node, fonts, assets, scale)?;
        if node["matrix"].is_array() {
            let mut m: [f64; 6] = std::array::from_fn(|i| num(&node["matrix"][i], [1.0, 0.0, 0.0, 1.0, 0.0, 0.0][i]));
            m[4] *= scale;
            m[5] *= scale;
            Ok(rotated(&q, Some(m)))
        } else {
            composite(&q)
        }
    };
    let rects: Vec<_> = arr(&node["selection"]["rects"])
        .iter()
        .map(snapped_rect)
        .collect();
    let mut out = Vec::new();
    if !rects.is_empty() {
        let c = std::array::from_fn(|i| {
            if i == 3 {
                255
            } else {
                num(&node["selection"]["color"][i], 0.0).clamp(0.0, 255.0) as u8
            }
        });
        for r in &rects {
            if let Some(q) = solid(*r, c) {
                out.push(q);
            }
        }
        let mut normal = prepare(node)?;
        for q in &mut normal {
            q.selection_mode = 1;
            q.selection_rects = rects.clone();
        }
        out.extend(normal);
        let mut selected = node.clone();
        selected["gradient"] = Value::Null;
        selected["text"]["color"] = node["selection"]
            .get("textColor")
            .cloned()
            .unwrap_or_else(|| serde_json::json!([1, 1, 1]));
        let mut highlighted = prepare(&selected)?;
        for q in &mut highlighted {
            q.selection_mode = 2;
            q.selection_rects = rects.clone();
        }
        out.extend(highlighted);
    } else {
        out.extend(prepare(node)?);
    }
    if node["caret"]["visible"].as_bool() == Some(true) {
        let caret = &node["caret"];
        let x = snap(n(caret, "x", 0.0));
        let y = snap(n(caret, "y", 0.0));
        let r = snap(n(caret, "x", 0.0) + n(caret, "w", 0.0)).max(x + 1.0);
        let b = snap(n(caret, "y", 0.0) + n(caret, "h", 0.0));
        let c = color(&caret["color"], [0.0; 3]);
        if let Some(q) = solid([x, y, r, b], [c[0] as u8, c[1] as u8, c[2] as u8, 255]) {
            out.push(q);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn intrinsic_requirements_detect_partial_atlases_even_while_transparent() {
        let fonts = serde_json::json!([{"file":"font.ttf","ascender":900,"descender":-300,"unitsPerEm":1000,"weight":400}]);
        let mut node = serde_json::json!({"culled":true,"strokes":[{"mode":"Contextual","thickness":1.25,"joins":"Miter","transparency":1},{"mode":"Border","thickness":7}],"text":{"weight":700,"transparency":1,"stroke":{"transparency":1},"runs":[{"weight":700,"transparency":1,"stroke":{"thickness":2,"joins":"Bevel","transparency":1}}],"lines":[{"x":0,"top":0,"glyphs":[[0,7,0,20],[0,8,12,20,0],[0,7,25,20]]}]}});
        let requirements = glyph_requirements(&node, &fonts, 1.5);
        let strings: Vec<Vec<_>> = requirements
            .iter()
            .map(|keys| keys.iter().map(|k| k.0.as_str()).collect())
            .collect();
        assert_eq!(
            strings,
            vec![
                vec!["0|20|7|1.796875|miter+b46", "0|20|7|1.796875|miter"],
                vec!["0|20|8|3|bevel+b46", "0|20|8|3|bevel"],
                vec!["0|20|7|0|b46", "0|20|7|0"],
                vec!["0|20|8|0|b46", "0|20|8|0"]
            ]
        );
        let ready = |present: &std::collections::HashSet<GlyphKey>| {
            requirements
                .iter()
                .all(|alternatives| alternatives.iter().any(|key| present.contains(key)))
        };
        let mut present: std::collections::HashSet<_> = requirements
            .iter()
            .map(|keys| keys.last().unwrap().clone())
            .collect();
        assert!(ready(&present));
        present.remove(requirements[0].last().unwrap());
        assert!(!ready(&present));
        present.insert(GlyphKey::new(0, 20.0, 999, 0.0, None));
        assert!(!ready(&present));
        node["text"]["transparency"] = serde_json::json!(0);
        node["strokes"][0]["transparency"] = serde_json::json!(0);
        node["text"]["runs"][0]["stroke"]["transparency"] = serde_json::json!(0);
        assert_eq!(glyph_requirements(&node, &fonts, 1.5), requirements);
    }
    /// Arimo's metrics at S = 54: a scene of today sizes the face with its line gap and says so
    /// (`sizeUnits`); a scene saved before sized it over ascender - descender. Both are 54 px text whose
    /// baseline is the ascender's share of the 55 px line, 1854 * 55 / 2355 = 43.3.
    #[test]
    fn line_gap_faces_keep_their_size_and_baseline() {
        let sized = serde_json::json!({"ascender":1854,"descender":-434,"unitsPerEm":2048,"lineGap":67,"sizeUnits":2355});
        let saved = serde_json::json!({"ascender":1854,"descender":-434,"unitsPerEm":2048,"lineGap":67});
        for (font, units) in [(&sized, 2355.0), (&saved, 2288.0)] {
            let ppem = 55.0 * 2048.0 / units;
            assert_eq!(font_size(font, ppem), 54.0);
            assert_eq!(ascent(font, ppem), 43.0);
        }
        let plain = serde_json::json!({"ascender":984,"descender":-273,"unitsPerEm":1000,"lineGap":0});
        assert_eq!(font_size(&plain, 55.0 * 1000.0 / 1257.0), 54.0);
        assert_eq!(ascent(&plain, 55.0 * 1000.0 / 1257.0), 43.0);
    }
    #[test]
    fn shaped_positions_share_largest_run_baseline() {
        let fonts = serde_json::json!([{"ascender":900,"descender":-300,"unitsPerEm":1000,"lineGap":120,"weight":400},{"ascender":1100,"descender":-100,"unitsPerEm":1000,"lineGap":0,"weight":700}]);
        let text = serde_json::json!({"weight":700,"quadScale":1.025,"runs":[{"weight":400},{"weight":700,"quadScale":1.125}],"lines":[{"x":10.5,"top":20.5,"glyphs":[[0,2,-0.5,20],[1,3,12.6,20,0],[0,4,28.4,21,1]],"runs":[[0,12,27],[1,27,40]]}]});
        for rotated in [false, true] {
            let (p, lines) = placements(&text, &fonts, rotated);
            assert_eq!(
                p.iter().map(|p| p.x).collect::<Vec<_>>(),
                [11.0, 24.0, 39.0]
            );
            assert_eq!(p.iter().map(|p| p.bold).collect::<Vec<_>>(), [46, 0, 48]);
            assert_eq!(
                p.iter().map(|p| p.scale).collect::<Vec<_>>(),
                [1.025, 1.025, 1.125]
            );
            assert!(
                p.iter()
                    .all(|p| p.baseline == if rotated { 37.5 } else { 38.0 })
            );
            assert_eq!(lines.len(), 1);
        }
    }
    #[test]
    fn layers_match_javascript_reference_bytes() {
        // The renderer's own composited text layers (premultiplied like putImageData): overlapping alpha,
        // gradients, and rounded scaled glyph boxes. First frozen from the retired JS reference renderer;
        // re-recorded when gradients took Studio 0.747's shading (notes/gradients.md).
        let expected = [
            152133933u32,
            2151987651,
            3865013693,
            107218451,
            36224343,
            3596900980,
            3831697897,
            745336558,
            2346253978,
            2407493611,
            2971524314,
            484022522,
            3602327637,
            3510380532,
            3983779230,
            1881406681,
            1012277439,
            1317866715,
            3383924190,
            4141522597,
            2497448911,
            938811360,
            1539427742,
            293323101,
            4217771824,
            3216222403,
            290088886,
            1799847425,
            3347736062,
            9123480,
            1367455976,
            431923343,
        ];
        // Scaled + shaded cases (seed + z = 0 mod 6) re-recorded when shadedGlyph started evaluating the
        // gradient where a scaled quad's texel is drawn (in the retired JS reference renderer)
        let rotated_expected = [
            3460676127u32,
            3845739527,
            2348228749,
            2794967889,
            743280629,
            3142216915,
            1181634440,
            1353965785,
            1912614205,
            2840578386,
            1610427896,
            3524184730,
            1806629210,
            2539997954,
            145893460,
            3304151603,
            494620619,
            1505683533,
            4025088656,
            2305794818,
            2680432833,
            4019654550,
            2334939259,
            3272199215,
            2904627692,
            1488435707,
            4118134363,
            1021702607,
            3772958744,
            3139266782,
            2297257170,
            3906520739,
        ];
        for seed in 0..32usize {
            let gradient = serde_json::json!({"rotation":(seed*29)%360,"offset":[(seed as i32%3-1) as f64*0.13,(seed as i32%5-2) as f64*0.07],"color":[[0,0.1000,0.5020,1],[0.35,1,0.25,0],[0.8,0,0.7,1],[1,0.8,0,0.3]],"transparency":[[0,0.1],[0.6,0.7],[1,0]]});
            let mut quads = Vec::new();
            for z in 0..3usize {
                let w = 3 + z;
                let h = 2 + z;
                let x = (1 + z * 2) as f64;
                let y = (2 + z) as f64;
                let scaled = (seed + z) % 3 == 0;
                let bounds = if scaled {
                    [x - 1.0, y - 1.0, x + w as f64 + 3.0, y + h as f64 + 2.0]
                } else {
                    [x - 1.0, y - 1.0, x + w as f64 + 1.0, y + h as f64 + 1.0]
                };
                let color = [
                    ((seed * 17 + z * 31) % 256) as f64,
                    ((seed * 23 + z * 7) % 256) as f64,
                    ((seed * 11 + z * 47) % 256) as f64,
                ];
                let alpha = ((seed * 19 + z * 59) % 256) as f64;
                let shade = if (seed + z) % 2 == 0 {
                    Some(triangles(
                        &gradient,
                        [0.3, 0.7, 12.25, 9.5],
                        bounds,
                        color,
                        alpha,
                    ))
                } else {
                    None
                };
                quads.push(Quad {
                    x,
                    y,
                    w,
                    h,
                    coverage: (0..w * h)
                        .map(|i| ((i * 37 + seed * 13 + z * 51) % 256) as u8)
                        .collect::<Vec<_>>()
                        .into(),
                    bounds,
                    scaled,
                    skew: 0.0,
                    color,
                    alpha,
                    shade,
                    margin: f64::INFINITY,
                });
            }
            let layer = composite(&quads).unwrap();
            let hash = layer[0].rgba.iter().fold(2166136261u32, |hash, b| {
                (hash ^ (*b as u32)).wrapping_mul(16777619)
            });
            assert_eq!(hash, expected[seed], "JS reference mismatch, case {seed}");
            let images = rotated(&quads, None);
            let hash = images
                .iter()
                .flat_map(|q| q.rgba.iter())
                .fold(2166136261u32, |hash, b| {
                    (hash ^ (*b as u32)).wrapping_mul(16777619)
                });
            assert_eq!(
                hash, rotated_expected[seed],
                "JS rotated quad mismatch, case {seed}"
            );
            for (q, image) in quads.iter().zip(images.iter()) {
                assert_eq!(
                    image.opacity,
                    if q.shade.is_some() {
                        1.0
                    } else {
                        q.alpha / 255.0
                    }
                );
            }
        }
    }
    #[test]
    fn rotated_gradient_quads_match_javascript_pixels() {
        // The renderer's own pixels (straight bytes premultiplied like premul, with the layer's device
        // x, y, w, h), rotation 0.3 rad plus a translation; scaled and skewed quads included. First frozen
        // from the retired JS reference; re-recorded when gradients took Studio 0.747's shading
        // (notes/gradients.md).
        let expected: [u32; 32] = [586838623, 1011659745, 1958216108, 1335915821, 286540834, 1261274220, 3249525594, 1159370069, 2313921354, 3430172423, 190058411, 4224135584, 3446523647, 1319520680, 1378494408, 2524234903, 967837526, 3049370069, 1356555190, 1611016131, 2168049967, 2746809398, 3050758267, 2817750039, 1680019585, 1070852938, 3542117968, 2246948006, 899939119, 1561029642, 3006450711, 2244093526];
        let (cos, sin) = (0.3f64.cos(), 0.3f64.sin());
        let m = [cos, sin, -sin, cos, 7.25, -3.5];
        for seed in 0..32usize {
            let gradient = serde_json::json!({"rotation":(seed*29)%360,"offset":[(seed as i32%3-1) as f64*0.13,(seed as i32%5-2) as f64*0.07],"color":[[0,0.1,0.502,1],[0.35,1,0.25,0],[0.8,0,0.7,1],[1,0.8,0,0.3]],"transparency":[[0,0.1],[0.6,0.7],[1,0]]});
            let mut hash = 2166136261u32;
            let mut mix = |v: u32| hash = (hash ^ v).wrapping_mul(16777619);
            for z in 0..3usize {
                if (seed + z) % 2 != 0 {
                    continue;
                }
                let (w, h) = (3 + z, 2 + z);
                let (x, y) = ((1 + z * 2) as f64, (2 + z) as f64);
                let scaled = (seed + z) % 3 == 0;
                let bounds = if scaled {
                    [x - 1.0, y - 1.0, x + w as f64 + 3.0, y + h as f64 + 2.0]
                } else {
                    [x - 1.0, y - 1.0, x + w as f64 + 1.0, y + h as f64 + 1.0]
                };
                let color = [
                    ((seed * 17 + z * 31) % 256) as f64,
                    ((seed * 23 + z * 7) % 256) as f64,
                    ((seed * 11 + z * 47) % 256) as f64,
                ];
                let alpha = ((seed * 19 + z * 59) % 256) as f64;
                let q = Quad {
                    x,
                    y,
                    w,
                    h,
                    coverage: (0..w * h).map(|i| ((i * 37 + seed * 13 + z * 51) % 256) as u8).collect::<Vec<_>>().into(),
                    bounds,
                    scaled,
                    skew: if z == 2 { TEXT_ITALIC_SKEW } else { 0.0 },
                    color,
                    alpha,
                    shade: Some(triangles(&gradient, [0.3, 0.7, 12.25, 9.5], bounds, color, alpha)),
                    margin: f64::INFINITY,
                };
                let Some(layer) = rotated_shaded(&q, m) else { continue };
                assert!(layer.translation_only);
                for v in [layer.x + m[4], layer.y + m[5], layer.w, layer.h] {
                    mix((v as i64 as u32) & 0xff);
                }
                for px in layer.rgba.chunks(4) {
                    for c in px {
                        mix(*c as u32);
                    }
                }
            }
            assert_eq!(hash, expected[seed], "JS rotatedShadedPixels mismatch, case {seed}");
        }
    }

    #[test]
    fn alpha_follows_gpu_twelve_bit_conversion() {
        assert_eq!(gpu8(128.0 / 255.0 * 128.0 / 255.0), 64.0);
        assert_eq!(byte(0.5), 128.0);
        assert_eq!(snap(-0.5), 0.0);
    }
    #[test]
    fn overlapping_glyphs_round_once() {
        let q = Quad {
            x: 1.0,
            y: 1.0,
            w: 1,
            h: 1,
            coverage: vec![255].into(),
            bounds: [0.0, 0.0, 3.0, 3.0],
            scaled: false,
            skew: 0.0,
            color: [100.0, 200.0, 50.0],
            alpha: 128.0,
            shade: None,
            margin: f64::INFINITY,
        };
        let layer = composite(&[q.clone(), q]).unwrap();
        assert_eq!(&layer[0].rgba[16..20], &[75, 150, 38, 192]);
    }
    #[test]
    fn text_takes_the_gradient_at_its_pixel() {
        let g = serde_json::json!({"color":[[0,1,0,0],[0.5,0,1,0],[1,0,0,1]],"transparency":[[0,0],[1,0]]});
        let shade = triangles(
            &g,
            [0.0, 0.0, 10.0, 10.0],
            [0.0, 0.0, 10.0, 10.0],
            [255.0; 3],
            255.0,
        );
        let q = Quad {
            x: 0.0,
            y: 0.0,
            w: 1,
            h: 1,
            coverage: vec![255].into(),
            bounds: [0.0, 0.0, 10.0, 10.0],
            scaled: false,
            skew: 0.0,
            color: [255.0; 3],
            alpha: 255.0,
            shade: Some(shade),
            margin: f64::INFINITY,
        };
        assert_eq!(shading(&q, 5.0, 5.0, 255.0), [0.0, 255.0, 0.0, 255.0]);
    }
}
