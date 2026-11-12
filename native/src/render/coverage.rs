//! Measured UICorner, UIStroke and UIShadow coverage rules, evaluated in device space.
use super::geometry::{Rect, Transform, apply};

pub fn corner_coverage(w: u32, h: u32, radii: [f64; 4], ox: i32, oy: i32) -> Vec<f64> {
    corner_coverage_raw(w, h, radii, ox, oy, w as f64, h as f64)
}

/// Evaluate only the requested local pixel region, retaining the original rectangle and
/// derivative-quad origin: every value equals corner_coverage's for the whole rect (test
/// clipped_regions_equal_full_coverage_without_reanchoring_derivatives).
#[allow(clippy::too_many_arguments)]
pub fn corner_coverage_region(
    w: u32,
    h: u32,
    radii: [f64; 4],
    ox: i32,
    oy: i32,
    x0: i32,
    y0: i32,
    pw: u32,
    ph: u32,
) -> Vec<f64> {
    let limit = (w as f64 / 2.0).min(h as f64 / 2.0);
    corner_coverage_region_limited(w, h, radii, limit, ox, oy, x0, y0, pw, ph)
}

/// corner_coverage_region with the radii clamped to `limit`: half the unsnapped short side of a snapped
/// rounded fill (fixtures/stroke-button.luau FillOnly: 254 px > 2 levels with the snapped half, 0 unsnapped).
#[allow(clippy::too_many_arguments)]
pub fn corner_coverage_region_limited(
    w: u32,
    h: u32,
    radii: [f64; 4],
    limit: f64,
    ox: i32,
    oy: i32,
    x0: i32,
    y0: i32,
    pw: u32,
    ph: u32,
) -> Vec<f64> {
    if pw == 0 || ph == 0 {
        return Vec::new();
    }
    let hw = w as f64 / 2.0;
    let hh = h as f64 / 2.0;
    let radii = radii.map(|r| r.max(0.0).min(limit));
    let px2 = ox.rem_euclid(2);
    let py2 = oy.rem_euclid(2);
    let sd = |i: i32, j: i32| {
        let px = i as f64 + 0.5 - hw;
        let py = j as f64 + 0.5 - hh;
        let r = radii[if py < 0.0 {
            if px < 0.0 { 0 } else { 1 }
        } else if px < 0.0 {
            3
        } else {
            2
        }];
        let qx = px.abs() - hw + r;
        let qy = py.abs() - hh + r;
        qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - r
    };
    let mut data = vec![0.0; pw as usize * ph as usize];
    // fw belongs to the 2x2 quad: computed once per quad (cached by quad column, reset per quad
    // row). Per pixel step sd changes by at most 1, plus under 0.21 where neighbouring corners of
    // different radius above the half side meet: fw / 2 < 1.21, so d < -1.25 gives t = 1.
    let mut quad_fw = vec![f64::NAN; pw as usize + 2];
    let mut quad_row = None;
    for dy in 0..ph {
        let j = y0 + dy as i32;
        let qj = j - (j + py2) % 2;
        if quad_row != Some(qj) {
            quad_row = Some(qj);
            quad_fw.fill(f64::NAN);
        }
        for dx in 0..pw {
            let i = x0 + dx as i32;
            let d = sd(i, j);
            if d < -1.25 {
                data[(dy * pw + dx) as usize] = 1.0;
                continue;
            }
            let qi = i - (i + px2) % 2;
            let slot = (qi - x0 + 1) as usize;
            let mut fw = quad_fw[slot];
            if fw.is_nan() {
                let origin = sd(qi, qj);
                fw = (sd(qi + 1, qj) - origin).abs() + (sd(qi, qj + 1) - origin).abs();
                quad_fw[slot] = fw;
            }
            let t = if fw > 0.0 {
                (0.5 - d / fw).clamp(0.0, 1.0)
            } else if d < 0.0 {
                1.0
            } else {
                0.0
            };
            data[(dy * pw + dx) as usize] = t * t * (3.0 - 2.0 * t);
        }
    }
    data
}

