//! UIGradient: the gradient's parameter at a pixel, its colours between the keypoints and the dither
//! (notes/gradients.md).
use super::geometry::{array, component, f32_round as f, gpu8, number, Rect};
use serde::Serialize;
use serde_json::Value;

fn sequence(keys: &[Vec<f64>], t: f64, n: usize, default: f64) -> Vec<f64> {
    if keys.is_empty() {
        return vec![default; n];
    }
    let val = |k: &Vec<f64>, i: usize| f(*k.get(i + 1).unwrap_or(&default));
    if t <= keys[0][0] {
        return (0..n).map(|i| val(&keys[0], i)).collect();
    }
    for pair in keys.windows(2) {
        let a = &pair[0];
        let b = &pair[1];
        if t <= b[0] {
            let s = if b[0] == a[0] {
                0.
            } else {
                f(f(f(t) - f(a[0])) / f(f(b[0]) - f(a[0])))
            };
            return (0..n)
                .map(|i| f(val(a, i) + f(f(val(b, i) - val(a, i)) * s)))
                .collect();
        }
    }
    (0..n).map(|i| val(keys.last().unwrap(), i)).collect()
}
pub fn gradient_vertex_field(
    g: &Value,
    raw: [f64; 4],
    bounds: [f64; 4],
    colour: [f64; 3],
    alpha: f64,
) -> Vec<f32> {
    gradient_vertex_field_quad(g, raw, bounds, colour, alpha, bounds)
}
/// The UIGradient field at the pixels `bounds` (device px). A crop (a clip, rows scrolled out) only limits
/// which pixels are evaluated, never how they are shaded.
pub fn gradient_vertex_field_quad(
    g: &Value,
    raw: [f64; 4],
    bounds: [f64; 4],
    colour: [f64; 3],
    alpha: f64,
    quad: [f64; 4],
) -> Vec<f32> {
    gradient_mesh(g, raw, colour, alpha, quad).field(bounds)
}

/// How the gradient's parameter runs over its rect (UIGradient.Type)
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
enum Kind {
    Linear,
    Radial,
    Conical,
}

/// What lies beyond the gradient's ends (UIGradient.TileMode)
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
enum Tile {
    Clamp,
    Repeat,
    Mirror,
}

/// Keypoints closer together than this are drawn from a table of [`TABLE`] entries, not from the keypoints
const CLOSE: f64 = 1. / 256.;
/// The entries of that table, from one end of the part of the gradient its rect shows to the other
const TABLE: usize = 512;
/// How far past 1 the last keypoint of an unevenly spaced list sits
const LAST: f64 = 1. / 2048.;
/// The ordered dither added before a colour is cut to 8 bits, by device pixel (row y mod 4, column x mod 4)
const DITHER: [[f64; 4]; 4] = [
    [0. / 16., 8. / 16., 2. / 16., 10. / 16.],
    [12. / 16., 4. / 16., 14. / 16., 6. / 16.],
    [3. / 16., 11. / 16., 1. / 16., 9. / 16.],
    [15. / 16., 7. / 16., 13. / 16., 5. / 16.],
];

/// A UIGradient over a rect as Studio 0.747 draws it (notes/gradients.md): the parameter t of a pixel from
/// its place relative to the rect, the colours between the keypoints interpolated in squared space, and the
/// dither. [`GradientMesh::field`] shades pixels from these alone, through differences of rect and pixel
/// coordinates.
#[derive(Clone, Debug)]
pub struct GradientMesh {
    /// The rect's origin (device px); everything else is relative to it
    origin: [f64; 2],
    form: Form,
}

