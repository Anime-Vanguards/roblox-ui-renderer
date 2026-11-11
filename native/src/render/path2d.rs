//! A Path2D's stroke, drawn as Studio draws it (notes/path2d.md; measured in Studio 0.747 on
//! fixtures/path2d-strokes.luau, path2d-curves.luau and the probes beside them).
//!
//! Studio builds a triangle strip along each segment of the curve and samples a small alpha texture across it:
//!
//! - A straight segment is one piece. A curved one is cut at thirtieths of its parameter, and two neighbouring
//!   thirtieths are one piece while the curve's point between them lies within a pixel of their chord.
//! - At every cut the strip has two vertices, half the stroke's width (`Thickness + 1` GUI pixels) to each side
//!   along the curve's normal there. Consecutive segments are joined by two triangles between the last two
//!   vertices of one and the first two of the next: a corner is that quad, not a mitre or a round join. The last
//!   and first segment of a closed path are not joined, and ends are cut square. A piece between two cuts is two
//!   triangles split from its first left vertex to its second right one; which diagonal shows where a tight bend
//!   folds a piece over itself (fixtures/path2d-curves.luau, the closed figure of eight).
//! - Across the strip the alpha is that of a texture `P` texels wide (the power of two that holds
//!   `Thickness + 1`, at least 4) whose border texels are clear, sampled bilinearly: `min(s, 1 - s) * P - 0.5`
//!   clamped to 0..1 for the cross coordinate `s`. The stroke is so `Thickness` wide with a ramp
//!   `(Thickness + 1) / P` GUI pixels wide centred on each edge. The ends have no ramp.
//! - Every triangle blends over what is there, so where triangles overlap (the inside of a corner, a curve that
//!   crosses itself) a half-covered pixel is covered twice.
//!
//! Path2D.Transparency does not reach the drawing: Studio 0.747 keeps and saves it (only its own scripts can
//! write it) and draws a path of Transparency 1 as opaque as one of 0. So a stroke has no opacity here.
//!
//! The triangles are filled as a GPU fills them: vertices snapped to 1/256 pixel, pixel centres, the top-left
//! rule. The result is the stroke's coverage; the layer is that coverage in the path's colour.

use serde_json::Value;

use super::geometry::{array, number};

/// A vertex of the strip in device pixels, with its cross coordinate (0 on the left edge, 1 on the right).
type Vertex = [f64; 3];

/// One segment's control points in GUI pixels: two (a line), three (a quadratic) or four (a cubic).
pub type Control = Vec<[f64; 2]>;

/// How many pieces a curved segment has at most (each a thirtieth of its parameter).
const STEPS: usize = 30;
/// Two thirtieths stay one piece while the curve between them is this near their chord (GUI pixels).
const FLATNESS: f64 = 1.0;
/// Triangle vertices snap to this fraction of a pixel.
const SUBPIXEL: f64 = 256.0;

/// What a `path2d` node draws.
pub struct Stroke {
    pub segments: Vec<Control>,
    pub thickness: f64,
    pub color: [f64; 3],
}

impl Stroke {
    /// The stroke of a node's `path2d`, its segments moved with the node (`rect` against `origin`).
    pub fn of(node: &Value) -> Option<Stroke> {
        let path = node.get("path2d")?;
        let moved = |axis: usize| number(&node["rect"][axis], 0.) - number(&path["origin"][axis], number(&node["rect"][axis], 0.));
        let (dx, dy) = (moved(0), moved(1));
        let segments = array(&path["segments"])
            .iter()
            .map(|segment| array(segment).chunks_exact(2).map(|point| [number(&point[0], 0.) + dx, number(&point[1], 0.) + dy]).collect::<Control>())
            .filter(|control| (2..=4).contains(&control.len()))
            .collect();
        Some(Stroke {
            segments,
            thickness: number(&path["thickness"], 1.),
            color: std::array::from_fn(|channel| number(&path["color"][channel], 0.)),
        })
    }

    /// The texture's width in texels.
    fn texels(&self) -> f64 {
        let mut size = 4.;
        while size < self.thickness + 1. {
            size *= 2.;
        }
        size
    }