/// Raw extents retain the unsnapped shader-local size when a CanvasGroup is resampled.
pub fn corner_coverage_raw(
    w: u32,
    h: u32,
    radii: [f64; 4],
    ox: i32,
    oy: i32,
    rw: f64,
    rh: f64,
) -> Vec<f64> {
    if w == 0 || h == 0 {
        return Vec::new();
    }
    let hw = rw / 2.0;
    let hh = rh / 2.0;
    let limit = hw.min(hh);
    let kx = rw / w as f64;
    let ky = rh / h as f64;
    let radii = radii.map(|r| r.max(0.0).min(limit));
    let px2 = ox.rem_euclid(2);
    let py2 = oy.rem_euclid(2);
    let sd = |i: i32, j: i32| {
        let px = (i as f64 + 0.5) * kx - hw;
        let py = (j as f64 + 0.5) * ky - hh;
        let r = radii[if py < 0.0 {
            if px < 0.0 { 0 } else { 1 }
        } else if px < 0.0 {
            3
        } else {
            2
        }];
        let qx = px.abs() - hw + r;
        let qy = py.abs() - hh + r;
        qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - r
    };
    // fw belongs to the 2x2 quad: computed once per quad (by quad column, tagged with its quad
    // row). Per pixel step sd changes by at most kx (ky), plus under 0.21 where neighbouring corners
    // of different radius above the half side meet: fw / 2 < (kx + ky) / 2 + 0.21, so a pixel with
    // d below minus that bound has t = 1.
    let deep = -((kx + ky) / 2.0 + 0.25);
    let mut quad_fw = vec![(i32::MIN, 0.0f64); w as usize + 1];
    let mut at = |i: i32, j: i32| {
        let d = sd(i, j);
        if d < deep {
            return 1.0;
        }
        let qi = i - (i + px2) % 2;
        let qj = j - (j + py2) % 2;
        let slot = &mut quad_fw[(qi + 1) as usize];
        let fw = if slot.0 == qj {
            slot.1
        } else {
            let origin = sd(qi, qj);
            let fw = (sd(qi + 1, qj) - origin).abs() + (sd(qi, qj + 1) - origin).abs();
            *slot = (qj, fw);
            fw
        };
        let t = if fw > 0.0 {
            (0.5 - d / fw).clamp(0.0, 1.0)
        } else if d < 0.0 {
            1.0
        } else {
            0.0
        };
        t * t * (3.0 - 2.0 * t)
    };
    let mut data = vec![1.0; (w * h) as usize];
    for (index, (right, bottom)) in [(false, false), (true, false), (true, true), (false, true)]
        .into_iter()
        .enumerate()
    {
        let cw = w.min((radii[index] / kx).ceil() as u32 + 3);
        let ch = h.min((radii[index] / ky).ceil() as u32 + 3);
        let x0 = if right { w - cw } else { 0 };
        let y0 = if bottom { h - ch } else { 0 };
        for y in y0..y0 + ch {
            for x in x0..x0 + cw {
                data[(y * w + x) as usize] = at(x as i32, y as i32);
            }
        }
    }
    data
}

#[derive(Clone, Copy)]
enum Mode {
    Smooth,
    Ring,
    Hard,
}
#[derive(Clone, Copy)]
struct Quadrant {
    r: f64,
    sharp: bool,
    mode: Mode,
    margin: f64,
    outer_radius: f64,
    legs: f64,
}

/// Border UIStroke alpha, including Studio's asymmetric top-left fill rules (notes/strokes.md).
#[allow(clippy::too_many_arguments)]
pub fn border_stroke_alpha(
    rect: Rect,
    radii: [f64; 4],
    thickness: f64,
    position: &str,
    joins: &str,
    grid: [i32; 2],
    size: [u32; 2],
    to_local: Transform,
    rotated: bool,
    scale: f64,
) -> Vec<f32> {
    let limit = (rect.w / 2.0).min(rect.h / 2.0);
    border_stroke_alpha_limited(
        rect, radii, limit, thickness, position, joins, grid, size, to_local, rotated, scale,
    )
}

/// border_stroke_alpha with the corner radii clamped to `limit` (half the unsnapped short side, grown by a
/// BorderOffset) instead of half the (snapped) rect's short side. fixtures/stroke-button.luau (a pill button under
/// UIScale 0.885: 86.29 device px high snapped to 86, UICorner 43.14 px): the snapped half left 1,105 stroke
/// pixels > 2 levels on the pill ends, the unsnapped half 0.
#[allow(clippy::too_many_arguments)]
pub fn border_stroke_alpha_limited(
    rect: Rect,
    radii: [f64; 4],
    limit: f64,
    thickness: f64,
    position: &str,
    joins: &str,
    grid: [i32; 2],
    size: [u32; 2],
    to_local: Transform,
    rotated: bool,
    scale: f64,
) -> Vec<f32> {
    border_stroke_alpha_modes(
        rect, radii, limit, thickness, position, joins, grid, size, to_local, rotated, scale,
    )
    .0
}

/// Border UIStroke alpha byte of a pixel from its coverage and vertex alpha `a` (0..255: alpha8 of
/// 1 - Transparency, or a UIGradient's interpolated vertex alpha). Ring-mode pixels (the linear ramps of rounded
/// corners and their straight sides) convert coverage * a through the GPU's 12-bit path, gpu8(coverage * a / 255)
/// computed as floor(coverage * a * 4096 / 255); clip / hard mode pixels (sharp corners' smoothstep contour)
/// truncate, floor(coverage * a). Measured on fixtures/stroke-fringe.luau (strokes round 6, notes/images.md):
/// white fringes at coverage .75 / .8625 / .975 / .6 / .7125 / .825 / .9375 read 191 / 220 / 249 / 153 / 182 /
/// 210 / 239 (14 of 14); Transparency 0.5 at full coverage reads 128.
pub fn stroke_alpha_byte(coverage: f64, a: f64, ring: bool) -> u8 {
    if !ring {
        return (coverage * a + 1e-6).floor().clamp(0., 255.) as u8;
    }
    let t = (coverage.clamp(0., 1.) * a * 4096. / 255.).floor().min(4096.);
    ((t * 255. + 2047.) / 4096.).floor() as u8
}

