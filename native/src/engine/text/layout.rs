//! Port of runtime/text/layout.luau: text measurement and line layout in physical pixels,
//! reproducing Roblox's measured behaviour:
//!   size S = floor(floor(TextSize) * pixelScale);
//!   font ppem = (S + 1) * unitsPerEm / (ascender - descender + lineGap); a face with a line gap gets
//!   its ascender - descender requested at that share of S + 1 px, cut to 26.6 (`scales`)
//!   glyph advance = FT_MulFix(nominal, 16.16 scale) + floor(GPOS adjustment at the HarfBuzz scale), in
//!   26.6; the pen adds the exact advances and is rounded half up to a whole pixel (`round_pen`).
//! Every measured rule keeps the comment of the Luau reference (and its measurement).
//! All arithmetic is f64 like Luau numbers; float32 steps go through `f32r`.

use std::sync::Arc;

use super::fonts::{FaceId, FontRegistry};
use super::num::{clamp, f32r, idiv, lmax};
use super::opentype::Font;
use super::utf8::{Codepoint, codepoints_into};
use super::{KeyHasher, TextEngine};
use crate::cache_counts::{self, Cache};

/// A positioned glyph (`TextLayout.Glyph`). `x` is the pen offset in whole px from the line start,
/// `size` the pixel size it is laid out at (rich `<font size>` runs differ from the label's size),
/// `run` the 1-based rich run index (0 for plain text).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glyph {
    pub glyph: u32,
    pub face: FaceId,
    pub x: f64,
    pub x_offset: f64,
    pub advance: f64,
    /// 1-based byte position of the glyph's first code point in its (run) text.
    pub cluster: u32,
    pub code: u32,
    pub size: f64,
    pub run: u32,
    /// The exact pen before the glyph in px (26.6 steps), from the start of the text (`round_pen`). A
    /// truncation's ellipsis is a pen of its own, below zero (`TextEngine::ellipsis`).
    pub pen: f64,
    /// The exact pen after the glyph in px.
    pub end: f64,
    /// The GPOS x offset in px, exact in 26.6 (`x_offset` is it in whole pixels).
    pub offset: f64,
}

impl Glyph {
    /// Whether the glyph is part of a truncation's ellipsis (`TextEngine::ellipsis`: the only pens below
    /// zero).
    pub fn is_ellipsis(&self) -> bool {
        self.pen < 0.0
    }
}

/// One laid-out line (`TextLayout.Line`). `first` / `last` are the 1-based glyph range in the
/// paragraph (`last = first - 1` when empty), `width` the drawn width used for alignment.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub glyphs: Vec<Glyph>,
    pub width: f64,
    pub first: usize,
    pub last: usize,
    pub paragraph_end: bool,
    pub split: bool,
    pub truncated: bool,
    pub top: f64,
    pub height: f64,
    pub default_size: f64,
}

impl Line {
    fn new(glyphs: Vec<Glyph>, width: f64, first: usize, last: usize) -> Line {
        Line { glyphs, width, first, last, paragraph_end: false, split: false, truncated: false, top: 0.0, height: 0.0, default_size: 0.0 }
    }
}

