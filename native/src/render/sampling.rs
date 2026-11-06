//! Studio's measured image sampling (notes/images.md "Measured rules"): 8-bit bilinear weights, 12-bit
//! truncation floor(v / 255 * 4096) / 4096 (exact on a texel centre), trilinear with an 8-bit LOD fraction and
//! derivative length max + 0.375 min, 2:1 mips horizontal pass first, Pixelated = nearest mip round(LOD) and
//! nearest texel (an exact boundary picks the lower texel).
pub use super::geometry::{alpha8, gpu8, premultiply as premultiply_bytes, quantize, snap_device};
use std::{collections::HashMap, sync::Arc};

#[derive(Clone, Debug)]
pub struct MipLevel {
    pub width: usize,
    pub height: usize,
    /// Straight RGBA8, including RGB in transparent texels.
    pub data: Arc<[u8]>,
}

pub fn texture_levels(width: usize, height: usize, data: Arc<[u8]>) -> Vec<MipLevel> {
    if width == 0
        || height == 0
        || width.checked_mul(height).and_then(|n| n.checked_mul(4)) != Some(data.len())
    {
        return Vec::new();
    }
    let mut levels = vec![MipLevel {
        width,
        height,
        data,
    }];
    while levels.last().is_some_and(|l| l.width > 1 || l.height > 1) {
        // Measured: a level with both sides even averages texel pairs rounding half up ((a + b + 1) >> 1:
        // 1000 x 1000 / 1024 x 1024 icons bit-exact); a level with an odd side takes the floored 2-tap path on
        // both axes, whose even axis rounds down ((a + b) >> 1: an item grid's 442 x 677
        // icon 236 -> 2 px > 2; notes/images.md)
        let last = levels.last().unwrap();
        let round_up = u16::from(last.width % 2 == 0 && last.height % 2 == 0);
        let next = halve(&halve(last, true, round_up), false, round_up);
        levels.push(next);
    }
    levels
}