/// border_stroke_alpha_limited plus a mask that is 1 where the pixel's corner quadrant renders in ring
/// mode (corner radius r > I, except Inner Miter / Bevel; stroke_alpha_byte converts those differently).
#[allow(clippy::too_many_arguments)]
pub fn border_stroke_alpha_modes(
    rect: Rect,
    radii: [f64; 4],
    limit: f64,
    thickness: f64,
    position: &str,
    joins: &str,
    grid: [i32; 2],
    size: [u32; 2],
    to_local: Transform,
    rotated: bool,
    scale: f64,
) -> (Vec<f32>, Vec<u8>) {
    let x0 = rect.x;
    let y0 = rect.y;
    let x1 = x0 + rect.w;
    let y1 = y0 + rect.h;
    let hw = rect.w / 2.0;
    let hh = rect.h / 2.0;
    let cx = x0 + hw;
    let cy = y0 + hh;
    let d = match position {
        "Inner" => 0.0,
        "Center" => thickness / 2.0,
        _ => thickness,
    };
    let inset = match position {
        "Inner" => thickness,
        "Center" => thickness / 2.0,
        _ => 0.0,
    };
    let round = joins != "Miter" && joins != "Bevel";
    let bevel = joins == "Bevel";
    let radii = radii.map(|r| r.max(0.0).min(limit));
    // (measured on unrotated frames; a rotated frame keeps the ramp)
    let mixed = !rotated && radii.iter().any(|r| *r != radii[0]);
    let quadrants = radii.map(|r| {
        let sharp = r == 0.0;
        let mut mode = Mode::Smooth;
        // Measured: pixel centres up to 0.1 logical px past the outer edge are drawn: at 1.5x up to 0.1475 px,
        // clipped from 0.1525 (15 degree rotated frames; axis-aligned probes bracket (1/8, 3/16)); at 1x up to
        // 0.10, clipped from 0.11 (fixtures/stroke-sweep-fine.luau)
        let mut margin = scale / 10.0;
        if r - inset > 0.0 && (round || position != "Inner") {
            mode = Mode::Ring;
        } else if !round && d > 0.0 {
            mode = Mode::Hard;
            if !(sharp && bevel && position == "Center") {
                margin = 0.0;
            }
        } else if sharp && d == 0.0 {
            mode = Mode::Hard;
            margin = 0.0;
        } else if sharp && d <= 3.0 && !rotated {
            margin = 0.0;
        }
        Quadrant {
            r,
            sharp,
            mode,
            margin,
            outer_radius: r + d + 0.5,
            legs: if sharp && position == "Center" {
                d
            } else {
                thickness
            },
        }
    });
    let outer = |q: Quadrant, x: f64, y: f64| {
        let qx = (x - cx).abs() - hw - d + q.outer_radius;
        let qy = (y - cy).abs() - hh - d + q.outer_radius;
        (if qx <= 0.0 || qy <= 0.0 {
            qx.max(qy)
        } else {
            qx.hypot(qy)
        }) - q.outer_radius
    };
    let interior =
        inset + radii.into_iter().fold(0.0, f64::max) + 2.0 + if bevel { inset } else { 0.0 };
    let [gw, gh] = size;
    let [gx, gy] = grid;
    let mut out = vec![0.0; (gw * gh) as usize];
    let mut ring = vec![0u8; (gw * gh) as usize];
    // Under a translation each row's hole (pixels every branch below leaves at 0) is one span
    // around cx, skipped whole: on pill-shaped frames `interior` exceeds the half height and skips
    // nothing. The span is shrunk by HOLE_MARGIN so pixels near its ends take the exact path.
    const HOLE_MARGIN: f64 = 1e-4;
    let translated = to_local[0] == 1.0 && to_local[1] == 0.0 && to_local[2] == 0.0 && to_local[3] == 1.0;
    let [lx0, ly0] = apply(to_local, gx as f64 + 0.5, gy as f64 + 0.5);
    // The hole's half width left or right of cx in quadrant q on a row at local y (negative: none)
    let hole_half = |q: Quadrant, y: f64| -> f64 {
        let ay = (y - cy).abs();
        if matches!(q.mode, Mode::Ring) {
            // a = 0 where distance <= H = r - inset - 1/2: rows inside the straight sides (qy <= 0)
            // out to qx = H, rows in the corner band out to the arc qx = sqrt(H^2 - qy^2)
            let big_h = q.r - inset - 0.5;
            let qy = ay - hh + q.r;
            if qy > big_h - HOLE_MARGIN {
                return -1.0;
            }
            return hw - q.r + if qy <= 0.0 { big_h } else { (big_h * big_h - qy * qy).sqrt() };
        }
        // Clip modes cut the hole by geometry: inside the rect shrunk by inset (Inner Bevel also
        // cuts its corners)
        if bevel && position == "Inner" {
            return -1.0;
        }
        if ay <= hh - inset - HOLE_MARGIN { hw - inset } else { -1.0 }
    };
    for j in 0..gh {
        let (mut skip_from, mut skip_to) = (gw as i64, gw as i64);
        if translated {
            let y = ly0 + j as f64;
            if (y - cy).abs() > HOLE_MARGIN {
                let top = y < cy;
                let left = hole_half(quadrants[if top { 0 } else { 3 }], y) - HOLE_MARGIN;
                let right = hole_half(quadrants[if top { 1 } else { 2 }], y) - HOLE_MARGIN;
                // Pixel i sits at local x lx0 + i; left of cx takes the left quadrant, cx and
                // right of it the right one
                let from = if left > 0.0 {
                    (cx - left - lx0).ceil()
                } else {
                    (cx - lx0 + HOLE_MARGIN).ceil()
                };
                let to = if right > 0.0 {
                    (cx + right - lx0).floor() + 1.0
                } else {
                    (cx - lx0 - HOLE_MARGIN).ceil()
                };
                if (left > 0.0 || right > 0.0) && to > from {
                    skip_from = (from.max(0.0) as i64).min(gw as i64);
                    skip_to = (to as i64).clamp(0, gw as i64);
                }
            }
        }
        let mut i = 0u32;
        while i < gw {
            if i as i64 == skip_from && skip_to > skip_from {
                i = skip_to as u32;
                continue;
            }
            let current = i;
            i += 1;
            let i = current;
            let [x, y] = apply(
                to_local,
                gx as f64 + i as f64 + 0.5,
                gy as f64 + j as f64 + 0.5,
            );
            let v = (y - y0).min(y1 - y);
            if v > interior && (x - x0).min(x1 - x) > interior {
                continue;
            }
            let left = x < cx;
            let q = quadrants[if y < cy {
                if left { 0 } else { 1 }
            } else if left {
                3
            } else {
                2
            }];
            let m = q.margin;
            if x < x0 - d - m || x >= x1 + d + m || y < y0 - d - m || y >= y1 + d + m {
                continue;
            }
            let u = (x - x0).min(x1 - x);
            if bevel && d > 0.0 {
                let s = u + v + 2.0 * d - q.legs + m * std::f64::consts::SQRT_2;
                if s < 0.0 || (s == 0.0 && !left) {
                    continue;
                }
            }
            let a = if matches!(q.mode, Mode::Ring) {
                let qx = (x - cx).abs() - hw + q.r;
                let qy = (y - cy).abs() - hh + q.r;
                let distance = if qx <= 0.0 || qy <= 0.0 {
                    qx.max(qy)
                } else {
                    qx.hypot(qy)
                };
                // A frame whose corners differ in radius cuts the hole out of its straight sides by geometry
                // (pixel centres, top-left rule) and ramps only inside each corner's square: an inner edge
                // 0.3 px past a pixel centre leaves that pixel empty where equal radii leave it 0.2
                // (fixtures/stroke-probe-mixed.luau)
                if mixed
                    && (qx <= 0.0 || qy <= 0.0)
                    && x >= x0 + inset
                    && x < x1 - inset
                    && y >= y0 + inset
                    && y < y1 - inset
                {
                    continue;
                }
                // In the last half pixel of a left corner's square, a pixel centred half a pixel or more
                // past the straight inner edges is empty (the arc's ramp would leave it 1 to 5 %); nearer
                // the edge it holds the ramp (0.25 px past: 26 %)
                if mixed
                    && left
                    && (qx <= 0.5 || qy <= 0.5)
                    && x >= x0 + inset + 0.5
                    && y >= y0 + inset + 0.5
                    && x <= x1 - inset - 0.5
                    && y <= y1 - inset - 0.5
                {
                    continue;
                }
                // (one device pixel wide at any scale: Studio 0.747, notes/strokes.md Round 8)
                let mut a = (distance - (q.r - inset) + 0.5).clamp(0.0, 1.0);
                if round {
                    a *= (q.r + d + 0.5 - distance).clamp(0.0, 1.0);
                }
                ring[(j * gw + i) as usize] = 1;
                a
            } else {
                // (a sharp corner of a frame with other corners rounded keeps the top-left rule:
                // fixtures/stroke-modes.luau S10_8)
                let flipped = position == "Inner" && q.sharp && !mixed;
                let mut hole = if flipped {
                    x > x0 + inset && x <= x1 - inset && y > y0 + inset && y <= y1 - inset
                } else {
                    x >= x0 + inset && x < x1 - inset && y >= y0 + inset && y < y1 - inset
                };
                if !flipped && hole && bevel && position == "Inner" {
                    let s = u + v - 3.0 * inset;
                    hole = s > 0.0 || (s == 0.0 && left);
                }
                if hole {
                    continue;
                }
                if matches!(q.mode, Mode::Hard) {
                    1.0
                } else {
                    let qx = gx as f64 + (i - i % 2) as f64 + 0.5;
                    let qy = gy as f64 + (j - j % 2) as f64 + 0.5;
                    let p00 = apply(to_local, qx, qy);
                    let p10 = apply(to_local, qx + 1.0, qy);
                    let p01 = apply(to_local, qx, qy + 1.0);
                    let d00 = outer(q, p00[0], p00[1]);
                    let fw = (outer(q, p10[0], p10[1]) - d00).abs()
                        + (outer(q, p01[0], p01[1]) - d00).abs();
                    let a = (0.5 - outer(q, x, y) / fw.max(1e-9)).clamp(0.0, 1.0);
                    a * a * (3.0 - 2.0 * a)
                }
            };
            out[(j * gw + i) as usize] = a as f32;
        }
    }
    (out, ring)
}

