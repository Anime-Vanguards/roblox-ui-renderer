//! Image geometry and exact CPU raster preparation. The GPU consumes these byte layers without
//! replacing the measured Roblox mip/filter/quantization rules with hardware texture filtering.
use super::{
    geometry::{apply, device_span, invert},
    gradients::gradient_mesh,
    sampling::*,
};
use crate::assets::TextureAsset;
use serde_json::Value;
use std::{collections::HashMap, sync::Arc, time::Instant};

/// An image node whose matrix is a pure translation (linear part exactly the identity, a Rotation cancelled
/// by an ancestor's) as the unrotated node at its translated rect. Studio
/// snaps and samples it like any axis-aligned image (measured on a close button's Line_1 / Line_2
/// images, notes/images.md round 4).
pub fn translated_image_node(node: &Value) -> Option<Value> {
    // A node in a turned CanvasGroup's texture holds its matrix in the group's axes (groups.rs localized):
    // on the screen it is turned, and it is drawn as a turned node
    if node.get("fringe").is_some() {
        return None;
    }
    let m = node.get("matrix")?.as_array()?;
    let at = |i: usize| m.get(i).and_then(Value::as_f64);
    if at(0)? != 1. || at(1)? != 0. || at(2)? != 0. || at(3)? != 1. {
        return None;
    }
    let (e, f) = (at(4).unwrap_or(0.), at(5).unwrap_or(0.));
    let r = node.get("rect")?.as_array()?;
    let v = |i: usize| r.get(i).and_then(Value::as_f64).unwrap_or(0.);
    let mut moved = node.clone();
    let object = moved.as_object_mut()?;
    object.remove("matrix");
    object.insert("rect".into(), serde_json::json!([v(0) + e, v(1) + f, v(2), v(3)]));
    Some(moved)
}
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct ImageQuad {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    pub u0: f64,
    pub v0: f64,
    pub u1: f64,
    pub v1: f64,
    pub wrap: bool,
    /// A 9-slice part without texels on an axis (a degenerate SliceCenter): UICorner leaves it square
    /// (fixtures/image-probe-slicecorner.luau, notes/images.md round 10)
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub unrounded: bool,
}
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct ImageRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub raw: [f64; 4],
    pub snapped: bool,
}
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct ImageSpace {
    pub l: f64,
    pub t: f64,
    pub sx: f64,
    pub sy: f64,
    pub nw: u32,
    pub nh: u32,
}
#[derive(Clone, Debug)]
pub struct ImageLayer {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<[u8]>,
    /// False for CanvasGroup source-image blending, true for normal screen layers.
    pub premultiplied: bool,
}

/// A layer whose pixels come from `job` (run later: DrawPreparer queues a frame's jobs and runs
/// them in parallel, then stores each result under `key` exactly as a synchronous miss would).
pub struct PendingLayer {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub premultiplied: bool,
    pub key: String,
    pub source: Arc<[u8]>,
    pub job: Box<dyn FnOnce() -> Vec<u8> + Send>,
    /// Stored on probation (ImageCache::store_with): a layer cheap to make again
    pub probation: bool,
}
impl PendingLayer {
    /// Runs the job now and stores its layer like a synchronous miss.
    pub fn finish(self, cache: &mut ImageCache) -> ImageLayer {
        let started = Instant::now();
        let rgba = (self.job)();
        let layer = ImageLayer {
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
            rgba: rgba.into(),
            premultiplied: self.premultiplied,
        };
        cache.store_with(self.key, layer, self.source, started, self.probation)
    }
}
pub enum PlannedLayer {
    Ready(ImageLayer),
    Pending(PendingLayer),
}

