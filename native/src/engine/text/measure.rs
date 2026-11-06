//! Port of `LayoutText` / `RichScaledSize` / `LargestFitting` in runtime/layout/init.luau: lays out a
//! text object's text for a content box (TextWrapped, TextScaled, UITextSizeConstraint, TextTruncate,
//! automatic width) and converts TextBounds to logical and local px. Pure: the layout port reads the
//! instance's properties into a [`TextObject`] and calls [`TextEngine::layout_text`].

use std::sync::Arc;

use super::TextEngine;
use super::layout::{LayoutParams, TextLayout, Truncate, quad_size_for, rich_scaled_pixels, rich_scaled_unfloored, size_multiplier};
use super::num::{clamp, f32r, lmax, next_f32};

/// A UITextSizeConstraint child's raw MinTextSize / MaxTextSize.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SizeConstraint {
    pub min_text_size: f64,
    pub max_text_size: f64,
}

/// The text properties of a TextLabel / TextButton / TextBox the layout reads.
#[derive(Clone, Copy, Debug)]
pub struct TextObject<'a> {
    /// DisplayText: Text, or PlaceholderText for an empty TextBox.
    pub text: &'a [u8],
    pub rich: bool,
    /// FontFace parts: family URI, weight (100-900), style ("normal" / "italic").
    pub family: &'a str,
    pub weight: f64,
    pub style: &'a str,
    pub wrapped: bool,
    pub scaled: bool,
    /// `TextSize` (14 when unset).
    pub text_size: f64,
    /// `LineHeight` (1 when unset).
    pub line_height: f64,
    pub truncate: Truncate,
    pub constraint: Option<SizeConstraint>,
}

/// `LayoutText`'s result.
#[derive(Clone, Debug)]
pub struct TextMeasure {
    pub layout: Arc<TextLayout>,
    /// The TextSize actually used (TextScaled / constraints), unscaled.
    pub text_size: f64,
    /// The UIScale'd size the text is sized by, in logical px: floor(TextSize x UIScale) (size_for's E),
    /// TextScaled's or a UITextSizeConstraint's pick; rich TextScaled: used_size x UIScale. The reference
    /// of a ScaledSize text UIStroke (export.rs).
    pub effective_size: f64,
    /// DPI x UIScale.
    pub pixel_scale: f64,
    /// TextBounds in absolute (UIScale-multiplied) logical px.
    pub absolute_x: f64,
    pub absolute_y: f64,
    /// Local (unscaled) bounds for automatic sizes.
    pub bounds_x: f64,
    pub bounds_y: f64,
    pub weight: f64,
    pub style: String,
    /// Quad scale for the renderer (unfloored over floored pixel size), None when 1.
    pub quad_scale: Option<f64>,
}

/// Largest candidate in [low, high] that fits (low when none does), for fitting sizes that form a
/// prefix of the range. A range without a finite end (high inf or NaN: a box or UIScale of 0 / NaN
/// divided through) is not searched: low.
fn largest_fitting(low: f64, high: f64, mut fits: impl FnMut(f64) -> bool) -> f64 {
    if high <= low || fits(high) {
        return lmax(high, low);
    }
    let mut good = low - 1.0;
    let mut bad = high;
    while bad - good > 1.0 && bad - good < f64::INFINITY {
        let middle = ((good + bad) / 2.0).floor();
        if fits(middle) {
            good = middle;
        } else {
            bad = middle;
        }
    }
    lmax(good, low)
}

struct Box {
    rich: bool,
    box_width: f64,
    strict_width: f64,
    logical_height: f64,
    inverse: f64,
    dpi: f64,
}

impl Box {
    fn logical(&self, px: f64) -> f64 {
        if self.rich { f32r(px / self.dpi) } else { f32r(px * self.inverse) }
    }

    fn fits(&self, full: &TextLayout) -> bool {
        full.natural_width <= (if self.rich { self.box_width } else { self.strict_width }) + 1e-6
            && self.logical(full.height) <= self.logical_height
            && (self.rich || !full.broken)
    }
}