    /// The strip's triangles in device pixels.
    pub fn mesh(&self, scale: f64) -> Vec<[Vertex; 3]> {
        let half = (self.thickness + 1.) / 2. * scale;
        let mut triangles = Vec::new();
        let mut previous: Option<[Vertex; 2]> = None;
        if !(self.thickness > 0.) {
            return triangles;
        }
        for control in &self.segments {
            let pieces = tessellate(control);
            let mut strip = Vec::with_capacity(pieces.len());
            for ([x, y], [tx, ty]) in pieces {
                let length = tx.hypot(ty);
                if length == 0. || !length.is_finite() {
                    strip.clear();
                    break;
                }
                let (nx, ny) = (ty / length * half, -tx / length * half);
                strip.push([[x * scale + nx, y * scale + ny, 0.], [x * scale - nx, y * scale - ny, 1.]]);
            }
            // (a segment of no length, or one whose curve stands still at a cut, draws nothing)
            let (Some(first), Some(last)) = (strip.first().copied(), strip.last().copied()) else { continue };
            if let Some([left, right]) = previous {
                triangles.push([left, right, first[1]]);
                triangles.push([left, first[1], first[0]]);
            }
            for pair in strip.windows(2) {
                let ([left, right], [next_left, next_right]) = (pair[0], pair[1]);
                triangles.push([left, right, next_right]);
                triangles.push([left, next_right, next_left]);
            }
            previous = Some(last);
        }
        triangles
    }

    /// The device box `[x0, y0, x1, y1)` (whole pixels) the triangles cover, None when they cover nothing.
    pub fn extent(triangles: &[[Vertex; 3]]) -> Option<[i64; 4]> {
        let mut bounds = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
        for vertex in triangles.iter().flatten() {
            bounds = [bounds[0].min(vertex[0]), bounds[1].min(vertex[1]), bounds[2].max(vertex[0]), bounds[3].max(vertex[1])];
        }
        if !bounds.iter().all(|value| value.is_finite()) {
            return None;
        }
        let bounds = [bounds[0].floor() as i64, bounds[1].floor() as i64, bounds[2].ceil() as i64 + 1, bounds[3].ceil() as i64 + 1];
        (bounds[2] > bounds[0] && bounds[3] > bounds[1]).then_some(bounds)
    }

    /// A hash of the triangles as [`Self::coverage`] fills them from the box corner (`x`, `y`): their vertices
    /// snapped to the subpixel grid. Two meshes of one hash cover the same pixels of their boxes.
    pub fn mesh_hash(triangles: &[[Vertex; 3]], x: i64, y: i64) -> u128 {
        use std::hash::{Hash, Hasher};
        let mut hashers = [std::collections::hash_map::DefaultHasher::new(), std::collections::hash_map::DefaultHasher::new()];
        hashers[1].write_u8(1);
        for vertex in triangles.iter().flatten() {
            let snapped = (snap(vertex[0], x), snap(vertex[1], y), vertex[2].to_bits());
            for hasher in &mut hashers {
                snapped.hash(hasher);
            }
        }
        (hashers[0].finish() as u128) << 64 | hashers[1].finish() as u128
    }

    /// The stroke's alpha over the box at (`x`, `y`) of `width` by `height` device pixels: every triangle
    /// blended over the ones before it.
    pub fn coverage(&self, triangles: &[[Vertex; 3]], x: i64, y: i64, width: usize, height: usize) -> Vec<f32> {
        let mut alpha = vec![0f32; width * height];
        let texels = self.texels();
        let sub = SUBPIXEL as i64;
        for triangle in triangles {
            let mut v: [(i64, i64, f64); 3] = std::array::from_fn(|index| (snap(triangle[index][0], x), snap(triangle[index][1], y), triangle[index][2]));
            let mut area = (v[1].0 - v[0].0) * (v[2].1 - v[0].1) - (v[2].0 - v[0].0) * (v[1].1 - v[0].1);
            if area == 0 {
                continue;
            }
            if area < 0 {
                v.swap(1, 2);
                area = -area;
            }
            let least = |pick: fn(&(i64, i64, f64)) -> i64| v.iter().map(pick).min().unwrap_or(0);
            let most = |pick: fn(&(i64, i64, f64)) -> i64| v.iter().map(pick).max().unwrap_or(0);
            let left = ((least(|p| p.0) - sub / 2).div_euclid(sub)).max(0);
            let right = ((most(|p| p.0) + sub / 2).div_euclid(sub)).min(width as i64 - 1);
            let top = ((least(|p| p.1) - sub / 2).div_euclid(sub)).max(0);
            let bottom = ((most(|p| p.1) + sub / 2).div_euclid(sub)).min(height as i64 - 1);
            // The three edges, each opposite one vertex: (from, to, whether a pixel centre on it is inside)
            let edges: [((i64, i64), (i64, i64), bool); 3] = std::array::from_fn(|index| {
                let (a, b) = (v[(index + 1) % 3], v[(index + 2) % 3]);
                // (y grows down and the vertices run clockwise: a top edge is level with the inside below it,
                // a left edge has the inside to its right)
                let top_left = (b.1 == a.1 && b.0 > a.0) || b.1 < a.1;
                ((a.0, a.1), (b.0, b.1), top_left)
            });
            for row in top..=bottom {
                let py = row * sub + sub / 2;
                for column in left..=right {
                    let px = column * sub + sub / 2;
                    let mut weights = [0i64; 3];
                    let mut inside = true;
                    for (index, &((ax, ay), (bx, by), top_left)) in edges.iter().enumerate() {
                        let edge = (bx - ax) * (py - ay) - (by - ay) * (px - ax);
                        if edge < 0 || (edge == 0 && !top_left) {
                            inside = false;
                            break;
                        }
                        weights[index] = edge;
                    }
                    if !inside {
                        continue;
                    }
                    let s = (weights[0] as f64 * v[0].2 + weights[1] as f64 * v[1].2 + weights[2] as f64 * v[2].2) / area as f64;
                    let covered = (s.min(1. - s) * texels - 0.5).clamp(0., 1.) as f32;
                    let pixel = &mut alpha[row as usize * width + column as usize];
                    *pixel = 1. - (1. - *pixel) * (1. - covered);
                }
            }
        }
        alpha
    }

