//! Device geometry and byte rules shared by native draw preparation.
//! The arithmetic order (float32 steps, rounding points) is calibrated against Studio captures: keep it.
use serde_json::Value;
use std::sync::Arc;

pub type Transform = [f64; 6];
pub const IDENTITY: Transform = [1., 0., 0., 1., 0., 0.];
pub const NO_CLIP: [f64; 4] = [-1e9, -1e9, 1e9, 1e9];

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}
impl Rect {
    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }
    pub fn bounds(self) -> [f64; 4] {
        [self.x, self.y, self.x + self.w, self.y + self.h]
    }
    pub fn translated(self, x: f64, y: f64) -> Self {
        Self {
            x: self.x + x,
            y: self.y + y,
            ..self
        }
    }
}

/// Ordered premultiplied RGBA8 layer in node-local device coordinates. The draw builder applies
/// the node transform and ancestor clip. Pixel dimensions may differ from the destination quad.
#[derive(Clone, Debug)]
pub struct RasterQuad {
    pub rect: Rect,
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<[u8]>,
}

#[derive(Clone, Copy, Debug)]
pub struct PhysicalRect {
    pub rect: Rect,
    pub raw: Rect,
    pub snapped: bool,
}

pub fn number(value: &Value, default: f64) -> f64 {
    value.as_f64().unwrap_or(default)
}
pub fn component(value: &Value, index: usize, default: f64) -> f64 {
    number(&value[index], default)
}
pub fn array(value: &Value) -> &[Value] {
    value.as_array().map(Vec::as_slice).unwrap_or(&[])
}
pub fn f32_round(x: f64) -> f64 {
    x as f32 as f64
}
pub fn snap(x: f64) -> f64 {
    (x + 0.5).floor()
}
pub fn snap_device(x: f64, scale: f64) -> f64 {
    // Measured: an edge up to 1/960 logical px below k + 0.5 still lands on k + 1 (1/640 device px at 1.5x,
    // 1/960 at 1x: the 1/256 grid with scale * 4 / 15 added before flooring). 1.5x: e <= 0.0015 up, e >= 0.00159
    // down at device x 150 to 1600 (fixtures/image-probe-ties.luau, image-probe-ties-far.luau); 1x: e <= 0.0010
    // up, e >= 0.0012 down on all 81 rows / columns of fixtures/snap-probe-ties1x.luau (the 1.5x constant 0.4:
    // 162 mismatches -> 0).
    ((x * 256. + scale * 4. / 15.).floor() + 128.).div_euclid(256.)
}
pub fn device_span(position: f64, size: f64, scale: f64) -> [f64; 2] {
    let start = f32_round(f32_round(position) * f32_round(scale));
    [
        snap_device(start, scale),
        snap_device(
            f32_round(start + f32_round(f32_round(size) * f32_round(scale))),
            scale,
        ),
    ]
}
/// A Color3 channel or opacity as Studio's 8-bit vertex value, min(255, floor(x * 256)). Measured: ImageColor3 is
/// quantized this way before it multiplies the texels; transparencies likewise (alpha8, fixtures/group-blend.luau).
pub fn quantize(x: f64) -> u8 {
    (x * 256.).floor().clamp(0., 255.) as u8
}
pub fn alpha8(x: f64) -> u8 {
    quantize(x)
}
/// A gradient stop's colour byte: round(c * 255), where element colours use quantize's floor(c * 256)
/// (notes/interaction.md "Colour quantization": UIGradient keypoints are rounded).
pub fn stop_byte(x: f64) -> u8 {
    (x * 255. + 0.5).floor().clamp(0., 255.) as u8
}
pub fn gpu8(x: f64) -> u8 {
    // floor((floor(x * 4096) * 255 + 2047) / 4096) in integers: every intermediate is exact in f64
    // (below 2^21), so this is the same value; NaN clamps to NaN and casts to 0 either way
    let steps = (x.clamp(0., 1.) * 4096.) as u32;
    ((steps * 255 + 2047) >> 12) as u8
}