/// A gradient without its place: equal forms shade equal pixels at equal places relative to their origins
/// and to the dither's 4 px grid
#[derive(Clone, Debug, PartialEq, Serialize)]
struct Form {
    kind: Kind,
    tile: Tile,
    /// The centre relative to the rect's origin, None when the rect has no area
    centre: Option<[f64; 2]>,
    /// A rect without area: the x of its middle, relative to its origin. t is 0 left of it and 1 from it on,
    /// whatever the rotation (fixtures/text-gradient-degenerate.luau: text in a box 0 px high or wide)
    middle: f64,
    /// Linear: the unit direction over the device px t spans from 0 to 1; radial: 1 / that radius
    slope: [f64; 2],
    /// Conical: the rotation in turns and 1 / min(Scale, 1)
    turn: [f64; 2],
    /// The keypoints of colour and transparency merged: t, the squared colour (0-1) and the opacity (0-1)
    knots: Vec<[f64; 5]>,
    /// The table of keypoints too close to draw apart: the t of its first and last entry and the entries
    table: Option<(f64, f64, Vec<[f64; 4]>)>,
    /// The colour the keypoints multiply (0-255) and the opacity's scale
    colour: [f64; 3],
    alpha: f64,
}

/// `v - origin` when it is exact (TwoSum: no rounding error), else None
pub fn exact_difference(v: f64, origin: f64) -> Option<f64> {
    let s = v - origin;
    let b = s - v;
    let error = (v - (s - b)) + (-origin - b);
    (s.is_finite() && error == 0.0).then_some(s)
}

/// A field varying along one axis (GradientMesh::profile): RGBA per pixel of its one row or column
pub enum Profile {
    Row(Vec<f32>),
    Column(Vec<f32>),
}

/// The gradient relative to a whole-pixel origin (GradientMesh::local)
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LocalMesh<'a> {
    origin: [f64; 2],
    dither: [i64; 2],
    form: &'a Form,
}

impl Form {
    /// t at the pixel centre `p`, relative to the rect's origin
    fn parameter(&self, p: [f64; 2]) -> f64 {
        let Some(centre) = self.centre else {
            return if p[0] >= self.middle { 1. } else { 0. };
        };
        let (x, y) = (p[0] - centre[0], p[1] - centre[1]);
        match self.kind {
            Kind::Linear => x * self.slope[0] + y * self.slope[1] + 0.5,
            Kind::Radial => (x * x + y * y).sqrt() * self.slope[0],
            Kind::Conical => (y.atan2(x) / std::f64::consts::TAU - self.turn[0]).rem_euclid(1.) * self.turn[1],
        }
    }
    /// The squared colour and the opacity at t
    fn value(&self, t: f64) -> [f64; 4] {
        let t = match self.tile {
            Tile::Clamp => t.clamp(0., 1.),
            Tile::Repeat => t - t.floor(),
            Tile::Mirror => {
                let m = t.rem_euclid(2.);
                if m > 1. {
                    2. - m
                } else {
                    m
                }
            }
        };
        if let Some((lo, hi, entries)) = &self.table {
            let u = ((t - lo) / (hi - lo) * (entries.len() - 1) as f64).clamp(0., (entries.len() - 1) as f64);
            let i = (u.floor() as usize).min(entries.len() - 2);
            let s = u - i as f64;
            let (a, b) = (entries[i], entries[i + 1]);
            return std::array::from_fn(|c| a[c] + (b[c] - a[c]) * s);
        }
        knot_value(&self.knots, t)
    }
    /// The field's RGBA at the device pixel (`px`, `py`), whose centre is `p` relative to the rect's origin
    fn shade(&self, p: [f64; 2], px: i64, py: i64) -> [f32; 4] {
        let value = self.value(self.parameter(p));
        let dither = DITHER[py.rem_euclid(4) as usize][px.rem_euclid(4) as usize];
        let channel = |c: usize| (value[c].max(0.).sqrt() * self.colour[c] + dither).floor().clamp(0., 255.) as f32;
        [channel(0), channel(1), channel(2), (value[3] * self.alpha) as f32]
    }
}

/// The piecewise-linear value of `knots` (t, then four values) at t, held beyond its ends
fn knot_value(knots: &[[f64; 5]], t: f64) -> [f64; 4] {
    let tail = |k: &[f64; 5]| [k[1], k[2], k[3], k[4]];
    let (Some(first), Some(last)) = (knots.first(), knots.last()) else {
        return [1.; 4];
    };
    if t <= first[0] {
        return tail(first);
    }
    for pair in knots.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        if t <= b[0] {
            let s = if b[0] > a[0] { (t - a[0]) / (b[0] - a[0]) } else { 1. };
            return std::array::from_fn(|c| a[c + 1] + (b[c + 1] - a[c + 1]) * s);
        }
    }
    tail(last)
}

