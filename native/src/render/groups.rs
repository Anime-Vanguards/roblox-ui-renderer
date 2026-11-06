//! CanvasGroup passes and calibrated texture-space resampling. Screen-grid segments (flush_segment:
//! premultiplied bytes, premultiplied blend) and source images (group_image: straight colour and alpha,
//! Studio's image blend round((c A + D (255 - A)) / 255)) use different blends.
use anyhow::ensure;
use serde_json::Value;
use std::collections::HashMap;

use super::{
    geometry::*,
    gradients::gradient_mesh,
    images::{ImageRect, ImageSpace, plan_texture_image},
    packet::{BlendMode, DrawBatch, TextureFormat, TextureId, TextureLevel, TextureUpload},
    scene_draw::{BuildContext, DrawError, DrawPreparer},
};
use crate::{
    assets::AssetStore,
    scene::{SceneStore, node_id},
};

const GROUP: u32 = 11;
const GROUP_ROTATED: u32 = 14;
const SEGMENT: u32 = 17;
const LAYER: u32 = 18;

struct CachedField {
    id: TextureId,
    bytes: usize,
    used: u64,
}
struct CachedTarget {
    id: TextureId,
    width: u32,
    height: u32,
    used: u64,
}

/// Group-only immutable fields and reusable render targets. Scene teardown owns this cache.
#[derive(Default)]
pub(crate) struct GroupCache {
    fields: HashMap<String, CachedField>,
    targets: Vec<CachedTarget>,
    bytes: usize,
    frame: u64,
    disposable: Vec<TextureId>,
}

impl GroupCache {
    pub(crate) fn resident_bytes(&self) -> usize {
        self.bytes
    }
    pub(crate) fn begin_frame(&mut self) -> Vec<TextureId> {
        self.frame += 1;
        self.targets.retain(|target| target.used + 1 >= self.frame);
        std::mem::take(&mut self.disposable)
    }

    pub(crate) fn clear(&mut self) {
        self.disposable
            .extend(self.fields.drain().map(|(_, field)| field.id));
        self.targets.clear();
        self.bytes = 0;
    }
}

#[derive(Clone, Copy)]
struct GroupSpace {
    l: f64,
    t: f64,
    w: u32,
    h: u32,
    nw: u32,
    nh: u32,
}

enum ScreenSegment {
    Direct,
    Scratch(BuildContext),
}

impl DrawPreparer {
    fn group_texture(
        &mut self,
        key: String,
        width: u32,
        height: u32,
        format: TextureFormat,
        make: impl FnOnce() -> Vec<u8>,
    ) -> TextureId {
        if let Some(field) = self.group_cache.fields.get_mut(&key) {
            field.used = self.group_cache.frame;
            self.stats.upload_cache_hits += 1;
            return field.id;
        }
        let bytes = make();
        self.stats.pixel_bytes_processed += bytes.len() as u64;
        let size = bytes.len() + key.len();
        let id = self.texture_id();
        self.packet.textures.push(TextureUpload {
            id,
            revision: 1,
            format,
            levels: vec![TextureLevel {
                width,
                height,
                bytes: bytes.into(),
            }],
        });
        const BUDGET: usize = 64 * 1024 * 1024;
        let cache = &mut self.group_cache;
        while cache.bytes + size > BUDGET || cache.fields.len() >= 512 {
            let oldest = cache
                .fields
                .iter()
                .filter(|(_, field)| field.used < cache.frame)
                .min_by_key(|(_, field)| field.used)
                .map(|(key, _)| key.clone());
            let Some(oldest) = oldest else {
                break;
            };
            let field = cache.fields.remove(&oldest).unwrap();
            cache.bytes -= field.bytes;
            self.packet.retire_textures.push(field.id);
        }
        if size <= BUDGET && cache.bytes + size <= BUDGET && cache.fields.len() < 512 {
            cache.bytes += size;
            cache.fields.insert(
                key,
                CachedField {
                    id,
                    bytes: size,
                    used: cache.frame,
                },
            );
        } else {
            cache.disposable.push(id);
        }
        id
    }

    pub(crate) fn draw_group(
        &mut self,
        pass: &mut BuildContext,
        entry: &Value,
        scene: &SceneStore,
        assets: &AssetStore,
    ) -> Result<(), DrawError> {
        let started = std::time::Instant::now();
        let _scope = crate::microprofile::scope("CanvasGroup");
        let categories = self.stats.category_ms();
        self.stats.groups += 1;
        let result = self.draw_group_inner(pass, entry, scene, assets);
        self.stats.groups_ms += (started.elapsed().as_secs_f64() * 1000.
            - (self.stats.category_ms() - categories))
            .max(0.);
        result
    }
    fn draw_group_inner(
        &mut self,
        pass: &mut BuildContext,
        entry: &Value,
        scene: &SceneStore,
        assets: &AssetStore,
    ) -> Result<(), DrawError> {
        let Some(node) = node_id(&entry["group"]).and_then(|id| scene.node(id)) else {
            return Ok(());
        };
        if node["culled"].as_bool().unwrap_or(false) {
            return Ok(());
        }
        // The group's own UIShadows are drawn on the target outside its texture (not tinted, faded or cut
        // by the group) and before the opacity test: a group whose alpha8(1 - GroupTransparency) is 0 still
        // draws them. Its border UIStrokes surround the composite and are not faded either.
        self.group_shadows(pass, node, scene, assets)?;
        let opacity = alpha8(1.0 - number(&node["group"]["transparency"], 0.0));
        if opacity == 0 {
            return Ok(());
        }
        let physical = physical_rect(node, self.scale);
        let rect = physical.rect;
        let radii = radius(&node["corner"], self.scale);
        if !physical.snapped {
            let width = self.packet.width;
            let height = self.packet.height;
            if width == 0 || height == 0 {
                return Ok(());
            }
            let mut child = self.group_pass(width, height, [0.0, 0.0])?;
            let target = child.target;
            self.draw_entries(
                &mut child,
                array(&entry["draw"]),
                scene,
                assets,
                node_id(&node["id"]),
            )?;
            self.finish_pass(child);
            self.composite_rotated_group(pass, node, target, physical, radii, opacity);
        } else {
            if rect.w <= 0.0 || rect.h <= 0.0 {
                return Ok(());
            }
            let space = GroupSpace {
                l: rect.x,
                t: rect.y,
                w: rect.w as u32,
                h: rect.h as u32,
                nw: group_texture_dims(rect.x, rect.y, rect.w, rect.h, self.scale).0,
                nh: group_texture_dims(rect.x, rect.y, rect.w, rect.h, self.scale).1,
            };
            let mut texture = self.group_pass(space.nw, space.nh, [space.l, space.t])?;
            self.build_group_texture(&mut texture, entry, node, scene, assets, space)?;
            let target = texture.target;
            self.finish_pass(texture);
            self.composite_group(pass, node, target, physical, radii, opacity, space);
        }
        self.group_strokes(pass, node, scene, assets)?;
        Ok(())
    }