struct MipEntry {
    levels: Arc<[MipLevel]>,
    bytes: usize,
    used: u64,
}
/// A quad's texture samples (sample_quad), kept for images whose UIGradient changes every frame: the
/// samples depend on the texture and the quad's size and UVs alone, the gradient only shades them
struct SampleEntry {
    values: Arc<[f32]>,
    used: u64,
    _source: Arc<[u8]>,
}
struct LayerEntry {
    layer: ImageLayer,
    used: u64,
    bytes: usize,
    _source: Arc<[u8]>,
    /// On probation since this store (its stamp in ImageCache::probation) until its first hit
    probation: Option<u64>,
}
/// Layers on probation at most: a layer stored on probation (one cheap to make again, e.g. a stroke tinted
/// from its cached coverage) and not hit since leaves after this many newer ones, so the one-off layers
/// of an animation (a hover tween's every transparency) never push the cache's reused layers out
const PROBATION: usize = 64;
/// Bytes on probation at most (PROBATION large layers, an animated full-screen gradient's, would fill the budget)
const PROBATION_BYTES: usize = 24 * 1024 * 1024;
/// Keys of layers that left probation unhit, remembered (as hashes) so one stored again is kept outright:
/// it is reused after all, just later than probation allows (a looping animation's frames)
const GHOSTS: usize = 2048;
/// Per-prepare diagnostics. Mip time is included in raster time.
#[derive(Clone, Copy, Debug, Default, serde::Serialize)]
pub struct ImageCacheStats {
    pub screen_hits: u64,
    pub screen_misses: u64,
    pub group_hits: u64,
    pub group_misses: u64,
    pub mip_hits: u64,
    pub mip_misses: u64,
    pub layer_evictions: u64,
    pub mip_evictions: u64,
    pub raster_pixels: u64,
    pub raster_ms: f64,
    pub mip_ms: f64,
    pub layer_entries: usize,
    pub mip_entries: usize,
    pub layer_bytes: usize,
    pub mip_bytes: usize,
}
#[derive(Default)]
pub struct ImageCache {
    pub taps: TapCache,
    mips: HashMap<usize, MipEntry>,
    mip_bytes: usize,
    layers: HashMap<String, LayerEntry>,
    layer_sources: HashMap<usize, usize>,
    layer_bytes: usize,
    clock: u64,
    stats: ImageCacheStats,
    /// Keys stored on probation, oldest first, with their stamps (LayerEntry::probation)
    probation: std::collections::VecDeque<(String, u64)>,
    probation_stamp: u64,
    /// Bytes of the layers on probation
    probation_bytes: usize,
    samples: HashMap<String, SampleEntry>,
    sample_bytes: usize,
    ghosts: std::collections::VecDeque<u64>,
    ghost_set: rustc_hash::FxHashSet<u64>,
}
impl ImageCache {
    pub fn reset_stats(&mut self) {
        self.stats = ImageCacheStats::default();
    }
    pub fn stats(&self) -> ImageCacheStats {
        ImageCacheStats {
            layer_entries: self.layers.len(),
            mip_entries: self.mips.len(),
            layer_bytes: self.layer_bytes,
            mip_bytes: self.mip_bytes,
            ..self.stats
        }
    }
    pub fn clear(&mut self) {
        self.mips.clear();
        self.layers.clear();
        self.probation.clear();
        self.probation_bytes = 0;
        self.samples.clear();
        self.sample_bytes = 0;
        self.ghosts.clear();
        self.ghost_set.clear();
        self.layer_sources.clear();
        self.taps.clear();
        self.mip_bytes = 0;
        self.layer_bytes = 0;
    }
    pub fn bytes(&self) -> usize {
        self.mip_bytes + self.layer_bytes + self.sample_bytes + self.taps.bytes()
    }
    /// The samples of `quad` over a `width` x `height` grid of `source`'s `levels`, reused while
    /// resident (kept under a budget of their own, least recently used out first)
    fn quad_samples(
        &mut self,
        source: &TextureAsset,
        levels: &[MipLevel],
        width: usize,
        height: usize,
        quad: SampleQuad,
    ) -> Arc<[f32]> {
        const BUDGET: usize = 64 * 1024 * 1024;
        self.clock = self.clock.wrapping_add(1);
        let key = format!(
            "{}:{width}x{height}:{:?}:{:?}:{:?}:{:?}:{:?}:{}:{}",
            source.rgba.as_ptr() as usize,
            quad.u0,
            quad.v0,
            quad.u1,
            quad.v1,
            quad.lod,
            quad.wrap,
            quad.pixelated
        );
        if let Some(entry) = self.samples.get_mut(&key) {
            entry.used = self.clock;
            return entry.values.clone();
        }
        let values: Arc<[f32]> =
            sample_quad(levels, width, height, quad, SamplingGrid::pixels(width, height), &mut self.taps).into();
        let bytes = values.len() * 4;
        if bytes <= BUDGET {
            while self.sample_bytes + bytes > BUDGET {
                let oldest = self.samples.iter().min_by_key(|(_, e)| e.used).unwrap().0.clone();
                self.sample_bytes -= self.samples.remove(&oldest).unwrap().values.len() * 4;
            }
            self.sample_bytes += bytes;
            self.samples.insert(
                key,
                SampleEntry {
                    values: values.clone(),
                    used: self.clock,
                    _source: source.rgba.clone(),
                },
            );
        }
        values
    }
    fn levels(&mut self, source: &TextureAsset) -> Arc<[MipLevel]> {
        self.clock = self.clock.wrapping_add(1);
        let key = source.rgba.as_ptr() as usize;
        if let Some(entry) = self.mips.get_mut(&key) {
            self.stats.mip_hits += 1;
            entry.used = self.clock;
            return entry.levels.clone();
        }
        self.stats.mip_misses += 1;
        let started = Instant::now();
        let levels: Arc<[MipLevel]> = super::sampling::asset_levels(source).into();
        let bytes = levels.iter().map(|l| l.data.len()).sum::<usize>();
        const BUDGET: usize = 128 * 1024 * 1024;
        if bytes <= BUDGET {
            while self.mip_bytes + bytes > BUDGET || self.mips.len() >= 128 {
                let oldest = *self.mips.iter().min_by_key(|(_, e)| e.used).unwrap().0;
                self.mip_bytes -= self.mips.remove(&oldest).unwrap().bytes;
                self.stats.mip_evictions += 1;
            }
            self.mip_bytes += bytes;
            self.mips.insert(
                key,
                MipEntry {
                    levels: levels.clone(),
                    bytes,
                    used: self.clock,
                },
            );
        }
        self.stats.mip_ms += started.elapsed().as_secs_f64() * 1000.;
        levels
    }
    pub(crate) fn cached(&mut self, key: &str) -> Option<ImageLayer> {
        self.clock = self.clock.wrapping_add(1);
        let mut released = 0;
        let result = self.layers.get_mut(key).map(|e| {
            e.used = self.clock;
            // A hit takes the layer off probation
            if e.probation.take().is_some() {
                released = e.bytes;
            }
            e.layer.clone()
        });
        self.probation_bytes -= released;
        match (key.starts_with("texture:"), result.is_some()) {
            (false, true) => self.stats.screen_hits += 1,
            (false, false) => self.stats.screen_misses += 1,
            (true, true) => self.stats.group_hits += 1,
            (true, false) => self.stats.group_misses += 1,
        }
        result
    }
    pub(crate) fn store(
        &mut self,
        key: String,
        layer: ImageLayer,
        source: Arc<[u8]>,
        started: Instant,
    ) -> ImageLayer {
        self.store_with(key, layer, source, started, false)
    }
    /// Removes a stored layer (its bytes and its share of its source allocation)
    fn remove_layer(&mut self, key: &str) {
        let Some(entry) = self.layers.remove(key) else {
            return;
        };
        self.layer_bytes -= entry.bytes;
        if entry.probation.is_some() {
            self.probation_bytes -= entry.bytes;
        }
        let source = entry._source.as_ptr() as usize;
        let count = self.layer_sources.get_mut(&source).unwrap();
        *count -= 1;
        if *count == 0 {
            self.layer_sources.remove(&source);
            self.layer_bytes -= entry._source.len();
        }
        self.stats.layer_evictions += 1;
    }
    /// store(), on probation when `probation`: the layer leaves after PROBATION newer probation layers
    /// (or once those hold PROBATION_BYTES) unless it was hit before
    pub(crate) fn store_with(
        &mut self,
        key: String,
        layer: ImageLayer,
        source: Arc<[u8]>,
        started: Instant,
        probation: bool,
    ) -> ImageLayer {
        self.stats.raster_pixels += u64::from(layer.width) * u64::from(layer.height);
        self.stats.raster_ms += started.elapsed().as_secs_f64() * 1000.;
        const BUDGET: usize = 128 * 1024 * 1024;
        // Layers pin their pointer-keyed source allocation. Charge that shared allocation once,
        // so a moving image's dimension variants do not repeatedly spend the raster budget.
        let bytes = layer.rgba.len() + key.len() + 192;
        let source_key = source.as_ptr() as usize;
        let probation = probation && !self.ghost_set.contains(&ghost_hash(&key));
        if bytes + source.len() <= BUDGET {
            if self.layers.contains_key(&key) {
                self.remove_layer(&key);
            }
            while self.layer_bytes
                + bytes
                + if self.layer_sources.contains_key(&source_key) {
                    0
                } else {
                    source.len()
                }
                > BUDGET
                || self.layers.len() >= 512
            {
                let oldest = self
                    .layers
                    .iter()
                    .min_by_key(|(_, e)| e.used)
                    .unwrap()
                    .0
                    .clone();
                self.remove_layer(&oldest);
            }
            let count = self.layer_sources.entry(source_key).or_default();
            if *count == 0 {
                self.layer_bytes += source.len();
            }
            *count += 1;
            self.layer_bytes += bytes;
            let stamp = probation.then(|| {
                self.probation_bytes += bytes;
                self.probation_stamp += 1;
                self.probation.push_back((key.clone(), self.probation_stamp));
                self.probation_stamp
            });
            self.layers.insert(
                key,
                LayerEntry {
                    layer: layer.clone(),
                    used: self.clock,
                    bytes,
                    _source: source,
                    probation: stamp,
                },
            );
            while !self.probation.is_empty()
                && (self.probation.len() > PROBATION || self.probation_bytes > PROBATION_BYTES)
            {
                let (oldest, stamp) = self.probation.pop_front().unwrap();
                if self.layers.get(&oldest).is_some_and(|entry| entry.probation == Some(stamp)) {
                    self.remove_layer(&oldest);
                    let hash = ghost_hash(&oldest);
                    if self.ghost_set.insert(hash) {
                        self.ghosts.push_back(hash);
                        if self.ghosts.len() > GHOSTS {
                            let forgotten = self.ghosts.pop_front().unwrap();
                            self.ghost_set.remove(&forgotten);
                        }
                    }
                }
            }
        }
        layer
    }
}

fn ghost_hash(key: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = rustc_hash::FxHasher::default();
    key.hash(&mut hasher);
    hasher.finish()
}
fn n(value: &Value, fallback: f64) -> f64 {
    value.as_f64().unwrap_or(fallback)
}
fn or_number(value: &Value, fallback: f64) -> f64 {
    let x = n(value, fallback);
    if x == 0.0 { fallback } else { x }
}
fn a<const N: usize>(value: &Value, default: [f64; N]) -> [f64; N] {
    std::array::from_fn(|i| n(&value[i], default[i]))
}
fn q(rect: [f64; 4], uv: [f64; 4], wrap: bool) -> ImageQuad {
    ImageQuad {
        x0: rect[0],
        y0: rect[1],
        x1: rect[2],
        y1: rect[3],
        u0: uv[0],
        v0: uv[1],
        u1: uv[2],
        v1: uv[3],
        wrap,
        unrounded: false,
    }
}