impl GradientMesh {
    /// The gradient relative to the whole-pixel `origin`: its rect's origin minus `origin` (exact), the
    /// origin's place on the dither's grid and the gradient's form. Two gradients with equal local forms
    /// shade equal pixels at the same place relative to their origins (field() computes only differences of
    /// coordinates, each exact, so each operation rounds the same real number): a layer rasterized from one
    /// is the other's, moved by whole pixels. None when a coordinate minus the origin is inexact (or the
    /// origin is not whole).
    pub fn local(&self, origin: [f64; 2]) -> Option<LocalMesh<'_>> {
        if origin.iter().any(|v| v.fract() != 0.0 || !v.is_finite()) {
            return None;
        }
        Some(LocalMesh {
            origin: [exact_difference(self.origin[0], origin[0])?, exact_difference(self.origin[1], origin[1])?],
            dither: [(origin[0] as i64).rem_euclid(4), (origin[1] as i64).rem_euclid(4)],
            form: &self.form,
        })
    }
    /// The field's RGBA at the device position `p` (a pixel centre, or the place a resampled texel is drawn),
    /// dithered by the pixel `p` is in
    pub fn at(&self, p: [f64; 2]) -> [f64; 4] {
        let local = [p[0] - self.origin[0], p[1] - self.origin[1]];
        self.form.shade(local, p[0].floor() as i64, p[1].floor() as i64).map(f64::from)
    }
    /// Everything the field depends on, as bits (a cache key)
    pub fn bits(&self) -> Vec<u64> {
        let form = &self.form;
        let mut bits = vec![form.kind as u64, form.tile as u64, form.centre.is_some() as u64, form.middle.to_bits()];
        let numbers = self.origin.iter().chain(form.centre.iter().flatten()).chain(&form.slope).chain(&form.turn).chain(&form.colour);
        bits.extend(numbers.chain([&form.alpha]).chain(form.knots.iter().flatten()).map(|v| v.to_bits()));
        if let Some((lo, hi, entries)) = &form.table {
            bits.extend([lo.to_bits(), hi.to_bits()]);
            bits.extend(entries.iter().flatten().map(|v| v.to_bits()));
        }
        bits
    }
    /// The field (RGBA per pixel, row-major: the colour as whole levels 0-255 after the dither, the opacity
    /// 0-alpha) at the pixels `bounds` (whole device px)
    pub fn field(&self, bounds: [f64; 4]) -> Vec<f32> {
        self.field_where(bounds, |_| true)
    }
    /// field() at the pixels `keep` (by row-major index in `bounds`) takes, 0 at the rest: for pixels whose
    /// shading ignores the field (no coverage, a transparent sample)
    pub fn field_where(&self, bounds: [f64; 4], keep: impl Fn(usize) -> bool + Sync) -> Vec<f32> {
        let [x0, y0, x1, y1] = bounds;
        let w = (x1 - x0).max(0.) as usize;
        let h = (y1 - y0).max(0.) as usize;
        let mut field = vec![0.; w * h * 4];
        if w == 0 || h == 0 {
            return field;
        }
        // Rows are independent: a large field shades bands of rows in parallel
        if super::sampling::parallel_rows(w * h) {
            use rayon::prelude::*;
            let band = (super::sampling::PARALLEL_PIXELS / w).max(1);
            field.par_chunks_mut(band * w * 4).enumerate().for_each(|(i, rows)| {
                self.field_rows(rows, [x0, y0 + (i * band) as f64], w, i * band * w, &keep);
            });
        } else {
            self.field_rows(&mut field, [x0, y0], w, 0, &keep);
        }
        field
    }
    /// The colour (RGB) every pixel of field() takes when it is one whole colour throughout (a gradient of
    /// transparency alone over a whole colour: the dither leaves whole levels alone)
    pub fn uniform_colour(&self) -> Option<[f32; 3]> {
        let form = &self.form;
        let first = form.table.as_ref().map_or_else(|| form.knots.first().map(|k| [k[1], k[2], k[3]]), |t| t.2.first().map(|e| [e[0], e[1], e[2]]))?;
        let same = |v: [f64; 3]| v == first;
        let all = match &form.table {
            Some((.., entries)) => entries.iter().all(|e| same([e[0], e[1], e[2]])),
            None => form.knots.iter().all(|k| same([k[1], k[2], k[3]])),
        };
        let levels: [f64; 3] = std::array::from_fn(|c| first[c].max(0.).sqrt() * form.colour[c]);
        (all && levels.iter().all(|v| v.fract() < 1. / 16. && (0. ..=255.).contains(v))).then(|| levels.map(|v| v.floor() as f32))
    }
    /// A field varying along one axis as its one row or column. The dither varies along both, so no field is
    /// one row or column repeated: always None.
    pub fn profile(&self, _bounds: [f64; 4]) -> Option<Profile> {
        None
    }
    /// field_where() over rows from the pixel `start` into `field` (`w` pixels a row, the first `offset` in
    /// the whole)
    fn field_rows(&self, field: &mut [f32], start: [f64; 2], w: usize, offset: usize, keep: &(impl Fn(usize) -> bool + Sync)) {
        for (k, pixel) in field.chunks_exact_mut(4).enumerate() {
            if !keep(offset + k) {
                continue;
            }
            let (px, py) = (start[0] + (k % w) as f64, start[1] + (k / w) as f64);
            let p = [px + 0.5 - self.origin[0], py + 0.5 - self.origin[1]];
            pixel.copy_from_slice(&self.form.shade(p, px as i64, py as i64));
        }
    }
}