#[cfg(test)]
mod gpu8_tests {
    #[test]
    fn integer_gpu8_matches_the_float_formula() {
        let float = |x: f64| (((x.clamp(0., 1.) * 4096.).floor() * 255. + 2047.) / 4096.).floor() as u8;
        let mut x = -0.01f64;
        while x <= 1.01 {
            assert_eq!(super::gpu8(x), float(x), "{x}");
            for ulps in [-1i64, 1] {
                let y = f64::from_bits((x.to_bits() as i64 + ulps) as u64);
                assert_eq!(super::gpu8(y), float(y), "{y}");
            }
            x += 1.0 / 65536.0;
        }
        for k in 0..=4096u32 {
            let x = k as f64 / 4096.0;
            assert_eq!(super::gpu8(x), float(x));
        }
        assert_eq!(super::gpu8(f64::NAN), float(f64::NAN));
    }
}
pub fn mul_div255(a: u8, b: u8) -> u8 {
    ((u32::from(a) * u32::from(b) + 127) / 255) as u8
}
pub fn css_alpha_byte(alpha: f64) -> u8 {
    let a = alpha.clamp(0., 1.);
    if a < 1e-6 {
        return 0;
    }
    // Blink's one-decimal lookup differs from multiplication at 0.7.
    const TENTHS: [u8; 10] = [0, 26, 51, 77, 102, 128, 153, 179, 204, 230];
    for (i, byte) in TENTHS.iter().enumerate() {
        if a == i as f64 / 10. {
            return *byte;
        }
    }
    let text = a.to_string();
    let Some((whole, decimals)) = text.split_once('.') else {
        return stop_byte(a);
    };
    let value = if decimals.len() >= 6 {
        let mut fraction = 0u32;
        for i in 0..7 {
            fraction = fraction * 10
                + decimals
                    .as_bytes()
                    .get(i)
                    .map(|c| u32::from(c - b'0'))
                    .unwrap_or(0);
        }
        whole.parse::<f64>().unwrap_or(0.) + fraction as f64 * 0.000000100000000000000009
    } else {
        a
    };
    stop_byte(value)
}
pub fn premultiply(bytes: &mut [u8]) {
    for p in bytes.chunks_exact_mut(4) {
        for c in 0..3 {
            p[c] = mul_div255(p[c], p[3]);
        }
    }
}
pub fn solid_paint(color: [f64; 3], alpha: f64) -> [f32; 4] {
    let a = css_alpha_byte(alpha);
    [
        mul_div255(quantize(color[0]), a) as f32,
        mul_div255(quantize(color[1]), a) as f32,
        mul_div255(quantize(color[2]), a) as f32,
        a as f32,
    ]
}
pub fn color(value: &Value, default: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| component(value, i, default[i]))
}
pub fn radius(value: &Value, scale: f64) -> [f64; 4] {
    if value.is_array() {
        std::array::from_fn(|i| component(value, i, 0.) * scale)
    } else {
        [number(value, 0.) * scale; 4]
    }
}
pub fn physical_rect(node: &Value, scale: f64) -> PhysicalRect {
    let v = &node["rect"];
    let [x, y, w, h] = std::array::from_fn(|i| component(v, i, 0.));
    let raw = Rect::new(
        f32_round(f32_round(x) * f32_round(scale)),
        f32_round(f32_round(y) * f32_round(scale)),
        f32_round(f32_round(w) * f32_round(scale)),
        f32_round(f32_round(h) * f32_round(scale)),
    );
    if node["matrix"].is_array() {
        PhysicalRect {
            rect: raw,
            raw,
            snapped: false,
        }
    } else {
        let [l, r] = device_span(x, w, scale);
        let [t, b] = device_span(y, h, scale);
        PhysicalRect {
            rect: Rect::new(l, t, r - l, b - t),
            raw,
            snapped: true,
        }
    }
}
pub fn multiply(m: Transform, n: Transform) -> Transform {
    [
        m[0] * n[0] + m[2] * n[1],
        m[1] * n[0] + m[3] * n[1],
        m[0] * n[2] + m[2] * n[3],
        m[1] * n[2] + m[3] * n[3],
        m[0] * n[4] + m[2] * n[5] + m[4],
        m[1] * n[4] + m[3] * n[5] + m[5],
    ]
}
pub fn invert(m: Transform) -> Transform {
    let d = m[0] * m[3] - m[1] * m[2];
    [
        m[3] / d,
        -m[1] / d,
        -m[2] / d,
        m[0] / d,
        (m[2] * m[5] - m[3] * m[4]) / d,
        (m[1] * m[4] - m[0] * m[5]) / d,
    ]
}
pub fn apply(m: Transform, x: f64, y: f64) -> [f64; 2] {
    [m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]]
}
/// A turned quad as the rasterizer holds it: each corner of the local rect `b` (x0, y0, x1, y1), carried to the
/// screen by `m` (which already subtracts `origin`), lands on the 1/256 px grid, and what is interpolated over
/// the quad runs over its two triangles, (top left, top right, bottom left) and (bottom left, top right,
/// bottom right). Returns the two triangles' maps from the screen back to local space and the line through the
/// shared diagonal as (a, b, c) with a x + b y + c >= 0 on the first triangle's side, x and y measured from
/// `anchor`. Measured on a 16 x 16 Pixelated texture over 150 px turned 20 degrees (fixtures/image-rotation
/// MotorPix): four pixel centres lie 0.0005 px from a texel boundary, and the snapped corners move exactly those
/// four to the other texel; rounding down instead moves eight others.
pub fn snapped_quad_maps(m: Transform, b: [f64; 4], origin: [f64; 2], anchor: [f64; 2]) -> ([Transform; 2], [f64; 3]) {
    let corner = |x: f64, y: f64| {
        let p = apply(m, x, y);
        [
            ((p[0] + origin[0]) * 256.).round() / 256. - origin[0],
            ((p[1] + origin[1]) * 256.).round() / 256. - origin[1],
        ]
    };
    let (tl, tr, bl, br) = (corner(b[0], b[1]), corner(b[2], b[1]), corner(b[0], b[3]), corner(b[2], b[3]));
    let (w, h) = (b[2] - b[0], b[3] - b[1]);
    // The map taking the local rect's sides to the two given screen vectors, the local point `at` to `to`
    let map = |across: [f64; 2], down: [f64; 2], at: [f64; 2], to: [f64; 2]| -> Transform {
        let (m0, m1, m2, m3) = (across[0] / w, across[1] / w, down[0] / h, down[1] / h);
        [m0, m1, m2, m3, to[0] - m0 * at[0] - m2 * at[1], to[1] - m1 * at[0] - m3 * at[1]]
    };
    let first = map([tr[0] - tl[0], tr[1] - tl[1]], [bl[0] - tl[0], bl[1] - tl[1]], [b[0], b[1]], tl);
    let second = map([br[0] - bl[0], br[1] - bl[1]], [br[0] - tr[0], br[1] - tr[1]], [b[2], b[3]], br);
    let (mut la, mut lb) = (bl[1] - tr[1], -(bl[0] - tr[0]));
    if la * (tl[0] - tr[0]) + lb * (tl[1] - tr[1]) < 0. {
        (la, lb) = (-la, -lb);
    }
    let lc = -(la * (tr[0] - anchor[0]) + lb * (tr[1] - anchor[1]));
    ([invert(first), invert(second)], [la, lb, lc])
}
pub fn node_transform(node: &Value, origin: [f64; 2], scale: f64) -> Transform {
    let base = [1., 0., 0., 1., -origin[0], -origin[1]];
    if !node["matrix"].is_array() {
        return base;
    }
    let mut m = std::array::from_fn(|i| component(&node["matrix"], i, IDENTITY[i]));
    m[4] *= scale;
    m[5] *= scale;
    multiply(base, m)
}
pub fn clip_rect(node: &Value, origin: [f64; 2], scale: f64) -> Option<[f64; 4]> {
    let v = node.get("clip")?.as_array()?;
    if v.len() < 4 {
        return None;
    }
    let [l, r] = device_span(number(&v[0], 0.), number(&v[2], 0.), scale);
    let [t, b] = device_span(number(&v[1], 0.), number(&v[3], 0.), scale);
    Some([
        l - origin[0],
        t - origin[1],
        l.max(r) - origin[0],
        t.max(b) - origin[1],
    ])
}
/// The clip of a node's text: its clip, cut by its own rect when the object clips its descendants
/// (`text.clip`, measured on fixtures/text-probe-selfclip.luau: the own rect, axis-aligned in screen space
/// at `absolute` when present, also for rotated labels; a zero-size own rect draws nothing).
pub fn text_clip_rect(node: &Value, origin: [f64; 2], scale: f64) -> Option<[f64; 4]> {
    let clip = clip_rect(node, origin, scale);
    if node["text"]["clip"] != Value::Bool(true) {
        return clip;
    }
    let rect = &node["rect"];
    let (x, y) = match node["absolute"].as_array() {
        Some(a) if a.len() >= 2 => (number(&a[0], 0.), number(&a[1], 0.)),
        _ => (number(&rect[0], 0.), number(&rect[1], 0.)),
    };
    let [l, r] = device_span(x, number(&rect[2], 0.), scale);
    let [t, b] = device_span(y, number(&rect[3], 0.), scale);
    let own = [l - origin[0], t - origin[1], l.max(r) - origin[0], t.max(b) - origin[1]];
    Some(match clip {
        None => own,
        Some(c) => {
            let (x0, y0) = (own[0].max(c[0]), own[1].max(c[1]));
            [x0, y0, own[2].min(c[2]).max(x0), own[3].min(c[3]).max(y0)]
        }
    })
}
pub fn transformed_extent(m: Transform, r: Rect) -> [f64; 4] {
    let mut b = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for (x, y) in [
        (r.x, r.y),
        (r.x + r.w, r.y),
        (r.x, r.y + r.h),
        (r.x + r.w, r.y + r.h),
    ] {
        let p = apply(m, x, y);
        b[0] = b[0].min(p[0]);
        b[1] = b[1].min(p[1]);
        b[2] = b[2].max(p[0]);
        b[3] = b[3].max(p[1]);
    }
    b
}
pub fn transformed_bounds(m: Transform, r: Rect, pad: f64) -> [f64; 4] {
    let b = transformed_extent(m, r);
    [
        b[0].floor() - pad,
        b[1].floor() - pad,
        b[2].ceil() + pad,
        b[3].ceil() + pad,
    ]
}
pub fn local_texels(m: Transform, origin: [f64; 2], shift: [f64; 2]) -> [[f64; 4]; 2] {
    [
        [m[0], m[1], m[2], m[3]],
        [
            m[0] * origin[0] + m[2] * origin[1] + m[4] - shift[0],
            m[1] * origin[0] + m[3] * origin[1] + m[5] - shift[1],
            origin[0],
            origin[1],
        ],
    ]
}
/// Studio's float32 logical size of a group axis whose snapped quad is [start, start + q) device px,
/// L = f32(end / DPI) - f32(start / DPI).
fn group_logical_size(q: f64, scale: f64, start: f64) -> f64 {
    let d = scale as f32;
    (((start + q) as f32 / d) - (start as f32 / d)) as f64
}