/// Local physical quads, with float32 far edges, flipped rects, density and every ScaleType.
pub fn image_quads(
    image: &Value,
    texture: [u32; 2],
    raw: [f64; 4],
    pixel_scale: f64,
    ui_scale: f64,
) -> Vec<ImageQuad> {
    let density = or_number(&image["density"], 1.0);
    let (tw, th) = (texture[0] as f64 / density, texture[1] as f64 / density);
    let longest = tw.max(th);
    let (iw, ih) = if longest > 1024.0 {
        (tw * 1024.0 / longest, th * 1024.0 / longest)
    } else {
        (tw, th)
    };
    let [x, y, w, h] = raw;
    if !(w > 0.0 && h > 0.0 && iw > 0.0 && ih > 0.0) {
        return Vec::new();
    }
    let offset = a(&image["rectOffset"], [0.0; 2]);
    let size = a(&image["rectSize"], [0.0; 2]);
    let axis = |start: f64, length: f64, extent: f64| {
        let span = if length == 0.0 { extent } else { length };
        let lo = start.min(start + span).max(0.0).min(extent);
        let hi = lo.max(start.max(start + span).min(extent));
        if span >= 0.0 {
            [lo / extent, hi / extent, hi - lo]
        } else {
            [hi / extent, lo / extent, hi - lo]
        }
    };
    let [u0, u1, rw] = axis(offset[0], size[0], iw);
    let [v0, v1, rh] = axis(offset[1], size[1], ih);
    let (x1, y1) = ((x + w) as f32 as f64, (y + h) as f32 as f64);
    match image["scaleType"].as_str().unwrap_or("Stretch") {
        "Fit" if rw > 0.0 && rh > 0.0 => {
            let ratio = rw / rh;
            let (fw, fh) = if w / h > ratio {
                (h * ratio, h)
            } else {
                (w, w / ratio)
            };
            let (fx, fy) = (x + (w - fw) / 2.0, y + (h - fh) / 2.0);
            vec![q(
                [fx, fy, (fx + fw) as f32 as f64, (fy + fh) as f32 as f64],
                [u0, v0, u1, v1],
                false,
            )]
        }
        "Crop" if rw > 0.0 && rh > 0.0 => {
            let ratio = rw / rh;
            let mut uv = [u0, v0, u1, v1];
            if w / h > ratio {
                let keep = ratio / (w / h);
                uv[1] += (v1 - v0) * (1.0 - keep) / 2.0;
                uv[3] -= (v1 - v0) * (1.0 - keep) / 2.0;
            } else {
                let keep = (w / h) / ratio;
                uv[0] += (u1 - u0) * (1.0 - keep) / 2.0;
                uv[2] -= (u1 - u0) * (1.0 - keep) / 2.0;
            }
            vec![q([x, y, x1, y1], uv, false)]
        }
        "Tile" if size == [0.0; 2] => {
            let ts = a(&image["tileSize"], [1.0, 0.0, 1.0, 0.0]);
            let mut tile_w = ts[0] * w + ts[1] * pixel_scale * ui_scale;
            let mut tile_h = ts[2] * h + ts[3] * pixel_scale * ui_scale;
            if tile_w <= 0.0 {
                tile_w = w;
            }
            if tile_h <= 0.0 {
                tile_h = h;
            }
            vec![q([x, y, x1, y1], [0.0, 0.0, w / tile_w, h / tile_h], true)]
        }
        "Slice" => {
            let [cx0, cy0, cx1, cy1] = a(&image["sliceCenter"], [0.0; 4]);
            let (left, top, right, bottom) = (
                cx0.max(0.0),
                cy0.max(0.0),
                (rw - cx1).max(0.0),
                (rh - cy1).max(0.0),
            );
            let k = or_number(&image["sliceScale"], 1.0) * pixel_scale * ui_scale;
            let mut shrink: f64 = 1.0;
            if (left + right) * k > w {
                shrink = shrink.min(w / ((left + right) * k));
            }
            if (top + bottom) * k > h {
                shrink = shrink.min(h / ((top + bottom) * k));
            }
            let xs = [x, x + left * k * shrink, x1 - right * k * shrink, x1];
            let ys = [y, y + top * k * shrink, y1 - bottom * k * shrink, y1];
            // At most one texel per logical px of the unsnapped centre; a smaller centre shows only its first
            // texels (1.5x: 19.5 device px show 13 texels, not the snapped 20 px's 13.33;
            // fixtures/image-probe-slicecorner.luau 3712 -> 80 px > 2)
            let center_x = (rw - right - left).min((xs[2] - xs[1]).max(0.0) / pixel_scale);
            let center_y = (rh - bottom - top).min((ys[2] - ys[1]).max(0.0) / pixel_scale);
            let du = (u1 - u0) / if rw == 0.0 { 1.0 } else { rw };
            let dv = (v1 - v0) / if rh == 0.0 { 1.0 } else { rh };
            let us = [
                [u0, u0 + left * du],
                [u0 + left * du, u0 + (left + center_x.max(0.0)) * du],
                [u0 + (rw - right) * du, u1],
            ];
            let vs = [
                [v0, v0 + top * dv],
                [v0 + top * dv, v0 + (top + center_y.max(0.0)) * dv],
                [v0 + (rh - bottom) * dv, v1],
            ];
            // UICorner rounds each part on its own, except a part with no texels on an axis (a degenerate
            // SliceCenter, min == max), which Studio leaves square (fixtures/image-probe-slicecorner.luau)
            let mut out = Vec::with_capacity(9);
            for j in 0..3 {
                for i in 0..3 {
                    if xs[i + 1] > xs[i] && ys[j + 1] > ys[j] {
                        let mut part = q(
                            [xs[i], ys[j], xs[i + 1], ys[j + 1]],
                            [us[i][0], vs[j][0], us[i][1], vs[j][1]],
                            false,
                        );
                        part.unrounded = us[i][0] == us[i][1] || vs[j][0] == vs[j][1];
                        out.push(part);
                    }
                }
            }
            out
        }
        _ => vec![q([x, y, x1, y1], [u0, v0, u1, v1], false)],
    }
}

