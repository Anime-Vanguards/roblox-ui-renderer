//! Chrome 153 Skia linear-gradient setup in float32 (notes/gpu.md, canvas linear gradients) and the
//! uber-shader's paint blocks.
use super::geometry::{
    Rect, Transform, array, component, css_alpha_byte, f32_round as f, mul_div255, number,
    solid_paint, stop_byte,
};
use serde_json::Value;

type Color = [f64; 4];
const NEARLY_ZERO: f64 = 1.0 / 4096.0;
fn nearly(a: f64, b: f64) -> bool {
    f(a - b).abs() <= NEARLY_ZERO
}
fn sample(keys: &Value, t: f64, channel: usize, default: f64) -> f64 {
    let keys = array(keys);
    if keys.is_empty() {
        return default;
    }
    if t <= component(&keys[0], 0, 0.0) {
        return component(&keys[0], channel, default);
    }
    for pair in keys.windows(2) {
        let a = component(&pair[0], 0, 0.0);
        let b = component(&pair[1], 0, 0.0);
        if t <= b {
            let w = if b == a { 0.0 } else { (t - a) / (b - a) };
            let v = component(&pair[0], channel, default);
            return v + (component(&pair[1], channel, default) - v) * w;
        }
    }
    component(keys.last().unwrap(), channel, default)
}

#[derive(Clone, Copy, Debug)]
pub struct GradientLine {
    pub cx: f64,
    pub cy: f64,
    pub dx: f64,
    pub dy: f64,
    pub span: f64,
}
pub fn gradient_line(gradient: &Value, rect: Rect) -> GradientLine {
    let angle = number(&gradient["rotation"], 0.0) * std::f64::consts::PI / 180.0;
    let (dx, dy) = (super::trig::cos(angle), super::trig::sin(angle));
    let cx = rect.x + rect.w / 2.0 + component(&gradient["offset"], 0, 0.0) * rect.w;
    let cy = rect.y + rect.h / 2.0 + component(&gradient["offset"], 1, 0.0) * rect.h;
    let span = (if dx.abs() > 1e-9 {
        rect.w / dx.abs()
    } else {
        f64::INFINITY
    })
    .min(if dy.abs() > 1e-9 {
        rect.h / dy.abs()
    } else {
        f64::INFINITY
    });
    GradientLine {
        cx,
        cy,
        dx,
        dy,
        span,
    }
}
pub fn gradient_mapping(line: GradientLine, to_user: Transform) -> [f64; 3] {
    let [a, b, c, d, e, f] = to_user;
    [
        (a * line.dx + b * line.dy) / line.span,
        (c * line.dx + d * line.dy) / line.span,
        ((e - line.cx) * line.dx + (f - line.cy) * line.dy) / line.span + 0.5,
    ]
}
/// The canvas gradient's stops [t, r8, g8, b8, a8], unpremultiplied as Chrome stores them: one at every Color
/// and Transparency keypoint time plus the 1/32 grid (0, 1/32, ..., 1), sorted and deduplicated; colour
/// stop_byte(C(t) * base), alpha css_alpha_byte(alpha * (1 - T(t))); the offset is clamped to [0, 1] afterwards.
pub fn gradient_stops(g: &Value, base: [f64; 3], alpha: f64) -> Vec<[f64; 5]> {
    let mut times: Vec<f64> = (0..=32).map(|i| i as f64 / 32.0).collect();
    for k in array(&g["color"]).iter().chain(array(&g["transparency"])) {
        times.push(component(k, 0, 0.0));
    }
    times.sort_by(f64::total_cmp);
    times.dedup_by(|a, b| *a == *b);
    times
        .into_iter()
        .map(|t| {
            [
                t.clamp(0.0, 1.0),
                stop_byte(sample(&g["color"], t, 1, 1.0) * base[0]) as f64,
                stop_byte(sample(&g["color"], t, 2, 1.0) * base[1]) as f64,
                stop_byte(sample(&g["color"], t, 3, 1.0) * base[2]) as f64,
                css_alpha_byte(alpha * (1.0 - sample(&g["transparency"], t, 1, 0.0))) as f64,
            ]
        })
        .collect()
}