/// Hard UIShadow (BlurRadius 0): the shape itself, pixel centres inside the rounded rect, top-left fill rule.
#[derive(Clone, Copy, Debug)]
pub struct ShadowField {
    w: f64,
    h: f64,
    radius: f64,
    /// The straight pieces of a quarter arc; 0: the circle itself
    segments: u32,
}
impl ShadowField {
    pub fn new(w: f64, h: f64, radius: f64) -> Self {
        Self { w, h, radius, segments: 0 }
    }
    /// The shape as Studio's mesh has it: a corner's arc is 8 straight pieces, 32 above a radius of 50
    /// logical px (shadow_mesh), so a pixel centre between the chord and the circle is outside
    /// (fixtures/shadows.luau, the hard shadow of a frame with radius 16: 2 px a corner).
    pub fn polygonal(w: f64, h: f64, radius: f64, scale: f64) -> Self {
        let r = radius.min(w / 2.0).min(h / 2.0);
        Self { w, h, radius, segments: if r / scale > 50.0 { 32 } else { 8 } }
    }
    pub fn at(self, x: f64, y: f64) -> f64 {
        let hw = self.w / 2.0;
        let hh = self.h / 2.0;
        let r = self.radius.min(hw).min(hh);
        if x < -hw || x >= hw || y < -hh || y >= hh {
            return 0.0;
        }
        if r > 0.0 {
            let qx = x.abs() - (hw - r);
            let qy = y.abs() - (hh - r);
            if qx > 0.0 && qy > 0.0 {
                if qx * qx + qy * qy > r * r {
                    return 0.0;
                }
                if self.segments > 0 {
                    let step = std::f64::consts::FRAC_PI_2 / self.segments as f64;
                    let j = ((qy.atan2(qx) / step).floor() as u32).min(self.segments - 1) as f64;
                    let (ay, ax) = (j * step).sin_cos();
                    let (by, bx) = ((j + 1.0) * step).sin_cos();
                    // (inside the chord from a to b: on the centre's side of it)
                    if (bx - ax) * (qy - r * ay) - (by - ay) * (qx - r * ax) < 0.0 {
                        return 0.0;
                    }
                }
            }
        }
        1.0
    }
}