impl TextEngine {
    /// `LayoutText`: lays out a text object's text for a content box of (width, height) logical px
    /// at the accumulated UIScale `scale` and device pixel scale `dpi`. `auto_width`: measuring an
    /// automatic width (no wrapping, no truncation against the box).
    pub fn layout_text(&mut self, object: &TextObject, width: f64, height: f64, scale: f64, dpi: f64, auto_width: bool) -> TextMeasure {
        let rich = object.rich;
        let wrapped = object.wrapped && !auto_width;
        let mut box_width = width * scale * dpi;
        let box_height = height * scale * dpi;

        // Measured (layout-probe-autorich2/3/4): a line of W whole px fits its box when W <= float32(box
        // x DPI) for plain text (340/340 boxes within a few ulps of their text). Rich text only wraps a
        // line that is at least 1 px wider than that (W px fits a box of W - 0.99 px, not W - 1 px, so
        // W <= ceil(box px)); TextScaled's size search keeps the strict test for both (a 26 px rich line
        // does not fit a 25.999998 px box, layout-probe-richscaled3).
        let inverse = f32r(1.0 / dpi);
        let logical_height = f32r(height * scale);
        let mut strict_width = box_width;
        if box_width < f64::INFINITY {
            let pixels = f32r(f32r(width * scale) * dpi);
            strict_width = pixels.floor();
            box_width = if rich { pixels.ceil() } else { strict_width };
        }
        // Measuring an automatic width: the text keeps its natural width (no truncation against the
        // box being measured; layout-probe-autotrunc).
        if auto_width {
            box_width = f64::INFINITY;
            strict_width = f64::INFINITY;
        }
        let line_height = object.line_height;
        let text_size = object.text_size;
        let truncate = object.truncate;

        // Measured (layout-probe-scaledcap): a UITextSizeConstraint bounds the UIScale'd text size
        // (floor(TextSize x UIScale) is clamped to [floor(MinTextSize), floor(MaxTextSize)]), so capped
        // text is as large under any UIScale.
        let constraint = object.constraint;
        let min_effective = constraint.map(|constraint| lmax(constraint.min_text_size.floor(), 1.0));
        let max_effective = constraint.map(|constraint| lmax(constraint.max_text_size.floor(), 1.0));

        let fit_box = Box { rich, box_width, strict_width, logical_height, inverse, dpi };
        let measure = |engine: &mut TextEngine, size: f64, capped: bool, pixel_size: Option<f64>, effective_size: Option<f64>, unwrapped: bool| {
            let params = LayoutParams {
                text: object.text,
                family: object.family,
                weight: object.weight,
                style: object.style,
                text_size: size,
                pixel_scale: dpi,
                wrapped: wrapped && !unwrapped,
                max_width: Some(if unwrapped { f64::INFINITY } else { box_width }),
                max_height: if capped { Some(box_height) } else { None },
                line_height: Some(line_height),
                truncate: if capped { truncate } else { Truncate::None },
                scale: Some(scale),
                pixel_size,
                effective_size,
            };
            if rich { engine.layout_rich(&params) } else { engine.layout_plain(&params) }
        };

        let used_size;
        let mut pixel_size: Option<f64> = None;
        let mut effective_size: Option<f64> = None;
        let mut rich_quad_size: Option<f64> = None;
        if object.scaled {
            // Measured: TextScaled picks the largest size whose layout fits the box without breaking a
            // word (every line within the width, all lines within the height). Plain text steps through
            // whole (UIScale'd) TextSizes; smaller text fits whenever larger text does, so the largest
            // fitting size is found by bisection.
            let low_effective = min_effective.unwrap_or_else(|| lmax((f32r(scale) + 1e-9).floor(), 1.0));
            let mut high_effective = max_effective.unwrap_or_else(|| lmax((f32r(100.0 * scale) + 1e-9).floor(), 1.0));
            high_effective = lmax(high_effective, low_effective);
            if rich {
                let low_pixels = lmax((low_effective * dpi + 1e-9).floor(), 1.0);
                let high_pixels = match max_effective {
                    Some(max_effective) => lmax((max_effective * dpi + 1e-9).floor(), 1.0),
                    None => lmax((100.0 * scale * dpi).floor(), 1.0),
                };
                // Every size Studio tries is a whole unscaled pixel size N drawn through UIScale (and the
                // UITextSizeConstraint), see rich_scaled_size.
                let high_clamp = lmax(high_pixels, low_pixels);
                let has_constraint = constraint.is_some();
                let family = object.family;
                let drawn = move |unscaled: f64| -> f64 {
                    let pixels = rich_scaled_pixels(unscaled, dpi, Some(family), Some(scale));
                    if has_constraint { clamp(pixels, low_pixels, high_clamp) } else { lmax(pixels, 1.0) }
                };
                // Rich text does not pick the largest fitting size but whatever Studio's own search lands
                // on (rich_scaled_size); when that search gives up, the largest fitting size among those
                // Studio can draw (whole unscaled px N).
                let unscaled = match self.rich_scaled_size(text_size, width, height, scale, dpi, &drawn, &fit_box, &measure) {
                    Some(unscaled) => unscaled,
                    None => {
                        let high_unscaled = if has_constraint {
                            lmax((high_clamp / f32r(scale)).ceil() + 2.0, 1.0)
                        } else {
                            lmax((100.0 * dpi).floor(), (high_clamp / f32r(scale)).ceil() + 2.0)
                        };
                        let best = largest_fitting(1.0, high_unscaled, |candidate| {
                            let full = measure(self, text_size, false, Some(drawn(candidate)), None, false);
                            fit_box.fits(&full)
                        });
                        best
                    }
                };
                let picked = drawn(unscaled);
                // Measured (2026-09-27, UI kit button labels at UIScale 0.885): the whole size N is drawn
                // from floor(unfloored) px glyphs, each quad scaled by unfloored / floor like a fractional
                // TextSize. A size clamped by a UITextSizeConstraint takes the bound's unfloored size, like
                // plain text: floor(MaxTextSize) (or floor(MinTextSize)) x multiplier x DPI (round 8,
                // fixtures/text-probe-richcap.luau: a list row's "VIEW" button, MaxTextSize 17 ->
                // 25.5 px drawn from 25 px glyphs, the "W" stroke band 27 -> 28 px wide).
                let raw = lmax(rich_scaled_pixels(unscaled, dpi, Some(family), Some(scale)), 1.0);
                let clamped_to = if has_constraint && raw != picked {
                    if picked == high_clamp { max_effective } else { min_effective }
                } else {
                    None
                };
                let unfloored = match clamped_to {
                    Some(effective) => effective * size_multiplier(Some(family)) * dpi,
                    None => rich_scaled_unfloored(unscaled, dpi, Some(family), Some(scale)),
                };
                if picked > 0.0 && unfloored.floor() == picked && unfloored - picked > 1e-6 {
                    rich_quad_size = Some(unfloored);
                }
                pixel_size = Some(picked);
                used_size = picked / (scale * dpi);
            } else {
                // Measured: the search runs over whole UIScale'd sizes (1.3x reaches 30, which no whole
                // TextSize gives).
                let picked = largest_fitting(low_effective, high_effective, |candidate| {
                    let full = measure(self, text_size, false, None, Some(candidate), false);
                    fit_box.fits(&full)
                });
                effective_size = Some(picked);
                used_size = picked / scale;
            }
        } else if let (Some(min_effective), Some(max_effective)) = (min_effective, max_effective) {
            let effective = lmax((f32r(clamp(text_size.floor(), 1.0, 100.0) * scale) + 1e-9).floor(), 1.0);
            let picked = clamp(effective, min_effective, lmax(max_effective, min_effective));
            effective_size = Some(picked);
            used_size = picked / scale;
        } else {
            used_size = text_size;
        }
        let result = measure(self, used_size, true, pixel_size, effective_size, false);

        // Measured: TextBounds is in absolute (UIScale-multiplied) px; BoundsX/Y stay local for sizing.
        // Plain text converts with the float32 reciprocal of the DPI scale (layout-probe-uiscaletext
        // 396/396 bit-exact); rich text divides (layout-probe-richsize 424/424).
        let (absolute_x, absolute_y) = if rich {
            (f32r(result.width / dpi), f32r(result.height / dpi))
        } else {
            (f32r(result.width * inverse), f32r(result.height * inverse))
        };

        // Local (unscaled) bounds for automatic sizes: float32(bounds / UIScale), except that a whole
        // number of px that would come back below itself (float32(local x UIScale) < bounds) rounds up
        // instead (layout-probe-autorich5: 54/54 such labels).
        let local_bounds = |absolute: f64| -> f64 {
            if scale == 1.0 {
                return absolute;
            }
            let mut value = f32r(absolute / scale);
            if absolute == absolute.floor() && f32r(value * scale) < absolute {
                value = next_f32(value);
            }
            value
        };

        // For the renderer: glyphs are rasterized at the floored pixel size but every glyph
        // quad is drawn scaled by the unfloored size over it, about the glyph's pen point
        // (TextLayout.QuadSizeFor; fixtures/text-probe-fracsize.luau). Rich TextScaled: the unfloored size
        // of the picked whole unscaled size (above).
        let mut quad_scale = None;
        if let Some(unfloored) = rich_quad_size {
            if result.size > 0.0 {
                quad_scale = Some(unfloored / result.size);
            }
        } else if pixel_size.is_none() && result.size > 0.0 {
            let unfloored = quad_size_for(used_size, dpi, Some(object.family), Some(scale), effective_size);
            if (unfloored - result.size).abs() > 1e-6 {
                quad_scale = Some(unfloored / result.size);
            }
        }

        let effective_size = match effective_size {
            Some(picked) => picked,
            None if pixel_size.is_some() => used_size * scale,
            None => lmax((f32r(clamp(used_size.floor(), 1.0, 100.0) * scale) + 1e-9).floor(), 1.0),
        };
        TextMeasure {
            text_size: used_size,
            effective_size,
            pixel_scale: dpi * scale,
            absolute_x,
            absolute_y,
            bounds_x: local_bounds(absolute_x),
            bounds_y: local_bounds(absolute_y),
            weight: object.weight,
            style: object.style.to_string(),
            quad_scale,
            layout: result,
        }
    }

