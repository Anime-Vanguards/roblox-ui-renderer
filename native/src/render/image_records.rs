//! Canonical IMAGE / IMAGE_ROTATED packets sample retained source mip textures on the GPU.
use super::{
    geometry::*,
    images::{ROTATED_FRINGE_INSET, dims, image_quads, quad_corner_mask, rotated_fringe_insets},
    packet::*,
    sampling::{derivative_length, mip_levels, pixelated_level, texture_levels},
    scene_draw::{BuildContext, DrawPreparer},
};
use crate::assets::{AssetId, TextureAsset};
use serde_json::Value;
use std::{collections::HashMap, sync::Arc};

const BUDGET: usize = 96 * 1024 * 1024;
const MASK_BUDGET: usize = 16 * 1024 * 1024;
#[derive(Clone, Copy, Hash, PartialEq, Eq)]
struct MaskKey {
    width: u32,
    height: u32,
    radii: [u64; 4],
    /// The target position's parity: quad_corner_mask's coarse derivative quads
    parity: [u8; 2],
}
struct MaskTexture {
    id: TextureId,
    bytes: usize,
    last: u64,
}
struct SourceTexture {
    id: TextureId,
    source: Arc<[u8]>,
    width: u32,
    height: u32,
    levels: usize,
    bytes: usize,
    last: u64,
}
#[derive(Default)]
pub(crate) struct GpuImageCache {
    entries: HashMap<AssetId, SourceTexture>,
    ids: rustc_hash::FxHashMap<TextureId, AssetId>,
    bytes: usize,
    masks: rustc_hash::FxHashMap<MaskKey, MaskTexture>,
    mask_ids: rustc_hash::FxHashMap<TextureId, MaskKey>,
    mask_bytes: usize,
}
impl GpuImageCache {
    /// Whether `id` is one of this cache's resident textures
    pub(crate) fn contains(&self, id: TextureId) -> bool {
        self.ids.contains_key(&id) || self.mask_ids.contains_key(&id)
    }
    /// Whether evict() would release anything (it keeps the textures touched this frame)
    pub(crate) fn over_budget(&self) -> bool {
        self.bytes > BUDGET || self.mask_bytes > MASK_BUDGET || self.masks.len() > 512
    }
    pub(crate) fn clear(&mut self) -> Vec<TextureId> {
        let ids = self
            .ids
            .keys()
            .chain(self.mask_ids.keys())
            .copied()
            .collect();
        self.entries.clear();
        self.ids.clear();
        self.bytes = 0;
        self.masks.clear();
        self.mask_ids.clear();
        self.mask_bytes = 0;
        ids
    }
    pub(crate) fn bytes(&self) -> usize {
        self.bytes + self.mask_bytes
    }
    pub(crate) fn touch(&mut self, id: TextureId, frame: u64) -> bool {
        if let Some(key) = self.mask_ids.get(&id) {
            if let Some(entry) = self.masks.get_mut(key) {
                entry.last = frame;
                return true;
            }
        }
        let Some(key) = self.ids.get(&id) else {
            return false;
        };
        let Some(entry) = self.entries.get_mut(key) else {
            return false;
        };
        entry.last = frame;
        true
    }
    pub(crate) fn evict(&mut self, frame: u64) -> Vec<TextureId> {
        let mut retired = Vec::new();
        while self.bytes > BUDGET {
            let key = self
                .entries
                .iter()
                .filter(|(_, e)| e.last < frame)
                .min_by_key(|(_, e)| e.last)
                .map(|(k, _)| k.clone());
            let Some(key) = key else { break };
            let e = self.entries.remove(&key).unwrap();
            self.ids.remove(&e.id);
            self.bytes -= e.bytes;
            retired.push(e.id);
        }
        while self.mask_bytes > MASK_BUDGET || self.masks.len() > 512 {
            let oldest = self
                .masks
                .iter()
                .filter(|(_, e)| e.last < frame)
                .min_by_key(|(_, e)| e.last)
                .map(|(key, _)| *key);
            let Some(key) = oldest else { break };
            let entry = self.masks.remove(&key).unwrap();
            self.mask_ids.remove(&entry.id);
            self.mask_bytes -= entry.bytes;
            retired.push(entry.id);
        }
        retired
    }
}
impl DrawPreparer {
    fn image_mask_texture(&mut self, width: u32, height: u32, radii: [f64; 4], ox: f64, oy: f64) -> TextureId {
        let parity = [ox, oy].map(|v| (v.round() as i64).rem_euclid(2) as u8);
        let key = MaskKey {
            width,
            height,
            radii: radii.map(f64::to_bits),
            parity,
        };
        if let Some(entry) = self.gpu_image_cache.masks.get_mut(&key) {
            entry.last = self.frame;
            self.stats.upload_cache_hits += 1;
            return entry.id;
        }
        let coverage = quad_corner_mask(width as usize, height as usize, radii, parity[0] as i64, parity[1] as i64);
        let bytes = bytemuck::cast_slice(&coverage).to_vec();
        let size = bytes.len();
        let id = self.texture_id();
        self.packet.textures.push(TextureUpload {
            id,
            revision: 1,
            format: TextureFormat::R32Float,
            levels: vec![TextureLevel {
                width,
                height,
                bytes: bytes.into(),
            }],
        });
        self.stats.pixel_bytes_processed += size as u64;
        self.gpu_image_cache.masks.insert(
            key,
            MaskTexture {
                id,
                bytes: size,
                last: self.frame,
            },
        );
        self.gpu_image_cache.mask_ids.insert(id, key);
        self.gpu_image_cache.mask_bytes += size;
        id
    }

