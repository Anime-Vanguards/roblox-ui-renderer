//! Canonical UIGradient vertex triangulation and float32 interpolation.
use super::geometry::{array, component, f32_round as f, gpu8, number, Rect};
use serde_json::Value;

pub fn gradient_plane(g: &Value, r: [f64; 4]) -> [f64; 3] {
    let angle = number(&g["rotation"], 0.) * std::f64::consts::PI / 180.;
    let dy = super::trig::sin(angle);
    let dx = super::trig::cos(angle);
    let cx = f(r[0] + f(r[2] / 2.) + f(f(component(&g["offset"], 0, 0.)) * r[2]));
    let cy = f(r[1] + f(r[3] / 2.) + f(f(component(&g["offset"], 1, 0.)) * r[3]));
    let span = (if dx.abs() > 1e-9 {
        r[2] / dx.abs()
    } else {
        f64::INFINITY
    })
    .min(if dy.abs() > 1e-9 {
        r[3] / dy.abs()
    } else {
        f64::INFINITY
    });
    [dx / span, dy / span, -(cx * dx + cy * dy) / span + 0.5]
}
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
fn clip(poly: Vec<[f64; 2]>, p: [f64; 3], keep: f64) -> Vec<[f64; 2]> {
    let mut out = Vec::new();
    let mut enter = None;
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[(i + 1) % poly.len()];
        let fa = keep * (p[0] * a[0] + p[1] * a[1] + p[2]);
        let fb = keep * (p[0] * b[0] + p[1] * b[1] + p[2]);
        if fa >= -1e-9 {
            out.push(a)
        }
        if (fa >= -1e-9) != (fb >= -1e-9) {
            let s = fa / (fa - fb);
            if fb >= -1e-9 {
                enter = Some(out.len())
            }
            out.push([a[0] + (b[0] - a[0]) * s, a[1] + (b[1] - a[1]) * s]);
        }
    }
    if let Some(n) = enter {
        if n > 0 {
            out.rotate_left(n)
        }
    }
    out
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
/// The per-vertex UIGradient field at the pixels `bounds` of the quad `quad` (device px). The keypoint cuts and
/// the triangulation come from the whole quad, so a crop (a clip, rows scrolled out) only limits which pixels
/// are evaluated, never how they are shaded.
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

/// A UIGradient triangulated over a quad: the triangles (device px, after the 1e-6 vertex grid) and their
/// vertex values (colour 0-255, alpha 0-alpha), degenerate triangles dropped. [`GradientMesh::field`]
/// shades pixels from these alone, through differences of vertex and pixel coordinates.
#[derive(Clone, Debug)]
pub struct GradientMesh {
    triangles: Vec<[[f64; 2]; 3]>,
    values: Vec<[[f64; 4]; 3]>,
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

/// The mesh relative to a whole-pixel origin (GradientMesh::local)
pub type LocalMesh<'a> = (Vec<[[f64; 2]; 3]>, &'a [[[f64; 4]; 3]]);