/// `TextLayout.Result` (plus the rich wrapper's `Runs` / `Rich`).
#[derive(Clone, Debug)]
pub struct TextLayout {
    pub lines: Vec<Line>,
    pub fits: bool,
    pub natural_width: f64,
    pub broken: bool,
    /// TextBounds.X in physical px.
    pub width: f64,
    /// TextBounds.Y in physical px.
    pub height: f64,
    pub line_height: f64,
    /// Pixel size of the text (the rich base size for rich text).
    pub size: f64,
    pub face: Option<FaceId>,
    /// Parsed rich runs (valid rich text only); glyph `run` indexes them 1-based.
    pub runs: Option<Arc<Vec<super::richtext::Run>>>,
    pub rich: bool,
    /// Wrapped lines left out because they did not fit the box height (a diagnostic: nothing draws them).
    pub dropped: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Truncate {
    #[default]
    None,
    AtEnd,
    SplitWord,
}

impl Truncate {
    /// Enum.TextTruncate item name ("AtEnd", "SplitWord"; anything else is none).
    pub fn from_name(name: &str) -> Truncate {
        match name {
            "AtEnd" => Truncate::AtEnd,
            "SplitWord" => Truncate::SplitWord,
            _ => Truncate::None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XAlign {
    Left,
    Center,
    Right,
}

impl XAlign {
    /// Enum.TextXAlignment item name; anything but Left / Right centres (like LinePen).
    pub fn from_name(name: &str) -> XAlign {
        match name {
            "Left" => XAlign::Left,
            "Right" => XAlign::Right,
            _ => XAlign::Center,
        }
    }
}

/// `TextLayout.Params` without the rich-only internals.
#[derive(Clone, Copy, Debug)]
pub struct LayoutParams<'a> {
    pub text: &'a [u8],
    pub family: &'a str,
    pub weight: f64,
    pub style: &'a str,
    pub text_size: f64,
    pub pixel_scale: f64,
    pub wrapped: bool,
    pub max_width: Option<f64>,
    pub max_height: Option<f64>,
    pub line_height: Option<f64>,
    pub truncate: Truncate,
    pub scale: Option<f64>,
    pub pixel_size: Option<f64>,
    pub effective_size: Option<f64>,
}

impl<'a> LayoutParams<'a> {
    pub fn new(text: &'a [u8], family: &'a str, weight: f64, style: &'a str, text_size: f64, pixel_scale: f64) -> Self {
        LayoutParams {
            text,
            family,
            weight,
            style,
            text_size,
            pixel_scale,
            wrapped: false,
            max_width: None,
            max_height: None,
            line_height: None,
            truncate: Truncate::None,
            scale: None,
            pixel_size: None,
            effective_size: None,
        }
    }
}

/// Pre-shaped paragraphs and extras a rich layout hands to the shared line layout.
pub(crate) struct Prepared<'p> {
    pub paragraphs: &'p [Vec<Glyph>],
    pub paragraph_sizes: &'p [f64],
    pub cache_key: &'p str,
    pub empty_height: Option<f64>,
}

#[inline]
pub fn is_space(code: u32) -> bool {
    code == 32 || code == 9 || code == 0x3000
}

#[inline]
fn is_cjk(code: u32) -> bool {
    (0x2E80..=0x9FFF).contains(&code)
        || (0xAC00..=0xD7AF).contains(&code)
        || (0xF900..=0xFAFF).contains(&code)
        || (0xFF00..=0xFFEF).contains(&code)
        || (0x20000..=0x2FFFF).contains(&code)
}

/// Luau `tostring` of a number, for diagnostics.
pub fn luau_number(value: f64) -> String {
    // Luau's tostring: "nan" / "inf" / "-inf" (Rust writes NaN / inf)
    if value.is_nan() {
        return "nan".into();
    }
    if value.is_infinite() {
        return if value > 0.0 { "inf".into() } else { "-inf".into() };
    }
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

/// Whole device pixel where a line's pen starts. Measured (fixtures/layout-textpen.luau 192/192,
/// tools/analysis/layout_textpen.py): Studio snaps the alignment anchor of the content box (left
/// edge, right edge or centre, device px) half up to the pixel grid, then places the line from it in
/// whole pixels. Rounded to 1/1000 px first: positions that are exactly .5 in Studio's float32
/// arithmetic can land just below .5 in the layout's double arithmetic.
#[inline]
pub fn snap_pixel(value: f64) -> f64 {
    ((value * 1000.0 + 0.5).floor() / 1000.0 + 0.5).floor()
}

/// `TextLayout.LinePen`: Left = round(x), Right = round(x + w) - width, Center = round(x + w / 2) -
/// floor(width / 2). Rich text halves its width in logical px and floors it back in device px, so a
/// line whose width / DPI is inexact comes out just under and centres one pixel further right: its
/// half width is floor((width - 1) / 2). Measured: fixtures/layout-textpen2.luau 128/128 rich lines at
/// 1.5x (widths not a multiple of 3 px) and at 1x (never: every width is exact). A right-aligned rich
/// line of such a width starts one pixel further right the same way: round(x + w) - (width - 1)
/// (fixtures/text-probe-tie-size.luau: 16 of 16 such lines, and the truncated lines of
/// fixtures/text-probe-richtrunc.luau: 50, 77, 139, 175 and 232 px wide but not 120 or 255). `exact`: the anchor
/// left unsnapped, as rotated text places its quads before resampling them (text-probe-rotation-size:
/// 0 differing pixels).
pub fn line_pen(alignment: XAlign, content_x: f64, content_w: f64, line_width: f64, rich: bool, exact: bool, pixel_scale: f64) -> f64 {
    let snap = |value: f64| if exact { value } else { snap_pixel(value) };
    match alignment {
        XAlign::Left => snap(content_x),
        XAlign::Right if rich && !divides_exactly(line_width, pixel_scale) => snap(content_x + content_w) - (line_width - 1.0),
        XAlign::Right => snap(content_x + content_w) - line_width,
        XAlign::Center => {
            let mut half = idiv(line_width, 2.0);
            if rich && !divides_exactly(line_width, pixel_scale) {
                half = idiv(line_width - 1.0, 2.0);
            }
            snap(content_x + content_w / 2.0) - half
        }
    }
}

/// Whether `width / scale` is exact in binary floating point (a multiple of 1/1024 px, as every
/// width / DPI is at 1x and 2x, and at 1.5x only for multiples of 3 px).
pub(crate) fn divides_exactly(width: f64, scale: f64) -> bool {
    let logical = width / scale * 1024.0;
    (logical - logical.round()).abs() < 1e-6
}

/// Families Roblox draws larger than their TextSize (measured: Legacy text is 1.5x).
#[inline]
pub fn size_multiplier(family: Option<&str>) -> f64 {
    match family {
        Some("rbxasset://fonts/families/LegacyArial.json") => 1.5,
        _ => 1.0,
    }
}

/// `TextLayout.SizeFor`. Measured (198 samples): under UIScale the size is floor(floor(TextSize *
/// UIScale) * DPI), i.e. the scaled TextSize is floored in logical px before the DPI scale (the
/// product in float32). Returns (integer TextSize, pixel size).
pub fn size_for(text_size: f64, pixel_scale: f64, family: Option<&str>, scale: Option<f64>, effective_size: Option<f64>) -> (f64, f64) {
    let integer = clamp(text_size.floor(), 1.0, 100.0);
    let effective = effective_size.unwrap_or_else(|| lmax((f32r(integer * scale.unwrap_or(1.0)) + 1e-9).floor(), 1.0));
    let multiplier = size_multiplier(family);
    (integer, (effective * multiplier * pixel_scale + 1e-9).floor())
}

/// `TextLayout.QuadSizeFor`: the pixel size before flooring, E * multiplier * DPI
/// (fixtures/text-probe-fracsize.luau): Studio rasterizes the glyphs at the floored size but draws every
/// glyph quad scaled by this size over the floored one about the glyph's pen point.
pub fn quad_size_for(text_size: f64, pixel_scale: f64, family: Option<&str>, scale: Option<f64>, effective_size: Option<f64>) -> f64 {
    let integer = clamp(text_size.floor(), 1.0, 100.0);
    let effective = effective_size.unwrap_or_else(|| lmax((f32r(integer * scale.unwrap_or(1.0)) + 1e-9).floor(), 1.0));
    effective * size_multiplier(family) * pixel_scale
}

/// `TextLayout.RichSizeFor`. Measured (layout-probe-richsize 424/424, -richsize2, -richempty): rich text
/// takes the scaled size E back to unscaled units and scales it again, all in float32: S =
/// floor(f32(f32(f32(E / UIScale) * UIScale) * DPI)), so E = 30 at UIScale 0.885 is 44 px where plain
/// text is 45. A size set by a UITextSizeConstraint (EffectiveSize) is used as it is, like plain text
/// (layout-probe-scaledcap). Also returns the scaled size E itself (rich text without any character
/// reports TextBounds.Y = E, unfloored: 13.5 px at E = 9).
pub fn rich_size_for(text_size: f64, pixel_scale: f64, family: Option<&str>, scale: Option<f64>, effective_size: Option<f64>) -> (f64, f64) {
    if let Some(effective) = effective_size {
        let (_, size) = size_for(text_size, pixel_scale, family, scale, Some(effective));
        return (size, effective * size_multiplier(family));
    }
    let integer = clamp(text_size.floor(), 1.0, 100.0);
    let ui_scale = f32r(scale.unwrap_or(1.0));
    let effective = lmax((f32r(integer * ui_scale) + 1e-9).floor(), 1.0);
    let multiplier = size_multiplier(family);
    let unscaled = f32r(effective / ui_scale);
    let rescaled = f32r(unscaled * ui_scale);
    (lmax(f32r(rescaled * multiplier * pixel_scale).floor(), 1.0), effective * multiplier)
}

/// `TextLayout.RichScaledPixels`. Rich TextScaled (layout-probe-richscaled2, 3005/3005 labels at
/// UIScale 0.75 .. 1.3): each candidate is a whole unscaled pixel size N; the size drawn is N taken
/// back to a TextSize and through UIScale like RichSizeFor, S = floor(f32(f32(f32(N / DPI) * UIScale)
/// * DPI)).
pub fn rich_scaled_pixels(unscaled: f64, pixel_scale: f64, family: Option<&str>, scale: Option<f64>) -> f64 {
    rich_scaled_unfloored(unscaled, pixel_scale, family, scale).floor()
}

/// The unfloored size behind `rich_scaled_pixels`: glyphs are rasterized at its floor, but every glyph
/// quad is drawn scaled by unfloored / floor (notes/text.md, 2026-09-27: UI kit button labels at UIScale
/// 0.885, N = 35 -> 30.975 px drawn from 30 px glyphs).
pub fn rich_scaled_unfloored(unscaled: f64, pixel_scale: f64, family: Option<&str>, scale: Option<f64>) -> f64 {
    let ui_scale = f32r(scale.unwrap_or(1.0));
    let multiplier = size_multiplier(family);
    let rescaled = f32r(f32r(unscaled / pixel_scale) * ui_scale);
    f32r(rescaled * multiplier * pixel_scale)
}

/// The hhea line height in font units, line gap included: Studio 0.746 sizes a face so that this is
/// S + 1 px (earlier versions left the gap out). Measured on every Studio face with a line gap (Arimo,
/// Ubuntu, Permanent Marker, AccanthisADFStd, Guru) and on Poppins, a cloud family:
/// data/studio/text-bounds-linegap-1.5x.json and -1x.json, where a face sized without the gap keeps 8 to
/// 111 of its 255 widths at DPI 1.5.
#[inline]
pub fn height_units(font: &Font) -> f64 {
    font.ascender - font.descender + font.line_gap
}

/// `TextLayout.Ppem`: FreeType's ppem for a line height of S + 1 px.
#[inline]
pub fn ppem(font: &Font, size: f64) -> f64 {
    (size + 1.0) * font.units_per_em / height_units(font)
}

/// `TextLayout.PixelAdvance`.
pub fn pixel_advance(units: f64, scale: f64) -> f64 {
    let fixed = (units * scale * 64.0 + 0.5).floor() / 64.0;
    (fixed + 0.5).floor()
}

/// `TextLayout.Scales`: the FreeType 16.16 x scale for a line height of S + 1 px and the HarfBuzz font
/// scale (26.6 units per em) derived from it. The scale is FreeType's for a real-dimension size request:
/// the face's ascender - descender at `request` 26.6 px. Without a line gap that is the whole line,
/// (S + 1) * 64. With one it is the line's share without the gap, cut to a whole 26.6 unit, so the scale
/// is a hair under (S + 1) / (ascender - descender + lineGap): Studio 0.746's widths fit only the cut
/// request (text-bounds-1.5x.json 3570/3570 where the uncut scale left 13 Arimo and LegacyArial strings
/// 1 px wide; the line gap datasets 3059/3060 at 1.5x and 3047/3060 at 1x, uncut 2980 and 2975).
#[inline]
pub fn scales(font: &Font, size: f64) -> (f64, f64) {
    let real = font.ascender - font.descender;
    let line = (size + 1.0) * 64.0;
    let request = if font.line_gap == 0.0 { line } else { (line * real / height_units(font)).floor() };
    let x_scale = idiv(request * 65536.0 + idiv(real, 2.0), real);
    let hb_scale = idiv(x_scale * font.units_per_em + 32768.0, 65536.0);
    (x_scale, hb_scale)
}

/// A glyph's unhinted advance in 26.6 as HarfBuzz reads it from FreeType: FT_Get_Advance's 16.16 value
/// (the font units times the scale over 64, rounded) rounded again to 26.6. The two roundings land one
/// 1/64 px above FT_MulFix for a few advances.
#[inline]
pub fn ft_advance(units: f64, x_scale: f64) -> f64 {
    idiv(idiv(units * x_scale + 32.0, 64.0) + 512.0, 1024.0)
}

/// `TextLayout.GlyphAdvance`: a glyph's advance in px, exact in 26.6: the nominal advance scaled with
/// FT_MulFix plus the GPOS adjustment scaled with the HarfBuzz scale and floored. `round_pen` turns a
/// text's advances into whole-pixel pen steps.
#[inline]
pub fn glyph_advance(nominal: f64, adjustment: f64, x_scale: f64, hb_scale: f64, units_per_em: f64) -> f64 {
    (ft_advance(nominal, x_scale) + idiv(adjustment * hb_scale, units_per_em)) / 64.0
}

/// The pen moves by the exact 26.6 advances and is rounded half up to whole pixels, rather than each
/// advance being rounded: every glyph's advance becomes round(pen after it) - round(pen before it). One
/// pen runs through the whole text, across rich text runs, line breaks and newlines, so a line is
/// round(pen at its end) - round(pen at its start) px wide. Measured (Studio 0.746, 1.5x): the
/// text-bounds, sweep and wrap datasets (data/studio/), 528/528 digit strings in four faces at TextSize
/// 8-40 ("11111" in Montserrat Bold at 18 px is 31, where rounding each 6.11 px advance gives 30),
/// "1<font>1</font>111" = "11111", and wrapped text after a newline (wrap-1.5x.json 538/540; restarting
/// the pen per paragraph gives 531, per line 489). Earlier Studio versions rounded each advance.
///
/// Without `subpixel` (FFlagLegacyTextSubpixelRendering off, production since 2026-11-12) each advance is
/// rounded half up on its own again, and a glyph's pen is the sum of the whole-pixel advances before it:
/// there is no exact pen. Measured (Studio 0.747, 1.5x): 1,280 glyphs of fixtures/text-probe-subpixel.luau
/// ("n" x 20 at 7.76 px advances 8 px every time), and TextBounds of tools/studio/measure-text.luau
/// ("iiiii" in SourceSansPro at 8 is 15 px, five advances of 3, where the pen gives 13).
pub fn round_pen(paragraphs: &mut [Vec<Glyph>], subpixel: bool) {
    let mut pen = 0.0;
    let mut rounded = 0.0;
    if !subpixel {
        for glyph in paragraphs.iter_mut().flatten() {
            glyph.pen = pen;
            glyph.advance = idiv(glyph.advance * 64.0 + 32.0, 64.0);
            pen += glyph.advance;
            glyph.end = pen;
        }
        return;
    }
    for glyph in paragraphs.iter_mut().flatten() {
        glyph.pen = pen / 64.0;
        pen += glyph.advance * 64.0;
        glyph.end = pen / 64.0;
        let next = idiv(pen + 32.0, 64.0);
        glyph.advance = next - rounded;
        rounded = next;
    }
}

/// Code points that continue an emoji sequence: variation selectors, ZWJ, skin tone modifiers, the
/// keycap and tag characters. They take the face of the code point before them when it has them.
#[inline]
fn continues_emoji(code: u32) -> bool {
    matches!(code, 0xFE00..=0xFE0F | 0x200D | 0x1F3FB..=0x1F3FF | 0x20E3 | 0xE0020..=0xE007F)
}

/// Default ignorables HarfBuzz hides when no ligature took them (hb_ot_hide_default_ignorables): zero
/// width spaces and joiners, word joiners, variation selectors, the BOM and tags.
#[inline]
fn hidden(code: u32) -> bool {
    matches!(code, 0x200B..=0x200F | 0x2060..=0x2064 | 0xFE00..=0xFE0F | 0xFEFF | 0xE0000..=0xE0FFF)
}

/// `TextLayout.ShapeParagraph`: shapes one paragraph (no newlines) into glyphs with pixel advances,
/// taking missing glyphs from the fallback faces (cloud fallbacks last). Appends to `out`.
pub fn shape_paragraph(fonts: &mut FontRegistry, codepoints: &[Codepoint], face: FaceId, size: f64, extra: Option<&[&str]>, out: &mut Vec<Glyph>) {
    if codepoints.is_empty() {
        return;
    }
    fonts.fallbacks();
    let primary = fonts.face(face).font.clone();
    // Runs of consecutive code points with the same chosen face: (face, start, end).
    let mut runs: Vec<(FaceId, usize, usize)> = Vec::new();
    for (index, &(code, _)) in codepoints.iter().enumerate() {
        let mut chosen = face;
        // An emoji sequence stays in one face, so it shapes as one run and its ligature applies (a ZWJ
        // family is one Twemoji glyph in Studio, fixtures/text-probe-emoji.luau Zwj / ZwjCouple)
        let previous = index.checked_sub(1).map(|before| (codepoints[before].0, runs.last().unwrap().0));
        let joined = previous.filter(|&(before, before_face)| {
            (continues_emoji(code) || before == 0x200D) && fonts.face(before_face).font.has_glyph(code)
        });
        if let Some((_, before_face)) = joined {
            chosen = before_face;
        } else if !primary.has_glyph(code) && !is_space(code) {
            let mut found = None;
            for &fallback in fonts.loaded_fallbacks() {
                if fonts.face(fallback).font.has_glyph(code) {
                    found = Some(fallback);
                    break;
                }
            }
            if found.is_none() {
                let cloud: Vec<FaceId> = fonts.cloud_fallbacks().to_vec();
                for fallback in cloud {
                    if fonts.face(fallback).font.has_glyph(code) {
                        found = Some(fallback);
                        break;
                    }
                }
            }
            if let Some(found) = found {
                chosen = found;
            }
        }
        match runs.last_mut() {
            Some(run) if run.0 == chosen => run.2 = index + 1,
            _ => runs.push((chosen, index, index + 1)),
        }
    }

    for (run_face, start, end) in runs {
        let font = fonts.face(run_face).font.clone();
        let (x_scale, hb_scale) = scales(&font, size);
        let units_per_em = font.units_per_em;
        let run_codepoints = &codepoints[start..end];
        let shaped = font.shape(run_codepoints, if run_face == face { extra } else { None });
        out.reserve(shaped.len());
        for info in shaped {
            let nominal = font.advance(info.glyph) as f64;
            let code = match run_codepoints.binary_search_by_key(&info.cluster, |entry| entry.1) {
                Ok(position) => run_codepoints[position].0,
                Err(_) => run_codepoints.iter().rev().find(|entry| entry.1 == info.cluster).map(|entry| entry.0).unwrap_or(0),
            };
            if hidden(code) {
                // HarfBuzz's hidden default ignorable: the space glyph, no advance (Studio: "A\u{FE0F}B" and
                // "A\u{200B}B" measure as "AB", "🏜\u{FE0F}" as "🏜"; Twemoji gives U+FE0F a full em)
                out.push(Glyph {
                    glyph: font.cmap.get(&32).copied().unwrap_or(info.glyph),
                    face: run_face,
                    x: 0.0,
                    x_offset: 0.0,
                    advance: 0.0,
                    cluster: info.cluster,
                    code,
                    size,
                    run: 0,
                    pen: 0.0,
                    end: 0.0,
                    offset: 0.0,
                });
                continue;
            }
            // A `kern` table pair (HarfBuzz's fallback kerning): the pair's value is scaled to 26.6 and
            // floored, then split: its floored half on the first glyph's advance, the rest on the second
            // glyph's advance and offset. Studio 0.747's quarter-pixel pens in Permanent Marker
            // (fixtures/text-fonts.luau: pairs of -10 and -20 units at 1229/1024 are -13 and -25, where
            // scaling each half gave -14 and -26).
            let scaled = |units: i64| idiv(units as f64 * hb_scale, units_per_em);
            let kern_first = idiv(scaled(info.kern[0]), 2.0);
            let kern_second = scaled(info.kern[1]) - idiv(scaled(info.kern[1]), 2.0);
            out.push(Glyph {
                glyph: info.glyph,
                face: run_face,
                x: 0.0,
                x_offset: idiv(idiv(info.x_offset as f64 * hb_scale, units_per_em) + kern_second + 32.0, 64.0),
                advance: glyph_advance(nominal, info.advance as f64 - nominal, x_scale, hb_scale, units_per_em) + (kern_first + kern_second) / 64.0,
                cluster: info.cluster,
                code,
                size,
                run: 0,
                pen: 0.0,
                end: 0.0,
                offset: (idiv(info.x_offset as f64 * hb_scale, units_per_em) + kern_second) / 64.0,
            });
        }
    }
}

/// `MakeLine` over glyphs[start..end] (0-based), with 1-based first / last like the Luau.
fn make_line(glyphs: &[Glyph], start: usize, end: usize, trim_trailing: bool) -> Line {
    let mut last_counted = end;
    if trim_trailing {
        while last_counted > start && is_space(glyphs[last_counted - 1].code) {
            last_counted -= 1;
        }
    }
    let mut width = 0.0;
    let mut out = Vec::with_capacity(end - start);
    for (index, glyph) in glyphs.iter().enumerate().take(end).skip(start) {
        let mut copy = *glyph;
        copy.x = width;
        out.push(copy);
        if index < last_counted {
            width += glyph.advance;
        }
    }
    Line::new(out, width, start + 1, end)
}

/// Measured: a break opportunity is taken only when more of the following word fits on a fresh line
/// than after the opportunity on the current line; otherwise the word is broken where it overflows.
fn prefer_word_break(glyphs: &[Glyph], line_start: usize, break_at: usize, max_width: f64) -> bool {
    let mut current = 0.0;
    for glyph in &glyphs[line_start..=break_at] {
        current += glyph.advance;
    }
    let mut fresh = 0.0;
    let mut fit_current = 0;
    let mut fit_fresh = 0;
    let mut index = break_at + 1;
    while index < glyphs.len() && !is_space(glyphs[index].code) {
        let advance = glyphs[index].advance;
        current += advance;
        fresh += advance;
        if fresh > max_width + 1e-6 {
            break;
        }
        fit_fresh += 1;
        if current <= max_width + 1e-6 {
            fit_current += 1;
        }
        index += 1;
    }
    fit_fresh > fit_current
}

/// Greedy word wrap of one shaped paragraph into lines no wider than `max_width` (physical px).
/// Whitespace never breaks a line by itself; lines ended at a break opportunity drop their trailing
/// whitespace, lines broken inside a word keep it (measured: "a " in a box narrower than "b").
/// Pushes (line, has source) pairs.
fn wrap_paragraph(glyphs: &[Glyph], max_width: Option<f64>, out: &mut Vec<(Line, bool)>) {
    let count = glyphs.len();
    if count == 0 {
        out.push((Line::new(Vec::new(), 0.0, 1, 0), false));
        return;
    }
    let Some(max_width) = max_width else {
        out.push((make_line(glyphs, 0, count, false), true));
        return;
    };

    // Measured: when the first glyph of a paragraph is wider than the box, the paragraph starts with
    // an empty line ("x" in a narrow box is two lines tall; "x\nx" four).
    if !is_space(glyphs[0].code) && glyphs[0].advance > max_width + 1e-6 {
        let mut line = Line::new(Vec::new(), 0.0, 1, 0);
        line.split = true;
        out.push((line, false));
    }

    let mut line_start = 0;
    while line_start < count {
        let mut width = 0.0;
        let mut last_break: Option<usize> = None;
        // 0-based index of the last glyph that fit, None = line_start - 1.
        let mut last_fit: Option<usize> = None;
        let mut index = line_start;
        while index < count {
            let glyph = &glyphs[index];
            let code = glyph.code;
            if is_space(code) {
                last_break = Some(index);
                width += glyph.advance;
                last_fit = Some(index);
                index += 1;
                continue;
            }
            if width + glyph.advance > max_width + 1e-6 && index > line_start {
                break;
            }
            width += glyph.advance;
            last_fit = Some(index);
            if is_cjk(code) {
                last_break = Some(index);
            }
            index += 1;
        }

        if index >= count {
            out.push((make_line(glyphs, line_start, count, false), true));
            break;
        }

        match last_break {
            Some(break_at) if break_at >= line_start && prefer_word_break(glyphs, line_start, break_at, max_width) => {
                out.push((make_line(glyphs, line_start, break_at + 1, true), true));
                line_start = break_at + 1;
            }
            _ => {
                let break_at = last_fit.map_or(line_start, |fit| fit.max(line_start));
                let mut line = make_line(glyphs, line_start, break_at + 1, false);
                line.split = true;
                out.push((line, true));
                line_start = break_at + 1;
            }
        }
    }
}

/// Width of the last run of whitespace in a line (0 when the line has none).
fn last_space_run(line: &Line) -> f64 {
    let glyphs = &line.glyphs;
    let mut index = glyphs.len();
    while index >= 1 && !is_space(glyphs[index - 1].code) {
        index -= 1;
    }
    let mut run = 0.0;
    while index >= 1 && is_space(glyphs[index - 1].code) {
        run += glyphs[index - 1].advance;
        index -= 1;
    }
    run
}

/// Measured: an unwrapped line wider than its box reports its width without the last whitespace run
/// when that makes it fit (the glyphs are still drawn with the spaces); otherwise its full width.
fn bounds_width(line: &Line, box_width: Option<f64>) -> f64 {
    let Some(box_width) = box_width else {
        return line.width;
    };
    if line.width <= box_width + 1e-6 {
        return line.width;
    }
    let trimmed = line.width - last_space_run(line);
    if trimmed <= box_width + 1e-6 { trimmed } else { line.width }
}

// TextTruncate

fn concat(prefix: &[Glyph], suffix: Option<&[Glyph]>, trim: bool) -> Line {
    let mut glyphs = Vec::with_capacity(prefix.len() + suffix.map_or(0, |suffix| suffix.len()));
    glyphs.extend_from_slice(prefix);
    if let Some(suffix) = suffix {
        glyphs.extend_from_slice(suffix);
    }
    let count = glyphs.len();
    make_line(&glyphs, 0, count, trim)
}

fn trimmed_count(glyphs: &[Glyph]) -> usize {
    let mut count = glyphs.len();
    while count >= 1 && is_space(glyphs[count - 1].code) {
        count -= 1;
    }
    count
}

/// Longest character prefix of glyphs[..limit] that fits `box_width` beside the ellipsis
/// (`ellipsis_width(count)`: the one after `count` glyphs). Measured (AtEnd): a prefix only survives when
/// adding its next character would make it wider than the ellipsis ("aaaa" in a narrow box shows "..."
/// then "aa...", never "a...").
fn character_prefix(glyphs: &[Glyph], limit: usize, box_width: f64, ellipsis_width: &dyn Fn(usize) -> f64) -> usize {
    let limit = limit.min(glyphs.len());
    let mut widths = Vec::with_capacity(limit);
    let mut width = 0.0;
    for glyph in &glyphs[..limit] {
        width += glyph.advance;
        widths.push(width);
    }
    for count in (1..=limit).rev() {
        let ellipsis = ellipsis_width(count);
        let next = if count < limit { widths[count] } else { f64::INFINITY };
        if widths[count - 1] <= box_width - ellipsis + 1e-6 && next > ellipsis + 1e-6 {
            return count;
        }
    }
    0
}

/// SplitWord fill: keeps glyphs[..forced], then more characters while they fit beside the ellipsis
/// (whitespace directly after kept glyphs always hangs on), and appends the ellipsis only if the kept
/// glyphs plus the ellipsis fit the box.
fn split_word_fill(glyphs: &[Glyph], forced: usize, box_width: f64, ellipsis: &[Glyph], ellipsis_width: f64) -> Line {
    let available = box_width - ellipsis_width + 1e-6;
    let mut width = 0.0;
    let mut count = forced;
    for glyph in &glyphs[..forced] {
        width += glyph.advance;
    }
    if width <= available {
        for (index, glyph) in glyphs.iter().enumerate().skip(forced) {
            if is_space(glyph.code) {
                if count < index {
                    break;
                }
                width += glyph.advance;
                count = index + 1;
            } else if width + glyph.advance <= available {
                width += glyph.advance;
                count = index + 1;
            } else {
                break;
            }
        }
    }
    if width + ellipsis_width <= box_width + 1e-6 {
        concat(&glyphs[..count], Some(ellipsis), false)
    } else {
        concat(&glyphs[..count], None, false)
    }
}

/// Measured (layout-probe-autoedge): TextBounds of an unwrapped rich line wider than its box span its
/// longest character prefix that fits, with either TextTruncate mode, and no ellipsis counts in them:
/// "Max Players" (119 px) reads 109 ("Max Player") in 117 px and 46 ("Max ", its space counted) in
/// 58.5 px. What is drawn is another line (`truncate_rich_drawn`).
fn truncate_rich(line: &Line, box_width: f64) -> Line {
    let mut count = 0;
    let mut width = 0.0;
    for (index, glyph) in line.glyphs.iter().enumerate() {
        if width + glyph.advance > box_width + 1e-6 {
            break;
        }
        width += glyph.advance;
        count = index + 1;
    }
    concat(&line.glyphs[..count], None, false)
}

/// The line Studio draws for an unwrapped rich line wider than its box (fixtures/text-probe-richtrunc.luau,
/// Studio 0.747 at 1.5x: 28 box widths by two modes, three alignments and one or two runs, 12 box widths
/// between whole and half pixels, five styled texts). The box counts up to its next whole pixel ("Rel…",
/// 50 px, is drawn from 49.2 px on where plain text needs 50). AtEnd then cuts like plain text. SplitWord
/// keeps the longest character prefix that fits beside the ellipsis, whitespace like any character, and
/// draws nothing when not even the ellipsis fits. The ellipsis is the label face's, at the size and in the
/// run of the last kept glyph (of the line's first glyph when none is kept), but as wide as the ellipsis
/// of that glyph's own face: after a bold run "goin…" would fit 227 px by the label's ellipsis (21 px) and
/// Studio draws "goi…". `ellipsis_at(count)` is the one after `count` kept glyphs. A bold or italic run's
/// ellipsis is the label face's glyph emboldened or slanted, and with no glyph kept it has the label's
/// colour (export.rs `export_runs`).
fn truncate_rich_drawn(line: &Line, box_width: f64, mode: Truncate, ellipsis_at: &dyn Fn(usize) -> Arc<(Vec<Glyph>, f64)>) -> Line {
    let glyphs = &line.glyphs;
    let box_width = (box_width - 1e-6).ceil();
    let width_at = |count: usize| ellipsis_at(count).1;
    if box_width - width_at(0) < -1e-6 {
        return concat(&[], None, false);
    }
    let keep = if mode == Truncate::SplitWord {
        let mut width = 0.0;
        let mut keep = 0;
        for (index, glyph) in glyphs.iter().enumerate() {
            width += glyph.advance;
            if width + width_at(index + 1) > box_width + 1e-6 {
                break;
            }
            keep = index + 1;
        }
        keep
    } else {
        at_end_keep(glyphs, box_width, &width_at)
    };
    let ellipsis = ellipsis_at(keep);
    let (size, run) = match glyphs.get(keep.max(1) - 1) {
        Some(glyph) => (glyph.size, glyph.run),
        None => (0.0, 0),
    };
    let tail: Vec<Glyph> = ellipsis.0.iter().map(|glyph| Glyph { run, size, ..*glyph }).collect();
    concat(&glyphs[..keep], Some(&tail), mode != Truncate::SplitWord)
}

/// Measured (unwrapped, line wider than the box): AtEnd keeps the longest prefix of whole words
/// (characters of the first word when none fits) beside the ellipsis and always appends it;
/// SplitWord keeps the longest character prefix that fits beside the ellipsis plus the whitespace
/// that follows it, and appends the ellipsis only if that still fits. TextBounds is the displayed
/// width.
fn truncate_unwrapped(line: &Line, box_width: f64, ellipsis: &[Glyph], ellipsis_width: f64, mode: Truncate) -> Line {
    let glyphs = &line.glyphs;
    if mode == Truncate::SplitWord {
        return split_word_fill(glyphs, 0, box_width, ellipsis, ellipsis_width);
    }
    if box_width - ellipsis_width < -1e-6 {
        return concat(&[], None, false);
    }
    let keep = at_end_keep(glyphs, box_width, &|_| ellipsis_width);
    concat(&glyphs[..keep], Some(ellipsis), true)
}

/// How many glyphs AtEnd keeps of an unwrapped line wider than `box_width`; `ellipsis_width(count)` is the
/// width of the ellipsis after `count` kept glyphs (one width for plain text).
fn at_end_keep(glyphs: &[Glyph], box_width: f64, ellipsis_width: &dyn Fn(usize) -> f64) -> usize {
    // 1-based counts like the Luau.
    let mut width = 0.0;
    let mut word_end = 0;
    let mut first_word_end = 0;
    for (index0, glyph) in glyphs.iter().enumerate() {
        let index = index0 + 1;
        width += glyph.advance;
        if is_space(glyph.code) {
            continue;
        }
        let available = box_width - ellipsis_width(index);
        let next = glyphs.get(index0 + 1);
        if next.is_none_or(|next| is_space(next.code)) {
            if first_word_end == 0 {
                first_word_end = index;
            }
            if width <= available + 1e-6 {
                word_end = index;
            }
        }
        if width > available + 1e-6 && first_word_end > 0 {
            break;
        }
    }

    // Measured (layout-probe-truncate4, -truncate5: 1527/1536 values): the cut goes inside the next
    // word when the part of that word that fits is wider than everything kept before it (its space
    // included): "A somewhat longer" at 54 px is "A so…" ("so" 19 > "A " 15) but "A…" at 51 px
    // ("s" 8); "W some words" stays "W…" until "W som…" ("som" 35 > "W " 20).
    let mut keep = word_end;
    if keep > 0 {
        let cut = character_prefix(glyphs, glyphs.len(), box_width, ellipsis_width);
        if cut > keep {
            let mut boundary = cut;
            while boundary > 0 && !is_space(glyphs[boundary - 1].code) {
                boundary -= 1;
            }
            if boundary >= keep {
                let mut before = 0.0;
                let mut part = 0.0;
                for glyph in &glyphs[..boundary] {
                    before += glyph.advance;
                }
                for glyph in &glyphs[boundary..cut] {
                    part += glyph.advance;
                }
                if part > before + 1e-6 {
                    keep = cut;
                }
            }
        }
    }
    if keep == 0 {
        keep = character_prefix(glyphs, first_word_end.max(1), box_width, ellipsis_width);
    }
    keep
}

/// Measured (wrapped, lines cut by the box height; only the last visible line changes): the ellipsis
/// is appended when it fits after the line; otherwise AtEnd drops characters of a line that ends
/// inside a word until it fits, and leaves a line ending at a word boundary as it is. SplitWord
/// counts the line's trailing whitespace and never drops characters. None: the line stays as it is.
fn truncate_wrapped(line: &Line, source: Option<&[Glyph]>, max_width: f64, ellipsis: &[Glyph], ellipsis_width: f64, mode: Truncate) -> Option<Line> {
    // Measured (layout-probe-truncate, "Supercalifragilistic" wrapped): on a line broken inside a word
    // SplitWord cuts like AtEnd.
    if mode == Truncate::SplitWord && !line.split {
        // Measured: SplitWord keeps the whole last line with its trailing whitespace, continues with
        // characters of the rest of the paragraph while they fit beside the ellipsis ("of the Nor..."
        // where word wrap gave "of the"), and shows no ellipsis when not even that fits.
        let mut glyphs = line.glyphs.clone();
        if let Some(source) = source {
            if line.last < source.len() {
                glyphs.extend_from_slice(&source[line.last..]);
            }
        }
        return Some(split_word_fill(&glyphs, line.glyphs.len(), max_width, ellipsis, ellipsis_width));
    }

    let count = trimmed_count(&line.glyphs);
    let mut width = 0.0;
    for glyph in &line.glyphs[..count] {
        width += glyph.advance;
    }
    if width + ellipsis_width <= max_width + 1e-6 {
        return Some(concat(&line.glyphs[..count], Some(ellipsis), true));
    }
    if !line.split || max_width < ellipsis_width - 1e-6 {
        return None;
    }
    let keep = character_prefix(&line.glyphs, count, max_width, &|_| ellipsis_width);
    Some(concat(&line.glyphs[..keep], Some(ellipsis), true))
}

// Cache keys

pub(crate) fn opt_bits(value: Option<f64>) -> u64 {
    match value {
        Some(value) => value.to_bits(),
        None => 0x7FF4_0000_0000_0001, // a NaN payload no computation produces
    }
}

impl TextEngine {
    /// Plain shaped paragraphs per (font, size, text) (`ShapedParagraphs`), so a new box only wraps
    /// again. The glyph arrays are shared: never mutated.
    fn shaped_paragraphs(&mut self, text: &[u8], family: &str, weight: f64, style: &str, face: FaceId, size: f64) -> Arc<Vec<Vec<Glyph>>> {
        let mut hasher = KeyHasher::new();
        hasher.bytes(family.as_bytes());
        hasher.u64(weight.to_bits());
        hasher.bytes(style.as_bytes());
        hasher.u64(size.to_bits());
        hasher.bytes(text);
        let hash = hasher.finish();
        if let Some(bucket) = self.shape_cache.get(&hash) {
            for entry in bucket {
                if &*entry.family == family && entry.weight == weight.to_bits() && &*entry.style == style && entry.size == size.to_bits() && &*entry.text == text {
                    cache_counts::hit(Cache::Shapes);
                    return entry.value.clone();
                }
            }
        }
        cache_counts::miss(Cache::Shapes);

        let mut result: Vec<Vec<Glyph>> = Vec::new();
        let mut points = std::mem::take(&mut self.scratch_codepoints);
        points.clear();
        codepoints_into(text, &mut points);
        let mut paragraph: Vec<Codepoint> = Vec::new();
        for &entry in &points {
            if entry.0 == 10 {
                let mut shaped = Vec::new();
                shape_paragraph(&mut self.fonts, &paragraph, face, size, None, &mut shaped);
                result.push(shaped);
                paragraph.clear();
            } else if entry.0 != 13 {
                paragraph.push(entry);
            }
        }
        let mut shaped = Vec::new();
        shape_paragraph(&mut self.fonts, &paragraph, face, size, None, &mut shaped);
        result.push(shaped);
        round_pen(&mut result, self.subpixel);
        self.scratch_codepoints = points;

        let value = Arc::new(result);
        self.shape_count += 1;
        if self.shape_count > 5000 {
            cache_counts::evicted(Cache::Shapes, self.shape_cache.values().map(Vec::len).sum());
            self.shape_cache.clear();
            self.shape_count = 0;
        }
        self.shape_cache.entry(hash).or_default().push(super::ShapeEntry {
            family: family.into(),
            weight: weight.to_bits(),
            style: style.into(),
            size: size.to_bits(),
            text: text.into(),
            value: value.clone(),
        });
        value
    }

    /// `TextLayout.Ellipsis`: the truncation glyphs ("…", else "...") and their width. They start a pen of
    /// their own after the kept text's whole-pixel width: Studio 0.746 reports round(kept pen) +
    /// round(ellipsis advance) (159/159 samples across three faces and TextSize 8-60; earlier versions
    /// used FreeType's hinted advance).
    pub fn ellipsis(&mut self, face: FaceId, size: f64) -> Arc<(Vec<Glyph>, f64)> {
        let key = (face, size.to_bits());
        if let Some(found) = self.ellipsis_cache.get(&key) {
            return found.clone();
        }
        let font = self.fonts.face(face).font.clone();
        let text: &[u8] = if font.has_glyph(0x2026) { "\u{2026}".as_bytes() } else { b"..." };
        let points = super::utf8::codepoints(text);
        let mut glyphs = Vec::new();
        shape_paragraph(&mut self.fonts, &points, face, size, None, &mut glyphs);
        if self.subpixel {
            round_pen(std::slice::from_mut(&mut glyphs), true);
        } else {
            // Whole-pixel text: the ellipsis advances by FreeType's hinted advance, not its rounded one
            // (SourceSansPro at 30 px: 24 px where the advance rounds to 23; "a…" needs 36 px + 1)
            let (x_scale, _) = scales(&font, size);
            let mut pen = 0.0;
            for glyph in glyphs.iter_mut() {
                glyph.advance = match self.fonts.hinted_advance(glyph.face, size, glyph.glyph) {
                    Some(hinted) => hinted,
                    None => (idiv((font.advance(glyph.glyph) as f64).abs() * x_scale + 32768.0, 65536.0) / 64.0).ceil(),
                };
                glyph.pen = pen;
                pen += glyph.advance;
                glyph.end = pen;
            }
        }
        let width: f64 = glyphs.iter().map(|glyph| glyph.advance).sum();
        // The ellipsis is a pen of its own: its exact pens sit below zero, under every pen of the text it
        // follows, so the export starts a new stretch at it (export.rs text_lines)
        for glyph in &mut glyphs {
            glyph.pen -= width + 1.0;
            glyph.end -= width + 1.0;
        }
        let value = Arc::new((glyphs, width));
        self.ellipsis_cache.insert(key, value.clone());
        value
    }

    /// `TextLayout.Layout`: lays out plain text (cached per inputs; equal inputs give the same Arc).
    pub fn layout_plain(&mut self, params: &LayoutParams) -> Arc<TextLayout> {
        self.layout_core(params, None, None)
    }

    pub(crate) fn layout_core(&mut self, params: &LayoutParams, prepared: Option<&Prepared>, runs: Option<&Arc<Vec<super::richtext::Run>>>) -> Arc<TextLayout> {
        let (_, mut size) = size_for(params.text_size, params.pixel_scale, Some(params.family), params.scale, params.effective_size);
        if let Some(pixel_size) = params.pixel_size {
            size = lmax(pixel_size.floor(), 1.0);
        }
        let max_width = if params.wrapped { params.max_width.map(|width| lmax(width, 0.0)) } else { None };
        let max_height = if params.wrapped { params.max_height.map(|height| lmax(height, 0.0)) } else { None };
        let box_width = match params.max_width {
            Some(width) if !params.wrapped && width < f64::INFINITY => Some(lmax(width, 0.0)),
            _ => None,
        };
        let truncate = params.truncate;
        let line_factor = params.line_height.unwrap_or(1.0);
        let cache_key = prepared.map_or("", |prepared| prepared.cache_key);
        let empty_height = prepared.and_then(|prepared| prepared.empty_height);

        let mut hasher = KeyHasher::new();
        hasher.bytes(params.family.as_bytes());
        hasher.u64(params.weight.to_bits());
        hasher.bytes(params.style.as_bytes());
        hasher.u64(size.to_bits());
        hasher.u64(opt_bits(max_width));
        hasher.u64(opt_bits(max_height));
        hasher.u64(opt_bits(box_width));
        hasher.u64(line_factor.to_bits());
        hasher.u64(truncate as u64);
        hasher.u64(opt_bits(empty_height));
        hasher.bytes(cache_key.as_bytes());
        hasher.bytes(params.text);
        let key = super::MeasureKey {
            family: params.family,
            weight: params.weight.to_bits(),
            style: params.style,
            size: size.to_bits(),
            max_width: opt_bits(max_width),
            max_height: opt_bits(max_height),
            box_width: opt_bits(box_width),
            line_factor: line_factor.to_bits(),
            truncate,
            empty_height: opt_bits(empty_height),
            cache_key,
            text: params.text,
        };
        let hash = hasher.finish();
        if let Some(found) = self.measure_lookup(hash, &key) {
            cache_counts::hit(Cache::Texts);
            return found;
        }
        cache_counts::miss(Cache::Texts);

        let face = self.fonts.get_face(params.family, params.weight, params.style);
        let plain_paragraphs;
        let (paragraphs, paragraph_sizes): (&[Vec<Glyph>], Option<&[f64]>) = match prepared {
            Some(prepared) => (prepared.paragraphs, Some(prepared.paragraph_sizes)),
            None => match face {
                Some(face) => {
                    plain_paragraphs = self.shaped_paragraphs(params.text, params.family, params.weight, params.style, face, size);
                    (&plain_paragraphs[..], None)
                }
                None => (&[], None),
            },
        };

        let mut lines: Vec<Line> = Vec::new();
        // Paragraph index of each line from the wrap (the Luau Line.Source), None for synthetic lines.
        let mut sources: Vec<Option<usize>> = Vec::new();
        let mut wrapped: Vec<(Line, bool)> = Vec::new();
        for (index, shaped) in paragraphs.iter().enumerate() {
            let default_size = match paragraph_sizes {
                Some(sizes) => sizes.get(index).copied().unwrap_or(size),
                None => size,
            };
            wrapped.clear();
            wrap_paragraph(shaped, max_width, &mut wrapped);
            if let Some(last) = wrapped.last_mut() {
                last.0.paragraph_end = true;
            }
            for (mut line, has_source) in wrapped.drain(..) {
                line.default_size = default_size;
                lines.push(line);
                sources.push(if has_source { Some(index) } else { None });
            }
        }

        // Measured: a line is as tall as its largest glyph size (the size of the break that ends an
        // empty rich line); each line starts LineHeight x the previous line's height below the previous
        // one, so plain text is S + (n - 1) * S * LineHeight tall. Wrapped text keeps only the lines that
        // fit the box height (TextBounds.Y and drawing drop the rest).
        let mut top = 0.0;
        for index in 0..lines.len() {
            let mut height: f64 = 0.0;
            for glyph in &lines[index].glyphs {
                height = lmax(height, glyph.size);
            }
            if lines[index].glyphs.is_empty() {
                height = lines[index].default_size;
            }
            if index > 0 {
                top += lines[index - 1].height * line_factor;
            }
            lines[index].top = top;
            lines[index].height = height;
        }
        // A box a few float32 roundings short of a line still holds it. An automatic height comes back from its
        // text's bounds through float32 local units and the UIScale, and can end up to about 3 float32 ulps
        // under the line it was measured to hold (87 px of text against an 86.9999974 px box at DPI 1.3333 under
        // UIScale 810 / 806); a 1e-6 px slack alone cut that line.
        let mut fit = lines.len();
        if let Some(max_height) = max_height {
            if lines.len() > 1 {
                fit = 1;
                let slack = lmax(1e-6, max_height * 4.0 * f64::from(f32::EPSILON));
                for index in 1..lines.len() {
                    if lines[index].top + lines[index].height <= max_height + slack {
                        fit = index + 1;
                    } else {
                        break;
                    }
                }
            }
        }

        let wrapped_lines = lines.len();

        let ellipsis = match (truncate, face) {
            (Truncate::AtEnd | Truncate::SplitWord, Some(face)) => Some(self.ellipsis(face, size)),
            _ => None,
        };

        // Width is TextBounds.X (per-line Width stays the drawn width used for alignment). The trailing
        // whitespace rule applies to the last line of each paragraph (wrapped lines end trimmed).
        // Measured: without truncation it spans every line, including wrapped lines cut off by the box
        // height; truncated text only measures what is displayed.
        let rich = prepared.is_some();
        let bounds_box = if params.wrapped { max_width } else { box_width };
        // Measured (layout-probe-autorich4): an unwrapped rich line keeps its whitespace in TextBounds
        // however far it overflows its box (the last line of a wrapped rich paragraph drops it like
        // plain text does).
        let trim_box = if rich && !params.wrapped { None } else { bounds_box };
        let mut natural_width: f64 = 0.0;
        let mut broken = false;
        for line in &lines {
            natural_width = lmax(natural_width, line.width);
            if line.split {
                broken = true;
            }
        }
        let mut fits = lines.len() <= fit;
        if let Some(bounds_box) = bounds_box {
            for line in &lines {
                if line.width > bounds_box + 1e-6 {
                    fits = false;
                }
            }
        }
        // Measured (layout-probe-richclip): wrapped rich text keeps (and counts) the lines below the box
        // unless it is truncated, and its truncated TextBounds.X still spans every line.
        let rich_wrapped = rich && params.wrapped;
        let mut all_width: f64 = 0.0;
        for line in &lines {
            all_width = lmax(all_width, if line.paragraph_end { bounds_width(line, trim_box) } else { line.width });
        }
        let keep = |mut replacement: Line, original: &Line| -> Line {
            replacement.top = original.top;
            replacement.height = original.height;
            replacement.default_size = original.default_size;
            replacement.truncated = true;
            replacement
        };
        // TextBounds widths of the truncated lines that are drawn as another line than the one measured
        // (rich lines: `truncate_rich_drawn`; a SplitWord line broken inside a word, below)
        let mut drawn_bounds: Vec<Option<f64>> = Vec::new();
        if let (Some(ellipsis), Some(box_width)) = (&ellipsis, box_width) {
            for index in 0..lines.len() {
                if lines[index].width > box_width + 1e-6 {
                    let replacement = if rich {
                        let bounds = truncate_rich(&lines[index], box_width);
                        drawn_bounds.resize(lines.len(), None);
                        drawn_bounds[index] = Some(bounds.glyphs.iter().map(|glyph| glyph.advance).sum());
                        // One ellipsis per face and size on the line: the label face's glyphs, as wide as
                        // the ellipsis of the glyph's own face (`truncate_rich_drawn`)
                        let mut styles: Vec<(FaceId, u64, Arc<(Vec<Glyph>, f64)>)> = Vec::new();
                        if let Some(face) = face {
                            for glyph in &lines[index].glyphs {
                                let glyph_size = if glyph.size != 0.0 { glyph.size } else { size };
                                if styles.iter().any(|style| style.0 == glyph.face && style.1 == glyph_size.to_bits()) {
                                    continue;
                                }
                                let drawn = self.ellipsis(face, glyph_size);
                                let own = self.ellipsis(glyph.face, glyph_size);
                                let entry = if own.1 > 0.0 && own.1 != drawn.1 && !drawn.0.is_empty() {
                                    let mut glyphs = drawn.0.clone();
                                    let last = glyphs.len() - 1;
                                    glyphs[last].advance += own.1 - drawn.1;
                                    Arc::new((glyphs, own.1))
                                } else {
                                    drawn
                                };
                                styles.push((glyph.face, glyph_size.to_bits(), entry));
                            }
                        }
                        let line = &lines[index];
                        let ellipsis_at = |count: usize| -> Arc<(Vec<Glyph>, f64)> {
                            match line.glyphs.get(count.max(1) - 1) {
                                Some(glyph) => {
                                    let glyph_size = if glyph.size != 0.0 { glyph.size } else { size };
                                    let found = styles.iter().find(|style| style.0 == glyph.face && style.1 == glyph_size.to_bits());
                                    found.map_or_else(|| ellipsis.clone(), |style| style.2.clone())
                                }
                                None => ellipsis.clone(),
                            }
                        };
                        truncate_rich_drawn(line, box_width, truncate, &ellipsis_at)
                    } else {
                        truncate_unwrapped(&lines[index], box_width, &ellipsis.0, ellipsis.1, truncate)
                    };
                    lines[index] = keep(replacement, &lines[index]);
                }
            }
        } else if let (Some(ellipsis), Some(max_width)) = (&ellipsis, max_width) {
            if lines.len() > fit {
                lines.truncate(fit);
                let index = fit - 1;
                let source = sources[index].map(|paragraph| &paragraphs[paragraph][..]);
                match truncate_wrapped(&lines[index], source, max_width, &ellipsis.0, ellipsis.1, truncate) {
                    // Measured (fixtures/layout-truncate.luau SplitWord_w_16, "G" / "ui" in 24 px): of a
                    // SplitWord line broken inside a word of which no character fits beside the ellipsis,
                    // the first character is drawn alone ("u"), while TextBounds are the lone ellipsis'
                    // (23 px; "u" is 14 px wide)
                    Some(replacement) if truncate == Truncate::SplitWord && lines[index].split && replacement.glyphs.iter().all(Glyph::is_ellipsis) => {
                        drawn_bounds.resize(lines.len(), None);
                        drawn_bounds[index] = Some(replacement.glyphs.iter().map(|glyph| glyph.advance).sum());
                        let first = concat(&lines[index].glyphs[..lines[index].glyphs.len().min(1)], None, false);
                        lines[index] = keep(first, &lines[index]);
                    }
                    Some(replacement) => lines[index] = keep(replacement, &lines[index]),
                    None => lines[index].truncated = true,
                }
            }
        }

        let mut width: f64 = 0.0;
        for (index, line) in lines.iter().enumerate() {
            if let Some(Some(bounds)) = drawn_bounds.get(index) {
                width = lmax(width, *bounds);
            } else if line.truncated {
                let mut displayed = 0.0;
                for glyph in &line.glyphs {
                    displayed += glyph.advance;
                }
                width = lmax(width, if truncate == Truncate::SplitWord { displayed } else { line.width });
            } else {
                width = lmax(width, if line.paragraph_end { bounds_width(line, trim_box) } else { line.width });
            }
        }
        if rich_wrapped {
            width = all_width;
        }
        if !rich_wrapped || truncate != Truncate::None {
            lines.truncate(fit);
        }
        let result_lines = lines.len();
        let mut height = match lines.last() {
            Some(last) => last.top + last.height,
            None => size,
        };
        if let Some(empty_height) = empty_height {
            height = empty_height;
        }
        let result = Arc::new(TextLayout {
            lines,
            fits,
            natural_width,
            broken,
            width,
            height,
            line_height: size * line_factor,
            size,
            face,
            runs: runs.cloned(),
            rich: runs.is_some(),
            dropped: wrapped_lines - result_lines,
        });
        self.measure_insert(hash, &key, result.clone());
        result
    }
}

#[cfg(test)]
mod truncation_tests {
    use super::*;

    /// One glyph per character, each `advance` px wide (spaces too), in run 1; `bold` characters are 2 px
    /// wider and in run 2 with another face.
    fn glyphs(text: &str, advance: f64, bold: std::ops::Range<usize>, faces: [FaceId; 2]) -> Vec<Glyph> {
        let mut pen = 0.0;
        text.chars()
            .enumerate()
            .map(|(index, character)| {
                let heavy = bold.contains(&index);
                let width = if heavy { advance + 2.0 } else { advance };
                let glyph = Glyph {
                    glyph: character as u32,
                    face: faces[usize::from(heavy)],
                    x: pen,
                    x_offset: 0.0,
                    advance: width,
                    cluster: index as u32 + 1,
                    code: character as u32,
                    size: 20.0,
                    run: if heavy { 2 } else { 1 },
                    pen,
                    end: pen + width,
                    offset: 0.0,
                };
                pen += width;
                glyph
            })
            .collect()
    }

    fn ellipsis(width: f64, face: FaceId) -> Arc<(Vec<Glyph>, f64)> {
        let glyph = Glyph { glyph: 9, face, x: 0.0, x_offset: 0.0, advance: width, cluster: 1, code: 0x2026, size: 0.0, run: 0, pen: -width - 1.0, end: -1.0, offset: 0.0 };
        Arc::new((vec![glyph], width))
    }

    fn shown(line: &Line) -> String {
        line.glyphs.iter().map(|glyph| char::from_u32(glyph.code).unwrap()).collect()
    }

    #[test]
    fn rich_lines_are_drawn_with_an_ellipsis_in_a_box_counted_to_its_next_pixel() {
        let faces: [FaceId; 2] = [1, 2];
        let line = make_line(&glyphs("Relax. This is", 10.0, 0..0, faces), 0, 14, false);
        let dots = ellipsis(20.0, faces[0]);
        let at = |_: usize| dots.clone();
        // "Rel" and the ellipsis are 50 px: drawn from 49.2 px on (plain text needs 50)
        assert_eq!(shown(&truncate_rich_drawn(&line, 49.2, Truncate::AtEnd, &at)), "Rel\u{2026}");
        assert_eq!(shown(&truncate_rich_drawn(&line, 49.0, Truncate::AtEnd, &at)), "Re\u{2026}");
        assert_eq!(shown(&truncate_unwrapped(&line, 49.2, &dots.0, dots.1, Truncate::AtEnd)), "Re\u{2026}");
        // AtEnd keeps whole words once one fits; SplitWord cuts anywhere and keeps a space like a character
        assert_eq!(shown(&truncate_rich_drawn(&line, 100.0, Truncate::AtEnd, &at)), "Relax.\u{2026}");
        assert_eq!(shown(&truncate_rich_drawn(&line, 100.0, Truncate::SplitWord, &at)), "Relax. T\u{2026}");
        assert_eq!(shown(&truncate_rich_drawn(&line, 90.0, Truncate::SplitWord, &at)), "Relax. \u{2026}");
        // Not even the ellipsis fits: nothing is drawn
        assert!(truncate_rich_drawn(&line, 19.0, Truncate::AtEnd, &at).glyphs.is_empty());
        assert!(truncate_rich_drawn(&line, 19.0, Truncate::SplitWord, &at).glyphs.is_empty());
        // The ellipsis is in the run and at the size of the last kept glyph, and stays an ellipsis
        let cut = truncate_rich_drawn(&line, 60.0, Truncate::SplitWord, &at);
        let last = cut.glyphs.last().unwrap();
        assert!(last.is_ellipsis() && last.run == 1 && last.size == 20.0);
        // TextBounds stay the longest prefix that fits, without an ellipsis
        assert_eq!(shown(&truncate_rich(&line, 49.2)), "Rela");
    }

    #[test]
    fn a_rich_ellipsis_is_as_wide_as_the_one_of_the_face_it_follows() {
        let faces: [FaceId; 2] = [1, 2];
        // "ab" regular (10 px each), "cdefgh" bold (12 px each); the ellipsis after a bold glyph counts
        // 24 px, after a regular one 20
        let line = make_line(&glyphs("abcdefgh", 10.0, 2..8, faces), 0, 8, false);
        let (regular, bold) = (ellipsis(20.0, faces[0]), ellipsis(24.0, faces[0]));
        let at = |count: usize| if count > 2 { bold.clone() } else { regular.clone() };
        // 20 + 3 x 12 + 24 = 80 fits 80 px; with a 20 px ellipsis a fourth bold glyph would fit 88
        assert_eq!(shown(&truncate_rich_drawn(&line, 80.0, Truncate::SplitWord, &at)), "abcde\u{2026}");
        assert_eq!(shown(&truncate_rich_drawn(&line, 88.0, Truncate::SplitWord, &at)), "abcde\u{2026}");
        assert_eq!(shown(&truncate_rich_drawn(&line, 92.0, Truncate::SplitWord, &at)), "abcdef\u{2026}");
    }
}