/// Studio's 256-texel UIShadow profile: texel i = floor(255 Phi(-3 + 6 i / 255)), sampled at texel centres over
/// [-3, 3] sigma (measured, notes/images.md "Round 4 (2026-09-26): UIShadow is a mesh with a profile texture").
pub fn shadow_profile() -> &'static [u8; 256] {
    static PROFILE: std::sync::OnceLock<[u8; 256]> = std::sync::OnceLock::new();
    PROFILE.get_or_init(|| {
        let mut t = [0u8; 256];
        for (i, v) in t.iter_mut().enumerate() {
            *v = (255.0 * normal_cdf(-3.0 + 6.0 * i as f64 / 255.0)).floor() as u8;
        }
        t
    })
}

/// The fitted UIShadow amplitude g of a half size h (in sigma) per axis: tanh of a max-margin degree-6 polynomial
/// reproducing every measured byte (shadow-probe-i/-l/-n bars, -k/-h squares); 1 from h = 3.25.
pub fn shadow_gain(h: f64) -> f64 {
    if !(h < 3.25) {
        return 1.0;
    }
    let p = h
        * (1.1194847702092372
            + h * (0.04884356429770024
                + h * (-0.053056182552928786
                    + h * (-0.01306252663876636
                        + h * (0.0200130793956547 + h * -0.0034392315659790887)))));
    p.tanh()
}

/// UIShadow vertex alpha: round half down of 255 g(hx) g(hy) (1 - transparency) (Transparency 0.5: 127, 0.8: 51).
pub fn shadow_vertex_alpha(hx: f64, hy: f64, sigma: f64, alpha: f64) -> u32 {
    (255.0 * shadow_gain(hx / sigma) * shadow_gain(hy / sigma) * alpha - 0.5)
        .ceil()
        .max(0.0) as u32
}

/// UIShadow alpha byte at profile coordinate D: round(255 trunc12(profile(D) va / 255)), the profile bilinear
/// with 8-bit weights.
pub fn shadow_alpha8(d: f64, va: u32) -> u8 {
    let t = shadow_profile();
    let c = (((d.clamp(-3.0, 3.0) + 3.0) / 6.0 * 256.0) - 0.5).clamp(0.0, 255.0);
    let i = (c.floor() as usize).min(254);
    let w = ((c - i as f64) * 256.0 + 0.5).floor() as u64;
    let n = (t[i] as u64 * (256 - w) + t[i + 1] as u64 * w) * va as u64;
    let t12 = n * 4096 / 16_646_400;
    ((255 * t12 + 2048) >> 12) as u8
}

/// The alpha byte of a UIShadow the frame's mask cuts (an Inset one, or one without ShowBehindParent) at the
/// profile coordinate `d` where the mask keeps `keep` of it: the profile times the vertex alpha and what is
/// kept, cut as shadow_alpha8 cuts it. An inner shadow passes -D of its hole (fixtures/shadow-probe-inset.luau:
/// every pixel of a sharp frame's shadow, where 1 - profile(D) is a level off on most).
pub fn shadow_cut_alpha8(d: f64, va: u32, keep: f64) -> u8 {
    let t = shadow_profile();
    let c = ((d.clamp(-3.0, 3.0) + 3.0) / 6.0 * 256.0 - 0.5).clamp(0.0, 255.0);
    let i = (c.floor() as usize).min(254);
    let w = ((c - i as f64) * 256.0 + 0.5).floor() as u64;
    let n = (t[i] as u64 * (256 - w) + t[i + 1] as u64 * w) * va as u64;
    let t12 = (n as f64 * keep.clamp(0.0, 1.0) * 4096.0 / 16_646_400.0 + 1e-9).floor() as u64;
    ((255 * t12 + 2048) >> 12) as u8
}