#[derive(Clone, Debug)]
pub enum Colorizer {
    Single {
        start: Color,
        end: Color,
    },
    Dual {
        scale: [Color; 2],
        bias: [Color; 2],
        threshold: f64,
    },
    Looping {
        thresholds: Vec<f64>,
        scales: Vec<Color>,
        biases: Vec<Color>,
    },
}
#[derive(Clone, Debug)]
pub enum SkiaGradient {
    Empty,
    Solid([u8; 4]),
    Linear {
        matrix: [f64; 6],
        colorizer: Colorizer,
        left: Color,
        right: Color,
        opaque: bool,
    },
}

fn skia_stops(stops: &[[f64; 5]]) -> (Vec<f64>, Vec<Color>) {
    let mut pos: Vec<f64> = stops.iter().map(|s| f(s[0])).collect();
    let mut colors: Vec<Color> = stops
        .iter()
        .map(|s| std::array::from_fn(|c| f(s[c + 1] / 255.0)))
        .collect();
    if pos[0] > 0.0 {
        pos.insert(0, 0.0);
        colors.insert(0, colors[0]);
    }
    if *pos.last().unwrap() < 1.0 {
        pos.push(1.0);
        colors.push(*colors.last().unwrap());
    }
    let mut positions = vec![0.0];
    let mut prev = 0.0;
    let mut uniform = true;
    let step = f(pos[1]);
    for p in pos.iter().skip(1) {
        let curr = p.min(1.0).max(prev);
        uniform &= nearly(step, f(curr - prev));
        positions.push(curr);
        prev = curr;
    }
    if uniform {
        let scale = f(1.0 / (positions.len() - 1) as f64);
        return (
            (0..positions.len()).map(|i| f(i as f64 * scale)).collect(),
            colors,
        );
    }
    let mut out_pos = Vec::new();
    let mut out_colors = Vec::new();
    let mut i = 0;
    for j in 1..=positions.len() {
        if j == positions.len() || positions[i] != positions[j] {
            out_pos.push(positions[i]);
            out_colors.push(colors[i]);
            if j - i > 1 {
                out_pos.push(positions[j - 1]);
                out_colors.push(colors[j - 1]);
            }
            i = j;
        }
    }
    (out_pos, out_colors)
}
/// GrGradientShader make_uniform_colorizer (Chrome 153's Skia): an end stop within 1/4096 of its neighbour is
/// dropped; 2 stops -> single interval; 3 stops (or 4 with the middle two equal) -> dual interval split at the
/// middle position; up to 128 stops -> the looping binary search, scale = (c1 - c0) / dt and bias = c0 - t0 scale
/// in float32, hard stops (dt <= 1/4096) skipped, intervals padded to a power of two (at least 4) by repeating
/// the last. Over 64 intervals (over 30 keypoints off the 1/32 grid) Skia bakes a 256-texel texture instead, and
/// only hard stops or over 128 stops fall back to the end colours: both are approximated here.
fn colorizer(positions: &[f64], colors: &[Color]) -> Colorizer {
    let mut first = 0;
    let mut count = positions.len();
    if nearly(positions[0], positions[1]) {
        first += 1;
        count -= 1;
    }
    if nearly(
        positions[positions.len() - 2],
        positions[positions.len() - 1],
    ) {
        count -= 1;
    }
    let pos = &positions[first..first + count];
    let cols = &colors[first..first + count];
    if count == 2 {
        return Colorizer::Single {
            start: cols[0],
            end: cols[1],
        };
    }
    if count <= 128 {
        let dual = |c0: Color, c1: Color, c2: Color, c3: Color, threshold: f64| {
            let s0 = std::array::from_fn(|k| f(f(c1[k] - c0[k]) / threshold));
            let s1 = std::array::from_fn(|k| f(f(c3[k] - c2[k]) / f(1.0 - threshold)));
            Colorizer::Dual {
                scale: [s0, s1],
                bias: [c0, std::array::from_fn(|k| f(c2[k] - f(threshold * s1[k])))],
                threshold,
            }
        };
        if count == 3 {
            return dual(cols[0], cols[1], cols[1], cols[2], pos[1]);
        }
        if count == 4 && nearly(pos[1], pos[2]) {
            return dual(cols[0], cols[1], cols[2], cols[3], pos[1]);
        }
        let mut thresholds = Vec::new();
        let mut scales = Vec::new();
        let mut biases = Vec::new();
        for i in 0..count.saturating_sub(1) {
            let dt = f(pos[i + 1] - pos[i]);
            if dt.abs() <= NEARLY_ZERO {
                continue;
            }
            let scale: Color = std::array::from_fn(|k| f(f(cols[i + 1][k] - cols[i][k]) / dt));
            thresholds.push(pos[i + 1]);
            scales.push(scale);
            biases.push(std::array::from_fn(
                |k| f(cols[i][k] - f(pos[i] * scale[k])),
            ));
        }
        if !thresholds.is_empty() {
            let size = thresholds.len().next_power_of_two().max(4);
            thresholds.resize(size, *thresholds.last().unwrap());
            scales.resize(size, *scales.last().unwrap());
            biases.resize(size, *biases.last().unwrap());
            return Colorizer::Looping {
                thresholds,
                scales,
                biases,
            };
        }
    }
    Colorizer::Single {
        start: cols[0],
        end: cols[count - 1],
    }
}