    fn group_pass(
        &mut self,
        width: u32,
        height: u32,
        origin: [f64; 2],
    ) -> Result<BuildContext, DrawError> {
        // A malformed scene must not allocate an unbounded offscreen target.
        ensure!(
            width > 0
                && height > 0
                && width <= 16384
                && height <= 16384
                && u64::from(width) * u64::from(height) <= 64 * 1024 * 1024,
            "CanvasGroup target exceeds the 64-megapixel/16384-pixel limit: {width}x{height}"
        );
        let target = if let Some(target) = self.group_cache.targets.iter_mut().find(|target| {
            target.width == width && target.height == height && target.used < self.group_cache.frame
        }) {
            target.used = self.group_cache.frame;
            target.id
        } else {
            let id = self.texture_id();
            self.group_cache.targets.push(CachedTarget {
                id,
                width,
                height,
                used: self.group_cache.frame,
            });
            id
        };
        Ok(BuildContext {
            origin,
            width,
            height,
            target,
            clear: Some([0; 4]),
            batches: Vec::new(),
        })
    }

    fn screen_segment<'a>(
        &mut self,
        segment: &'a mut Option<ScreenSegment>,
        texture: &'a mut BuildContext,
        space: GroupSpace,
        empty: bool,
    ) -> Result<&'a mut BuildContext, DrawError> {
        if segment.is_none() {
            *segment = Some(if empty && space.w == space.nw && space.h == space.nh {
                ScreenSegment::Direct
            } else {
                ScreenSegment::Scratch(self.group_pass(space.w, space.h, [space.l, space.t])?)
            });
        }
        Ok(match segment.as_mut().unwrap() {
            ScreenSegment::Direct => texture,
            ScreenSegment::Scratch(pass) => pass,
        })
    }

    /// `smooth`: the scratch is sampled bilinearly at the texel centres with 8-bit weights (anti-aliased
    /// content such as text, like Studio sampling the glyph atlas over quads scaled into the texture).
    fn flush_segment(
        &mut self,
        segment: &mut Option<ScreenSegment>,
        texture: &mut BuildContext,
        space: GroupSpace,
        empty: &mut bool,
        smooth: bool,
    ) {
        let Some(segment) = segment.take() else {
            return;
        };
        if let ScreenSegment::Scratch(scratch) = segment {
            let target = scratch.target;
            self.finish_pass(scratch);
            let start = self.begin(
                texture,
                [0.0, 0.0, space.nw as f64, space.nh as f64],
                None,
                SEGMENT,
            );
            self.push([
                space.w as f64,
                space.h as f64,
                space.nw as f64,
                space.nh as f64,
            ]);
            self.push([if smooth { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0]);
            self.emit(
                texture,
                start,
                DrawBatch {
                    layer: Some(target),
                    ..Default::default()
                },
            );
        }
        *empty = false;
    }

    fn build_group_texture(
        &mut self,
        texture: &mut BuildContext,
        entry: &Value,
        node: &Value,
        scene: &SceneStore,
        assets: &AssetStore,
        space: GroupSpace,
    ) -> Result<(), DrawError> {
        let mut segment = None;
        let mut empty = true;
        let resampled = space.nw != space.w || space.nh != space.h;
        for item in array(&entry["draw"]) {
            if item.is_object() {
                let inner = node_id(&item["group"]).and_then(|id| scene.node(id));
                // Descendants keep only the clips inside the group (texture_clip_node)
                let owned = inner.and_then(|inner| texture_clip_node(inner, node, self.scale, resampled));
                let inner = owned.as_ref().or(inner);
                if let Some(inner) = inner.filter(|inner| {
                    resampled
                        && !inner["culled"].as_bool().unwrap_or(false)
                        && !inner["matrix"].is_array()
                        && !inner["gradient"].is_object()
                        && array(&inner["shadows"]).is_empty()
                        && array(&inner["strokes"]).is_empty()
                }) {
                    self.flush_segment(&mut segment, texture, space, &mut empty, false);
                    self.texture_group(texture, item, inner, scene, assets, space)?;
                    empty = false;
                    continue;
                }
                let screen = self.screen_segment(&mut segment, texture, space, empty)?;
                self.draw_group(screen, item, scene, assets)?;
                continue;
            }
            let Some(id) = item.as_i64() else {
                continue;
            };
            let Some(child) = scene.node(id.unsigned_abs()) else {
                continue;
            };
            let owned = texture_clip_node(child, node, self.scale, resampled);
            let child = owned.as_ref().unwrap_or(child);
            if child["culled"].as_bool().unwrap_or(false) {
                continue;
            }
            let in_group = child["id"] == node["id"];
            if id < 0 {
                let screen = self.screen_segment(&mut segment, texture, space, empty)?;
                self.draw_post(screen, child, assets)?;
            } else if resampled
                && !child["image"].is_object()
                && !child["matrix"].is_array()
                && !in_group
            {
                // Rendered at the texture's scale (fixtures/group-misc.luau: the text, stroke, rounded and
                // nested groups 5,178 px > 2 -> 0, whole capture 99.726 -> 100%)
                let corner = radius(&child["corner"], 1.0);
                if corner.iter().any(|r| *r > 0.0)
                    && !child["gradient"].is_object()
                    && number(&child["border"]["size"], 0.0) <= 0.0
                    && array(&child["shadows"]).is_empty()
                {
                    self.flush_segment(&mut segment, texture, space, &mut empty, false);
                    self.texture_fill(texture, child, space);
                    empty = false;
                } else if has_part(child, false, false) {
                    let screen = self.screen_segment(&mut segment, texture, space, empty)?;
                    self.draw_node_parts(screen, child, scene, assets, false, "under")?;
                }
                let strokes = array(&child["strokes"]);
                if child["text"].is_object()
                    || child["selection"].is_object()
                    || child["caret"].is_object()
                {
                    self.flush_segment(&mut segment, texture, space, &mut empty, false);
                    // Glyph quads sampled once, straight into the texture (text.rs texture_text_layers);
                    // glyphs still loading take the screen path
                    let layers = super::text::texture_text_layers(
                        child,
                        scene.fonts(),
                        assets,
                        self.scale,
                        [space.l, space.t],
                        [space.nw, space.nh],
                        [space.nw as f64 / space.w as f64, space.nh as f64 / space.h as f64],
                        super::geometry::text_clip_rect(child, [0.0, 0.0], self.scale),
                    )
                    .ok()
                    .flatten();
                    if let Some(layers) = layers {
                        for layer in layers {
                            self.texture_layer(texture, layer.x, layer.y, layer.w, layer.h, layer.rgba);
                            empty = false;
                        }
                    } else {
                        let screen = self.screen_segment(&mut segment, texture, space, empty)?;
                        self.draw_node_parts(screen, child, scene, assets, false, "over")?;
                        self.flush_segment(&mut segment, texture, space, &mut empty, true);
                    }
                } else if !strokes.is_empty() && !strokes.iter().any(|s| s["gradient"].is_object())
                {
                    self.flush_segment(&mut segment, texture, space, &mut empty, false);
                    let sx = space.nw as f64 / space.w as f64;
                    let sy = space.nh as f64 / space.h as f64;
                    let clip = child["clip"].as_array().filter(|v| v.len() >= 4).map(|_| {
                        let [cx, cy, cw, ch] = std::array::from_fn(|i| component(&child["clip"], i, 0.0));
                        let [cx0, cx1] = device_span(cx, cw, self.scale);
                        let [cy0, cy1] = device_span(cy, ch, self.scale);
                        [
                            ((cx0 - space.l) * sx - 0.5).ceil(),
                            ((cy0 - space.t) * sy - 0.5).ceil(),
                            ((cx1 - space.l) * sx - 0.5).ceil(),
                            ((cy1 - space.t) * sy - 0.5).ceil(),
                        ]
                    });
                    let transform = [sx, 0.0, 0.0, sy, -space.l * sx, -space.t * sy];
                    self.texture_strokes(texture, child, scene, assets, transform, clip)?;
                    empty = false;
                } else if has_part(child, false, true) {
                    let screen = self.screen_segment(&mut segment, texture, space, empty)?;
                    self.draw_node_parts(screen, child, scene, assets, false, "over")?;
                }
            } else if resampled
                && child["matrix"].is_array()
                && super::images::translated_image_node(child).is_none()
                && !in_group
                && !child["text"].is_object()
                && !child["selection"].is_object()
                && !child["caret"].is_object()
                && !child["viewport"].is_object()
            {
                // Rotated children render at the texture's scale too (texture_space_node)
                self.flush_segment(&mut segment, texture, space, &mut empty, false);
                let moved = texture_space_node(child, space, self.scale);
                self.draw_node(texture, &moved, scene, assets, false)?;
                empty = false;
            } else if !child["image"].is_object() || child["matrix"].is_array() {
                let screen = self.screen_segment(&mut segment, texture, space, empty)?;
                self.draw_node(screen, child, scene, assets, in_group)?;
            } else {
                // Background and shadows precede the direct image, text and border strokes follow it.
                if has_part(child, in_group, false) {
                    let screen = self.screen_segment(&mut segment, texture, space, empty)?;
                    self.draw_node_parts(screen, child, scene, assets, in_group, "under")?;
                }
                self.flush_segment(&mut segment, texture, space, &mut empty, false);
                self.group_image(texture, child, assets, in_group, space)?;
                empty = false;
                if has_part(child, in_group, true) {
                    let screen = self.screen_segment(&mut segment, texture, space, empty)?;
                    self.draw_node_parts(screen, child, scene, assets, in_group, "over")?;
                }
            }
        }
        self.flush_segment(&mut segment, texture, space, &mut empty, false);
        Ok(())
    }

    /// A straight RGBA layer at texel (x, y) of a group texture, blended like an image: colour
    /// round((c A + D (255 - A)) / 255), alpha round(A + D (255 - A) / 255).
    fn texture_layer(&mut self, pass: &mut BuildContext, x: u32, y: u32, w: u32, h: u32, rgba: Vec<u8>) {
        let target = self.upload(w, h, rgba, TextureFormat::Rgba8);
        let (x, y) = (x as f64, y as f64);
        let start = self.begin(pass, [x, y, x + w as f64, y + h as f64], None, LAYER);
        self.push([0.0; 4]); // Texture slot zero; following texel is the disabled selection block.
        self.push([0.0; 4]);
        let mut batch = DrawBatch {
            blend: BlendMode::Image,
            ..Default::default()
        };
        batch.image_slots[0] = Some(target);
        self.emit(pass, start, batch);
    }

    /// A rounded solid fill rendered in a resampled group texture's space: the snapped quad scaled by N / Q
    /// covers the texels whose centre is inside, coverage at those centres (texture_corner_coverage), alpha
    /// gpu8(a8 / 255 x coverage).
    fn texture_fill(&mut self, pass: &mut BuildContext, node: &Value, space: GroupSpace) {
        let bg = &node["bg"];
        if !bg.is_object() {
            return;
        }
        let a8 = alpha8(1.0 - number(&bg["transparency"], 0.0));
        if a8 == 0 {
            return;
        }
        let mut colour = color(&bg["color"], [1.0; 3]);
        if node["interaction"]["bg"].is_array() {
            colour = color(&node["interaction"]["bg"], [255.0; 3]).map(|v| v / 255.0);
        }
        let colour = colour.map(quantize);
        let rect = physical_rect(node, self.scale).rect;
        let radii = radius(&node["corner"], self.scale);
        let sx = space.nw as f64 / space.w as f64;
        let sy = space.nh as f64 / space.h as f64;
        let texel_x = |e: f64| ((e - space.l) * sx - 0.5).ceil();
        let texel_y = |e: f64| ((e - space.t) * sy - 0.5).ceil();
        let (tx0, ty0) = (texel_x(rect.x), texel_y(rect.y));
        let (tx1, ty1) = (texel_x(rect.x + rect.w), texel_y(rect.y + rect.h));
        let [mut x0, mut y0, mut x1, mut y1] = [tx0, ty0, tx1, ty1];
        if node["clip"].as_array().is_some_and(|v| v.len() >= 4) {
            let [cx, cy, cw, ch] = std::array::from_fn(|i| component(&node["clip"], i, 0.0));
            let [cx0, cx1] = device_span(cx, cw, self.scale);
            let [cy0, cy1] = device_span(cy, ch, self.scale);
            x0 = x0.max(texel_x(cx0));
            y0 = y0.max(texel_y(cy0));
            x1 = x1.min(texel_x(cx1));
            y1 = y1.min(texel_y(cy1));
        }
        x0 = x0.max(0.0);
        y0 = y0.max(0.0);
        x1 = x1.min(space.nw as f64);
        y1 = y1.min(space.nh as f64);
        if x1 <= x0 || y1 <= y0 || tx1 <= tx0 || ty1 <= ty0 {
            return;
        }
        let (w, h) = ((x1 - x0) as usize, (y1 - y0) as usize);
        let cw = (tx1 - tx0) as usize;
        let off = [tx0 + 0.5 - (rect.x - space.l) * sx, ty0 + 0.5 - (rect.y - space.t) * sy];
        let coverage = texture_corner_coverage(
            cw,
            (ty1 - ty0) as usize,
            [rect.w, rect.h],
            radii,
            off,
            [sx, sy],
            [tx0 as i64, ty0 as i64],
        );
        let mut rgba = vec![0u8; w * h * 4];
        for j in 0..h {
            for i in 0..w {
                let k = (y0 - ty0) as usize + j;
                let c = coverage[k * cw + (x0 - tx0) as usize + i];
                let a = gpu8(a8 as f64 / 255.0 * c);
                if a == 0 {
                    continue;
                }
                let o = (j * w + i) * 4;
                rgba[o..o + 4].copy_from_slice(&[colour[0], colour[1], colour[2], a]);
            }
        }
        self.texture_layer(pass, x0 as u32, y0 as u32, w as u32, h as u32, rgba);
    }

    /// A CanvasGroup nested in a resampled group's texture: its own texture sampled in the outer texture's
    /// space (texture_group_placement: 8-bit bilinear weights, 12-bit truncation) by the GROUP record
    /// (straight output, flag 32, image blend).
    fn texture_group(
        &mut self,
        pass: &mut BuildContext,
        item: &Value,
        inner: &Value,
        scene: &SceneStore,
        assets: &AssetStore,
        space: GroupSpace,
    ) -> Result<(), DrawError> {
        let opacity = alpha8(1.0 - number(&inner["group"]["transparency"], 0.0));
        if opacity == 0 {
            return Ok(());
        }
        let physical = physical_rect(inner, self.scale);
        let rect = physical.rect;
        if rect.w <= 0.0 || rect.h <= 0.0 {
            return Ok(());
        }
        let inner_space = GroupSpace {
            l: rect.x,
            t: rect.y,
            w: rect.w as u32,
            h: rect.h as u32,
            nw: group_texture_dims(rect.x, rect.y, rect.w, rect.h, self.scale).0,
            nh: group_texture_dims(rect.x, rect.y, rect.w, rect.h, self.scale).1,
        };
        let radii = radius(&inner["corner"], self.scale);
        let clip = inner["clip"].as_array().filter(|v| v.len() >= 4).map(|_| {
            std::array::from_fn::<f64, 4, _>(|i| component(&inner["clip"], i, 0.0))
        });
        let Some(place) = texture_group_placement(
            physical,
            clip,
            self.scale,
            space,
            inner_space.nw,
            inner_space.nh,
            radii,
        ) else {
            return Ok(());
        };
        let mut texture = self.group_pass(inner_space.nw, inner_space.nh, [inner_space.l, inner_space.t])?;
        self.build_group_texture(&mut texture, item, inner, scene, assets, inner_space)?;
        let target = texture.target;
        self.finish_pass(texture);

        let tint = color(&inner["group"]["color"], [1.0; 3]).map(quantize);
        let masked = place.mask.is_some();
        let flags = u32::from(tint.iter().any(|v| *v < 255)) | (u32::from(masked) << 1) | 8 | 16 | 32;
        let start = self.begin(pass, [place.x0, place.y0, place.x1, place.y1], None, GROUP);
        self.push([place.tx0, place.ty0, place.tx1 - place.tx0, place.ty1 - place.ty0]);
        let info_at = self.packet.texels.len();
        self.push([flags as f64, opacity as f64, 0.0, 0.0]);
        self.push([tint[0] as f64, tint[1] as f64, tint[2] as f64, 0.0]);
        self.push([0.0; 4]);
        self.push([0.0; 4]);
        self.packet.texels[info_at][2] = (self.packet.texels.len() - start) as f32;
        for tap in &place.xs {
            self.push([tap[0], tap[1], tap[2], 0.0]);
        }
        self.packet.texels[info_at][3] = (self.packet.texels.len() - start) as f32;
        for tap in &place.ys {
            self.push([tap[0], tap[1], tap[2], 0.0]);
        }
        let mut batch = DrawBatch {
            layer: Some(target),
            blend: BlendMode::Image,
            ..Default::default()
        };
        if let Some(mask) = place.mask {
            let (cw, ch) = ((place.tx1 - place.tx0) as u32, (place.ty1 - place.ty0) as u32);
            let key = format!(
                "texgroupmask:{cw}:{ch}:{:?}:{:?}:{radii:?}:{:?}:{:?}:{:?}",
                [rect.w, rect.h],
                [physical.raw.w, physical.raw.h],
                [place.tx0, place.ty0],
                [space.nw, space.nh, space.w, space.h],
                [rect.x - space.l, rect.y - space.t]
            );
            batch.image_slots[0] = Some(self.group_texture(key, cw, ch, TextureFormat::R32Float, || {
                mask.into_iter().flat_map(|value| (value as f32).to_le_bytes()).collect()
            }));
        }
        self.emit(pass, start, batch);
        Ok(())
    }

    fn group_image(
        &mut self,
        pass: &mut BuildContext,
        node: &Value,
        assets: &AssetStore,
        in_group: bool,
        space: GroupSpace,
    ) -> Result<(), DrawError> {
        let started = std::time::Instant::now();
        let _scope = crate::microprofile::scope("Group image");
        if node["image"].is_object() {
            self.stats.images += 1;
        }
        let result = self.group_image_inner(pass, node, assets, in_group, space);
        self.stats.images_ms += started.elapsed().as_secs_f64() * 1000.;
        result
    }
    fn group_image_inner(
        &mut self,
        pass: &mut BuildContext,
        node: &Value,
        assets: &AssetStore,
        in_group: bool,
        space: GroupSpace,
    ) -> Result<(), DrawError> {
        let Some(uri) = node["image"]["uri"].as_str() else {
            return Ok(());
        };
        let Some(texture) = assets.image(uri) else {
            return Ok(());
        };
        let physical = physical_rect(node, self.scale);
        let r = physical.rect;
        let rect = ImageRect {
            x: r.x,
            y: r.y,
            w: r.w,
            h: r.h,
            raw: [
                physical.raw.x,
                physical.raw.y,
                physical.raw.w,
                physical.raw.h,
            ],
            snapped: physical.snapped,
        };
        let radii = if in_group {
            [0.0; 4]
        } else {
            radius(&node["corner"], self.scale)
        };
        let mut own;
        let node = if in_group {
            own = node.clone();
            own.as_object_mut().unwrap().remove("gradient");
            &own
        } else {
            node
        };
        let image_space = ImageSpace {
            l: space.l,
            t: space.t,
            nw: space.nw,
            nh: space.nh,
            sx: space.nw as f64 / space.w as f64,
            sy: space.nh as f64 / space.h as f64,
        };
        let Some(planned) = plan_texture_image(
            node,
            texture,
            rect,
            radii,
            image_space,
            self.scale,
            &mut self.image_cache,
        ) else {
            return Ok(());
        };
        let (lx, ly, lw, lh, target) = self.upload_image_layer(planned);
        let x = lx as f64;
        let y = ly as f64;
        let start = self.begin(
            pass,
            [x, y, x + lw as f64, y + lh as f64],
            None,
            LAYER,
        );
        self.push([0.0; 4]); // Texture slot zero; following texel is the disabled selection block.
        self.push([0.0; 4]);
        let mut batch = DrawBatch {
            blend: BlendMode::Image,
            ..Default::default()
        };
        batch.image_slots[0] = Some(target);
        self.emit(pass, start, batch);
        Ok(())
    }

    fn composite_group(
        &mut self,
        pass: &mut BuildContext,
        node: &Value,
        texture: TextureId,
        physical: PhysicalRect,
        radii: [f64; 4],
        opacity: u8,
        space: GroupSpace,
    ) {
        let rect = physical.rect;
        let rx = rect.x - pass.origin[0];
        let ry = rect.y - pass.origin[1];
        let mut region = [
            rx.max(0.0),
            ry.max(0.0),
            (rx + rect.w).min(pass.width as f64),
            (ry + rect.h).min(pass.height as f64),
        ];
        if let Some(clip) = clip_rect(node, pass.origin, self.scale) {
            region[0] = region[0].max(clip[0]);
            region[1] = region[1].max(clip[1]);
            region[2] = region[2].min(clip[2]);
            region[3] = region[3].min(clip[3]);
        }
        if region[2] <= region[0] || region[3] <= region[1] {
            return;
        }
        let tint = color(&node["group"]["color"], [1.0; 3]).map(quantize);
        let masked = radii.iter().any(|r| *r > 0.0);
        let gradient = node["gradient"].is_object();
        let flags = u32::from(tint.iter().any(|v| *v < 255))
            | (u32::from(masked) << 1)
            | (u32::from(gradient) << 2)
            | (u32::from(space.nw != space.w) << 3)
            | (u32::from(space.nh != space.h) << 4);
        let start = self.begin(pass, region, None, GROUP);
        self.push([rx, ry, rect.w, rect.h]);
        let info_at = self.packet.texels.len();
        self.push([flags as f64, opacity as f64, 0.0, 0.0]);
        let tint_at = self.packet.texels.len();
        self.push([tint[0] as f64, tint[1] as f64, tint[2] as f64, 0.0]);
        self.push([0.0; 4]);
        self.push([0.0; 4]);
        if space.nw != space.w {
            self.packet.texels[info_at][2] = (self.packet.texels.len() - start) as f32;
            for tap in group_taps(space.w, space.nw) {
                self.push([tap[0], tap[1], tap[2], 0.0]);
            }
        }
        if space.nh != space.h {
            self.packet.texels[info_at][3] = (self.packet.texels.len() - start) as f32;
            for tap in group_taps(space.h, space.nh) {
                self.push([tap[0], tap[1], tap[2], 0.0]);
            }
        }
        let mut batch = DrawBatch {
            layer: Some(texture),
            ..Default::default()
        };
        if masked {
            let key = format!(
                "mask:{}:{}:{:?}:{:?}:{:?}",
                space.w,
                space.h,
                radii,
                [physical.raw.w, physical.raw.h],
                [(rx as i32).rem_euclid(2), (ry as i32).rem_euclid(2)]
            );
            batch.image_slots[0] =
                Some(
                    self.group_texture(key, space.w, space.h, TextureFormat::R32Float, || {
                        super::coverage::corner_coverage_raw(
                            space.w,
                            space.h,
                            radii,
                            rx as i32,
                            ry as i32,
                            physical.raw.w,
                            physical.raw.h,
                        )
                        .into_iter()
                        .flat_map(|value| (value as f32).to_le_bytes())
                        .collect()
                    }),
                );
        }
        if gradient {
            let [x0, y0, x1, y1] = region;
            let bounds = [
                x0 + pass.origin[0],
                y0 + pass.origin[1],
                x1 + pass.origin[0],
                y1 + pass.origin[1],
            ];
            let raw = physical.raw;
            let raw = [raw.x, raw.y, raw.w, raw.h];
            let mesh = gradient_mesh(
                &node["gradient"],
                raw,
                tint.map(f64::from),
                opacity as f64,
                rect.bounds(),
            );
            // The field comes from the mesh alone (GradientMesh::field): keyed relative to the region
            // when the mesh is exact there, so a group moved by whole pixels reuses its field
            let key = match mesh.local([bounds[0], bounds[1]]) {
                Some(local) => format!(
                    "gradient-local:{}",
                    serde_json::to_string(&(local, [bounds[2] - bounds[0], bounds[3] - bounds[1]], tint, opacity))
                        .unwrap_or_default()
                ),
                None => format!(
                    "gradient:{}:{raw:?}:{bounds:?}:{tint:?}:{opacity}:{:?}",
                    node["gradient"],
                    rect.bounds()
                ),
            };
            let slot = usize::from(masked);
            batch.image_slots[slot] = Some(self.group_texture(
                key,
                (x1 - x0) as u32,
                (y1 - y0) as u32,
                TextureFormat::Rgba32Float,
                || {
                    mesh.field(bounds)
                    .into_iter()
                    .flat_map(|v| (v / 255.0).to_le_bytes())
                    .collect()
                },
            ));
            self.packet.texels[tint_at][3] = slot as f32;
        }
        self.emit(pass, start, batch);
    }

    fn composite_rotated_group(
        &mut self,
        pass: &mut BuildContext,
        node: &Value,
        texture: TextureId,
        physical: PhysicalRect,
        radii: [f64; 4],
        opacity: u8,
    ) {
        let rect = physical.rect;
        let tint = color(&node["group"]["color"], [1.0; 3]).map(quantize);
        let tinted = tint.iter().any(|value| *value < 255);
        let x = -pass.origin[0];
        let y = -pass.origin[1];
        let start = self.begin(
            pass,
            [
                x,
                y,
                x + self.packet.width as f64,
                y + self.packet.height as f64,
            ],
            clip_rect(node, pass.origin, self.scale),
            GROUP_ROTATED,
        );
        self.push([x, y, if tinted { 1.0 } else { 0.0 }, 257.0]);
        let gradient = node["gradient"].is_object();
        self.push([opacity as f64, if gradient { 1.0 } else { 0.0 }, 0.0, 0.0]);
        self.push(radii.map(|r| r.max(0.0).min(rect.w / 2.0).min(rect.h / 2.0)));
        for block in local_texels(
            invert(node_transform(node, pass.origin, self.scale)),
            [x, y],
            [rect.x + rect.w / 2.0, rect.y + rect.h / 2.0],
        ) {
            self.push(block);
        }
        self.push([
            rect.w / 2.0,
            rect.h / 2.0,
            if radii.iter().any(|r| *r > 0.0) {
                1.0
            } else {
                0.0
            },
            0.0,
        ]);
        let mut batch = DrawBatch {
            layer: Some(texture),
            ..Default::default()
        };
        if tinted {
            batch.lut = Some(self.group_texture(
                format!("tint:{tint:?}"),
                256,
                258,
                TextureFormat::Rgba8,
                || tint_lut(tint),
            ));
        }
        if gradient {
            // The UIGradient turns with the group: its field over the unrotated rect, sampled at each pixel's
            // local point. Colour and alpha only: GroupColor3 and the opacity byte are applied around it
            let width = rect.w.ceil().max(1.0);
            let height = rect.h.ceil().max(1.0);
            let bounds = [0.0, 0.0, width, height];
            let mesh = gradient_mesh(&node["gradient"], [0.0, 0.0, rect.w, rect.h], [255.0; 3], 255.0, bounds);
            batch.image_slots[0] = Some(self.group_texture(
                format!("gradient-rotated:{width}:{height}:{}", node["gradient"]),
                width as u32,
                height as u32,
                TextureFormat::Rgba32Float,
                || {
                    mesh.field(bounds)
                        .into_iter()
                        .flat_map(|v| (v / 255.0).to_le_bytes())
                        .collect()
                },
            ));
        }
        self.emit(pass, start, batch);
    }
}