    fn image_source_texture(&mut self, asset: &TextureAsset) -> Option<(TextureId, usize)> {
        if let Some(entry) = self.gpu_image_cache.entries.get_mut(&asset.id) {
            if entry.width == asset.width
                && entry.height == asset.height
                && Arc::ptr_eq(&entry.source, &asset.rgba)
            {
                entry.last = self.frame;
                self.stats.upload_cache_hits += 1;
                return Some((entry.id, entry.levels));
            }
        }
        let levels = texture_levels(
            asset.width as usize,
            asset.height as usize,
            asset.rgba.clone(),
        );
        if levels.is_empty() {
            return None;
        }
        let bytes = levels.iter().map(|l| l.data.len()).sum();
        let count = levels.len();
        let id = self.texture_id();
        self.packet.textures.push(TextureUpload {
            id,
            revision: 1,
            format: TextureFormat::Rgba8,
            levels: levels
                .into_iter()
                .map(|l| TextureLevel {
                    width: l.width as u32,
                    height: l.height as u32,
                    bytes: l.data.clone().into(),
                })
                .collect(),
        });
        self.stats.pixel_bytes_processed += bytes as u64;
        if let Some(old) = self.gpu_image_cache.entries.insert(
            asset.id.clone(),
            SourceTexture {
                id,
                source: asset.rgba.clone(),
                width: asset.width,
                height: asset.height,
                levels: count,
                bytes,
                last: self.frame,
            },
        ) {
            self.gpu_image_cache.bytes -= old.bytes;
            self.gpu_image_cache.ids.remove(&old.id);
            self.retired.push(old.id);
        }
        self.gpu_image_cache.ids.insert(id, asset.id.clone());
        self.gpu_image_cache.bytes += bytes;
        Some((id, count))
    }
    pub(crate) fn snapped_image_records(
        &mut self,
        ctx: &mut BuildContext,
        node: &Value,
        asset: &TextureAsset,
        p: PhysicalRect,
        radii: [f64; 4],
    ) -> anyhow::Result<()> {
        let image = &node["image"];
        let fade = alpha8(1. - number(&image["transparency"], 0.)) as f64 / 255.;
        if fade <= 0. || p.rect.w <= 0. || p.rect.h <= 0. {
            return Ok(());
        }
        anyhow::ensure!(
            dims(p.rect.w, p.rect.h).is_some(),
            "snapped image dimensions {}x{} exceed image allocation limits (32768 per axis, 64 Mi pixels)",
            p.rect.w,
            p.rect.h
        );
        let quads = image_quads(
            image,
            [asset.width, asset.height],
            [p.raw.x, p.raw.y, p.raw.w, p.raw.h],
            self.scale,
            {
                let scale = number(&node["scale"], 1.);
                if scale == 0. { 1. } else { scale }
            },
        );
        if quads.is_empty() {
            return Ok(());
        }
        // Full-quad taps and masks are allocated even when the draw is clipped.
        // Preflight every quad before uploading sources or mutating cached records.
        for q in &quads {
            let width = snap_device(q.x1, self.scale) - snap_device(q.x0, self.scale);
            let height = snap_device(q.y1, self.scale) - snap_device(q.y0, self.scale);
            if width <= 0. || height <= 0. {
                continue;
            }
            anyhow::ensure!(
                dims(width, height).is_some(),
                "snapped image quad dimensions {width}x{height} exceed image allocation limits (32768 per axis, 64 Mi pixels)"
            );
        }
        let Some((texture, levels)) = self.image_source_texture(asset) else {
            return Ok(());
        };
        let m = node_transform(node, ctx.origin, self.scale);
        let [tx, ty] = [m[4], m[5]];
        let clip = clip_rect(node, ctx.origin, self.scale);
        let pixelated = image["resample"] == "Pixelated";
        let rounded = radii.iter().any(|r| *r > 0.);
        let tint = color(&image["color"], [1.; 3]).map(|v| quantize(v) as f64 / 255.);
        let s = self.scale;
        for q in quads {
            let [x0, y0, x1, y1] = [q.x0, q.y0, q.x1, q.y1].map(|v| snap_device(v, s));
            // 9-slice parts without texels on an axis stay square (ImageQuad::unrounded)
            let rounded = rounded && !q.unrounded;
            let [width, height] = [x1 - x0, y1 - y0];
            if width <= 0. || height <= 0. {
                continue;
            }
            let visible = [
                x0.max(p.rect.x),
                y0.max(p.rect.y),
                x1.min(p.rect.x + p.rect.w),
                y1.min(p.rect.y + p.rect.h),
            ];
            if visible[2] <= visible[0] || visible[3] <= visible[1] {
                continue;
            }
            let lod = (asset.width as f64 * (q.u1 - q.u0).abs() / width)
                .max(asset.height as f64 * (q.v1 - q.v0).abs() / height)
                .log2();
            let (l0, fraction) = if pixelated {
                (pixelated_level(levels, lod), 0)
            } else {
                mip_levels(levels, lod)
            };
            let mask = rounded.then(|| self.image_mask_texture(width as u32, height as u32, radii, x0 + tx, y0 + ty));
            let start = self.begin(
                ctx,
                [
                    visible[0] + tx,
                    visible[1] + ty,
                    visible[2] + tx,
                    visible[3] + ty,
                ],
                clip,
                6,
            );
            self.push([x0 + tx, y0 + ty, width, height]);
            self.push([
                p.rect.x + tx,
                p.rect.y + ty,
                p.rect.x + p.rect.w + tx,
                p.rect.y + p.rect.h + ty,
            ]);
            self.push([
                0.,
                l0 as f64,
                fraction as f64,
                (pixelated as u8 | ((rounded as u8) << 2)) as f64,
            ]);
            self.push([0.; 4]);
            self.push([tint[0], tint[1], tint[2], fade]);
            self.push([if rounded { 1. } else { 0. }, 0., 0., 0.]);
            self.push([0.; 4]);
            // The draw may be clipped, but sampling still spans the original full quad.
            let mut tap_offsets = [0.; 4];
            for (index, level, horizontal) in [
                (0, l0, true),
                (1, l0, false),
                (2, l0 + 1, true),
                (3, l0 + 1, false),
            ] {
                if index >= 2 && fraction == 0 {
                    break;
                }
                let (count, from, to, size) = if horizontal {
                    (
                        width as usize,
                        q.u0,
                        q.u1,
                        (asset.width >> level).max(1) as usize,
                    )
                } else {
                    (
                        height as usize,
                        q.v0,
                        q.v1,
                        (asset.height >> level).max(1) as usize,
                    )
                };
                let taps = self.image_cache.taps.get(
                    pixelated,
                    count,
                    from,
                    to,
                    size,
                    q.wrap,
                    0.5,
                    count as f64,
                );
                tap_offsets[index] = (self.packet.texels.len() - start) as f32;
                self.packet.texels.extend_from_slice(&taps);
            }
            self.packet.texels[start + 6] = tap_offsets;
            // Straight colour and alpha, blended like Studio: round((c A + D (255 - A)) / 255), one rounding
            let mut batch = DrawBatch {
                blend: BlendMode::Image,
                ..DrawBatch::default()
            };
            batch.image_slots[0] = Some(texture);
            batch.image_slots[1] = mask;
            self.emit(ctx, start, batch);
        }
        Ok(())
    }
    pub(crate) fn rotated_image_record(
        &mut self,
        ctx: &mut BuildContext,
        node: &Value,
        asset: &TextureAsset,
        p: PhysicalRect,
        radii: [f64; 4],
    ) {
        let image = &node["image"];
        let fade = alpha8(1. - number(&image["transparency"], 0.)) as f64 / 255.;
        if fade <= 0. {
            return;
        }
        let raw = [p.raw.x, p.raw.y, p.raw.w, p.raw.h];
        let quads = image_quads(image, [asset.width, asset.height], raw, self.scale, {
            let scale = number(&node["scale"], 1.);
            if scale == 0. { 1. } else { scale }
        });
        if quads.is_empty() {
            return;
        }
        let mut bounds = [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ];
        for q in &quads {
            bounds[0] = bounds[0].min(q.x0);
            bounds[1] = bounds[1].min(q.y0);
            bounds[2] = bounds[2].max(q.x1);
            bounds[3] = bounds[3].max(q.y1);
        }
        if bounds[2] <= bounds[0] || bounds[3] <= bounds[1] {
            return;
        }
        let m = node_transform(node, ctx.origin, self.scale);
        let inv = invert(m);
        let rect = Rect::new(
            bounds[0],
            bounds[1],
            bounds[2] - bounds[0],
            bounds[3] - bounds[1],
        );
        let screen = transformed_extent(m, rect);
        let raster = transformed_bounds(m, rect, 2.);
        let clip = clip_rect(node, ctx.origin, self.scale);
        // Reject only disjoint record bounds; edge coverage has the canonical two-pixel pad.
        let c = clip.unwrap_or(NO_CLIP);
        if raster[2] <= c[0].max(0.)
            || raster[0] >= c[2].min(ctx.width as f64)
            || raster[3] <= c[1].max(0.)
            || raster[1] >= c[3].min(ctx.height as f64)
        {
            return;
        }
        let Some((texture, levels)) = self.image_source_texture(asset) else {
            return;
        };
        let quarter =
            (m[1].abs() < 1e-6 && m[2].abs() < 1e-6) || (m[0].abs() < 1e-6 && m[3].abs() < 1e-6);
        let rounded = radii.iter().any(|v| *v > 0.);
        let flags = (image["resample"] == "Pixelated") as u8
            | ((quarter as u8) << 1)
            | ((rounded as u8) << 2);
        let start = self.begin(ctx, raster, clip, 7);
        for v in local_texels(inv, [raster[0], raster[1]], [0., 0.]) {
            self.push(v)
        }
        self.push(bounds);
        self.push(screen);
        self.push([0., quads.len() as f64, flags as f64, levels as f64]);
        let tint = color(&image["color"], [1.; 3]).map(|v| quantize(v) as f64 / 255.);
        self.push([tint[0], tint[1], tint[2], fade]);
        self.push(radii.map(|r| r.max(0.)));
        let gradient = node.get("gradient").filter(|v| v.is_object());
        // Fringe sampling (images.rs rotated_fringe_sample): the edges that take the inset, as bits
        let insets = rotated_fringe_insets(&quads, bounds);
        let mask = (0..4).fold(0u8, |bits, i| bits | (((insets[i] > 0.) as u8) << i));
        self.push([gradient.is_some() as u8 as f64, 0., mask as f64, ROTATED_FRINGE_INSET]);
        debug_assert_eq!(self.packet.texels.len() - start, 11);
        for q in quads {
            let su = asset.width as f64 * (q.u1 - q.u0) / (q.x1 - q.x0);
            let sv = asset.height as f64 * (q.v1 - q.v0) / (q.y1 - q.y0);
            let rho = derivative_length(inv[0] * su, inv[1] * sv)
                .max(derivative_length(inv[2] * su, inv[3] * sv));
            self.push([q.x0, q.y0, q.x1, q.y1]);
            self.push([q.u0, q.v0, q.u1, q.v1]);
            self.push([rho.log2(), q.wrap as u8 as f64, q.unrounded as u8 as f64, 0.]);
        }
        if let Some(g) = gradient {
            // UIGradient multipliers: the raw keypoint values, not 8-bit stops, and t over the unsnapped
            // rect; t = 0 everywhere on a zero-width or zero-height box (its first keypoint)
            let mapping = if p.raw.w > 0. && p.raw.h > 0. {
                super::paint::gradient_mapping(super::paint::gradient_line(g, p.raw), IDENTITY)
            } else {
                [0.; 3]
            };
            self.packet.texels[start + 10][1] = (self.packet.texels.len() - start) as f32;
            self.push([mapping[0], mapping[1], mapping[2], 0.]);
            let colors = array(&g["color"]);
            let fades = array(&g["transparency"]);
            self.push([colors.len() as f64, fades.len() as f64, 0., 0.]);
            for k in colors {
                self.push(std::array::from_fn(|i| component(k, i, 0.)))
            }
            for k in fades {
                self.push([component(k, 0, 0.), component(k, 1, 0.), 0., 0.]);
            }
        }
        let mut batch = DrawBatch {
            blend: BlendMode::Image,
            ..DrawBatch::default()
        };
        batch.image_slots[0] = Some(texture);
        self.emit(ctx, start, batch);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn oversized_clipped_images_fail_before_allocating_records_or_textures() {
        let asset = TextureAsset {
            id: AssetId("oversized-fixture".into()),
            width: 1,
            height: 1,
            rgba: vec![255; 4].into(),
        };
        for (rect, raw) in [
            (Rect::new(0., 0., 1e9, 100.), Rect::new(0., 0., 1e9, 100.)),
            (
                Rect::new(0., 0., 8193., 8193.),
                Rect::new(0., 0., 8193., 8193.),
            ),
            (Rect::new(0., 0., 64., 64.), Rect::new(0., 0., 1e9, 100.)),
        ] {
            for radii in [[0.; 4], [4.; 4]] {
                let mut draw = DrawPreparer::new();
                draw.packet.texels = byte_float_texels();
                let mut ctx = BuildContext::new(FRAME_TARGET, 64, 64, [0.; 2]);
                let node = serde_json::json!({
                    "image": {"scaleType": "Stretch", "color": [1,1,1]},
                    "clip": [0,0,64,64]
                });
                let error = draw
                    .snapped_image_records(
                        &mut ctx,
                        &node,
                        &asset,
                        PhysicalRect {
                            rect,
                            raw,
                            snapped: true,
                        },
                        radii,
                    )
                    .unwrap_err();
                assert!(error.to_string().contains("image allocation limits"));
                assert_eq!(draw.packet.texels.len(), 64);
                assert!(draw.packet.textures.is_empty());
                assert!(ctx.batches.is_empty());
                assert_eq!(draw.gpu_image_cache.bytes(), 0);
                assert_eq!(draw.image_cache.bytes(), 0);
            }
        }
    }

    fn snapped_fixture(c: &Value, source: &Value) -> (DrawPreparer, BuildContext) {
        let src = &source["cases"][c["sourceCase"].as_u64().unwrap() as usize];
        let asset = TextureAsset {
            id: AssetId("snapped-fixture".into()),
            width: src["width"].as_u64().unwrap() as u32,
            height: src["height"].as_u64().unwrap() as u32,
            rgba: src["data"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap() as u8)
                .collect::<Vec<_>>()
                .into(),
        };
        let mut draw = DrawPreparer::new();
        draw.scale = c["scale"].as_f64().unwrap();
        draw.frame = 1;
        // Golden records include ones their clip hides
        draw.cull_records = false;
        draw.packet.width = 64;
        draw.packet.height = 64;
        draw.packet.texels = byte_float_texels();
        let m = values::<6>(&c["matrix"]);
        let mut ctx = BuildContext::new(FRAME_TARGET, 64, 64, [-m[4], -m[5]]);
        let mut node = c["node"].clone();
        if c["clip"].is_array() {
            let clip = values::<4>(&c["clip"]);
            node["clip"] = serde_json::json!([
                (clip[0] - m[4]) / draw.scale,
                (clip[1] - m[5]) / draw.scale,
                (clip[2] - clip[0]) / draw.scale,
                (clip[3] - clip[1]) / draw.scale,
            ]);
        }
        let r = &c["rect"];
        let raw = values::<4>(&r["raw"]);
        draw.snapped_image_records(
            &mut ctx,
            &node,
            &asset,
            PhysicalRect {
                rect: Rect::new(
                    r["x"].as_f64().unwrap(),
                    r["y"].as_f64().unwrap(),
                    r["w"].as_f64().unwrap(),
                    r["h"].as_f64().unwrap(),
                ),
                raw: Rect::new(raw[0], raw[1], raw[2], raw[3]),
                snapped: true,
            },
            values(&c["radius"]),
        )
        .unwrap();
        (draw, ctx)
    }

    #[test]
    fn exact_browser_snapped_image_records_and_masks() {
        let source: Value =
            serde_json::from_str(include_str!("../../tests/goldens/image-math.json")).unwrap();
        let gold: Value = serde_json::from_str(include_str!(
            "../../tests/goldens/snapped-image-records.json"
        ))
        .unwrap();
        for (index, c) in gold["cases"].as_array().unwrap().iter().enumerate() {
            let (draw, ctx) = snapped_fixture(c, &source);
            let expected: Vec<[u32; 4]> = c["texelBits"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| std::array::from_fn(|k| v[k].as_u64().unwrap() as u32))
                .collect();
            let actual: Vec<_> = draw.packet.texels[64..]
                .iter()
                .map(|v| v.map(f32::to_bits))
                .collect();
            assert_eq!(actual, expected, "snapped record case {index}");
            let instances: Vec<_> = ctx
                .batches
                .iter()
                .flat_map(|b| b.record_offsets.iter().map(move |offset| (offset, b)))
                .collect();
            assert_eq!(instances.len(), c["instances"].as_array().unwrap().len());
            for ((offset, batch), expected) in
                instances.iter().zip(c["instances"].as_array().unwrap())
            {
                assert_eq!(**offset as u64 - 64, expected["start"].as_u64().unwrap());
                assert_eq!(batch.blend, BlendMode::Image);
                if let Some(mask_index) = expected["images"].as_array().unwrap().get(1) {
                    let mask = &c["masks"][mask_index["texture"].as_u64().unwrap() as usize];
                    let upload = draw
                        .packet
                        .textures
                        .iter()
                        .find(|u| Some(u.id) == batch.image_slots[1])
                        .unwrap();
                    assert_eq!(upload.format, TextureFormat::R32Float);
                    let level = &upload.levels[0];
                    assert_eq!(
                        [level.width, level.height],
                        [
                            mask["width"].as_u64().unwrap() as u32,
                            mask["height"].as_u64().unwrap() as u32
                        ]
                    );
                    let actual: Vec<u32> = level
                        .bytes
                        .chunks_exact(4)
                        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
                        .collect();
                    let expected: Vec<u32> = mask["bits"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|b| b.as_u64().unwrap() as u32)
                        .collect();
                    assert_eq!(actual, expected, "mask case {index}");
                }
            }
        }
    }

    #[test]
    fn native_snapped_image_pixels_match_canonical_canvas() -> anyhow::Result<()> {
        let source: Value =
            serde_json::from_str(include_str!("../../tests/goldens/image-math.json"))?;
        let gold: Value = serde_json::from_str(include_str!(
            "../../tests/goldens/snapped-image-records.json"
        ))?;
        let mut gpu = pollster::block_on(super::super::gpu::GpuRenderer::headless())?;
        for (index, c) in gold["cases"].as_array().unwrap().iter().enumerate() {
            // Each fixture is a fresh CPU owner; reset GPU IDs so revisions cannot alias.
            gpu.reset_scene_resources();
            let (mut draw, ctx) = snapped_fixture(c, &source);
            draw.finish_pass(ctx);
            gpu.render(&draw.packet)?;
            assert_eq!(
                hash(&gpu.readback_rgba()?),
                c["pixelHash"].as_str().unwrap(),
                "snapped GPU pixels {index}"
            );
        }
        Ok(())
    }

    #[test]
    fn mask_cache_lifetime_is_validated_for_retained_records() {
        let mut draw = DrawPreparer::new();
        draw.frame = 1;
        let first = draw.image_mask_texture(4, 5, [2.; 4], 0., 0.);
        assert_eq!(first, draw.image_mask_texture(4, 5, [2.; 4], 0., 0.));
        assert_eq!(draw.packet.textures.len(), 1);
        assert!(draw.gpu_image_cache.touch(first, 2));
        let mut masks = Vec::new();
        for index in 0..512 {
            masks.push(draw.image_mask_texture(4, 5, [index as f64 / 1000.; 4], 0., 0.));
        }
        assert_eq!(draw.gpu_image_cache.mask_bytes, 513 * 4 * 5 * 4);
        // Eviction preserves a retained node's current-frame reference.
        let evicted = draw.gpu_image_cache.evict(2);
        assert_eq!(evicted.len(), 1);
        assert_ne!(evicted[0], first);
        assert!(!draw.gpu_image_cache.touch(evicted[0], 3));
        assert_eq!(draw.gpu_image_cache.mask_bytes, 512 * 4 * 5 * 4);
        draw.frame = 3;
        let index = masks.iter().position(|id| *id == evicted[0]).unwrap();
        let second = draw.image_mask_texture(4, 5, [index as f64 / 1000.; 4], 0., 0.);
        assert_ne!(evicted[0], second);
        draw.reset_resources();
        assert!(draw.retired.contains(&first));
        assert!(draw.retired.contains(&second));
        assert!(!draw.gpu_image_cache.touch(second, 4));
    }
    #[test]
    fn source_mips_reuse_replace_and_retire_without_aliasing() {
        let asset = TextureAsset {
            id: AssetId("source".into()),
            width: 2,
            height: 2,
            rgba: vec![255; 16].into(),
        };
        let mut draw = DrawPreparer::new();
        draw.frame = 1;
        let (first, count) = draw.image_source_texture(&asset).unwrap();
        assert_eq!(count, 2);
        assert_eq!(draw.packet.textures.len(), 1);
        assert_eq!(draw.image_source_texture(&asset), Some((first, count)));
        assert_eq!(draw.packet.textures.len(), 1);
        assert!(draw.gpu_image_cache.touch(first, 2));
        let changed = TextureAsset {
            rgba: vec![127; 16].into(),
            ..asset
        };
        let (second, _) = draw.image_source_texture(&changed).unwrap();
        assert_ne!(first, second);
        assert!(draw.retired.contains(&first));
        assert!(!draw.gpu_image_cache.touch(first, 3));
        assert!(draw.gpu_image_cache.touch(second, 3));
        draw.reset_resources();
        assert!(draw.retired.contains(&second));
        let (third, _) = draw.image_source_texture(&changed).unwrap();
        assert_ne!(second, third);
    }
    fn values<const N: usize>(v: &Value) -> [f64; N] {
        std::array::from_fn(|i| v[i].as_f64().unwrap())
    }
    fn hash(bytes: &[u8]) -> String {
        let mut n = 0xcbf29ce484222325u64;
        for b in bytes {
            n = (n ^ u64::from(*b)).wrapping_mul(0x100000001b3);
        }
        format!("{n:016x}")
    }
    #[test]
    fn exact_browser_rotated_image_records() {
        let source: Value =
            serde_json::from_str(include_str!("../../tests/goldens/image-math.json")).unwrap();
        let gold: Value = serde_json::from_str(include_str!(
            "../../tests/goldens/rotated-image-records.json"
        ))
        .unwrap();
        for (i, c) in gold["cases"].as_array().unwrap().iter().enumerate() {
            let src = &source["cases"][c["sourceCase"].as_u64().unwrap() as usize];
            let asset = TextureAsset {
                id: AssetId(format!("case:{i}")),
                width: src["width"].as_u64().unwrap() as u32,
                height: src["height"].as_u64().unwrap() as u32,
                rgba: src["data"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_u64().unwrap() as u8)
                    .collect::<Vec<_>>()
                    .into(),
            };
            let mut draw = DrawPreparer::new();
            draw.scale = c["scale"].as_f64().unwrap();
            draw.frame = 1;
            draw.packet.texels = byte_float_texels();
            let mut ctx = BuildContext::new(
                FRAME_TARGET,
                c["targetSize"][0].as_u64().unwrap() as u32,
                c["targetSize"][1].as_u64().unwrap() as u32,
                values(&c["origin"]),
            );
            let r = &c["rect"];
            let raw = values::<4>(&r["raw"]);
            let rect = Rect::new(
                r["x"].as_f64().unwrap(),
                r["y"].as_f64().unwrap(),
                r["w"].as_f64().unwrap(),
                r["h"].as_f64().unwrap(),
            );
            draw.rotated_image_record(
                &mut ctx,
                &c["node"],
                &asset,
                PhysicalRect {
                    rect,
                    raw: Rect::new(raw[0], raw[1], raw[2], raw[3]),
                    snapped: false,
                },
                values(&c["radius"]),
            );
            let expected: Vec<[u32; 4]> = c["texelBits"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| std::array::from_fn(|k| v[k].as_u64().unwrap() as u32))
                .collect();
            let actual: Vec<_> = draw.packet.texels[64..]
                .iter()
                .map(|v| v.map(f32::to_bits))
                .collect();
            assert_eq!(actual, expected, "record case {i}");
            for (level, expected) in draw.packet.textures[0]
                .levels
                .iter()
                .zip(c["mipHashes"].as_array().unwrap())
            {
                assert_eq!(
                    hash(&level.bytes),
                    expected.as_str().unwrap(),
                    "mip case {i}"
                );
            }
        }
    }
}