/// A canvas linear gradient from (x0, y0) to (x1, y1), set up in float32 like Skia: equal points paint nothing
/// (Empty); |p1 - p0| <= 1/32768 is Skia's degenerate gradient, the last stop's colour (Solid); otherwise the
/// pts-to-unit matrix (setSinCos(-v.y, v.x), translate by -p0, scale by 1 / |v|, the scale skipped when
/// 1 / |v| is exactly 1), the colorizer, and the border colours premultiplied unless every stop is opaque.
pub fn skia_linear_gradient(
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    stops: &[[f64; 5]],
) -> SkiaGradient {
    if stops.is_empty() {
        return SkiaGradient::Empty;
    }
    if stops.len() == 1 {
        return SkiaGradient::Solid(std::array::from_fn(|k| stops[0][k + 1] as u8));
    }
    let (p0x, p0y, p1x, p1y) = (f(x0), f(y0), f(x1), f(y1));
    if p0x == p1x && p0y == p1y {
        return SkiaGradient::Empty;
    }
    let mut vx = f(p1x - p0x);
    let mut vy = f(p1y - p0y);
    let mag = f(f(f(vx * vx) + f(vy * vy)).sqrt());
    if mag <= 1.0 / 32768.0 {
        return SkiaGradient::Solid(std::array::from_fn(|k| stops.last().unwrap()[k + 1] as u8));
    }
    let inv = f(1.0 / mag);
    vx = f(vx * inv);
    vy = f(vy * inv);
    let sin = f(-vy);
    let cos = vx;
    let one_minus = f(1.0 - cos);
    let tx = f(f(f(sin * p0y) + f(one_minus * p0x)) - p0x);
    let ty = f(f(f(f(-sin) * p0x) + f(one_minus * p0y)) - p0y);
    let matrix = if inv == 1.0 {
        [cos, f(-sin), tx, sin, cos, ty]
    } else {
        [
            f(inv * cos),
            f(inv * f(-sin)),
            f(inv * tx),
            f(inv * sin),
            f(inv * cos),
            f(inv * ty),
        ]
    };
    let (positions, colors) = skia_stops(stops);
    let opaque = colors.iter().all(|c| nearly(c[3], 1.0));
    let premul = |c: Color| {
        if opaque {
            c
        } else {
            [f(c[0] * c[3]), f(c[1] * c[3]), f(c[2] * c[3]), c[3]]
        }
    };
    SkiaGradient::Linear {
        matrix,
        colorizer: colorizer(&positions, &colors),
        left: premul(colors[0]),
        right: premul(*colors.last().unwrap()),
        opaque,
    }
}