/// A descendant of a resampled CanvasGroup as rendered into the group's texture, clipped by the group and the
/// clipping frames inside it but not by the group's ancestors (their clip applies to the composite): a clip edge
/// that coincides with the group's inherited clip edge inside the group's rect is widened to the rect's edge
/// (a spring-animated button's last Wrapper: 34 px -> 0; notes/images.md round 7). None when nothing changes.
fn texture_clip_node(node: &Value, group: &Value, scale: f64, resampled: bool) -> Option<Value> {
    if !resampled || node["id"] == group["id"] {
        return None;
    }
    let four = |v: &Value| -> Option<[f64; 4]> {
        v.as_array().filter(|a| a.len() >= 4).map(|_| std::array::from_fn(|i| component(v, i, 0.0)))
    };
    let (c, g, r) = (four(&node["clip"])?, four(&group["clip"])?, four(&group["rect"])?);
    let span = |v: [f64; 4]| {
        let [x0, x1] = device_span(v[0], v[2], scale);
        let [y0, y1] = device_span(v[1], v[3], scale);
        [x0, y0, x1, y1]
    };
    let (cd, gd, rd) = (span(c), span(g), span(r));
    let [mut l, mut t, mut right, mut bottom] = [c[0], c[1], c[0] + c[2], c[1] + c[3]];
    let mut changed = false;
    if cd[0] == gd[0] && gd[0] > rd[0] {
        l = r[0];
        changed = true;
    }
    if cd[2] == gd[2] && gd[2] < rd[2] {
        right = r[0] + r[2];
        changed = true;
    }
    if cd[1] == gd[1] && gd[1] > rd[1] {
        t = r[1];
        changed = true;
    }
    if cd[3] == gd[3] && gd[3] < rd[3] {
        bottom = r[1] + r[3];
        changed = true;
    }
    if !changed {
        return None;
    }
    let mut moved = node.clone();
    moved
        .as_object_mut()?
        .insert("clip".into(), serde_json::json!([l, t, right - l, bottom - t]));
    Some(moved)
}