    /// Rich TextScaled's size search (runtime/layout/init.luau RichScaledSize; round 4,
    /// tools/studio/layout-probe-richscaled2 .. 15, 50 000+ Studio labels). Studio does not take the
    /// largest size that fits: it bisects between the label's own TextSize and an estimate, over
    /// unscaled logical sizes, and whole sizes with no tried size in them are skipped:
    ///  * start S0 = floor(TextSize) (a UITextSizeConstraint does not move it, -richscaled15), laid out
    ///    at floor(S0 x DPI) px;
    ///  * estimate E = S0 x sqrt(box area / area of the start layout's TextBounds), all float32, the
    ///    start laid out in the box as the label lays it out (wrapped when TextWrapped, words that do not
    ///    fit a line broken: the TextBounds Studio reports at that size; round 7, -richscaled16/17);
    ///  * a tried size s is laid out at N = floor(f32(s x DPI) + 0.999) unscaled px, drawn at
    ///    RichScaledPixels(N) and clamped to the UITextSizeConstraint's pixel range (`drawn`), and fits
    ///    by the usual rules;
    ///  * lo = S0, hi = E; while hi fits, (lo, hi) = (hi, 2 hi - lo); then while hi - lo > 1: mid =
    ///    f32((lo + hi) / 2) replaces lo if it fits, else hi. The pick is lo's size.
    ///  * a start that does not fit (notes/text.md rounds 6 and 7; -richscaled8/10/12/14/16/17 and
    ///    fixtures/text-probe-richscaled.luau): bisect lo = 0, hi = 2 E (so E itself is tried first),
    ///    and whenever mid fits, hi also drops to S0 if it is above it.
    /// Returns lo's whole unscaled size N (drawn at `drawn(N)`).
    /// None when nothing tried fits (the caller takes the largest fitting drawable size) or when there
    /// is nothing to estimate from (empty text or box).
    #[allow(clippy::too_many_arguments)]
    fn rich_scaled_size<D, F>(&mut self, text_size: f64, width: f64, height: f64, scale: f64, dpi: f64, drawn: &D, fit_box: &Box, measure: &F) -> Option<f64>
    where
        D: Fn(f64) -> f64,
        F: Fn(&mut TextEngine, f64, bool, Option<f64>, Option<f64>, bool) -> Arc<TextLayout>,
    {
        let area = f32r(width * height);
        if !(area > 0.0 && area < f64::INFINITY) {
            return None;
        }
        let unscaled = |size: f64| -> f64 { f32r(f32r(size * dpi) + f32r(0.999)).floor() };
        let tried = |size: f64| -> f64 { drawn(unscaled(size)) };
        let try_size = |engine: &mut TextEngine, pixels: f64| -> bool {
            let full = measure(engine, text_size, false, Some(pixels), None, false);
            fit_box.fits(&full)
        };

        let start = clamp(text_size.floor(), 1.0, 100.0);
        let start_pixels = drawn(f32r(start * dpi).floor());
        let start_fits = try_size(self, start_pixels);
        // The start layout's TextBounds in unscaled logical px: drawn px / (DPI x UIScale), laid out in
        // the box like the label itself (wrapped, breaking words that do not fit a line, when TextWrapped).
        let layout = measure(self, text_size, false, Some(start_pixels), None, false);
        let unit = f32r(dpi * f32r(scale));
        let text_area = f32r(f32r(layout.width / unit) * f32r(layout.height / unit));
        if !(text_area > 0.0) {
            return None;
        }
        if !start_fits {
            let estimate = f32r(start * f32r(f32r(area / text_area).sqrt()));
            let mut low = 0.0;
            let mut high = f32r(2.0 * estimate);
            let mut found = false;
            while high - low > 1.0 && high - low < f64::INFINITY {
                let middle = f32r((low + high) / 2.0);
                if try_size(self, tried(middle)) {
                    low = middle;
                    found = true;
                    if high > start {
                        high = start;
                    }
                } else {
                    high = middle;
                }
            }
            return if found { Some(unscaled(low)) } else { None };
        }
        let mut low = start;
        let mut high = f32r(start * f32r(f32r(area / text_area).sqrt()));
        for _ in 0..32 {
            if !try_size(self, tried(high)) {
                break;
            }
            let next = f32r(2.0 * high - low);
            low = high;
            high = next;
        }
        // (An infinite estimate, from a start layout of denormal area, is not bisected.)
        while high - low > 1.0 && high - low < f64::INFINITY {
            let middle = f32r((low + high) / 2.0);
            if try_size(self, tried(middle)) {
                low = middle;
            } else {
                high = middle;
            }
        }
        Some(unscaled(low))
    }
}