/// One corner quadrant of Studio's shadow mesh in outward coordinates; `d` is the profile coordinate D, linear
/// over the mesh's triangles.
#[derive(Clone, Debug)]
pub struct ShadowCorner {
    ou: f64,
    ov: f64,
    ihx: f64,
    ihy: f64,
    di: f64,
    ri: f64,
    outer: Vec<(f64, f64)>,
    inner: Vec<(f64, f64)>,
}
impl ShadowCorner {
    #[allow(clippy::too_many_arguments)]
    pub fn new(hx: f64, hy: f64, sigma: f64, r: f64, ihx: f64, ihy: f64, di: f64, segments: usize) -> Self {
        let r = r.min(hx).min(hy).max(0.0);
        let k = 3.0 * sigma;
        let ro = r + k;
        let ri = (r - k).max(0.0);
        let mut outer = Vec::with_capacity(segments + 1);
        let mut inner = Vec::with_capacity(segments + 1);
        for j in 0..=segments {
            let a = j as f64 * (std::f64::consts::PI / 2.0) / segments as f64;
            let (sa, ca) = a.sin_cos();
            outer.push((hx - r + ro * ca, hy - r + ro * sa));
            inner.push(if ri > 0.0 { (ihx - ri + ri * ca, ihy - ri + ri * sa) } else { (ihx, ihy) });
        }
        Self { ou: hx + k, ov: hy + k, ihx, ihy, di, ri, outer, inner }
    }
    pub fn d(&self, u: f64, v: f64) -> f64 {
        let di = self.di;
        let n = self.outer.len() - 1;
        let spoke = |j: usize| {
            let (ox, oy) = self.outer[j];
            let (ix, iy) = self.inner[j];
            (ox - ix) * (v - iy) - (oy - iy) * (u - ix)
        };
        if spoke(0) < 0.0 {
            return if u <= self.ihx { di } else { -3.0 + (di + 3.0) * (self.ou - u) / (self.ou - self.ihx) };
        }
        if spoke(n) > 0.0 {
            return if v <= self.ihy { di } else { -3.0 + (di + 3.0) * (self.ov - v) / (self.ov - self.ihy) };
        }
        let (mut lo, mut hi) = (0usize, n - 1);
        while lo < hi {
            let mid = (lo + hi + 1) >> 1;
            if spoke(mid) >= 0.0 {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        let (ox0, oy0) = self.outer[lo];
        let (ox1, oy1) = self.outer[lo + 1];
        let (ix0, iy0) = self.inner[lo];
        let (ix1, iy1) = self.inner[lo + 1];
        if self.ri > 0.0 {
            if (ix1 - ix0) * (v - iy0) - (iy1 - iy0) * (u - ix0) >= 0.0 {
                return di;
            }
            let side = (ix0 - ox1) * (v - oy1) - (iy0 - oy1) * (u - ox1);
            let side_o = (ix0 - ox1) * (oy0 - oy1) - (iy0 - oy1) * (ox0 - ox1);
            if (side >= 0.0) != (side_o >= 0.0) {
                let area = (ix1 - ox1) * (iy0 - oy1) - (iy1 - oy1) * (ix0 - ox1);
                let w_o = ((ix1 - u) * (iy0 - v) - (iy1 - v) * (ix0 - u)) / area;
                return di - (di + 3.0) * w_o;
            }
        }
        let area = (ox1 - ox0) * (iy0 - oy0) - (oy1 - oy0) * (ix0 - ox0);
        let w_i = ((ox1 - ox0) * (v - oy0) - (oy1 - oy0) * (u - ox0)) / area;
        -3.0 + (di + 3.0) * w_i
    }
}

/// The four shadow mesh quadrants [tl, tr, br, bl] of a w x h rect at device scale (inner half sizes 3 sigma
/// in, at least 0.5 logical px; arcs of 8 segments, 32 above 50 logical px).
pub fn shadow_mesh(w: f64, h: f64, sigma: f64, radii: [f64; 4], scale: f64) -> [ShadowCorner; 4] {
    let hx = w / 2.0;
    let hy = h / 2.0;
    let k = 3.0 * sigma;
    let e = 0.5 * scale;
    let ihx = (hx - k).max(e);
    let ihy = (hy - k).max(e);
    let di = (hx.min(hy) / sigma).min(3.0);
    radii.map(|r| {
        let segments = if r.min(hx).min(hy) / scale > 50.0 { 32 } else { 8 };
        ShadowCorner::new(hx, hy, sigma, r, ihx, ihy, di, segments)
    })
}
fn normal_cdf(x: f64) -> f64 {
    let x = x / std::f64::consts::SQRT_2;
    let ax = x.abs();
    let t = 1.0 / (1.0 + 0.3275911 * ax);
    let y = 1.0
        - ((((1.061405429 * t - 1.453152027) * t + 1.421413741) * t - 0.284496736) * t
            + 0.254829592)
            * t
            * (-ax * ax).exp();
    0.5 * (1.0 + if x < 0.0 { -y } else { y })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clipped_regions_equal_full_coverage_without_reanchoring_derivatives() {
        for ox in -3..=3 {
            for oy in -3..=3 {
                for radii in [[0.0; 4], [3.25, 8.5, 12.0, 1.0], [100.0, 2.0, 20.0, 9.5]] {
                    let full = corner_coverage(57, 43, radii, ox, oy);
                    for [x, y, w, h] in [
                        [0, 0, 13, 11],
                        [43, 0, 14, 11],
                        [0, 31, 13, 12],
                        [43, 31, 14, 12],
                        [7, 5, 31, 29],
                        [20, 18, 9, 7],
                    ] {
                        let region =
                            corner_coverage_region(57, 43, radii, ox, oy, x, y, w as u32, h as u32);
                        for j in 0..h {
                            for i in 0..w {
                                assert_eq!(
                                    region[(j * w + i) as usize],
                                    full[((y + j) * 57 + x + i) as usize],
                                    "origin {ox},{oy}; crop {x},{y}; pixel {i},{j}"
                                );
                            }
                        }
                    }
                }
            }
        }
        assert!(corner_coverage_region(57, 43, [3.0; 4], 0, 0, 4, 4, 0, 8).is_empty());
    }
    #[test]
    fn shadow_fields_match_javascript_reference() {
        // Frozen alpha-byte hashes recorded from the retired JS reference (shadow mesh, profile, vertex alpha)
        // (radii up to 153 px: 32-segment arcs; spheres of small and large half sizes).
        let expected = [
            4065001865u32,
            1889079697,
            1287730340,
            2940523893,
            389982261,
            3489616965,
            1002867202,
            370872316,
            191608220,
            1118689675,
            1231976369,
            3876740402,
            3952472493,
            855796690,
            1558663185,
            3416391923,
            4127531247,
            510691342,
            1084420224,
            80754072,
            1067581556,
            80068893,
            519081178,
            1573289911,
        ];
        for seed in 0..24usize {
            let w = (20 + seed * 12) as f64;
            let h = (30 + (seed % 6) * 40) as f64;
            let sigma = 1.0 + (seed % 5) as f64 * 2.3;
            let radii = [(seed % 7) as f64 * 3.0, (seed % 4) as f64 * 9.0, 0.0, seed as f64 * 9.0];
            let va = shadow_vertex_alpha(w / 2.0, h / 2.0, sigma, 1.0 - (seed % 3) as f64 * 0.3);
            let corners = shadow_mesh(w, h, sigma, radii, 1.5);
            let mut hash = 2166136261u32;
            for y in -60..60 {
                for x in -120..120 {
                    let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
                    let q = if py < 0.0 {
                        if px < 0.0 { 0 } else { 1 }
                    } else if px < 0.0 {
                        3
                    } else {
                        2
                    };
                    let value = shadow_alpha8(corners[q].d(px.abs(), py.abs()), va) as u32;
                    hash = (hash ^ value).wrapping_mul(16777619);
                }
            }
            assert_eq!(hash, expected[seed], "shadow case {seed}");
        }
    }
    #[test]
    fn rounded_and_stroke_masks_match_javascript_reference() {
        // Frozen 8-bit mask hashes from cornerCoverage / SceneRenderer.borderStrokeAlpha; the ring strokes at
        // scales other than 1 are this renderer's own since their ramp stopped taking the scale (Studio 0.747).
        let corners = [
            4054628274u32,
            2025123922,
            3849570539,
            853292376,
            1901630666,
            1519488559,
            1721615550,
            3708805196,
            3520324440,
            2321926979,
            958468560,
            1010210618,
            281864109,
            1377374589,
            4045949094,
            339789231,
            4205879582,
            3220501143,
            2658888581,
            4126212392,
            3269546905,
            1138318262,
            1208204907,
            693515872,
        ];
        let strokes = [
            2436972246u32,
            3002565937,
            1734602426,
            3430542588,
            3047246943,
            1339572075,
            1819584623,
            53136845,
            1574060111,
            2514612688,
            117468618,
            1074766995,
            2878728380,
            2562567434,
            1685356165,
            3606728384,
            4211294253,
            2982063452,
            3302788288,
            1214368773,
            1153362503,
            2377840651,
            2905977538,
            162110940,
        ];
        let hash = |values: Vec<f64>| {
            values.iter().fold(2166136261u32, |h, v| {
                (h ^ ((v * 255.0 + 0.5).floor() as u32)).wrapping_mul(16777619)
            })
        };
        for seed in 0..24usize {
            let w = 9 + seed as u32 % 7;
            let h = 8 + seed as u32 % 5;
            let radii = [
                (seed % 4) as f64,
                ((seed + 1) % 5) as f64,
                ((seed + 2) % 6) as f64,
                ((seed + 3) % 7) as f64,
            ];
            let mask = corner_coverage_raw(
                w,
                h,
                radii,
                seed as i32 - 13,
                seed as i32 - 11,
                w as f64 + (seed % 3) as f64 * 0.15,
                h as f64 - (seed % 2) as f64 * 0.125,
            );
            assert_eq!(hash(mask), corners[seed], "corner case {seed}");
            let angle = (seed % 4) as f64 * 13.0 * std::f64::consts::PI / 180.0;
            let c = super::super::trig::cos(angle);
            let s = super::super::trig::sin(angle);
            let mask = border_stroke_alpha(
                Rect::new(1.25, 2.5, 13.0, 11.0),
                radii,
                1.0 + (seed % 5) as f64 * 0.75,
                ["Inner", "Center", "Outer"][seed % 3],
                ["Round", "Miter", "Bevel"][(seed / 3) % 3],
                [-5, -4],
                [25, 25],
                [c, s, -s, c, 0.23, -0.17],
                seed % 4 != 0,
                1.0 + (seed % 3) as f64 * 0.25,
            );
            assert_eq!(
                hash(mask.iter().map(|v| *v as f64).collect()),
                strokes[seed],
                "stroke case {seed}"
            );
        }
    }
    #[test]
    fn stroke_alpha_bytes_match_studio_fringes() {
        // fixtures/stroke-fringe.luau: white ring-mode fringes on black at these coverages (Transparency 0)
        let measured = [(0.75, 191), (0.8625, 220), (0.975, 249), (0.6, 153), (0.7125, 182), (0.825, 210), (0.9375, 239)];
        for (coverage, byte) in measured {
            assert_eq!(stroke_alpha_byte(coverage as f32 as f64, 255., true), byte, "coverage {coverage}");
        }
        // Full coverage gives the vertex alpha byte in both modes; clip mode truncates
        for a in 0..=255 {
            assert_eq!(stroke_alpha_byte(1., a as f64, true), a as u8);
            assert_eq!(stroke_alpha_byte(1., a as f64, false), a as u8);
        }
        assert_eq!(stroke_alpha_byte(0.975, 255., false), 248);
    }
    #[test]
    fn shadow_hard_edges_use_top_left_rule() {
        let f = ShadowField::new(10.0, 8.0, 0.0);
        assert_eq!(f.at(-5.0, -4.0), 1.0);
        assert_eq!(f.at(5.0, 0.0), 0.0);
        assert_eq!(f.at(0.0, 4.0), 0.0);
    }
    /// A hard shadow's arc is the mesh's 8 chords: a pixel centre between a chord and the circle is outside
    #[test]
    fn hard_shadow_arcs_are_chords() {
        let angle = std::f64::consts::FRAC_PI_2 / 16.0;
        let (x, y) = (34.0 + 15.96 * angle.cos(), 34.0 + 15.96 * angle.sin());
        assert_eq!(ShadowField::new(100.0, 100.0, 16.0).at(x, y), 1.0);
        assert_eq!(ShadowField::polygonal(100.0, 100.0, 16.0, 1.0).at(x, y), 0.0);
        assert_eq!(ShadowField::polygonal(100.0, 100.0, 16.0, 1.0).at(34.0 + 15.9 * angle.cos(), 34.0 + 15.9 * angle.sin()), 1.0);
    }
    /// A shadow the frame's mask cuts: the profile's ends are nothing and the vertex alpha, scaled by what
    /// the mask keeps
    #[test]
    fn cut_shadow_alpha_scales_with_the_mask() {
        let ends = [shadow_cut_alpha8(-3.0, 255, 1.0), shadow_cut_alpha8(3.0, 255, 1.0)];
        assert_eq!(*ends.iter().min().unwrap(), 0);
        assert!(*ends.iter().max().unwrap() >= 254);
        for d in [-3.0, -1.0, 0.0, 0.7, 3.0] {
            assert_eq!(shadow_cut_alpha8(d, 255, 0.0), 0);
            let (full, most) = (shadow_cut_alpha8(d, 255, 1.0) as f64, shadow_cut_alpha8(d, 255, 0.84375) as f64);
            assert!((most - full * 0.84375).abs() <= 1.0, "d {d}: {most} of {full}");
            assert!((shadow_cut_alpha8(d, 128, 1.0) as f64 - full * 128.0 / 255.0).abs() <= 1.0);
        }
    }
    /// A frame with different corner radii cuts the hole of its stroke by geometry on its straight sides
    /// (fixtures/stroke-probe-mixed.luau): an inner edge 0.3 px past a pixel centre leaves the pixel empty,
    /// where equal radii ramp to 0.2; and the last pixel of a left corner's square, half a pixel past the
    /// straight inner edge, is empty where a right corner's keeps the arc's ramp
    #[test]
    fn mixed_radii_cut_the_stroke_hole_by_geometry() {
        let stroke = |radii: [f64; 4], thickness: f64, position: &str| {
            border_stroke_alpha(
                Rect::new(0.0, 0.0, 90.0, 60.0),
                radii,
                thickness,
                position,
                "Round",
                [-3, -3],
                [96, 66],
                super::super::geometry::IDENTITY,
                false,
                1.5,
            )
        };
        let at = |a: &[f32], x: usize, y: usize| a[(y + 3) * 96 + x + 3];
        let equal = stroke([12.0; 4], 1.2, "Inner");
        let mixed = stroke([12.0, 0.0, 4.5, 18.0], 1.2, "Inner");
        assert!((at(&equal, 40, 1) - 0.2).abs() < 1e-6);
        assert_eq!(at(&mixed, 40, 1), 0.0);
        assert_eq!(at(&mixed, 40, 0), 1.0);
        let outer = stroke([12.0, 12.0, 0.0, 0.0], 1.5, "Outer");
        assert_eq!(at(&outer, 11, 0), 0.0);
        assert!(at(&outer, 78, 0) > 0.005 && at(&outer, 78, 0) < 0.02);
        // On the straight inner edge itself the arc's ramp stays (a Center stroke of 9 px: 4.5 in)
        let center = stroke([12.0, 12.0, 0.0, 0.0], 9.0, "Center");
        assert!((at(&center, 11, 4) - 0.5167).abs() < 0.001);
    }
    #[test]
    fn inner_stroke_has_a_hole() {
        let a = border_stroke_alpha(
            Rect::new(0.0, 0.0, 8.0, 8.0),
            [0.0; 4],
            2.0,
            "Inner",
            "Miter",
            [0, 0],
            [8, 8],
            super::super::geometry::IDENTITY,
            false,
            1.0,
        );
        assert_eq!(a[0], 1.0);
        assert_eq!(a[3 * 8 + 3], 0.0);
        assert_eq!(a[7 * 8 + 7], 1.0);
    }
}