/// A rotated child of a resampled CanvasGroup in the texture pass (origin at the group's snapped corner): the
/// rotated node
/// with its matrix composed with the screen -> texture scale, [sx, 0, 0, sy, -L sx, -T sy] * M, and its clip
/// replaced by the texels whose centre lies inside the scaled clip rect (whole texels, as clip_rect yields them
/// back through device_span), so every part of it (fill, corner, strokes, image) is evaluated at texel centres.
fn texture_space_node(node: &Value, space: GroupSpace, scale: f64) -> Value {
    let sx = space.nw as f64 / space.w as f64;
    let sy = space.nh as f64 / space.h as f64;
    let m: [f64; 6] = std::array::from_fn(|i| component(&node["matrix"], i, IDENTITY[i]));
    let matrix = [
        sx * m[0],
        sy * m[1],
        sx * m[2],
        sy * m[3],
        (sx * (m[4] * scale - space.l) + space.l) / scale,
        (sy * (m[5] * scale - space.t) + space.t) / scale,
    ];
    let mut moved = node.clone();
    let Some(object) = moved.as_object_mut() else {
        return moved;
    };
    object.insert("matrix".into(), serde_json::json!(matrix));
    if node["clip"].as_array().is_some_and(|v| v.len() >= 4) {
        let [cx, cy, cw, ch] = std::array::from_fn(|i| component(&node["clip"], i, 0.0));
        let [cx0, cx1] = device_span(cx, cw, scale);
        let [cy0, cy1] = device_span(cy, ch, scale);
        let x0 = ((cx0 - space.l) * sx - 0.5).ceil() + space.l;
        let y0 = ((cy0 - space.t) * sy - 0.5).ceil() + space.t;
        let x1 = ((cx1 - space.l) * sx - 0.5).ceil() + space.l;
        let y1 = ((cy1 - space.t) * sy - 0.5).ceil() + space.t;
        object.insert(
            "clip".into(),
            serde_json::json!([x0 / scale, y0 / scale, (x1 - x0).max(0.0) / scale, (y1 - y0).max(0.0) / scale]),
        );
    }
    moved
}