impl GradientMesh {
    /// The mesh relative to the whole-pixel `origin`: vertex coordinates minus the origin (exact) and the
    /// vertex values. Two meshes with equal local forms shade equal pixels at the same place relative to
    /// their origins (field() computes only differences of coordinates, each exact, so each operation rounds
    /// the same real number): a layer rasterized from one is the other's, moved by whole pixels. None when a
    /// coordinate minus the origin is inexact (or the origin is not whole).
    pub fn local(&self, origin: [f64; 2]) -> Option<LocalMesh<'_>> {
        if origin.iter().any(|v| v.fract() != 0.0 || !v.is_finite()) {
            return None;
        }
        let triangles = self
            .triangles
            .iter()
            .map(|triangle| {
                let mut local = [[0.0; 2]; 3];
                for (point, out) in triangle.iter().zip(local.iter_mut()) {
                    *out = [
                        exact_difference(point[0], origin[0])?,
                        exact_difference(point[1], origin[1])?,
                    ];
                }
                Some(local)
            })
            .collect::<Option<Vec<_>>>()?;
        Some((triangles, &self.values))
    }
    /// The interpolated field (RGBA per pixel, row-major) at the pixels `bounds` (whole device px)
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
        // A field varying along one axis only is one row (column) repeated
        if let Some(profile) = self.profile(bounds) {
            match &profile {
                Profile::Row(row) => {
                    for out in field.chunks_exact_mut(w * 4) {
                        out.copy_from_slice(row);
                    }
                }
                Profile::Column(column) => {
                    for (out, value) in field.chunks_exact_mut(w * 4).zip(column.chunks_exact(4)) {
                        for pixel in out.chunks_exact_mut(4) {
                            pixel.copy_from_slice(value);
                        }
                    }
                }
            }
            for (k, pixel) in field.chunks_exact_mut(4).enumerate() {
                if !keep(k) {
                    pixel.fill(0.);
                }
            }
            return field;
        }
        // Rows are independent (a pixel takes the first triangle covering it, in the same order in
        // every band): a large field shades bands of rows in parallel
        if super::sampling::parallel_rows(w * h) {
            use rayon::prelude::*;
            let band = (super::sampling::PARALLEL_PIXELS / w).max(1);
            field.par_chunks_mut(band * w * 4).enumerate().for_each(|(i, rows)| {
                let top = y0 + (i * band) as f64;
                self.field_rows(rows, [x0, top, x1, top + (rows.len() / (w * 4)) as f64], w, i * band * w, &keep);
            });
        } else {
            self.field_rows(&mut field, bounds, w, 0, &keep);
        }
        field
    }
    /// The colour (RGB) every pixel of field() takes when all the vertices share it (a gradient of
    /// transparency alone): the interpolation of equal whole values rounds back to them exactly in f32
    pub fn uniform_colour(&self) -> Option<[f32; 3]> {
        let first = self.values.first()?[0];
        self.values
            .iter()
            .flatten()
            .all(|value| value[..3] == first[..3])
            .then(|| [first[0] as f32, first[1] as f32, first[2] as f32])
    }
    /// The field at the pixels `bounds` as one row every row equals, or one column every column equals, when
    /// it varies along one axis only (a UIGradient at 0 / 90 / 180 / 270 degrees on a rect): interpolated
    /// once, exactly as field() interpolates the first row (column), which the rest repeat
    pub fn profile(&self, bounds: [f64; 4]) -> Option<Profile> {
        let [x0, y0, x1, y1] = bounds;
        let w = (x1 - x0).max(0.) as usize;
        let h = (y1 - y0).max(0.) as usize;
        if h > 1 && self.varies_along(0, [y0 + 0.5, y1 - 0.5]) {
            let mut row = vec![0.; w * 4];
            self.field_rows(&mut row, [x0, y0, x1, y0 + 1.], w, 0, &|_| true);
            return Some(Profile::Row(row));
        }
        if w > 1 && self.varies_along(1, [x0 + 0.5, x1 - 0.5]) {
            let mut column = vec![0.; h * 4];
            self.field_rows(&mut column, [x0, y0, x0 + 1., y1], 1, 0, &|_| true);
            return Some(Profile::Column(column));
        }
        None
    }
    /// Whether the field varies along coordinate `axis` alone over pixel centres `span` (the other coordinate's
    /// range): every triangle is half an axis-aligned rect spanning `span` across, its two vertices on one
    /// line along `axis` valued alike, every coordinate along `axis` has one value throughout, and the
    /// triangles' extents along `axis` are equal or disjoint (each pixel is interpolated between the same
    /// two values on every line across)
    fn varies_along(&self, axis: usize, span: [f64; 2]) -> bool {
        let across = 1 - axis;
        let mut knots: Vec<(f64, [f64; 4])> = Vec::new();
        let mut extents: Vec<[f64; 2]> = Vec::new();
        for (triangle, values) in self.triangles.iter().zip(&self.values) {
            let at = triangle.map(|point| point[axis]);
            let (low, high) = (at[0].min(at[1]).min(at[2]), at[0].max(at[1]).max(at[2]));
            if low == high || at.iter().any(|v| *v != low && *v != high) {
                return false;
            }
            let side = triangle.map(|point| point[across]);
            if side[0].min(side[1]).min(side[2]) > span[0] || side[0].max(side[1]).max(side[2]) < span[1] {
                return false;
            }
            for (v, value) in at.iter().zip(values) {
                match knots.iter().find(|knot| knot.0 == *v) {
                    Some(knot) if knot.1 != *value => return false,
                    Some(_) => {}
                    None => knots.push((*v, *value)),
                }
            }
            extents.push([low, high]);
        }
        !extents.is_empty()
            && extents.iter().all(|a| extents.iter().all(|b| a == b || a[1] <= b[0] || b[1] <= a[0]))
    }
    /// field_where() over the rows `bounds` into `field` (`w` pixels a row, the first `offset` in the whole)
    fn field_rows(&self, field: &mut [f32], bounds: [f64; 4], w: usize, offset: usize, keep: &(impl Fn(usize) -> bool + Sync)) {
        let [x0, y0, x1, y1] = bounds;
        // A pixel left out counts as shaded
        let mut done: Vec<bool> = (0..field.len() / 4).map(|k| !keep(offset + k)).collect();
        for ([p, q, r], [vp, vq, vr]) in self.triangles.iter().zip(&self.values) {
            let det = (q[1] - r[1]) * (p[0] - r[0]) + (r[0] - q[0]) * (p[1] - r[1]);
            let bx0 = x0.max(p[0].min(q[0]).min(r[0]).floor()) as i64;
            let bx1 = x1.min(p[0].max(q[0]).max(r[0]).ceil()) as i64;
            let by0 = y0.max(p[1].min(q[1]).min(r[1]).floor()) as i64;
            let by1 = y1.min(p[1].max(q[1]).max(r[1]).ceil()) as i64;
            for py in by0..by1 {
                for px in bx0..bx1 {
                    let k = ((py as f64 - y0) as usize) * w + (px as f64 - x0) as usize;
                    if done[k] {
                        continue;
                    }
                    let x = px as f64 + 0.5;
                    let y = py as f64 + 0.5;
                    let a = ((q[1] - r[1]) * (x - r[0]) + (r[0] - q[0]) * (y - r[1])) / det;
                    let b = ((r[1] - p[1]) * (x - r[0]) + (p[0] - r[0]) * (y - r[1])) / det;
                    let c = 1. - a - b;
                    if a < -1e-9 || b < -1e-9 || c < -1e-9 {
                        continue;
                    }
                    done[k] = true;
                    for ch in 0..4 {
                        field[k * 4 + ch] = (a * vp[ch] + b * vq[ch] + c * vr[ch]) as f32;
                    }
                }
            }
        }
    }
}