fn halve(level: &MipLevel, horizontal: bool, round_up: u16) -> MipLevel {
    let (w, h) = (level.width, level.height);
    let n = if horizontal { w } else { h };
    if n <= 1 {
        return level.clone();
    }
    let m = n / 2;
    let even = n == m * 2;
    let (nw, nh) = if horizontal { (m, h) } else { (w, m) };
    let mut out = vec![0; nw * nh * 4];
    for k in 0..m {
        let (a0, a1, f) = if even {
            (2 * k, 2 * k + 1, 0.0)
        } else {
            let pos = (k as f64 + 0.5) * n as f64 / m as f64 - 0.5;
            let i = pos.floor() as usize;
            (i, (i + 1).min(n - 1), pos - i as f64)
        };
        for line in 0..if horizontal { h } else { w } {
            let a = (if horizontal {
                line * w + a0
            } else {
                a0 * w + line
            }) * 4;
            let b = (if horizontal {
                line * w + a1
            } else {
                a1 * w + line
            }) * 4;
            let o = (if horizontal {
                line * nw + k
            } else {
                k * nw + line
            }) * 4;
            for c in 0..4 {
                out[o + c] = if even {
                    ((level.data[a + c] as u16 + level.data[b + c] as u16 + round_up) / 2) as u8
                } else {
                    (level.data[a + c] as f64
                        + (level.data[b + c] as f64 - level.data[a + c] as f64) * f)
                        .floor() as u8
                };
            }
        }
    }
    MipLevel {
        width: nw,
        height: nh,
        data: out.into(),
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SamplingGrid {
    pub off_x: f64,
    pub off_y: f64,
    pub span_x: f64,
    pub span_y: f64,
}
impl SamplingGrid {
    pub fn pixels(width: usize, height: usize) -> Self {
        Self {
            off_x: 0.5,
            off_y: 0.5,
            span_x: width as f64,
            span_y: height as f64,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SampleQuad {
    pub u0: f64,
    pub v0: f64,
    pub u1: f64,
    pub v1: f64,
    pub lod: f64,
    pub wrap: bool,
    pub pixelated: bool,
}

fn fix_index(i: f64, size: usize, wrap: bool) -> usize {
    if wrap {
        ((i % size as f64 + size as f64) % size as f64) as usize
    } else {
        i.max(0.0).min(size.saturating_sub(1) as f64) as usize
    }
}

fn bilinear_tap_at(scaled: f64, size: usize, wrap: bool) -> [f32; 4] {
    let s = scaled - 0.5;
    let mut base = s.floor();
    let mut weight = ((s - base) * 256.0 + 0.5).floor();
    if weight == 256.0 {
        base += 1.0;
        weight = 0.0;
    }
    [
        fix_index(base, size, wrap) as f32,
        fix_index(base + 1.0, size, wrap) as f32,
        weight as f32,
        0.0,
    ]
}

/// Packed GPU-compatible [first index, second index, weight/256 numerator, 0].
pub fn taps(
    pixelated: bool,
    count: usize,
    t0: f64,
    t1: f64,
    size: usize,
    wrap: bool,
    off: f64,
    span: f64,
) -> Vec<[f32; 4]> {
    let mut out = vec![[0.0; 4]; count];
    if size == 0 || span == 0.0 {
        return out;
    }
    for (i, tap) in out.iter_mut().enumerate() {
        let s = (t0 + (i as f64 + off) / span * (t1 - t0)) * size as f64;
        if pixelated {
            tap[0] = fix_index(s.ceil() - 1.0, size, wrap) as f32;
        } else {
            *tap = bilinear_tap_at(s, size, wrap);
        }
    }
    out
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct TapKey {
    pixelated: bool,
    count: usize,
    size: usize,
    wrap: bool,
    values: [u64; 4],
}
struct TapBlock {
    data: Arc<[[f32; 4]]>,
    used: u64,
    bytes: usize,
}
pub struct TapCache {
    blocks: HashMap<TapKey, TapBlock>,
    bytes: usize,
    clock: u64,
}
impl Default for TapCache {
    fn default() -> Self {
        Self {
            blocks: HashMap::new(),
            bytes: 0,
            clock: 0,
        }
    }
}
impl TapCache {
    pub fn clear(&mut self) {
        self.blocks.clear();
        self.bytes = 0;
    }
    pub fn bytes(&self) -> usize {
        self.bytes
    }
    pub fn get(
        &mut self,
        pixelated: bool,
        count: usize,
        t0: f64,
        t1: f64,
        size: usize,
        wrap: bool,
        off: f64,
        span: f64,
    ) -> Arc<[[f32; 4]]> {
        self.clock = self.clock.wrapping_add(1);
        let key = TapKey {
            pixelated,
            count,
            size,
            wrap,
            values: [t0.to_bits(), t1.to_bits(), off.to_bits(), span.to_bits()],
        };
        if let Some(block) = self.blocks.get_mut(&key) {
            block.used = self.clock;
            return block.data.clone();
        }
        let data: Arc<[[f32; 4]]> = taps(pixelated, count, t0, t1, size, wrap, off, span).into();
        let bytes = data.len() * 16 + 192;
        const BUDGET: usize = 8 * 1024 * 1024;
        if bytes <= BUDGET {
            while self.bytes + bytes > BUDGET || self.blocks.len() >= 1024 {
                let oldest = *self.blocks.iter().min_by_key(|(_, b)| b.used).unwrap().0;
                self.bytes -= self.blocks.remove(&oldest).unwrap().bytes;
            }
            self.bytes += bytes;
            self.blocks.insert(
                key,
                TapBlock {
                    data: data.clone(),
                    used: self.clock,
                    bytes,
                },
            );
        }
        data
    }
}

/// Returns the low mip level and the 8-bit trilinear weight numerator.
pub fn mip_levels(count: usize, lod: f64) -> (usize, u16) {
    let last = count.saturating_sub(1);
    let clamped = (if lod.is_finite() { lod } else { 0.0 })
        .max(0.0)
        .min(last as f64);
    let mut l0 = clamped.floor() as usize;
    let mut f = ((clamped - l0 as f64) * 256.0 + 0.5).floor() as u16;
    if f == 256 {
        l0 += 1;
        f = 0;
    }
    if l0 >= last { (last, 0) } else { (l0, f) }
}
pub fn pixelated_level(count: usize, lod: f64) -> usize {
    ((lod.max(0.0) + 0.5).floor() as usize).min(count.saturating_sub(1))
}
pub fn derivative_length(a: f64, b: f64) -> f64 {
    a.abs().max(b.abs()) + 0.375 * a.abs().min(b.abs())
}
pub fn truncate_sample(value: f64) -> f64 {
    (value * (4096.0 / 255.0)).floor() / 4096.0
}

fn bilinear(level: &MipLevel, x: [f32; 4], y: [f32; 4], channel: usize) -> f64 {
    let (xa, xb, ya, yb) = (x[0] as usize, x[1] as usize, y[0] as usize, y[1] as usize);
    let (ax, ay) = (x[2] as f64 / 256.0, y[2] as f64 / 256.0);
    let a = level.data[(ya * level.width + xa) * 4 + channel] as f64;
    let b = level.data[(ya * level.width + xb) * 4 + channel] as f64;
    let c = level.data[(yb * level.width + xa) * 4 + channel] as f64;
    let d = level.data[(yb * level.width + xb) * 4 + channel] as f64;
    let top = a + (b - a) * ax;
    let bottom = c + (d - c) * ax;
    top + (bottom - top) * ay
}

/// Pixel regions at least this large are sampled on the shared worker pool.
pub(crate) const PARALLEL_PIXELS: usize = 8192;
/// Pixels of one queued layer's job (on the raster pool already) past which its rows run in parallel
/// too: the frame's layers are rasterized in parallel, but a frame with a few large ones (a full-screen
/// gradient resized every frame) otherwise waits on the largest alone
pub(crate) const NESTED_PARALLEL_PIXELS: usize = 16 * PARALLEL_PIXELS;
/// Whether work over `pixels` independent pixels splits into parallel rows here
pub(crate) fn parallel_rows(pixels: usize) -> bool {
    pixels >= if rayon::current_thread_index().is_none() { PARALLEL_PIXELS } else { NESTED_PARALLEL_PIXELS }
}

/// Sampling preserves float32 write boundaries and 12-bit texture truncation.
pub fn sample_quad(
    levels: &[MipLevel],
    width: usize,
    height: usize,
    quad: SampleQuad,
    grid: SamplingGrid,
    cache: &mut TapCache,
) -> Vec<f32> {
    sample_quad_region(
        levels,
        width,
        height,
        quad,
        grid,
        cache,
        [0, 0, width, height],
    )
}

/// Region uses original quad pixel indices. Full tap arrays preserve the exact floating
/// point evaluation and mip selection of sample_quad; only unused pixels are skipped.
pub fn sample_quad_region(
    levels: &[MipLevel],
    width: usize,
    height: usize,
    quad: SampleQuad,
    grid: SamplingGrid,
    cache: &mut TapCache,
    region: [usize; 4],
) -> Vec<f32> {
    let [x0, y0, x1, y1] = region;
    assert!(x0 <= x1 && x1 <= width && y0 <= y1 && y1 <= height);
    let stride = x1 - x0;
    let mut out = vec![0.0; stride * (y1 - y0) * 4];
    if out.is_empty() {
        return out;
    }
    if levels.is_empty() {
        return out;
    }
    let (l0, fraction) = if quad.pixelated {
        (pixelated_level(levels.len(), quad.lod), 0)
    } else {
        mip_levels(levels.len(), quad.lod)
    };
    let level = &levels[l0];
    let xs = cache.get(
        quad.pixelated,
        width,
        quad.u0,
        quad.u1,
        level.width,
        quad.wrap,
        grid.off_x,
        grid.span_x,
    );
    let ys = cache.get(
        quad.pixelated,
        height,
        quad.v0,
        quad.v1,
        level.height,
        quad.wrap,
        grid.off_y,
        grid.span_y,
    );
    let second = if fraction > 0 {
        let l = &levels[l0 + 1];
        Some((
            l,
            cache.get(
                false,
                width,
                quad.u0,
                quad.u1,
                l.width,
                quad.wrap,
                grid.off_x,
                grid.span_x,
            ),
            cache.get(
                false,
                height,
                quad.v0,
                quad.v1,
                l.height,
                quad.wrap,
                grid.off_y,
                grid.span_y,
            ),
        ))
    } else {
        None
    };
    // Tap tables as byte offsets and 8-bit weights (bilinear4's index arithmetic, done once per
    // column / row). The bilinear value is exactly V / 65536 for the integer
    // V = (a * 256 + (b - a) * wx) * 256 + (bottom - top) * wy (weights k / 256, texels bytes: every
    // step of the f64 formula is exact), and the mip blend exactly (V0 * 256 + (V1 - V0) * f) / 2^24,
    // so both are computed in integers and converted once for truncate_sample.
    struct Axis {
        a: usize,
        b: usize,
        w: i32,
    }
    let columns = |taps: &[[f32; 4]]| -> Vec<Axis> {
        (x0..x1)
            .map(|i| Axis {
                a: taps[i][0] as usize * 4,
                b: taps[i][1] as usize * 4,
                w: taps[i][2] as i32,
            })
            .collect()
    };
    let rows_of = |taps: &[[f32; 4]], width: usize| -> Vec<Axis> {
        (y0..y1)
            .map(|j| Axis {
                a: taps[j][0] as usize * width * 4,
                b: taps[j][1] as usize * width * 4,
                w: taps[j][2] as i32,
            })
            .collect()
    };
    let (cols0, rows0) = (columns(&xs), rows_of(&ys, level.width));
    let tables1 = second
        .as_ref()
        .map(|(l, xx, yy)| (columns(xx), rows_of(yy, l.width), &l.data));
    #[inline(always)]
    fn lerp4(data: &[u8], row: &Axis, col: &Axis) -> [i32; 4] {
        let (ia, ib, ic, id) = (row.a + col.a, row.a + col.b, row.b + col.a, row.b + col.b);
        let (pa, pb, pc, pd) = (&data[ia..ia + 4], &data[ib..ib + 4], &data[ic..ic + 4], &data[id..id + 4]);
        std::array::from_fn(|c| {
            let (a, b, cc, e) = (pa[c] as i32, pb[c] as i32, pc[c] as i32, pd[c] as i32);
            let top = a * 256 + (b - a) * col.w;
            let bottom = cc * 256 + (e - cc) * col.w;
            top * 256 + (bottom - top) * row.w
        })
    }
    const TO_UNIT16: f64 = 1.0 / 65536.0;
    const TO_UNIT24: f64 = 1.0 / 16777216.0;
    let fill_row = |j: usize, row: &mut [f32]| {
        if quad.pixelated {
            for i in x0..x1 {
                let out = &mut row[(i - x0) * 4..(i - x0) * 4 + 4];
                let at = (ys[j][0] as usize * level.width + xs[i][0] as usize) * 4;
                for c in 0..4 {
                    out[c] = (level.data[at + c] as f64 / 255.0) as f32;
                }
            }
            return;
        }
        let r0 = &rows0[j - y0];
        let data0: &[u8] = &level.data;
        match &tables1 {
            Some((cols1, rows1, data1)) => {
                let r1 = &rows1[j - y0];
                let data1: &[u8] = data1;
                let f = fraction as i64;
                for (k, out) in row.chunks_exact_mut(4).enumerate() {
                    let p0 = lerp4(data0, r0, &cols0[k]);
                    let p1 = lerp4(data1, r1, &cols1[k]);
                    for c in 0..4 {
                        let blended = p0[c] as i64 * 256 + (p1[c] as i64 - p0[c] as i64) * f;
                        out[c] = truncate_sample(blended as f64 * TO_UNIT24) as f32;
                    }
                }
            }
            None => {
                for (k, out) in row.chunks_exact_mut(4).enumerate() {
                    let col = &cols0[k];
                    let p0 = lerp4(data0, r0, col);
                    if col.w == 0 && r0.w == 0 {
                        for c in 0..4 {
                            out[c] = ((p0[c] as f64 * TO_UNIT16) / 255.0) as f32;
                        }
                    } else {
                        for c in 0..4 {
                            out[c] = truncate_sample(p0[c] as f64 * TO_UNIT16) as f32;
                        }
                    }
                }
            }
        }
    };
    // Rows are independent and evaluated with the same arithmetic in either order.
    // Within a worker (a queued layer's job) rows stay on that thread
    if rayon::current_thread_index().is_none() && stride * (y1 - y0) >= PARALLEL_PIXELS {
        use rayon::prelude::*;
        out.par_chunks_mut(stride * 4)
            .enumerate()
            .for_each(|(row, values)| fill_row(y0 + row, values));
    } else {
        for (row, values) in out.chunks_mut(stride * 4).enumerate() {
            fill_row(y0 + row, values);
        }
    }
    out
}

pub fn sample_at(
    levels: &[MipLevel],
    u: f64,
    v: f64,
    lod: f64,
    wrap: bool,
    pixelated: bool,
) -> [f32; 4] {
    if levels.is_empty() {
        return [0.0; 4];
    }
    let (l0, f) = if pixelated {
        (pixelated_level(levels.len(), lod), 0)
    } else {
        mip_levels(levels.len(), lod)
    };
    let at = |level: &MipLevel, c: usize| {
        if pixelated {
            let x = fix_index((u * level.width as f64).ceil() - 1.0, level.width, wrap);
            let y = fix_index((v * level.height as f64).ceil() - 1.0, level.height, wrap);
            level.data[(y * level.width + x) * 4 + c] as f64 / 255.0
        } else {
            let x = bilinear_tap_at(u * level.width as f64, level.width, wrap);
            let y = bilinear_tap_at(v * level.height as f64, level.height, wrap);
            bilinear(level, x, y, c)
        }
    };
    std::array::from_fn(|c| {
        let p0 = at(&levels[l0], c);
        if pixelated {
            p0 as f32
        } else {
            truncate_sample(if f > 0 {
                p0 + (at(&levels[l0 + 1], c) - p0) * f as f64 / 256.0
            } else {
                p0
            }) as f32
        }
    })
}

#[cfg(test)]
mod region_tests {
    use super::*;
    #[test]
    fn clipped_regions_match_full_quad_bits() {
        let levels = texture_levels(
            17,
            11,
            (0..17 * 11 * 4)
                .map(|i| ((i * 73 + i / 7) % 256) as u8)
                .collect::<Vec<_>>()
                .into(),
        );
        let mut cache = TapCache::default();
        for pixelated in [false, true] {
            for wrap in [false, true] {
                for flip in [false, true] {
                    for lod in [-1.0, 0.0, 0.49, 1.375, 2.75, 8.0] {
                        for offset in [-19.75, -0.125, 0.5, 13.875] {
                            let quad = SampleQuad {
                                u0: if flip { 1.4 } else { -0.3 },
                                u1: if flip { -0.3 } else { 1.4 },
                                v0: if flip { 1.2 } else { -0.2 },
                                v1: if flip { -0.2 } else { 1.2 },
                                lod,
                                wrap,
                                pixelated,
                            };
                            let grid = SamplingGrid {
                                off_x: offset,
                                off_y: offset * 0.7,
                                span_x: 23.375,
                                span_y: 17.625,
                            };
                            let full = sample_quad(&levels, 23, 19, quad, grid, &mut cache);
                            for region in [
                                [0, 0, 23, 19],
                                [0, 0, 1, 1],
                                [22, 18, 23, 19],
                                [3, 4, 17, 12],
                                [7, 0, 7, 19],
                                [0, 9, 23, 9],
                            ] {
                                let cropped = sample_quad_region(
                                    &levels, 23, 19, quad, grid, &mut cache, region,
                                );
                                let [x0, y0, x1, y1] = region;
                                let expected: Vec<_> = (y0..y1)
                                    .flat_map(|y| {
                                        full[(y * 23 + x0) * 4..(y * 23 + x1) * 4]
                                            .iter()
                                            .map(|v| v.to_bits())
                                    })
                                    .collect();
                                assert_eq!(
                                    cropped.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                                    expected,
                                    "{pixelated} {wrap} {flip} {lod} {offset} {region:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod integer_sampling_tests {
    use super::*;

    /// The former per-channel f64 evaluation of sample_quad_region's pixels (bilinear, mip blend,
    /// truncate_sample), for comparison with the integer one
    fn reference(levels: &[MipLevel], w: usize, h: usize, quad: SampleQuad, grid: SamplingGrid) -> Vec<f32> {
        let (l0, fraction) = mip_levels(levels.len(), quad.lod);
        let level = &levels[l0];
        let tx = |l: &MipLevel| taps(false, w, quad.u0, quad.u1, l.width, quad.wrap, grid.off_x, grid.span_x);
        let ty = |l: &MipLevel| taps(false, h, quad.v0, quad.v1, l.height, quad.wrap, grid.off_y, grid.span_y);
        let (xs, ys) = (tx(level), ty(level));
        let second = (fraction > 0).then(|| (&levels[l0 + 1], tx(&levels[l0 + 1]), ty(&levels[l0 + 1])));
        let mut out = vec![0.0; w * h * 4];
        for j in 0..h {
            for i in 0..w {
                for c in 0..4 {
                    let p0 = bilinear(level, xs[i], ys[j], c);
                    let value = if let Some((l, xx, yy)) = &second {
                        truncate_sample(p0 + (bilinear(l, xx[i], yy[j], c) - p0) * fraction as f64 / 256.0)
                    } else if xs[i][2] == 0.0 && ys[j][2] == 0.0 {
                        p0 / 255.0
                    } else {
                        truncate_sample(p0)
                    };
                    out[(j * w + i) * 4 + c] = value as f32;
                }
            }
        }
        out
    }

    #[test]
    fn integer_sampling_matches_the_f64_formula() {
        let mut seed = 0x2545f4914f6cdd1du64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for case in 0..60 {
            let (tw, th) = (1 + (next() % 97) as usize, 1 + (next() % 61) as usize);
            let data: Vec<u8> = (0..tw * th * 4).map(|_| next() as u8).collect();
            let levels = texture_levels(tw, th, data.into());
            let (w, h) = (1 + (next() % 40) as usize, 1 + (next() % 30) as usize);
            let u0 = (next() % 100) as f64 / 250.0;
            let v0 = (next() % 100) as f64 / 250.0;
            let quad = SampleQuad {
                u0,
                v0,
                u1: u0 + 0.2 + (next() % 100) as f64 / 120.0,
                v1: v0 + 0.2 + (next() % 100) as f64 / 120.0,
                lod: if case % 3 == 0 { 0.0 } else { (next() % 400) as f64 / 100.0 - 0.5 },
                wrap: case % 4 == 1,
                pixelated: false,
            };
            let grid = SamplingGrid {
                off_x: 0.5 + (next() % 7) as f64 / 8.0,
                off_y: 0.5,
                span_x: w as f64 * (0.75 + (next() % 5) as f64 / 8.0),
                span_y: h as f64,
            };
            let expected = reference(&levels, w, h, quad, grid);
            let actual = sample_quad(&levels, w, h, quad, grid, &mut TapCache::default());
            assert_eq!(
                expected.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                actual.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                "case {case}"
            );
        }
    }
}