fn has_part(node: &Value, in_group: bool, over: bool) -> bool {
    if over {
        node["text"].is_object()
            || node["selection"].is_object()
            || node["caret"].is_object()
            || (!in_group && !array(&node["strokes"]).is_empty())
    } else {
        (!in_group && !array(&node["shadows"]).is_empty())
            || (node["bg"].is_object()
                && (number(&node["bg"]["transparency"], 0.0) < 1.0
                    || !node["interaction"]["bg"].is_null()))
    }
}

/// UICorner coverage at the centres of cw x ch texels of a resampled group texture starting at texel `t0`,
/// local coordinates (i + off) / s spanning `size`, coarse derivatives on the texture's 2 x 2 texel quads
/// (on even texel coordinates, taken at the quad's top-left texel).
fn texture_corner_coverage(
    cw: usize,
    ch: usize,
    size: [f64; 2],
    radii: [f64; 4],
    off: [f64; 2],
    s: [f64; 2],
    t0: [i64; 2],
) -> Vec<f64> {
    let hw = size[0] / 2.0;
    let hh = size[1] / 2.0;
    let limit = hw.min(hh);
    let radii = radii.map(|r| r.min(limit).max(0.0));
    let sd = |i: i64, j: i64| {
        let px = (i as f64 + off[0]) / s[0] - hw;
        let py = (j as f64 + off[1]) / s[1] - hh;
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
    let px2 = t0[0].rem_euclid(2);
    let py2 = t0[1].rem_euclid(2);
    let mut data = vec![0.0; cw * ch];
    for j in 0..ch as i64 {
        let qj = j - (j + py2) % 2;
        for i in 0..cw as i64 {
            let qi = i - (i + px2) % 2;
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
            data[j as usize * cw + i as usize] = t * t * (3.0 - 2.0 * t);
        }
    }
    data
}

struct TexturePlacement {
    tx0: f64,
    ty0: f64,
    tx1: f64,
    ty1: f64,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    xs: Vec<[f64; 3]>,
    ys: Vec<[f64; 3]>,
    mask: Option<Vec<f64>>,
}

/// A nested CanvasGroup's snapped quad scaled into the outer group's texture: the texels whose centre is inside
/// (and inside its clip), the bilinear taps of its own texture per covered column / row (8-bit weights) and its
/// UICorner coverage (notes/images.md, nested groups in a resampled texture).
fn texture_group_placement(
    physical: PhysicalRect,
    clip: Option<[f64; 4]>,
    scale: f64,
    space: GroupSpace,
    nw: u32,
    nh: u32,
    radii: [f64; 4],
) -> Option<TexturePlacement> {
    let rect = physical.rect;
    let sx = space.nw as f64 / space.w as f64;
    let sy = space.nh as f64 / space.h as f64;
    let texel_x = |e: f64| ((e - space.l) * sx - 0.5).ceil();
    let texel_y = |e: f64| ((e - space.t) * sy - 0.5).ceil();
    let (tx0, ty0) = (texel_x(rect.x), texel_y(rect.y));
    let (tx1, ty1) = (texel_x(rect.x + rect.w), texel_y(rect.y + rect.h));
    let [mut x0, mut y0, mut x1, mut y1] = [tx0, ty0, tx1, ty1];
    if let Some([cx, cy, cw, ch]) = clip {
        let [cx0, cx1] = device_span(cx, cw, scale);
        let [cy0, cy1] = device_span(cy, ch, scale);
        x0 = x0.max(texel_x(cx0));
        y0 = y0.max(texel_y(cy0));
        x1 = x1.min(texel_x(cx1));
        y1 = y1.min(texel_y(cy1));
    }
    x0 = x0.max(0.0);
    y0 = y0.max(0.0);
    x1 = x1.min(space.nw as f64);
    y1 = y1.min(space.nh as f64);
    if x1 <= x0 || y1 <= y0 || tx1 <= tx0 || ty1 <= ty0 {
        return None;
    }
    let off_x = tx0 + 0.5 - (rect.x - space.l) * sx;
    let off_y = ty0 + 0.5 - (rect.y - space.t) * sy;
    let taps = |count: usize, off: f64, span: f64, n: u32| -> Vec<[f64; 3]> {
        (0..count)
            .map(|i| {
                let v = (i as f64 + off) * n as f64 / span - 0.5;
                let mut k = v.floor();
                let mut weight = ((v - k) * 256.0 + 0.5).floor();
                if weight == 256.0 {
                    k += 1.0;
                    weight = 0.0;
                }
                let last = n as f64 - 1.0;
                [k.clamp(0.0, last), (k + 1.0).clamp(0.0, last), weight]
            })
            .collect()
    };
    let xs = taps((tx1 - tx0) as usize, off_x, rect.w * sx, nw);
    let ys = taps((ty1 - ty0) as usize, off_y, rect.h * sy, nh);
    let mask = radii.iter().any(|r| *r > 0.0).then(|| {
        texture_corner_coverage(
            (tx1 - tx0) as usize,
            (ty1 - ty0) as usize,
            [physical.raw.w, physical.raw.h],
            radii,
            [off_x, off_y],
            [sx * rect.w / physical.raw.w, sy * rect.h / physical.raw.h],
            [tx0 as i64, ty0 as i64],
        )
    });
    Some(TexturePlacement {
        tx0,
        ty0,
        tx1,
        ty1,
        x0,
        y0,
        x1,
        y1,
        xs,
        ys,
        mask,
    })
}

fn group_taps(q: u32, n: u32) -> Vec<[f64; 3]> {
    (0..q)
        .map(|i| {
            if q == n {
                return [i as f64, i as f64, 0.0];
            }
            let numerator = (2 * i64::from(i) + 1) * i64::from(n) - i64::from(q);
            let denominator = 2 * i64::from(q);
            let mut k = numerator.div_euclid(denominator);
            let mut weight =
                ((numerator - k * denominator) * 512 + denominator) / (2 * denominator);
            if weight == 256 {
                k += 1;
                weight = 0;
            }
            [
                k.clamp(0, i64::from(n) - 1) as f64,
                (k + 1).clamp(0, i64::from(n) - 1) as f64,
                weight as f64,
            ]
        })
        .collect()
}

fn tint_lut(tint: [u8; 3]) -> Vec<u8> {
    let mut bytes = vec![0; 256 * 258 * 4];
    for alpha in 1..256usize {
        for p in 0..=alpha {
            let value = ((p * 255 + alpha / 2) / alpha).min(255) as u8;
            bytes[(alpha * 256 + p) * 4..(alpha * 256 + p) * 4 + 4]
                .copy_from_slice(&[value, value, value, 255]);
        }
    }
    for value in 0..256usize {
        let offset = (257 * 256 + value) * 4;
        for channel in 0..3 {
            bytes[offset + channel] =
                ((value as f64 * tint[channel] as f64 / 255.0) + 0.5).floor() as u8;
        }
        bytes[offset + 3] = 255;
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_targets_reuse_between_frames_but_not_within_a_frame() {
        let mut builder = DrawPreparer::new();
        builder.group_cache.begin_frame();
        let a = builder.group_pass(17, 19, [0.0; 2]).unwrap();
        let b = builder.group_pass(17, 19, [0.0; 2]).unwrap();
        assert_ne!(a.target, b.target);
        builder.group_cache.begin_frame();
        assert_eq!(
            builder.group_pass(17, 19, [0.0; 2]).unwrap().target,
            a.target
        );
        assert_eq!(
            builder.group_pass(17, 19, [0.0; 2]).unwrap().target,
            b.target
        );
    }

    #[test]
    fn group_fields_are_generated_once_and_retired_on_clear() {
        let mut builder = DrawPreparer::new();
        builder.group_cache.begin_frame();
        let id = builder.group_texture("field".into(), 1, 1, TextureFormat::Rgba8, || vec![0; 4]);
        let same = builder.group_texture("field".into(), 1, 1, TextureFormat::Rgba8, || {
            panic!("field was regenerated")
        });
        assert_eq!(id, same);
        assert_eq!(builder.packet.textures.len(), 1);
        builder.group_cache.clear();
        assert_eq!(builder.group_cache.begin_frame(), vec![id]);
    }

    #[test]
    fn scratch_segment_is_a_dependency_and_uses_premultiplied_blending() {
        let mut builder = DrawPreparer::new();
        builder.group_cache.begin_frame();
        let space = GroupSpace {
            l: 7.0,
            t: 9.0,
            w: 13,
            h: 11,
            nw: 14,
            nh: 12,
        };
        let mut texture = builder
            .group_pass(space.nw, space.nh, [space.l, space.t])
            .unwrap();
        let mut segment = None;
        let mut empty = true;
        let screen = builder
            .screen_segment(&mut segment, &mut texture, space, empty)
            .unwrap();
        let scratch_id = screen.target;
        assert_eq!((screen.width, screen.height), (13, 11));
        builder.flush_segment(&mut segment, &mut texture, space, &mut empty, false);
        assert!(!empty);
        assert_eq!(builder.packet.passes[0].target, scratch_id);
        assert_eq!(texture.batches[0].layer, Some(scratch_id));
        assert_eq!(texture.batches[0].blend, BlendMode::Premultiplied);
        let at = texture.batches[0].record_offsets[0] as usize;
        assert_eq!(builder.packet.texels[at + 2][0], SEGMENT as f32);
        assert_eq!(builder.packet.texels[at + 3], [13.0, 11.0, 14.0, 12.0]);
    }

    #[test]
    fn fractional_group_taps_match_integer_reference() {
        assert_eq!(
            group_taps(3, 3),
            vec![[0.0, 0.0, 0.0], [1.0, 1.0, 0.0], [2.0, 2.0, 0.0]]
        );
        assert_eq!(
            group_taps(3, 4),
            vec![[0.0, 1.0, 43.0], [1.0, 2.0, 128.0], [2.0, 3.0, 213.0]]
        );
        assert_eq!(group_taps(1, 2), vec![[0.0, 1.0, 128.0]]);
    }
    #[test]
    fn masks_keep_unsnapped_size_and_device_parity() {
        let a = super::super::coverage::corner_coverage_raw(13, 11, [4.0; 4], 0, 0, 12.6, 10.7);
        let b = super::super::coverage::corner_coverage_raw(13, 11, [4.0; 4], 1, 0, 12.6, 10.7);
        assert_ne!(a, b);
        assert_eq!(a[5 * 13 + 6], 1.0);
        assert!(a.iter().all(|value| *value >= 0.0 && *value <= 1.0));
    }
}