/// CanvasGroup texture size along an axis: whole logical px, N = ceil(ceil(L) * DPI) with L the float32 logical
/// size (group_logical_size; notes/images.md round 6: [320, 452) at 1.5 DPI is 88.000015 logical -> N = 134), with
/// the large-group factor `fit` (group_texture_fit) N = ceil(ceil(L * fit) * DPI) when fit < 1.
pub fn group_texture_size_fit(q: f64, scale: f64, start: f64, fit: f64) -> u32 {
    let l = group_logical_size(q, scale, start);
    if fit < 1.0 {
        return (((l * fit - 1e-6).ceil() * scale - 1e-9).ceil()).max(1.0) as u32;
    }
    q.max((l.ceil() * scale - 1e-9).ceil()) as u32
}

/// Large CanvasGroups get a smaller texture (fixtures/group-probe-bigtex.luau, notes/images.md round 11): both logical
/// sides scale by one factor so that neither device side exceeds 2048 px and the area stays within 2048 x 1024 px.
pub fn group_texture_fit(w: f64, h: f64, x: f64, y: f64, scale: f64) -> f64 {
    const LIMIT: f64 = 2048.0;
    const AREA: f64 = 2048.0 * 1024.0;
    let w = group_logical_size(w, scale, x) * scale;
    let h = group_logical_size(h, scale, y) * scale;
    let mut fit = 1.0f64;
    if w.max(h) > LIMIT {
        fit = LIMIT / w.max(h);
    }
    if w * h > AREA {
        fit = fit.min((AREA / (w * h)).sqrt());
    }
    fit
}