#[derive(Clone, Debug)]
pub enum Paint {
    Solid([f32; 4]),
    Gradient {
        skia: SkiaGradient,
        to_local: Transform,
        dither: [f64; 2],
        varying: bool,
    },
}
impl Paint {
    pub fn solid(colour: [f64; 3], alpha: f64) -> Self {
        Self::Solid(solid_paint(colour, alpha))
    }
    pub fn gradient(
        g: &Value,
        raw: Rect,
        colour: [f64; 3],
        alpha: f64,
        to_local: Transform,
        dither: [f64; 2],
        varying: bool,
    ) -> Self {
        if raw.w <= 0.0 || raw.h <= 0.0 {
            let a = css_alpha_byte(alpha * (1.0 - sample(&g["transparency"], 0.0, 1, 0.0)));
            let c: [u8; 3] = std::array::from_fn(|k| {
                stop_byte(sample(&g["color"], 0.0, k + 1, 1.0) * colour[k])
            });
            return Self::Solid([
                mul_div255(c[0], a) as f32,
                mul_div255(c[1], a) as f32,
                mul_div255(c[2], a) as f32,
                a as f32,
            ]);
        }
        let line = gradient_line(g, raw);
        let skia = skia_linear_gradient(
            line.cx - line.dx * line.span / 2.0,
            line.cy - line.dy * line.span / 2.0,
            line.cx + line.dx * line.span / 2.0,
            line.cy + line.dy * line.span / 2.0,
            &gradient_stops(g, colour, alpha),
        );
        if let SkiaGradient::Solid([r, g, b, a]) = skia {
            return Self::Solid([
                mul_div255(r, a) as f32,
                mul_div255(g, a) as f32,
                mul_div255(b, a) as f32,
                a as f32,
            ]);
        }
        Self::Gradient {
            skia,
            to_local,
            dither,
            varying,
        }
    }
    /// Write the two fixed paint texels; finish after the shape's remaining fixed fields.
    pub fn write(&self, texels: &mut Vec<[f32; 4]>) -> Option<usize> {
        match self {
            Self::Solid([r, g, b, a]) => {
                texels.push([0.0, *r, *g, *b]);
                texels.push([*a, 0.0, 0.0, 0.0]);
                None
            }
            Self::Gradient { dither, .. } => {
                let at = texels.len();
                texels.push([1.0, 0.0, -1.0, -1.0]);
                texels.push([0.0, dither[0] as f32, dither[1] as f32, 0.0]);
                Some(at)
            }
        }
    }
    pub fn finish(&self, texels: &mut Vec<[f32; 4]>, start: usize, pending: Option<usize>) {
        let (
            Some(at),
            Self::Gradient {
                skia,
                to_local,
                varying,
                ..
            },
        ) = (pending, self)
        else {
            return;
        };
        let offset = (texels.len() - start) as f32;
        texels[at][1] = offset;
        if *varying && !matches!(skia, SkiaGradient::Empty) {
            texels[start + 2][3] = offset;
        }
        push_gradient_block(texels, skia, *to_local, *varying);
    }
    pub fn append(&self, texels: &mut Vec<[f32; 4]>, start: usize) {
        let pending = self.write(texels);
        self.finish(texels, start, pending);
    }
}

pub fn push_gradient_block(
    texels: &mut Vec<[f32; 4]>,
    skia: &SkiaGradient,
    to_local: Transform,
    varying: bool,
) {
    let SkiaGradient::Linear {
        matrix: m,
        colorizer,
        left,
        right,
        opaque,
    } = skia
    else {
        texels.push([0.0; 4]);
        return;
    };
    let kind = match colorizer {
        Colorizer::Single { .. } => 1,
        Colorizer::Dual { .. } => 2,
        Colorizer::Looping { .. } => 3,
    };
    let intervals = match colorizer {
        Colorizer::Looping { thresholds, .. } => thresholds.len(),
        _ => 0,
    };
    texels.push([
        kind as f32,
        ((*opaque as u8) | if varying { 2 } else { 0 }) as f32,
        intervals as f32,
        0.0,
    ]);
    let mut push = |v: [f64; 4]| texels.push(v.map(|v| v as f32));
    push([m[0], m[3], m[1], m[4]]);
    push([m[2], m[5], 0.0, 0.0]);
    push([to_local[0], to_local[1], to_local[2], to_local[3]]);
    push([to_local[4], to_local[5], 0.0, 0.0]);
    push(*left);
    push(*right);
    match colorizer {
        Colorizer::Single { start, end } => {
            push(*start);
            push(*end);
        }
        Colorizer::Dual {
            scale,
            bias,
            threshold,
        } => {
            push(scale[0]);
            push(scale[1]);
            push(bias[0]);
            push(bias[1]);
            push([*threshold, 0.0, 0.0, 0.0]);
        }
        Colorizer::Looping {
            thresholds,
            scales,
            biases,
        } => {
            for t in thresholds.chunks_exact(4) {
                push(t.try_into().unwrap());
            }
            for s in scales {
                push(*s);
            }
            for b in biases {
                push(*b);
            }
        }
    }
}