/// The UIGradient `g` over `raw` (the unsnapped rect it spans), multiplying `colour` (0-255) and `alpha`
/// (the opacity's scale). `_quad` (the pixels it is drawn over) does not change how a pixel is shaded.
pub fn gradient_mesh(
    g: &Value,
    raw: [f64; 4],
    colour: [f64; 3],
    alpha: f64,
    _quad: [f64; 4],
) -> GradientMesh {
    // Scenes exported with 4-decimal keypoints put fromRGB colours a hair off k / 255 (0.2510 for 64 / 255):
    // colour values within 0.0128 levels of k / 255 (|v * 255 - k| < 0.0128; 4 decimals move v * 255 by at
    // most 0.01275) snap back to k / 255; exact exports are unaffected.
    let colours: Vec<Vec<f64>> = array(&g["color"])
        .iter()
        .map(|k| {
            array(k)
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    let v = number(v, 0.);
                    let n = (v * 255. + 0.5).floor();
                    if i > 0 && (v * 255. - n).abs() < 0.0128 {
                        n / 255.
                    } else {
                        v
                    }
                })
                .collect()
        })
        .filter(|v: &Vec<f64>| !v.is_empty())
        .collect();
    let transparency: Vec<Vec<f64>> = array(&g["transparency"])
        .iter()
        .map(|k| array(k).iter().map(|v| number(v, 0.)).collect())
        .filter(|v: &Vec<f64>| !v.is_empty())
        .collect();
    let kind = match g["type"].as_str() {
        Some("Radial") => Kind::Radial,
        Some("Conical") => Kind::Conical,
        _ => Kind::Linear,
    };
    let tile = match g["tile"].as_str() {
        Some("Repeat") => Tile::Repeat,
        Some("Mirror") => Tile::Mirror,
        _ => Tile::Clamp,
    };
    let scale = number(&g["scale"], 1.).max(1e-6);
    let angle = number(&g["rotation"], 0.) * std::f64::consts::PI / 180.;
    let (dx, dy) = (super::trig::cos(angle), super::trig::sin(angle));
    let (w, h) = (raw[2], raw[3]);
    let centre = (w > 0. && h > 0.).then(|| {
        [
            w / 2. + f(component(&g["offset"], 0, 0.)) * w,
            h / 2. + f(component(&g["offset"], 1, 0.)) * h,
        ]
    });
    let slope = match kind {
        Kind::Linear => {
            let along = |d: f64, size: f64| if d.abs() > 1e-9 { size / d.abs() } else { f64::INFINITY };
            let span = along(dx, w).min(along(dy, h)) * scale;
            [dx / span, dy / span]
        }
        Kind::Radial => [1. / ((w + h) / 4. * scale), 0.],
        Kind::Conical => [0., 0.],
    };
    let turn = [angle / std::f64::consts::TAU, 1. / scale.min(1.)];
    let mut form = Form { kind, tile, centre, middle: w.max(0.) / 2., slope, turn, knots: Vec::new(), table: None, colour, alpha };

    // The keypoints of the two sequences merged; a keypoint of one takes the other's value there, which
    // is interpolated between that one's keypoints without the squaring
    let mut times: Vec<f64> = colours.iter().chain(transparency.iter()).map(|v| v[0]).filter(|t| (0. ..=1.).contains(t)).collect();
    times.extend([0., 1.]);
    times.sort_by(f64::total_cmp);
    times.dedup();
    let close = times.windows(2).any(|pair| pair[1] - pair[0] < CLOSE);
    // Close keypoints: the table runs over the part of the gradient the rect shows, and takes that
    // part's two ends as keypoints
    let mut window = [0., 1.];
    if close && tile == Tile::Clamp && centre.is_some() {
        let corners = [[0., 0.], [w, 0.], [0., h], [w, h]].map(|p| form.parameter(p));
        let (low, high) = corners.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(low, high), t| (low.min(*t), high.max(*t)));
        let shown = match kind {
            Kind::Linear => [low.max(0.), high.min(1.)],
            Kind::Radial => [0., high.min(1.)],
            Kind::Conical => [0., 1.],
        };
        if shown[1] > shown[0] {
            window = shown;
            times.extend(window);
            times.sort_by(f64::total_cmp);
            times.dedup();
        }
    }
    let knot = |t: f64| {
        let c = sequence(&colours, t, 3, 1.);
        let tr = sequence(&transparency, t, 1, 0.);
        // The squared colour is held in 8 bits
        let squared = |v: f64| (v * v * 255. + 0.5).floor() / 255.;
        [t, squared(c[0]), squared(c[1]), squared(c[2]), 1. - tr[0]]
    };
    form.knots = times.iter().map(|t| knot(*t)).collect();
    if close {
        let entries = (0..TABLE)
            .map(|i| knot_value(&form.knots, window[0] + (window[1] - window[0]) * i as f64 / (TABLE - 1) as f64))
            .collect();
        form.table = Some((window[0], window[1], entries));
    } else {
        let gaps: Vec<f64> = times.windows(2).map(|pair| pair[1] - pair[0]).collect();
        let even = gaps.iter().all(|gap| (gap - gaps[0]).abs() < 1e-6);
        if !even {
            if let Some(last) = form.knots.last_mut() {
                last[0] += LAST;
            }
        }
    }
    GradientMesh { origin: [raw[0], raw[1]], form }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn constant_field() {
        let g = json!({});
        let field = gradient_vertex_field(&g, [0., 0., 4., 4.], [0., 0., 4., 4.], [23., 51., 209.], 127.);
        for p in field.chunks_exact(4) {
            assert_eq!(p, &[23., 51., 209., 127.]);
        }
    }
    /// A rect without area takes the first colour left of its middle and the last from it on
    #[test]
    fn rect_without_area_steps_at_its_middle() {
        let g = json!({"rotation":90,"color":[[0,1,0,0],[1,0,0,1]]});
        let field = gradient_vertex_field(&g, [10., 5., 6., 0.], [10., 3., 16., 5.], [255.; 3], 255.);
        assert_eq!(&field[..4], &[255., 0., 0., 255.]);
        assert_eq!(&field[2 * 4..3 * 4], &[255., 0., 0., 255.]);
        assert_eq!(&field[3 * 4..4 * 4], &[0., 0., 255., 255.]);
    }
    #[test]
    fn exact_difference_rejects_rounded_results() {
        assert_eq!(exact_difference(1000.375, 1000.), Some(0.375));
        assert_eq!(exact_difference(0.1, 1000.), None);
        assert_eq!(exact_difference(f64::INFINITY, 0.), None);
    }
    /// Colours run between two keypoints in squared space: halfway from white to black is 180, not 128
    /// (fixtures/gradient-probe-keys.luau), and the squared keypoint colour is held in 8 bits (20 of 255
    /// is 2 / 255 squared: 22.6, fixtures/gradient-probe-types.luau)
    #[test]
    fn colours_interpolate_in_squared_space() {
        let g = json!({"color":[[0,1,1,1],[1,0,0,0]]});
        let field = gradient_vertex_field(&g, [0., 0., 101., 4.], [0., 0., 101., 4.], [255.; 3], 255.);
        let middle = field[50 * 4];
        assert!((middle - 180.).abs() <= 1., "{middle}");
        assert_eq!(&field[..4], &[254., 254., 254., 255.]);
        let dark = json!({"color":[[0,20.0 / 255.0,0,0],[1,20.0 / 255.0,0,0]]});
        let field = gradient_vertex_field(&dark, [0., 0., 8., 8.], [0., 0., 8., 8.], [255.; 3], 255.);
        assert!(field.chunks_exact(4).all(|p| p[0] == 22. || p[0] == 23.));
        assert!(field.chunks_exact(4).any(|p| p[0] == 23.));
    }
    /// Radial: t is the distance from the centre over a quarter of the rect's width plus height; conical:
    /// the angle from the rotation, clockwise, in turns; Scale divides t about the middle, and TileMode
    /// repeats or mirrors it beyond 0 and 1 (fixtures/gradient-probe-types.luau)
    #[test]
    fn types_scale_and_tiling_set_the_parameter() {
        let raw = [10., 20., 180., 120.];
        let form = |g: Value| gradient_mesh(&g, raw, [255.; 3], 255., [0.; 4]).form;
        let radial = form(json!({"type":"Radial"}));
        assert!((radial.parameter([90. + 37.5, 60.]) - 0.5).abs() < 1e-12);
        assert!((radial.parameter([90., 60. - 75.]) - 1.).abs() < 1e-12);
        let conical = form(json!({"type":"Conical","rotation":90}));
        assert!((conical.parameter([90., 60. + 10.]) - 0.).abs() < 1e-12);
        assert!((conical.parameter([90. - 10., 60.]) - 0.25).abs() < 1e-12);
        let scaled = form(json!({"scale":0.5,"tile":"Mirror","color":[[0,0,0,0],[1,1,1,1]]}));
        assert!((scaled.parameter([0., 0.]) + 0.5).abs() < 1e-12);
        assert_eq!(scaled.value(-0.25)[0], scaled.value(0.25)[0]);
        let repeated = form(json!({"scale":0.5,"tile":"Repeat","color":[[0,0,0,0],[1,1,1,1]]}));
        assert_eq!(repeated.value(-0.25)[0], repeated.value(0.75)[0]);
    }
    /// Keypoints under 1 / 256 apart are drawn from a table over the part of the gradient the rect shows,
    /// with the colours at that part's ends interpolated without the squaring; an unevenly spaced list's
    /// last keypoint sits 1 / 2048 past 1 (fixtures/gradient-probe-close.luau, gradient-probe-ends.luau)
    #[test]
    fn close_keypoints_and_uneven_ends() {
        let raw = [0., 0., 1350., 24.];
        let form = |g: Value| gradient_mesh(&g, raw, [255.; 3], 255., [0.; 4]).form;
        let far = form(json!({"scale":4,"color":[[0,0,0,0],[0.3,1,1,1],[0.8,0,0,0],[0.85,1,1,1],[1,0,0,0]]}));
        assert!(far.table.is_none());
        assert!((far.value(0.375)[0] - 0.85).abs() < 1e-3);
        let close = form(json!({"scale":4,"color":[[0,0,0,0],[0.3,1,1,1],[0.8,0,0,0],[0.801,1,1,1],[1,0,0,0]]}));
        let (lo, hi, entries) = close.table.as_ref().expect("a table");
        assert_eq!((*lo, *hi, entries.len()), (0.375, 0.625, 512));
        assert!((close.value(0.375)[0] - 0.85f64.powi(2)).abs() < 3e-3);
        let uneven = form(json!({"color":[[0,0,0,0],[0.3,1,1,1],[1,0,0,0]]}));
        assert!((uneven.value(1.)[0] - LAST / (0.7 + LAST)).abs() < 1e-12);
        let even = form(json!({"color":[[0,0,0,0],[0.5,1,1,1],[1,0,0,0]]}));
        assert_eq!(even.value(1.)[0], 0.);
    }
    /// A gradient with the same local form at another whole-pixel place shades the same pixels (the layer
    /// cache reuses one for the other: scene_draw's fill-straight-local / stroke-local keys)
    #[test]
    fn equal_local_meshes_shade_equal_fields() {
        let g = json!({"rotation":90,"color":[[0,1,0,0],[0.37,0,1,0],[1,0,0,1]],"transparency":[[0,0],[0.6,0.4],[1,0.1]]});
        let raw = |dy: f64| [612.3374f32 as f64, (401.2256 + dy) as f32 as f64, 180.75, 57.3f32 as f64];
        let mut compared = 0;
        for dy in [0., 3., 64., 516., 1024.] {
            let (a, b) = (raw(0.), raw(dy));
            let quad = |r: [f64; 4]| [r[0].floor(), r[1].floor(), (r[0] + r[2]).ceil(), (r[1] + r[3]).ceil()];
            let (qa, qb) = (quad(a), quad(b));
            let ma = gradient_mesh(&g, a, [255., 200., 100.], 230., qa);
            let mb = gradient_mesh(&g, b, [255., 200., 100.], 230., qb);
            let (Some(la), Some(lb)) = (ma.local([qa[0], qa[1]]), mb.local([qb[0], qb[1]])) else {
                continue;
            };
            if la == lb {
                compared += 1;
                assert_eq!(ma.field(qa), mb.field(qb), "dy {dy}");
            } else {
                assert!(dy % 4. != 0. || la.origin != lb.origin, "dy {dy}");
            }
        }
        assert!(compared > 1);
    }
    /// field_where leaves the pixels it is not asked for at 0, and shades the rest as field does
    #[test]
    fn field_where_shades_the_pixels_kept() {
        for rotation in [0., 90., 37.] {
            let g = json!({"rotation":rotation,"color":[[0,1,0,0],[0.37,0,1,0],[1,0,0,1]],"transparency":[[0,0],[0.6,0.4],[1,0.1]]});
            let quad = [100., 40., 435., 138.];
            let mesh = gradient_mesh(&g, [100.25, 40.5, 333.75, 97.3], [255., 200., 100.], 230., quad);
            let each = mesh.field(quad);
            assert!(mesh.profile(quad).is_none());
            let some = mesh.field_where(quad, |k| k % 3 == 0);
            for (k, (a, b)) in some.chunks_exact(4).zip(each.chunks_exact(4)).enumerate() {
                assert_eq!(a, if k % 3 == 0 { b } else { &[0.; 4][..] });
            }
        }
    }
    /// A gradient of transparency alone gives every pixel the colour it multiplies
    #[test]
    fn uniform_colour_is_the_fields_colour() {
        let g = json!({"rotation":23,"transparency":[[0,1],[0.55,1],[1,0.2]]});
        let quad = [3., 5., 290., 77.];
        let mesh = gradient_mesh(&g, [3.4, 5.1, 286.2, 71.7], [200., 120., 33.], 255., quad);
        let colour = mesh.uniform_colour().expect("one colour");
        assert_eq!(colour, [200., 120., 33.]);
        for pixel in mesh.field(quad).chunks_exact(4) {
            assert_eq!(pixel[..3], colour);
        }
        let coloured = json!({"rotation":23,"color":[[0,1,0,0],[1,0,0,1]]});
        assert!(gradient_mesh(&coloured, [3.4, 5.1, 286.2, 71.7], [200., 120., 33.], 255., quad).uniform_colour().is_none());
    }
    #[test]
    fn crop_preserves_original_quad() {
        let g = json!({"rotation":37,"color":[[0,1,0,0],[0.4,0,1,0],[1,0,0,1]]});
        let full = gradient_vertex_field(&g, [0., 0., 10., 8.], [0., 0., 10., 8.], [255.; 3], 255.);
        let crop = gradient_vertex_field_quad(&g, [0., 0., 10., 8.], [2., 3., 7., 6.], [255.; 3], 255., [0., 0., 10., 8.]);
        for y in 0..3 {
            assert_eq!(&crop[y * 20..y * 20 + 20], &full[((y + 3) * 10 + 2) * 4..((y + 3) * 10 + 7) * 4]);
        }
    }
}