/// The texture size (nw, nh) of an axis-aligned CanvasGroup whose snapped quad is (x, y, w, h) device px.
pub fn group_texture_dims(x: f64, y: f64, w: f64, h: f64, scale: f64) -> (u32, u32) {
    let fit = group_texture_fit(w, h, x, y, scale);
    (group_texture_size_fit(w, scale, x, fit), group_texture_size_fit(h, scale, y, fit))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn byte_rules() {
        assert_eq!(quantize(0.75), 192);
        assert_eq!(css_alpha_byte(0.7), 179);
        assert_eq!(css_alpha_byte(0.08431372549019608), 21);
        assert_eq!(mul_div255(128, 128), 64);
    }
    #[test]
    fn group_texture_limits() {
        // a wide group of orbit rings (area limit), probe rows (side limit), a prompt's float32 column
        assert_eq!(group_texture_dims(-262., 74., 2324., 1202., 1.5), (2015, 1043));
        assert_eq!(group_texture_dims(0., 333., 2100., 75., 1.5), (2049, 74));
        assert_eq!(group_texture_dims(0., 0., 4200., 75., 1.5), (2049, 38));
        assert_eq!(group_texture_dims(320., 0., 132., 40., 1.5), (134, 41));
    }
    #[test]
    fn turned_quads_sit_on_the_subpixel_grid() {
        // fixtures/image-rotation MotorPix at device scale 1.5: 150 px turned 20 degrees, 16 texels a side.
        // The centre of pixel (1078, 55) lies 0.0005 px right of the boundary of texels 6 and 7 on the exact
        // quad and Studio draws texel 6: the corners on the 1/256 px grid put it there
        let m = [0.939693, 0.34202, -0.34202, 0.939693, 105.26977155, -357.0145575];
        let b = [990., 45., 1140., 195.];
        let texel = |inv: Transform, x: f64, y: f64| ((apply(inv, x, y)[0] - 990.) / 150. * 16.).floor();
        assert_eq!(texel(invert(m), 1078.5, 55.5), 7.);
        let (maps, diagonal) = snapped_quad_maps(m, b, [0., 0.], [0., 0.]);
        assert!(diagonal[0] * 1078.5 + diagonal[1] * 55.5 + diagonal[2] >= 0.);
        assert_eq!(texel(maps[0], 1078.5, 55.5), 6.);
        // (1129, 133) lies in the second triangle and moves from row 7 to row 6
        let row = |inv: Transform, x: f64, y: f64| ((apply(inv, x, y)[1] - 45.) / 150. * 16.).floor();
        assert!(diagonal[0] * 1129.5 + diagonal[1] * 133.5 + diagonal[2] < 0.);
        assert_eq!(row(invert(m), 1129.5, 133.5), 7.);
        assert_eq!(row(maps[1], 1129.5, 133.5), 6.);
        // Both maps take the ends of the shared diagonal to the same local points
        for (local, map) in [([1140., 45.], maps[0]), ([1140., 45.], maps[1]), ([990., 195.], maps[0]), ([990., 195.], maps[1])] {
            let screen = apply(m, local[0], local[1]);
            let p = apply(map, screen[0], screen[1]);
            assert!((p[0] - local[0]).abs() < 0.01 && (p[1] - local[1]).abs() < 0.01);
        }
    }
    #[test]
    fn device_ties() {
        assert_eq!(snap_device(10.499, 1.5), 11.);
        assert_eq!(snap_device(10.498, 1.5), 10.);
        assert_eq!(device_span(186.833328, 133.5, 1.5), [280., 481.]);
        assert_eq!(snap_device(-0.5, 1.5), 0.);
        // 1x (fixtures/snap-probe-ties1x.luau): e <= 0.0010 up, e >= 0.0012 down
        assert_eq!(snap_device(20.499, 1.), 21.);
        assert_eq!(snap_device(20.4988, 1.), 20.);
        assert_eq!(snap_device(423.498779296875, 1.), 423.);
    }
    #[test]
    fn text_own_clip() {
        let plain = serde_json::json!({"rect":[10,20,0,40],"clip":[0,0,100,100],"text":{}});
        assert_eq!(text_clip_rect(&plain, [0., 0.], 1.5), Some([0., 0., 150., 150.]));
        // A zero-width clipping label: an empty cut (no text), intersected with the inherited clip
        let zero = serde_json::json!({"rect":[10,20,0,40],"clip":[0,0,100,100],"text":{"clip":true}});
        assert_eq!(text_clip_rect(&zero, [0., 0.], 1.5), Some([15., 30., 15., 90.]));
        // Rotated under an ancestor: the axis-aligned rect at `absolute`
        let rotated = serde_json::json!({"rect":[10,20,60,30],"absolute":[40,50],"matrix":[0.9,0.4,-0.4,0.9,0,0],"text":{"clip":true}});
        assert_eq!(text_clip_rect(&rotated, [4., 0.], 1.5), Some([56., 75., 146., 120.]));
    }
    #[test]
    fn inverse_roundtrip() {
        let m = [0.8, 0.6, -0.6, 0.8, 12., -23.];
        let p = apply(m, 100., 50.);
        let q = apply(invert(m), p[0], p[1]);
        assert!((q[0] - 100.).abs() < 1e-10 && (q[1] - 50.).abs() < 1e-10);
    }
}