    /// The layer's bytes (premultiplied RGBA) for a coverage.
    pub fn shade(&self, alpha: &[f32]) -> Vec<u8> {
        let mut bytes = vec![0u8; alpha.len() * 4];
        for (pixel, &covered) in bytes.chunks_exact_mut(4).zip(alpha) {
            if covered <= 0. {
                continue;
            }
            let covered = covered as f64;
            for channel in 0..3 {
                pixel[channel] = (self.color[channel].clamp(0., 1.) * covered * 255. + 0.5).floor() as u8;
            }
            pixel[3] = (covered * 255. + 0.5).floor() as u8;
        }
        bytes
    }
}

/// A device coordinate on the subpixel grid, from a box corner.
fn snap(value: f64, origin: i64) -> i64 {
    ((value - origin as f64) * SUBPIXEL + 0.5).floor() as i64
}

/// The cuts of one segment: each a point of the curve and the curve's direction there (GUI pixels).
pub fn tessellate(control: &[[f64; 2]]) -> Vec<([f64; 2], [f64; 2])> {
    let at = |u: f64| -> [f64; 2] {
        let a = 1. - u;
        std::array::from_fn(|axis| match control {
            [p0, p1, p2, p3] => a * a * a * p0[axis] + 3. * a * a * u * p1[axis] + 3. * a * u * u * p2[axis] + u * u * u * p3[axis],
            [p0, p1, p2] => a * a * p0[axis] + 2. * a * u * p1[axis] + u * u * p2[axis],
            [p0, p1] => a * p0[axis] + u * p1[axis],
            _ => 0.,
        })
    };
    let direction = |u: f64| -> [f64; 2] {
        let a = 1. - u;
        std::array::from_fn(|axis| match control {
            [p0, p1, p2, p3] => a * a * (p1[axis] - p0[axis]) + 2. * a * u * (p2[axis] - p1[axis]) + u * u * (p3[axis] - p2[axis]),
            [p0, p1, p2] => a * (p1[axis] - p0[axis]) + u * (p2[axis] - p1[axis]),
            [p0, p1] => p1[axis] - p0[axis],
            _ => 0.,
        })
    };
    if control.len() == 2 {
        return vec![(control[0], direction(0.)), (control[1], direction(1.))];
    }
    let mut cuts = vec![(at(0.), direction(0.))];
    let mut index = 0;
    while index < STEPS {
        let mut step = 2.min(STEPS - index);
        if step == 2 {
            let (from, to) = (index as f64 / STEPS as f64, (index + 2) as f64 / STEPS as f64);
            let (start, middle, end) = (at(from), at((from + to) / 2.), at(to));
            let (dx, dy) = (end[0] - start[0], end[1] - start[1]);
            let length = dx.hypot(dy);
            let (mx, my) = (middle[0] - start[0], middle[1] - start[1]);
            let away = if length > 0. { (mx * dy - my * dx).abs() / length } else { mx.hypot(my) };
            if away > FLATNESS {
                step = 1;
            }
        }
        index += step;
        let u = index as f64 / STEPS as f64;
        cuts.push((at(u), direction(u)));
    }
    cuts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stroke(segments: Vec<Control>, thickness: f64) -> Stroke {
        Stroke { segments, thickness, color: [1., 1., 1.] }
    }

    fn render(stroke: &Stroke, scale: f64) -> (i64, i64, usize, usize, Vec<f32>) {
        let mesh = stroke.mesh(scale);
        let [x0, y0, x1, y1] = Stroke::extent(&mesh).expect("something to draw");
        let (width, height) = ((x1 - x0) as usize, (y1 - y0) as usize);
        (x0, y0, width, height, stroke.coverage(&mesh, x0, y0, width, height))
    }

    /// Across a level line the alpha is the texture's ramp: Thickness wide, half covered at its edges.
    #[test]
    fn a_level_line_is_its_thickness_wide_with_a_ramp_on_each_edge() {
        // 4 thick at y = 10.25, scale 1: the quad spans y 7.75..12.75, the texture is 8 texels over those 5 pixels
        let line = stroke(vec![vec![[2., 10.25], [30., 10.25]]], 4.);
        let (x0, y0, width, _, alpha) = render(&line, 1.);
        let at = |x: i64, y: i64| alpha[((y - y0) as usize) * width + (x - x0) as usize];
        // pixel centres at 7.5 (outside), 8.5 (s = 0.15: 0.15 * 8 - 0.5), 9.5 .. 11.5 (inside), 12.5 (s = 0.95)
        let across: Vec<f32> = (7..13).map(|y| at(10, y)).collect();
        for (got, want) in across.iter().zip([0., 0.7, 1., 1., 1., 0.]) {
            assert!((got - want).abs() < 1e-5, "{across:?}");
        }
        // the ends are cut square: the centres from 2.5 to 29.5 are covered, none past them
        assert_eq!((at(2, 10), at(29, 10), at(30, 10)), (1., 1., 0.));
    }

    /// The texture grows by powers of two with the thickness, so the ramp's width steps with it.
    #[test]
    fn the_texture_is_the_power_of_two_that_holds_the_thickness() {
        for (thickness, texels) in [(0.5, 4.), (1., 4.), (3., 4.), (3.5, 8.), (7., 8.), (8., 16.), (20., 32.), (100., 128.)] {
            assert_eq!(stroke(Vec::new(), thickness).texels(), texels, "thickness {thickness}");
        }
    }

    /// A corner is the quad between the two strips' ends; a closed path's last corner is left open.
    #[test]
    fn segments_are_bridged_by_two_triangles_except_where_a_closed_path_ends() {
        let corner = stroke(vec![vec![[0., 0.], [20., 0.]], vec![[20., 0.], [20., 20.]]], 4.);
        assert_eq!(corner.mesh(1.).len(), 2 + 2 + 2);
        let triangle = stroke(vec![vec![[0., 0.], [20., 0.]], vec![[20., 0.], [10., 20.]], vec![[10., 20.], [0., 0.]]], 4.);
        assert_eq!(triangle.mesh(1.).len(), 3 * 2 + 2 * 2);
        // a segment of no length draws nothing, and nothing bridges to it
        let dot = stroke(vec![vec![[5., 5.], [5., 5.]]], 10.);
        assert!(dot.mesh(1.).is_empty());
        assert!(stroke(vec![vec![[0., 0.], [20., 0.]]], 0.).mesh(1.).is_empty());
    }

    /// A piece is split from its first left vertex to its second right vertex (Studio's diagonal: the other one
    /// draws a folded piece of a tight bend differently).
    #[test]
    fn a_piece_is_split_from_its_first_left_vertex_to_its_second_right_vertex() {
        let mesh = stroke(vec![vec![[0., 0.], [20., 0.]]], 4.).mesh(1.);
        let (left, right, next_left, next_right) = ([0., -2.5, 0.], [0., 2.5, 1.], [20., -2.5, 0.], [20., 2.5, 1.]);
        assert_eq!(mesh, vec![[left, right, next_right], [left, next_right, next_left]]);
    }

    /// A gentle curve keeps pairs of thirtieths together; a tight one is cut at every thirtieth.
    #[test]
    fn a_curve_is_cut_at_thirtieths_and_flat_pairs_stay_whole() {
        let gentle = tessellate(&[[0., 0.], [50., 4.], [100., 0.]]);
        assert_eq!(gentle.len(), 16);
        let tight = tessellate(&[[0., 0.], [0., -2000.], [2000., -2000.], [2000., 0.]]);
        assert_eq!(tight.len(), 31);
        assert_eq!(tessellate(&[[0., 0.], [9., 9.]]).len(), 2);
    }

    /// Triangles blend over each other: where the stroke crosses itself a half-covered pixel is covered twice.
    #[test]
    fn overlapping_triangles_add_their_coverage() {
        // (a path that crosses itself at (20.25, 10.25), away from its corners; each run's edge pixel is 0.7 covered)
        let crossing = stroke(vec![vec![[0., 10.25], [40., 10.25]], vec![[40., 10.25], [40., 30.]], vec![[40., 30.], [20.25, 30.]], vec![[20.25, 30.], [20.25, 0.]]], 4.);
        let (x0, y0, width, _, alpha) = render(&crossing, 1.);
        let at = |x: i64, y: i64| alpha[((y - y0) as usize) * width + (x - x0) as usize];
        assert!((at(5, 8) - 0.7).abs() < 1e-5 && (at(18, 3) - 0.7).abs() < 1e-5, "{} {}", at(5, 8), at(18, 3));
        assert!((at(18, 8) - 0.91).abs() < 1e-5, "{}", at(18, 8));
        assert_eq!(at(20, 10), 1.);
        assert_eq!(crossing.shade(&[0.75])[..], [191, 191, 191, 191]);
    }
}