/// UICorner coverage of an image quad scaled into a resampled CanvasGroup texture, at a texel centre (px, py)
/// from the quad's centre in screen px (half size hw x hh): smoothstep(0, 1, 0.5 - d / fw) with the analytic
/// derivative of the quad's screen units per texel (1 / s across the nearest edge); callers round it to 1/255.
/// Not measured (notes/images.md round 10: the texture masks were left unchanged).
fn corner(px: f64, py: f64, hw: f64, hh: f64, radii: [f64; 4], sx: f64, sy: f64) -> f64 {
    let r = if py < 0.0 {
        if px < 0.0 { radii[0] } else { radii[1] }
    } else if px < 0.0 {
        radii[3]
    } else {
        radii[2]
    }
    .max(0.0);
    let (qx, qy) = (px.abs() - hw + r, py.abs() - hh + r);
    let (d, fw) = if qx > 0.0 && qy > 0.0 {
        let len = qx.hypot(qy);
        (len - r, (qx / sx + qy / sy) / len)
    } else {
        (qx.max(qy) - r, if qx >= qy { 1.0 / sx } else { 1.0 / sy })
    };
    let t = (0.5 - d / fw).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
/// The target parity of a rounded image's layer origin (its corner masks depend on it), None unrounded
fn rounded_parity(radius: [f64; 4], rect: ImageRect, transform: [f64; 6]) -> Option<[i64; 2]> {
    radius.iter().any(|r| *r > 0.0).then(|| {
        [rect.x + transform[4], rect.y + transform[5]].map(|v| (v.round() as i64).rem_euclid(2))
    })
}

/// The UICorner SDF of a w x h image quad whose top-left pixel sits at device (ox, oy), fwidth from coarse
/// 2 x 2 derivative quads on even device coordinates (taken at the quad's top-left pixel), the radius not
/// limited to the quad: 9-slice corner parts smaller than the radius vanish, edge parts get lens-shaped ends.
/// Measured at 1x (fixtures/image-probe-slicecorner.luau + image-corner.luau): the analytic per-pixel fwidth
/// left 3-6 level fringes on small radii, 172 + 30 px > 2 -> 19 + 0.
pub fn quad_corner_mask(w: usize, h: usize, radii: [f64; 4], ox: i64, oy: i64) -> Vec<f32> {
    let (px2, py2) = (ox.rem_euclid(2), oy.rem_euclid(2));
    let (hw, hh) = (w as f64 / 2.0, h as f64 / 2.0);
    let [tl, tr, br, bl] = radii.map(|r| r.max(0.0));
    let sd = |i: i64, j: i64| {
        let (px, py) = (i as f64 + 0.5 - hw, j as f64 + 0.5 - hh);
        let r = if py < 0.0 {
            if px < 0.0 { tl } else { tr }
        } else if px < 0.0 {
            bl
        } else {
            br
        };
        let (qx, qy) = (px.abs() - hw + r, py.abs() - hh + r);
        if qx > 0.0 && qy > 0.0 { qx.hypot(qy) - r } else { qx.max(qy) - r }
    };
    (0..w * h)
        .map(|k| {
            let (i, j) = ((k % w) as i64, (k / w) as i64);
            let (qi, qj) = (i - (i + px2) % 2, j - (j + py2) % 2);
            let origin = sd(qi, qj);
            let fw = (sd(qi + 1, qj) - origin).abs() + (sd(qi, qj + 1) - origin).abs();
            let d = sd(i, j);
            let t = if fw > 0.0 {
                (0.5 - d / fw).clamp(0.0, 1.0)
            } else if d < 0.0 {
                1.0
            } else {
                0.0
            };
            (t * t * (3.0 - 2.0 * t)) as f32
        })
        .collect()
}

/// A UIGradient's canvas-style sampling over `raw` (the gradient line and its keypoints read once;
/// `at` evaluates each pixel with the same arithmetic as reading them per pixel).
struct GradientSampler {
    dx: f64,
    dy: f64,
    cx: f64,
    cy: f64,
    span: f64,
    sized: bool,
    color: Option<Vec<[f64; 4]>>,
    transparency: Option<Vec<[f64; 2]>>,
}

impl GradientSampler {
    fn new(gradient: &Value, raw: [f64; 4]) -> Self {
        let [x, y, w, h] = raw;
        let angle = n(&gradient["rotation"], 0.0) * std::f64::consts::PI / 180.0;
        let (dx, dy) = (super::trig::cos(angle), super::trig::sin(angle));
        let offset = a(&gradient["offset"], [0.0; 2]);
        let (cx, cy) = (x + w / 2.0 + offset[0] * w, y + h / 2.0 + offset[1] * h);
        let span = (if dx.abs() > 1e-9 {
            w / dx.abs()
        } else {
            f64::INFINITY
        })
        .min(if dy.abs() > 1e-9 {
            h / dy.abs()
        } else {
            f64::INFINITY
        });
        let keys = |name: &str| gradient[name].as_array().filter(|s| !s.is_empty());
        Self {
            dx,
            dy,
            cx,
            cy,
            span,
            sized: w > 0.0 && h > 0.0,
            color: keys("color").map(|seq| {
                seq.iter()
                    .map(|k| [n(&k[0], 0.0), n(&k[1], 1.0), n(&k[2], 1.0), n(&k[3], 1.0)])
                    .collect()
            }),
            transparency: keys("transparency")
                .map(|seq| seq.iter().map(|k| [n(&k[0], 0.0), n(&k[1], 0.0)]).collect()),
        }
    }

    fn at(&self, px: f64, py: f64) -> [f64; 4] {
        let t = if self.sized {
            (((px - self.cx) * self.dx + (py - self.cy) * self.dy) / self.span + 0.5).clamp(0.0, 1.0)
        } else {
            0.0
        };
        // Keypoint k's time is k[0], its channel c value k[c]
        fn sample<const N: usize>(seq: &[[f64; N]], t: f64, channel: usize) -> f64 {
            if t <= seq[0][0] {
                return seq[0][channel];
            }
            for pair in seq.windows(2) {
                let (ta, tb) = (pair[0][0], pair[1][0]);
                if t <= tb {
                    let f = if tb == ta { 0.0 } else { (t - ta) / (tb - ta) };
                    let va = pair[0][channel];
                    return va + (pair[1][channel] - va) * f;
                }
            }
            seq[seq.len() - 1][channel]
        }
        let colour = |c: usize| self.color.as_deref().map_or(1.0, |s| sample(s, t, c));
        [
            colour(1),
            colour(2),
            colour(3),
            1.0 - self.transparency.as_deref().map_or(0.0, |s| sample(s, t, 1)),
        ]
    }
}

fn shade(
    values: [f32; 4],
    tint: [f64; 3],
    fade: f64,
    coverage: f64,
    gradient: Option<[f64; 4]>,
    vertex: Option<&[f32]>,
) -> [f32; 4] {
    let mut out = [0.0; 4];
    if let Some(v) = vertex {
        for c in 0..3 {
            out[c] = (values[c] as f64 * v[c] as f64 / 255.0) as f32;
        }
        out[3] = (values[3] as f64 * v[3] as f64 / 255.0 * coverage) as f32;
    } else {
        for c in 0..3 {
            let base = values[c] as f64 * tint[c];
            out[c] = (if let Some(g) = gradient {
                base * g[c]
            } else {
                base
            }) as f32;
        }
        let alpha = values[3] as f64 * fade * coverage;
        out[3] = (if let Some(g) = gradient {
            alpha * g[3]
        } else {
            alpha
        }) as f32;
    }
    out
}

fn layer(
    x: i32,
    y: i32,
    width: usize,
    height: usize,
    pixels: Vec<f32>,
    premultiplied: bool,
) -> ImageLayer {
    let mut rgba: Vec<u8> = if rayon::current_thread_index().is_none()
        && pixels.len() >= super::sampling::PARALLEL_PIXELS * 4
    {
        use rayon::prelude::*;
        pixels.into_par_iter().map(|v| gpu8(v as f64)).collect()
    } else {
        pixels.into_iter().map(|v| gpu8(v as f64)).collect()
    };
    if premultiplied {
        premultiply_bytes(&mut rgba);
    }
    ImageLayer {
        x,
        y,
        width: width as u32,
        height: height as u32,
        rgba: rgba.into(),
        premultiplied,
    }
}
pub(crate) fn dims(w: f64, h: f64) -> Option<(usize, usize)> {
    if !w.is_finite()
        || !h.is_finite()
        || w <= 0.0
        || h <= 0.0
        || w > 32768.0
        || h > 32768.0
        || w * h > 64.0 * 1024.0 * 1024.0
    {
        None
    } else {
        Some((w as usize, h as usize))
    }
}

pub fn prepare_image(
    node: &Value,
    texture: &TextureAsset,
    rect: ImageRect,
    radius: [f64; 4],
    transform: [f64; 6],
    scale: f64,
    cache: &mut ImageCache,
) -> Option<ImageLayer> {
    let image = &node["image"];
    let alpha = alpha8(1.0 - n(&image["transparency"], 0.0));
    if alpha == 0 {
        return None;
    }
    let quads = image_quads(
        image,
        [texture.width, texture.height],
        rect.raw,
        scale,
        or_number(&node["scale"], 1.0),
    );
    if quads.is_empty() {
        return None;
    }
    let translation_invariant = rect.snapped && !node["gradient"].is_object();
    let key = if translation_invariant {
        // Raw positions may round differently at another magnitude. Key the actual snapped
        // quad geometry relative to the layer, exactly as the canonical JS image cache does.
        let placed: Vec<_> = quads
            .iter()
            .map(|q| {
                (
                    snap_device(q.x0, scale) - rect.x,
                    snap_device(q.y0, scale) - rect.y,
                    snap_device(q.x1, scale) - rect.x,
                    snap_device(q.y1, scale) - rect.y,
                    q.u0,
                    q.u1,
                    q.v0,
                    q.v1,
                    q.wrap,
                )
            })
            .collect();
        serde_json::to_string(&(
            "image-local",
            texture.rgba.as_ptr() as usize,
            image,
            placed,
            rect.w,
            rect.h,
            radius,
            // The corner masks' derivative-quad parity on the target (quad_corner_mask)
            rounded_parity(radius, rect, transform),
        ))
        .ok()?
    } else {
        serde_json::to_string(&(
            "image-positioned",
            texture.rgba.as_ptr() as usize,
            image,
            &node["gradient"],
            rect,
            radius,
            transform,
            scale,
        ))
        .ok()?
    };
    if let Some(mut hit) = cache.cached(&key) {
        if translation_invariant {
            hit.x = rect.x as i32;
            hit.y = rect.y as i32;
        }
        return Some(hit);
    }
    let started = Instant::now();
    let levels = cache.levels(texture);
    if levels.is_empty() {
        return None;
    }
    let color = a(&image["color"], [1.0; 3]).map(quantize);
    let tint = color.map(|v| v as f64 / 255.0);
    let fade = alpha as f64 / 255.0;
    let pixelated = image["resample"] == "Pixelated";
    let gradient = node.get("gradient").filter(|v| v.is_object());
    let rounded = radius.iter().any(|r| *r > 0.0);
    if !rect.snapped {
        let result = rotated_image(
            texture, &levels, &quads, rect.raw, radius, transform, tint, fade, pixelated, gradient,
        )?;
        return Some(cache.store(key, result, texture.rgba.clone(), started));
    }
    let (w, h) = dims(rect.w, rect.h)?;
    // Straight bytes (gpu8 of the shaded value, as layer() converts): the draw builder blends them like
    // Studio, round((c A + D (255 - A)) / 255), one rounding
    let mut out = vec![0u8; w * h * 4];
    for quad in quads {
        let (x0, y0, x1, y1) = (
            snap_device(quad.x0, scale),
            snap_device(quad.y0, scale),
            snap_device(quad.x1, scale),
            snap_device(quad.y1, scale),
        );
        let Some((qw, qh)) = dims(x1 - x0, y1 - y0) else {
            continue;
        };
        let lod = (texture.width as f64 * (quad.u1 - quad.u0).abs() / qw as f64)
            .max(texture.height as f64 * (quad.v1 - quad.v0).abs() / qh as f64)
            .log2();
        let sample = SampleQuad {
            u0: quad.u0,
            v0: quad.v0,
            u1: quad.u1,
            v1: quad.v1,
            lod,
            wrap: quad.wrap,
            pixelated,
        };
        // A layer is keyed by its gradient, tint and transparency, so an image that animates any of them
        // misses every frame: the texture's samples (which depend on the quad alone) are kept
        let values: Arc<[f32]> = cache.quad_samples(texture, &levels, qw, qh, sample);
        // Target position of the quad (the layer sits at rect.x, rect.y; transform's translation to the target)
        let mask = (rounded && !quad.unrounded).then(|| {
            quad_corner_mask(qw, qh, radius, (x0 + transform[4]).round() as i64, (y0 + transform[5]).round() as i64)
        });
        let mesh = gradient.map(|g| gradient_mesh(g, rect.raw, color.map(|v| v as f64), alpha as f64, [x0, y0, x1, y1]));
        // A transparent sample shades to alpha 0 whatever the gradient (a 9-slice frame's empty centre), and to
        // the gradient's colour alone: with one colour throughout, those pixels skip the field
        let clear = mesh.as_ref().and_then(|mesh| mesh.uniform_colour()).map(|[r, g, b]| [r, g, b, 0.]);
        // The quad's rows of the layer (row ly is the quad's row ly - top), shaded in bands of rows, in
        // parallel when large
        let top = y0 as i64 - rect.y as i64;
        let (first, last) = (top.max(0) as usize, (top + qh as i64).min(h as i64).max(0) as usize);
        if first >= last {
            continue;
        }
        let shade_band = |ly0: usize, rows: &mut [u8]| {
            let (j0, n) = ((ly0 as i64 - top) as usize, rows.len() / (w * 4));
            // The band's gradient field (GradientMesh::field rows j0..j0 + n of the quad)
            let vertices = mesh.as_ref().map(|mesh| {
                mesh.field_where([x0, y0 + j0 as f64, x1, y0 + (j0 + n) as f64], |k| {
                    clear.is_none() || values[(j0 * qw + k) * 4 + 3] != 0.
                })
            });
            for (b, row) in rows.chunks_exact_mut(w * 4).enumerate() {
                let j = j0 + b;
                for i in 0..qw {
                    let lx = x0 as i64 - rect.x as i64 + i as i64;
                    if lx < 0 || lx >= w as i64 {
                        continue;
                    }
                    let k = (j * qw + i) * 4;
                    let dest = lx as usize * 4;
                    let coverage = mask.as_ref().map_or(1.0, |v| v[j * qw + i] as f64);
                    let vertex = match (&clear, &vertices) {
                        (Some(clear), Some(_)) if values[k + 3] == 0. => Some(&clear[..]),
                        (_, Some(v)) => Some(&v[(b * qw + i) * 4..(b * qw + i) * 4 + 4]),
                        _ => None,
                    };
                    let rgba = shade(values[k..k + 4].try_into().unwrap(), tint, fade, coverage, None, vertex);
                    for c in 0..4 {
                        row[dest + c] = gpu8(rgba[c] as f64);
                    }
                }
            }
        };
        let rows = &mut out[first * w * 4..last * w * 4];
        if super::sampling::parallel_rows(qw * qh) {
            use rayon::prelude::*;
            let band = (super::sampling::PARALLEL_PIXELS / qw).max(1);
            rows.par_chunks_mut(band * w * 4).enumerate().for_each(|(i, band_rows)| shade_band(first + i * band, band_rows));
        } else {
            shade_band(first, rows);
        }
    }
    // Snapped layer coordinates are local; translations are retained for the draw builder. A gradient's
    // layer is on probation: an animated gradient's every frame is a one-off, quick to shade again
    let layer = ImageLayer {
        x: rect.x as i32,
        y: rect.y as i32,
        width: w as u32,
        height: h as u32,
        rgba: out.into(),
        premultiplied: false,
    };
    Some(cache.store_with(key, layer, texture.rgba.clone(), started, gradient.is_some()))
}

/// Coverage of a rotated image quad at local point (px, py) from its centre (half size hw x hh), as measured for
/// rotated frames: a plain quad gets the 1 px outward fringe clamp(1 - d); a rounded one (`radius`) the corner
/// SDF smoothstep(0.5 - d / fwidth), fwidth from the inverse transform `inv`, over the quad grown by 0.09 px.
/// The GPU path (uber.frag.glsl rotatedCoverage) has no grow; notes/lead.md calls the fitted 0.09 px grow
/// wrong for rounded frames.
fn rotated_coverage(
    px: f64,
    py: f64,
    hw: f64,
    hh: f64,
    radius: Option<[f64; 4]>,
    inv: [f64; 6],
) -> f64 {
    let Some(radii) = radius else {
        let (qx, qy) = (px.abs() - hw, py.abs() - hh);
        // Past a corner the nearer edge's ramp alone: a mitred corner, as a turned frame's
        return (1.0 - qx.max(qy)).clamp(0.0, 1.0);
    };
    let grow = 0.09;
    if px < -hw - grow || px >= hw + grow || py < -hh - grow || py >= hh + grow {
        return 0.0;
    }
    let r = if py < 0.0 {
        if px < 0.0 { radii[0] } else { radii[1] }
    } else if px < 0.0 {
        radii[3]
    } else {
        radii[2]
    }
    .max(0.0);
    let (qx, qy) = (px.abs() - hw + r, py.abs() - hh + r);
    let (d, gx, gy) = if qx > 0.0 && qy > 0.0 {
        let len = qx.hypot(qy);
        (len - r, px.signum() * qx / len, py.signum() * qy / len)
    } else {
        (
            qx.max(qy) - r - grow,
            if qx >= qy { px.signum() } else { 0.0 },
            if qx >= qy { 0.0 } else { py.signum() },
        )
    };
    let fw = (gx * inv[0] + gy * inv[1]).abs() + (gx * inv[2] + gy * inv[3]).abs();
    let t = (0.5 - d / fw).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Measured fringe sampling of rotated images (notes/images.md rounds 6, 11 and "Rotated images at Studio 0.747";
/// fixtures/image-probe-rotstrip.luau, image-probe-rotlod.luau, image-probe-rotchain.luau): Studio's
/// anti-aliasing strip around each edge is two triangles whose outer vertices sit 1 px past the corners (mitred)
/// and carry the corners' UVs, the UV affine per triangle. At distance d outside an edge of length L and t from
/// its start corner the sample lies t + d along it up to t = L - d (L + 1), then (t - d + 2) L / (L + 2). Across
/// the edge it lies on the edge line, whether the edge is the texture's border or an ImageRect crop's. Edges
/// [top, right, bottom, left] run clockwise.
pub fn rotated_fringe_sample(lx: f64, ly: f64, b: [f64; 4]) -> [f64; 2] {
    let (w, h) = (b[2] - b[0], b[3] - b[1]);
    let (dt, dr, db, dl) = (b[1] - ly, lx - b[2], ly - b[3], b[0] - lx);
    let d = dt.max(dr).max(db).max(dl);
    // Position along an edge of length l from its start corner, as a fraction of l
    let along = |t: f64, l: f64| {
        if t < l - d * (l + 1.0) {
            (t + d) / l
        } else {
            (t - d + 2.0) / (l + 2.0)
        }
    };
    if d == dt {
        [b[0] + along(lx - b[0], w) * w, b[1]]
    } else if d == dr {
        [b[2], b[1] + along(ly - b[1], h) * h]
    } else if d == db {
        [b[2] - along(b[2] - lx, w) * w, b[3]]
    } else {
        [b[0], b[3] - along(b[3] - ly, h) * h]
    }
}

/// The mip level of a fringe pixel (rotated_fringe_sample), from its strip triangle's own texture coordinates:
/// they do not change across the edge, and along it they run as the triangle lays them (t + d, or t - d
/// squeezed by L / (L + 2)), so a pixel spans the edge's `density` (texels a local px along [x, y]) along one
/// diagonal of the frame, whatever the image's density across the edge. `inv` maps screen to local. A thin
/// image minified across only (16 texels over 9 px, 0.07 texel a px along) keeps level 0 in its long fringes
/// where its inside reads level 0.83; a square one at 1.2 px a texel reads level 0.1 there and 0 inside, which
/// the first model took for a sample 0.7 px inside the edge.
pub fn rotated_fringe_lod(lx: f64, ly: f64, b: [f64; 4], density: [f64; 2], inv: [f64; 6]) -> f64 {
    let (w, h) = (b[2] - b[0], b[3] - b[1]);
    let (dt, dr, db, dl) = (b[1] - ly, lx - b[2], ly - b[3], b[0] - lx);
    let d = dt.max(dr).max(db).max(dl);
    let (gx, gy) = if d == dt || d == db {
        let t = if d == dt { lx - b[0] } else { b[2] - lx };
        let first = t < w - d * (w + 1.0);
        let s = density[0] * if first { 1.0 } else { w / (w + 2.0) };
        (s, if (d == dt) == first { -s } else { s })
    } else {
        let t = if d == dr { ly - b[1] } else { b[3] - ly };
        let first = t < h - d * (h + 1.0);
        let s = density[1] * if first { 1.0 } else { h / (h + 2.0) };
        (s, if (d == dr) == first { s } else { -s })
    };
    super::sampling::footprint_length(
        [gx * inv[0] + gy * inv[1], 0.0],
        [gx * inv[2] + gy * inv[3], 0.0],
    )
    .log2()
}

fn rotated_image(
    texture: &TextureAsset,
    levels: &[MipLevel],
    quads: &[ImageQuad],
    raw: [f64; 4],
    radius: [f64; 4],
    transform: [f64; 6],
    tint: [f64; 3],
    fade: f64,
    pixelated: bool,
    gradient: Option<&Value>,
) -> Option<ImageLayer> {
    let sampler = gradient.map(|g| GradientSampler::new(g, raw));
    let inv = invert(transform);
    if !inv.iter().all(|v| v.is_finite()) {
        return None;
    }
    let mut bounds = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for q in quads {
        bounds[0] = bounds[0].min(q.x0);
        bounds[1] = bounds[1].min(q.y0);
        bounds[2] = bounds[2].max(q.x1);
        bounds[3] = bounds[3].max(q.y1);
    }
    let mut screen = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for p in [
        [bounds[0], bounds[1]],
        [bounds[2], bounds[1]],
        [bounds[0], bounds[3]],
        [bounds[2], bounds[3]],
    ] {
        let p = apply(transform, p[0], p[1]);
        screen[0] = screen[0].min(p[0]);
        screen[1] = screen[1].min(p[1]);
        screen[2] = screen[2].max(p[0]);
        screen[3] = screen[3].max(p[1]);
    }
    let (bx, by) = (screen[0].floor() - 2.0, screen[1].floor() - 2.0);
    let (w, h) = dims(screen[2].ceil() + 2.0 - bx, screen[3].ceil() + 2.0 - by)?;
    let quarter = (transform[1].abs() < 1e-6 && transform[2].abs() < 1e-6)
        || (transform[0].abs() < 1e-6 && transform[3].abs() < 1e-6);
    let rounded = radius.iter().any(|r| *r > 0.0);
    let (hw, hh) = ((bounds[2] - bounds[0]) / 2.0, (bounds[3] - bounds[1]) / 2.0);
    // Texels a local px along each axis, and the mip level inside the quad
    let densities: Vec<[f64; 2]> = quads
        .iter()
        .map(|q| {
            [
                texture.width as f64 * (q.u1 - q.u0).abs() / (q.x1 - q.x0),
                texture.height as f64 * (q.v1 - q.v0).abs() / (q.y1 - q.y0),
            ]
        })
        .collect();
    let lods: Vec<f64> = densities
        .iter()
        .map(|[su, sv]| {
            super::sampling::footprint_length([inv[0] * su, inv[1] * sv], [inv[2] * su, inv[3] * sv])
                .log2()
        })
        .collect();
    // The quad's corners sit on the rasterizer's 1/256 px grid, so each of its two triangles has its own map
    // (geometry.rs snapped_quad_maps)
    let (maps, diagonal) = if quarter {
        ([inv, inv], [0., 0., 1.])
    } else {
        super::geometry::snapped_quad_maps(transform, bounds, [0., 0.], [0., 0.])
    };
    let mut out = vec![0.0; w * h * 4];
    for j in 0..h {
        for i in 0..w {
            let (x, y) = (bx + i as f64 + 0.5, by + j as f64 + 0.5);
            let first = diagonal[0] * x + diagonal[1] * y + diagonal[2] >= 0.;
            let [lx, ly] = apply(maps[if first { 0 } else { 1 }], x, y);
            let (qx, qy) = (
                lx.clamp(bounds[0], bounds[2]),
                ly.clamp(bounds[1], bounds[3]),
            );
            let Some((index, q)) = quads
                .iter()
                .enumerate()
                .find(|(_, q)| qx >= q.x0 && qx <= q.x1 && qy >= q.y0 && qy <= q.y1)
            else {
                continue;
            };
            let coverage = if rounded {
                let (qw, qh) = ((q.x1 - q.x0) / 2.0, (q.y1 - q.y0) / 2.0);
                let radii = if q.unrounded { [0.0; 4] } else { radius };
                rotated_coverage(lx - q.x0 - qw, ly - q.y0 - qh, qw, qh, Some(radii), inv)
            } else if quarter {
                if x >= screen[0] && x < screen[2] && y >= screen[1] && y < screen[3] {
                    1.0
                } else {
                    0.0
                }
            } else {
                rotated_coverage(lx - bounds[0] - hw, ly - bounds[1] - hh, hw, hh, None, inv)
            };
            if coverage <= 0.0 {
                continue;
            }
            // A fringe pixel outside the quad samples where Studio's anti-aliasing strip puts it
            // (rotated_fringe_sample)
            let (mut sx, mut sy, mut index, mut q) = (lx, ly, index, q);
            let fringe = !quarter && (lx < bounds[0] || lx > bounds[2] || ly < bounds[1] || ly > bounds[3]);
            if fringe {
                [sx, sy] = rotated_fringe_sample(lx, ly, bounds);
                if let Some((k, found)) = quads
                    .iter()
                    .enumerate()
                    .find(|(_, q)| sx >= q.x0 && sx <= q.x1 && sy >= q.y0 && sy <= q.y1)
                {
                    index = k;
                    q = found;
                }
            }
            let u = q.u0 + (sx - q.x0) / (q.x1 - q.x0) * (q.u1 - q.u0);
            let v = q.v0 + (sy - q.y0) / (q.y1 - q.y0) * (q.v1 - q.v0);
            let lod = if fringe {
                rotated_fringe_lod(lx, ly, bounds, densities[index], inv)
            } else {
                lods[index]
            };
            let sample = sample_at(levels, u, v, lod, q.wrap, pixelated);
            out[(j * w + i) * 4..(j * w + i + 1) * 4].copy_from_slice(&shade(
                sample,
                tint,
                fade,
                coverage,
                sampler.as_ref().map(|g| g.at(lx, ly)),
                None,
            ));
        }
    }
    Some(layer(bx as i32, by as i32, w, h, out, false))
}

pub fn plan_texture_image(
    node: &Value,
    texture: &TextureAsset,
    rect: ImageRect,
    radius: [f64; 4],
    space: ImageSpace,
    scale: f64,
    cache: &mut ImageCache,
) -> Option<PlannedLayer> {
    let image = &node["image"];
    let alpha = alpha8(1.0 - n(&image["transparency"], 0.0));
    if alpha == 0 || space.sx <= 0.0 || space.sy <= 0.0 {
        return None;
    }
    let quads = image_quads(
        image,
        [texture.width, texture.height],
        rect.raw,
        scale,
        or_number(&node["scale"], 1.0),
    );
    let tx = move |e: f64| ((e - space.l) * space.sx - 0.5).ceil();
    let ty = move |e: f64| ((e - space.t) * space.sy - 0.5).ceil();
    let (mut x0, mut y0, mut x1, mut y1) = (
        tx(rect.x),
        ty(rect.y),
        tx(rect.x + rect.w),
        ty(rect.y + rect.h),
    );
    if node["clip"].is_array() {
        let [cx, cy, cw, ch] = a(&node["clip"], [0.0; 4]);
        let [cl, cr] = device_span(cx, cw, scale);
        let [ct, cb] = device_span(cy, ch, scale);
        x0 = x0.max(tx(cl));
        y0 = y0.max(ty(ct));
        x1 = x1.min(tx(cr));
        y1 = y1.min(ty(cb));
    }
    x0 = x0.max(0.0);
    y0 = y0.max(0.0);
    x1 = x1.min(space.nw as f64);
    y1 = y1.min(space.nh as f64);
    let (w, h) = dims(x1 - x0, y1 - y0)?;
    let tint = a(&image["color"], [1.0; 3]).map(|v| quantize(v) as f64 / 255.0);
    let fade = alpha as f64 / 255.0;
    let pixelated = image["resample"] == "Pixelated";
    let gradient = node.get("gradient").filter(|v| v.is_object());
    // Everything the raster reads, relative to the layer (its placement aside): the key, so a frame
    // whose snapped geometry only moved in the group texture reuses the layer bit for bit
    let (texture_width, texture_height) = (texture.width as f64, texture.height as f64);
    let jobs: Vec<QuadJob> = quads
        .iter()
        .filter_map(|q| {
            let (qx0, qy0, qx1, qy1) = (
                snap_device(q.x0, scale),
                snap_device(q.y0, scale),
                snap_device(q.x1, scale),
                snap_device(q.y1, scale),
            );
            let (qw, qh) = (qx1 - qx0, qy1 - qy0);
            if qw <= 0.0 || qh <= 0.0 {
                return None;
            }
            let (tx0, ty0) = (tx(qx0), ty(qy0));
            let (cw, ch) = dims(tx(qx1) - tx0, ty(qy1) - ty0)?;
            let grid = [
                tx0 + 0.5 - (qx0 - space.l) * space.sx,
                ty0 + 0.5 - (qy0 - space.t) * space.sy,
                qw * space.sx,
                qh * space.sy,
            ];
            let lod = (texture_width * (q.u1 - q.u0).abs() / grid[2])
                .max(texture_height * (q.v1 - q.v0).abs() / grid[3])
                .log2();
            let ix0 = ((x0 - tx0).max(0.0) as usize).min(cw);
            let iy0 = ((y0 - ty0).max(0.0) as usize).min(ch);
            let ix1 = ((x1 - tx0).max(0.0) as usize).min(cw);
            let iy1 = ((y1 - ty0).max(0.0) as usize).min(ch);
            if ix0 >= ix1 || iy0 >= iy1 {
                return None;
            }
            Some(QuadJob {
                dx: (tx0 - x0) as i64,
                dy: (ty0 - y0) as i64,
                size: [cw, ch],
                region: [ix0, iy0, ix1, iy1],
                grid,
                lod,
                uv: [q.u0, q.v0, q.u1, q.v1],
                wrap: q.wrap,
                unrounded: q.unrounded,
                quad: [qw, qh],
                // Only a UIGradient reads group texture positions
                at: gradient.map(|_| [tx0, ty0]),
            })
        })
        .collect();
    let key = format!(
        "texture:{}",
        serde_json::to_string(&(
            texture.rgba.as_ptr() as usize,
            [w, h],
            tint,
            fade,
            pixelated,
            radius,
            [space.sx, space.sy],
            &jobs,
            gradient.map(|g| (g, rect.raw, [space.l, space.t])),
        ))
        .ok()?
    );
    if let Some(mut hit) = cache.cached(&key) {
        hit.x = x0 as i32;
        hit.y = y0 as i32;
        return Some(PlannedLayer::Ready(hit));
    }
    let levels = cache.levels(texture);
    if levels.is_empty() {
        return None;
    }
    let gradient = gradient.map(|g| GradientSampler::new(g, rect.raw));
    let rounded = radius.iter().any(|r| *r > 0.0);
    let raster = move || {
        let mut taps = TapCache::default();
        let mut out = vec![0.0; w * h * 4];
        for job in jobs {
            let [cw, ch] = job.size;
            let [ix0, iy0, ix1, iy1] = job.region;
            let [qw, qh] = job.quad;
            let grid = SamplingGrid {
                off_x: job.grid[0],
                off_y: job.grid[1],
                span_x: job.grid[2],
                span_y: job.grid[3],
            };
            let values = sample_quad_region(
                &levels,
                cw,
                ch,
                SampleQuad {
                    u0: job.uv[0],
                    v0: job.uv[1],
                    u1: job.uv[2],
                    v1: job.uv[3],
                    lod: job.lod,
                    wrap: job.wrap,
                    pixelated,
                },
                grid,
                &mut taps,
                [ix0, iy0, ix1, iy1],
            );
            // Each output row receives exactly one source row of this quad, so rows can be shaded
            // independently; later quads still overwrite earlier ones in order.
            let shade_row = |ly: usize, row: &mut [f32]| {
                let j = ly as i64 - job.dy;
                if j < iy0 as i64 || j >= iy1 as i64 {
                    return;
                }
                let j = j as usize;
                for i in ix0..ix1 {
                    let lx = job.dx + i as i64;
                    if lx < 0 || lx >= w as i64 {
                        continue;
                    }
                    let coverage = if rounded && !job.unrounded {
                        let c = corner(
                            (i as f64 + grid.off_x) / space.sx - qw / 2.0,
                            (j as f64 + grid.off_y) / space.sy - qh / 2.0,
                            qw / 2.0,
                            qh / 2.0,
                            radius,
                            space.sx,
                            space.sy,
                        );
                        (((c * 255.0 + 0.5).floor() / 255.0) as f32) as f64
                    } else {
                        1.0
                    };
                    let k = ((j - iy0) * (ix1 - ix0) + i - ix0) * 4;
                    let dest = lx as usize * 4;
                    let shaded = shade(
                        values[k..k + 4].try_into().unwrap(),
                        tint,
                        fade,
                        coverage,
                        // The UIGradient at the texel centre's screen point (L + (X + 0.5) / sx,
                        // T + (Y + 0.5) / sy), over the unsnapped rect like any image gradient
                        gradient.as_ref().zip(job.at).map(|(g, [tx0, ty0])| {
                            g.at(
                                space.l + (tx0 + i as f64 + 0.5) / space.sx,
                                space.t + (ty0 + j as f64 + 0.5) / space.sy,
                            )
                        }),
                        None,
                    );
                    row[dest..dest + 4].copy_from_slice(&shaded);
                }
            };
            // Jobs already run in parallel with each other (DrawPreparer::run_pending): rows split
            // only when called from outside the worker pool
            if rayon::current_thread_index().is_none()
                && (ix1 - ix0) * (iy1 - iy0) >= super::sampling::PARALLEL_PIXELS
            {
                use rayon::prelude::*;
                out.par_chunks_mut(w * 4)
                    .enumerate()
                    .for_each(|(ly, row)| shade_row(ly, row));
            } else {
                for (ly, row) in out.chunks_mut(w * 4).enumerate() {
                    shade_row(ly, row);
                }
            }
        }
        layer(0, 0, w, h, out, false).rgba.to_vec()
    };
    Some(PlannedLayer::Pending(PendingLayer {
        x: x0 as i32,
        y: y0 as i32,
        width: w as u32,
        height: h as u32,
        premultiplied: false,
        key,
        source: texture.rgba.clone(),
        job: Box::new(raster),
        probation: false,
    }))
}

/// One image quad's share of a group texture layer, relative to the layer (plan_texture_image)
#[derive(serde::Serialize)]
struct QuadJob {
    dx: i64,
    dy: i64,
    size: [usize; 2],
    region: [usize; 4],
    grid: [f64; 4],
    lod: f64,
    uv: [f64; 4],
    wrap: bool,
    unrounded: bool,
    quad: [f64; 2],
    at: Option<[f64; 2]>,
}

/// prepare_texture_image, with a miss's pixels left to its job (PendingLayer)
pub fn prepare_texture_image(
    node: &Value,
    texture: &TextureAsset,
    rect: ImageRect,
    radius: [f64; 4],
    space: ImageSpace,
    scale: f64,
    cache: &mut ImageCache,
) -> Option<ImageLayer> {
    Some(match plan_texture_image(node, texture, rect, radius, space, scale, cache)? {
        PlannedLayer::Ready(layer) => layer,
        PlannedLayer::Pending(pending) => pending.finish(cache),
    })
}

#[cfg(test)]
mod cache_tests {
    use super::*;

    #[test]
    fn a_source_image_layers_share_is_charged_once_and_released_after_last_layer() {
        let mut cache = ImageCache::default();
        let source: Arc<[u8]> = vec![0; 1024 * 1024].into();
        let replacement: Arc<[u8]> = vec![1; 1024 * 1024].into();
        let pixel = ImageLayer {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            rgba: vec![0; 4].into(),
            premultiplied: true,
        };
        for i in 0..512 {
            assert!(cache.cached(&format!("first:{i}")).is_none());
            cache.store(
                format!("first:{i}"),
                pixel.clone(),
                source.clone(),
                Instant::now(),
            );
        }
        assert_eq!(cache.stats().layer_evictions, 0);
        assert_eq!(cache.stats().layer_entries, 512);
        assert!(cache.stats().layer_bytes < source.len() + 128 * 1024);
        assert_eq!(cache.layer_sources.len(), 1);
        for i in 0..512 {
            assert!(cache.cached(&format!("second:{i}")).is_none());
            cache.store(
                format!("second:{i}"),
                pixel.clone(),
                replacement.clone(),
                Instant::now(),
            );
        }
        assert_eq!(cache.stats().layer_evictions, 512);
        assert_eq!(Arc::strong_count(&source), 1);
        assert_eq!(cache.layer_sources.len(), 1);
        assert!(cache.stats().layer_bytes < replacement.len() + 128 * 1024);
        cache.clear();
        assert_eq!(cache.bytes(), 0);
        assert!(cache.layer_sources.is_empty());
        assert_eq!(Arc::strong_count(&replacement), 1);
    }

    #[test]
    fn probation_layers_leave_first_unless_hit() {
        let mut cache = ImageCache::default();
        let source: Arc<[u8]> = Vec::new().into();
        let pixel = ImageLayer { x: 0, y: 0, width: 1, height: 1, rgba: vec![0; 4].into(), premultiplied: true };
        for i in 0..100 {
            cache.store(format!("kept:{i}"), pixel.clone(), source.clone(), Instant::now());
        }
        cache.store_with("tint:hit".into(), pixel.clone(), source.clone(), Instant::now(), true);
        assert!(cache.cached("tint:hit").is_some(), "a hit takes a layer off probation");
        for i in 0..PROBATION * 3 {
            cache.store_with(format!("tint:{i}"), pixel.clone(), source.clone(), Instant::now(), true);
        }
        // The oldest one-off layers left; the regular layers and the one that was hit stayed
        assert!(cache.cached("tint:0").is_none());
        assert!(cache.cached(&format!("tint:{}", PROBATION * 3 - 1)).is_some());
        assert!(cache.cached("tint:hit").is_some());
        assert!((0..100).all(|i| cache.cached(&format!("kept:{i}")).is_some()));
        assert_eq!(cache.stats().layer_entries, 100 + 1 + PROBATION);
    }
    /// Large one-off layers leave probation by bytes (PROBATION_BYTES) before their count fills the budget,
    /// and one stored again after leaving unhit is kept outright (ghosts)
    #[test]
    fn probation_is_bounded_by_bytes_and_returning_layers_stay() {
        let mut cache = ImageCache::default();
        let source: Arc<[u8]> = Vec::new().into();
        let big = |n: u8| ImageLayer { x: 0, y: 0, width: 1024, height: 1024, rgba: vec![n; 1024 * 1024 * 4].into(), premultiplied: true };
        cache.store("kept".into(), big(0), source.clone(), Instant::now());
        for i in 0..20 {
            cache.store_with(format!("frame:{i}"), big(1), source.clone(), Instant::now(), true);
        }
        assert!(cache.probation_bytes <= PROBATION_BYTES);
        assert!(cache.cached("kept").is_some(), "one-offs never push a kept layer out");
        assert!(cache.cached("frame:0").is_none());
        cache.store_with("frame:0".into(), big(1), source.clone(), Instant::now(), true);
        for i in 20..40 {
            cache.store_with(format!("frame:{i}"), big(1), source.clone(), Instant::now(), true);
        }
        assert!(cache.cached("frame:0").is_some(), "a layer that came back is kept");
    }
}