/// The UIGradient `g` over `raw` (the unsnapped rect its plane spans), triangulated over the quad `quad`
pub fn gradient_mesh(
    g: &Value,
    raw: [f64; 4],
    colour: [f64; 3],
    alpha: f64,
    quad: [f64; 4],
) -> GradientMesh {
    // Scenes exported with 4-decimal keypoints put fromRGB colours a hair off k / 255 (0.2510 for 64 / 255),
    // which flips vertex colours that sit on .5 (every remaining miss of fixtures/text-probe-gradient.luau):
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
    let vertex = |t: f64| {
        let t = t.clamp(0., 1.);
        let c = sequence(&colours, t, 3, 1.);
        let tr = sequence(&transparency, t, 1, 0.);
        [
            (f(f(c[0] * f(colour[0] / 255.)) * 255.) + 0.5).floor(),
            (f(f(c[1] * f(colour[1] / 255.)) * 255.) + 0.5).floor(),
            (f(f(c[2] * f(colour[2] / 255.)) * 255.) + 0.5).floor(),
            (f(f(1. - tr[0]) * alpha) + 0.5).floor(),
        ]
    };
    let [l, t, r, b] = quad;
    let corners = vec![[l, t], [r, t], [r, b], [l, b]];
    let plane = if raw[2] > 0. && raw[3] > 0. {
        Some(gradient_plane(g, raw))
    } else {
        None
    };
    let mut pieces = vec![corners.clone()];
    if let Some(p) = plane {
        let mut times = vec![0., 1.];
        times.extend(
            colours
                .iter()
                .chain(transparency.iter())
                .map(|v| v[0])
                .filter(|t| *t >= 0. && *t <= 1.),
        );
        times.sort_by(f64::total_cmp);
        times.dedup();
        times.insert(0, f64::NEG_INFINITY);
        times.push(f64::INFINITY);
        pieces.clear();
        for interval in times.windows(2) {
            let mut poly = corners.clone();
            if interval[0].is_finite() {
                poly = clip(poly, [p[0], p[1], p[2] - interval[0]], 1.)
            }
            if interval[1].is_finite() && poly.len() >= 3 {
                poly = clip(poly, [p[0], p[1], p[2] - interval[1]], -1.)
            }
            if poly.len() >= 3 {
                pieces.push(poly)
            }
        }
    }
    let mut triangles = Vec::new();
    for poly in pieces {
        let mut xs: Vec<f64> = poly
            .iter()
            .map(|p| (p[0] * 1e6 + 0.5).floor() / 1e6)
            .collect();
        let mut ys: Vec<f64> = poly
            .iter()
            .map(|p| (p[1] * 1e6 + 0.5).floor() / 1e6)
            .collect();
        xs.sort_by(f64::total_cmp);
        ys.sort_by(f64::total_cmp);
        xs.dedup();
        ys.dedup();
        if poly.len() == 4 && xs.len() == 2 && ys.len() == 2 {
            triangles.push([[xs[0], ys[0]], [xs[1], ys[0]], [xs[1], ys[1]]]);
            triangles.push([[xs[0], ys[0]], [xs[1], ys[1]], [xs[0], ys[1]]]);
        } else {
            for i in 1..poly.len() - 1 {
                triangles.push([poly[0], poly[i], poly[i + 1]])
            }
        }
    }
    let value = |p: [f64; 2]| vertex(plane.map_or(0., |a| a[0] * p[0] + a[1] * p[1] + a[2]));
    let mut mesh = GradientMesh {
        triangles: Vec::with_capacity(triangles.len()),
        values: Vec::with_capacity(triangles.len()),
    };
    for [p, q, r] in triangles {
        let det = (q[1] - r[1]) * (p[0] - r[0]) + (r[0] - q[0]) * (p[1] - r[1]);
        if det.abs() < 1e-9 {
            continue;
        }
        mesh.values.push([value(p), value(q), value(r)]);
        mesh.triangles.push([p, q, r]);
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn constant_field() {
        let g = json!({});
        let field = gradient_vertex_field(
            &g,
            [0., 0., 4., 4.],
            [0., 0., 4., 4.],
            [23., 51., 209.],
            127.,
        );
        for p in field.chunks_exact(4) {
            assert_eq!(p, &[23., 51., 209., 127.]);
        }
    }
    #[test]
    fn exact_difference_rejects_rounded_results() {
        assert_eq!(exact_difference(1000.375, 1000.), Some(0.375));
        assert_eq!(exact_difference(0.1, 1000.), None);
        assert_eq!(exact_difference(f64::INFINITY, 0.), None);
    }
    /// A mesh with the same local form at another whole-pixel place shades the same pixels (the layer
    /// cache reuses one for the other: scene_draw's fill-straight-local / stroke-local keys)
    #[test]
    fn equal_local_meshes_shade_equal_fields() {
        let g = json!({"rotation":90,"color":[[0,1,0,0],[0.37,0,1,0],[1,0,0,1]],"transparency":[[0,0],[0.6,0.4],[1,0.1]]});
        let raw = |dy: f64| {
            [
                f32_round_test(612.3374) as f64,
                f32_round_test(401.2256 + dy) as f64,
                f32_round_test(180.75) as f64,
                f32_round_test(57.3) as f64,
            ]
        };
        let mut compared = 0;
        for dy in [0., 3., 64., 517., 1021.] {
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
            }
        }
        assert!(compared > 0);
    }
    fn f32_round_test(v: f64) -> f32 {
        v as f32
    }
    /// A gradient varying along one axis (field's repeated row or column, profile) shades exactly the pixels
    /// interpolating every one does; field_where leaves the pixels it is not asked for at 0
    #[test]
    fn one_axis_fields_repeat_their_first_row_or_column() {
        let mut repeated = 0;
        for rotation in [0., 90., 180., 270., 37.] {
            let g = json!({"rotation":rotation,"color":[[0,1,0,0],[0.37,0,1,0],[1,0,0,1]],"transparency":[[0,0],[0.6,0.4],[1,0.1]]});
            let raw = [100.25, 40.5, 333.75, 97.3];
            let quad = [100., 40., 435., 138.];
            let mesh = gradient_mesh(&g, raw, [255., 200., 100.], 230., quad);
            let (w, h) = (335, 98);
            let mut each = vec![0.; w * h * 4];
            mesh.field_rows(&mut each, quad, w, 0, &|_| true);
            assert_eq!(mesh.profile(quad).is_some(), rotation != 37., "rotation {rotation}");
            repeated += usize::from(mesh.profile(quad).is_some());
            assert_eq!(mesh.field(quad), each, "rotation {rotation}");
            let some = mesh.field_where(quad, |k| k % 3 == 0);
            for (k, (a, b)) in some.chunks_exact(4).zip(each.chunks_exact(4)).enumerate() {
                assert_eq!(a, if k % 3 == 0 { b } else { &[0.; 4][..] });
            }
        }
        assert_eq!(repeated, 4);
    }
    /// A gradient of transparency alone gives every pixel the vertices' colour
    #[test]
    fn uniform_colour_is_the_fields_colour() {
        let g = json!({"rotation":23,"transparency":[[0,1],[0.55,1],[1,0.2]]});
        let quad = [3., 5., 290., 77.];
        let mesh = gradient_mesh(&g, [3.4, 5.1, 286.2, 71.7], [200., 120., 33.], 255., quad);
        let colour = mesh.uniform_colour().expect("one colour");
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
        let crop = gradient_vertex_field_quad(
            &g,
            [0., 0., 10., 8.],
            [2., 3., 7., 6.],
            [255.; 3],
            255.,
            [0., 0., 10., 8.],
        );
        for y in 0..3 {
            assert_eq!(
                &crop[y * 20..y * 20 + 20],
                &full[((y + 3) * 10 + 2) * 4..((y + 3) * 10 + 7) * 4]
            );
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