/// A bordered frame's UIGradient per vertex on each quad (top / bottom border strips across the outer rect,
/// left / right strips between them, then the fill), straight RGBA bytes over `outer` (colour gpu8(field / 255),
/// alpha gpu8(field alpha / 255)). fixtures/gradient-edges.luau 99.66% bit-exact (outer quad under the fill
/// 99.49%, full-height side strips 99.31%, mitred ring 99.41%).
pub fn border_gradient_pixels(
    g: &Value,
    raw: [f64; 4],
    outer: Rect,
    fill: Rect,
    border_colour: [f64; 3],
    fill_colour: [f64; 3],
    alpha: f64,
) -> Vec<u8> {
    border_gradient_mesh_pixels(&border_gradient_meshes(g, raw, outer, fill, border_colour, fill_colour, alpha), outer)
}

/// border_gradient_pixels' quads (pixel bounds, clipped to `outer`) with their gradient meshes
pub fn border_gradient_meshes(
    g: &Value,
    raw: [f64; 4],
    outer: Rect,
    fill: Rect,
    border_colour: [f64; 3],
    fill_colour: [f64; 3],
    alpha: f64,
) -> Vec<([f64; 4], GradientMesh)> {
    let mut parts = Vec::new();
    let mut put = |q: Rect, colour: [f64; 3]| {
        let x0 = q.x.max(outer.x);
        let y0 = q.y.max(outer.y);
        let x1 = (q.x + q.w).min(outer.x + outer.w);
        let y1 = (q.y + q.h).min(outer.y + outer.h);
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let bounds = [x0, y0, x1, y1];
        parts.push((bounds, gradient_mesh(g, raw, colour, alpha, bounds)));
    };
    if fill.w > 0. && fill.h > 0. {
        put(Rect::new(outer.x, outer.y, outer.w, fill.y - outer.y), border_colour);
        put(
            Rect::new(outer.x, fill.y + fill.h, outer.w, outer.y + outer.h - fill.y - fill.h),
            border_colour,
        );
        put(Rect::new(outer.x, fill.y, fill.x - outer.x, fill.h), border_colour);
        put(
            Rect::new(fill.x + fill.w, fill.y, outer.x + outer.w - fill.x - fill.w, fill.h),
            border_colour,
        );
        put(fill, fill_colour);
    } else {
        put(outer, border_colour);
    }
    parts
}

/// Straight RGBA bytes over `outer` from border_gradient_meshes' quads
pub fn border_gradient_mesh_pixels(parts: &[([f64; 4], GradientMesh)], outer: Rect) -> Vec<u8> {
    let ow = outer.w.max(0.) as usize;
    let oh = outer.h.max(0.) as usize;
    let mut out = vec![0u8; ow * oh * 4];
    for (bounds, mesh) in parts {
        let [x0, y0, x1, y1] = *bounds;
        let field = mesh.field(*bounds);
        let w = (x1 - x0) as usize;
        let h = (y1 - y0) as usize;
        for j in 0..h {
            for i in 0..w {
                let s = (j * w + i) * 4;
                let d = ((j + (y0 - outer.y) as usize) * ow + i + (x0 - outer.x) as usize) * 4;
                out[d + 3] = gpu8(field[s + 3] as f64 / 255.);
                for c in 0..3 {
                    out[d + c] = gpu8(field[s + c] as f64 / 255.);
                }
            }
        }
    }
    out
}
