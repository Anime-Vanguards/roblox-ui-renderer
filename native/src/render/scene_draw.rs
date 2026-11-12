//! Ordered native scene preparation; all raster operations are uber-shader records (packet.rs).
use super::paint::Paint;
use super::{geometry::*, packet::*};
use crate::{assets::AssetStore, scene::{SceneStore, layer_pass}};
use serde_json::Value;
use rustc_hash::FxHashMap;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
pub type DrawError = anyhow::Error;
use std::time::Instant;
/// Per-prepare CPU timings. Categories are exclusive; upload_ms is a nested diagnostic
/// included in the drawing category that requested the upload. Counts describe actual work.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct DrawStats {
    pub total_ms: f64,
    pub background_ms: f64,
    pub images_ms: f64,
    pub text_ms: f64,
    pub strokes_ms: f64,
    pub shadows_ms: f64,
    pub groups_ms: f64,
    pub scrollbars_ms: f64,
    pub node_cache_ms: f64,
    /// Layers rasterized together at the end of prepare (DrawPreparer::run_pending)
    pub deferred_raster_ms: f64,
    pub other_ms: f64,
    /// Diagnostic subdivisions of other_ms; do not add them to the exclusive categories.
    pub setup_ms: f64,
    pub finalize_ms: f64,
    pub record_capture_ms: f64,
    pub record_insert_ms: f64,
    pub upload_ms: f64,
    pub backgrounds: u64,
    pub images: u64,
    pub texts: u64,
    pub strokes: u64,
    pub shadows: u64,
    pub groups: u64,
    pub scrollbars: u64,
    pub node_cache_hits: u64,
    pub node_cache_misses: u64,
    pub nodes_prepared: u64,
    pub upload_cache_hits: u64,
    pub record_texels: usize,
    pub instances: usize,
    pub batches: usize,
    pub passes: usize,
    /// Layer bytes presented to the upload cache, plus newly generated group fields.
    /// This measures output volume, not every intermediate sampler or mip operation.
    pub pixel_bytes_processed: u64,
    pub texture_upload_bytes: u64,
    pub texture_uploads: usize,
    pub retained_texture_bytes: usize,
    pub retained_record_bytes: usize,
    pub group_field_bytes: usize,
    /// The scene was the last packet's: nothing was prepared (RenderPacket::unchanged)
    pub unchanged: bool,
    /// Records dropped as drawing nothing (clipped out or off the target), and layers not rasterized
    /// because only such records asked for them
    pub culled_records: u64,
    /// Display-list hits taken by id without reading the node (unchanged since last frame's scene)
    pub retained_walk_hits: u64,
    /// Nodes not prepared as lying wholly outside their clip (node_offscreen)
    pub offscreen_nodes: u64,
    /// Nodes whose scene node differs from the last prepared scene's (added, removed or replaced)
    pub changed_nodes: u64,
    /// The asset revision or fonts changed since the last packet (text and image records were dropped)
    pub assets_changed: bool,
    /// Unchanged nodes were taken from last frame's display list by id (no exact shadow boxes, same assets)
    pub walk_retained: bool,
    /// Exactly blended shadow parts this frame (exact_shadow_boxes)
    pub exact_shadows: usize,
    /// Deferred rasters by kind: text, stroke, fill, shadow, other image-cache layer, other
    pub raster_counts: [u64; 6],
    pub raster_pixels: [u64; 6],
    /// Diagnostics: the draw-order walk, draw_node calls in it, and nodes prepared from scratch in those
    pub walk_ms: f64,
    pub draw_node_ms: f64,
    pub prepared_node_ms: f64,
    pub skipped_rasters: u64,
    /// Node records reused from last frame's display list (also counted in node_cache_hits)
    pub display_list_hits: u64,
    pub text_cache_hits: u64,
    /// Text layers found by the node's text itself (text::TextLayerCache)
    pub text_node_hits: u64,
    pub text_cache_misses: u64,
    pub text_cache_bytes: usize,
    pub image_cache_bytes: usize,
    pub source_texture_bytes: usize,
    pub image_cache_stats: super::images::ImageCacheStats,
}
impl DrawStats {
    pub(crate) fn category_ms(&self) -> f64 {
        self.background_ms
            + self.images_ms
            + self.text_ms
            + self.strokes_ms
            + self.shadows_ms
            + self.groups_ms
            + self.scrollbars_ms
            + self.node_cache_ms
            + self.deferred_raster_ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn gradient_border_outer_half_pixel_ties_round_down() {
        let mut scene = SceneStore::default();
        scene.apply(&json!({"viewport":{"width":300,"height":200,"scale":1.5},"nodes":[{"id":1,"rect":[0,0,240,120],"bg":{"color":[1,1,1]},"border":{"size":1,"color":[0,0,0]},"gradient":{"color":[[0,1,0,0],[1,0,1,0]]}}],"layers":[{"draw":[1]}],"fonts":[],"images":[]})).unwrap();
        let assets = AssetStore::new(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap(),
            true,
        );
        let packet = DrawPreparer::new().prepare(&scene, &assets).unwrap();
        assert_eq!(packet.texels[64], [-2., -2., 361., 181.]);
    }
    #[test]
    fn a_fill_whose_rotation_cancels_is_drawn_on_snapped_edges() {
        // fixtures/image-probe-translated Holder7.Fill: a frame turned back by -90 in a holder turned 90 has
        // the matrix of a translation, and its edges at 943.5 and 526.5 device px round up as an unturned
        // frame's do, where a turned quad keeps its left and top ties and leaves the right and bottom out
        let draw = |node: serde_json::Value| {
            let mut scene = SceneStore::default();
            scene.apply(&json!({"viewport":{"width":1200,"height":900,"scale":1.5},"nodes":[node],"layers":[{"draw":[1]}],"fonts":[],"images":[]})).unwrap();
            let assets = AssetStore::new(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .unwrap(),
                true,
            );
            DrawPreparer::new().prepare(&scene, &assets).unwrap().texels[64]
        };
        let turned = draw(json!({"id":1,"rect":[611,491,70,30],"matrix":[1,0,0,1,18,-140],"bg":{"color":[1,1,1]}}));
        let plain = draw(json!({"id":1,"rect":[629,351,70,30],"bg":{"color":[1,1,1]}}));
        assert_eq!(plain, [944., 527., 1049., 572.]);
        assert_eq!(turned, plain);
    }
    #[test]
    fn a_turned_frame_keeps_its_gradients_steps() {
        // fixtures/gradient-probe-turned.luau: black to white on a frame turned 180 degrees runs right to
        // left in the 256 steps and squared colours of an unturned frame (the pixel half a percent in is
        // 255 / 16, not 1), without a fringe
        let mut scene = SceneStore::default();
        scene.apply(&json!({"viewport":{"width":200,"height":40,"scale":1},"nodes":[{"id":1,"rect":[10,10,100,10],"matrix":[-1,0,0,-1,120,30],"bg":{"color":[1,1,1]},"gradient":{"color":[[0,0,0,0],[1,1,1,1]]}}],"layers":[{"draw":[1]}],"fonts":[],"images":[]})).unwrap();
        let assets = AssetStore::new(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap(),
            true,
        );
        let packet = DrawPreparer::new().prepare(&scene, &assets).unwrap();
        let pixels = &packet.textures[0].levels[0].bytes;
        assert_eq!(pixels.len(), 100 * 10 * 4);
        assert!(pixels.chunks_exact(4).all(|p| p[3] == 255));
        assert!((253..=254).contains(&pixels[0]), "{}", pixels[0]);
        assert!((180..=182).contains(&pixels[49 * 4]), "{}", pixels[49 * 4]);
        assert!((15..=16).contains(&pixels[99 * 4]), "{}", pixels[99 * 4]);
    }
    #[test]
    fn square_gradient_does_not_apply_a_corner_mask() {
        let mut scene = SceneStore::default();
        scene.apply(&json!({"viewport":{"width":8,"height":8,"scale":1},"nodes":[{"id":1,"rect":[0,0,5,5],"bg":{"color":[1,1,1]},"gradient":{"color":[[0,1,0,0],[1,0,1,0]]}}],"layers":[{"draw":[1]}],"fonts":[],"images":[]})).unwrap();
        let assets = AssetStore::new(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap(),
            true,
        );
        let packet = DrawPreparer::new().prepare(&scene, &assets).unwrap();
        let pixels = &packet.textures[0].levels[0].bytes;
        assert_eq!(pixels.len(), 100);
        assert!(pixels.chunks_exact(4).all(|p| p[3] == 255));
    }
    #[test]
    fn retained_nodes_reuse_records_and_reset_never_aliases_textures() {
        let mut scene = SceneStore::default();
        scene.apply(&json!({"viewport":{"width":32,"height":24,"scale":1},"background":[0,0,0],"nodes":[{"id":1,"rect":[2,3,15,12],"bg":{"color":[1,0.5,0.25]},"corner":4}],"layers":[{"draw":[1]}],"fonts":[],"images":[]})).unwrap();
        let assets = AssetStore::new(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap(),
            true,
        );
        let mut draw = DrawPreparer::new();
        let first = draw.prepare(&scene, &assets).unwrap();
        let cold = draw.last_stats().clone();
        // The same scene again draws nothing new
        let same = draw.prepare(&scene, &assets).unwrap();
        assert!(same.unchanged && same.texels.is_empty() && same.textures.is_empty());
        // A scene holding the same node 1 (and one more, not drawn) reuses its records
        scene
            .apply(&json!({"delta":{"nodes":[{"id":2,"rect":[0,0,1,1]}]}}))
            .unwrap();
        let second = draw.prepare(&scene, &assets).unwrap();
        let warm = draw.last_stats();
        assert!(!second.unchanged);
        assert_eq!(cold.node_cache_misses, 1);
        assert_eq!(cold.backgrounds, 1);
        assert_eq!(warm.node_cache_hits, 1);
        assert_eq!(warm.nodes_prepared, 0);
        assert_eq!(warm.backgrounds, 0);
        assert_eq!(warm.pixel_bytes_processed, 0);
        assert_eq!(warm.record_texels, second.texels.len());
        assert_eq!(cold.texture_upload_bytes, 15 * 12 * 4);
        assert!(warm.total_ms >= warm.category_ms());
        assert!(!first.textures.is_empty());
        assert!(second.textures.is_empty());
        assert_eq!(first.texels, second.texels);
        assert_eq!(
            first.passes[0].batches[0].record_offsets,
            second.passes[0].batches[0].record_offsets
        );
        let old = first.textures[0].id;
        draw.reset_resources();
        let third = draw.prepare(&scene, &assets).unwrap();
        assert!(third.retire_textures.contains(&old));
        assert_ne!(third.textures[0].id, old);
    }
    #[test]
    fn culled_leaf_preserves_separate_visible_child_and_layer_order() {
        let mut scene = SceneStore::default();
        scene.apply(&json!({"viewport":{"width":32,"height":24,"scale":1},"nodes":[{"id":1,"rect":[0,0,10,10],"culled":true,"bg":{"color":[1,0,0]}},{"id":2,"rect":[4,5,6,7],"bg":{"color":[0,1,0]}}],"layers":[{"displayOrder":1,"draw":[1]},{"displayOrder":0,"draw":[2]}],"fonts":[],"images":[]})).unwrap();
        let assets = AssetStore::new(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap(),
            true,
        );
        let packet = DrawPreparer::new().prepare(&scene, &assets).unwrap();
        assert_eq!(
            packet.passes[0]
                .batches
                .iter()
                .map(|b| b.record_offsets.len())
                .sum::<usize>(),
            1
        );
        assert_eq!(packet.texels[64], [4., 5., 10., 12.]);
    }
}

pub(crate) struct BuildContext {
    pub origin: [f64; 2],
    pub width: u32,
    pub height: u32,
    pub target: TextureId,
    pub batches: Vec<DrawBatch>,
    pub clear: Option<[u8; 4]>,
}
impl BuildContext {
    pub fn new(target: TextureId, width: u32, height: u32, origin: [f64; 2]) -> Self {
        Self {
            origin,
            width,
            height,
            target,
            batches: Vec::new(),
            clear: Some([0; 4]),
        }
    }
}
struct CachedTexture {
    id: TextureId,
    bytes: usize,
    last: u64,
    width: u32,
    height: u32,
    format: TextureFormat,
    /// The uploaded bytes (shared with the image cache's layer and the upload), compared on a hash hit
    data: std::sync::Arc<[u8]>,
}
pub struct DrawPreparer {
    pub(crate) stats: DrawStats,
    pub(crate) packet: RenderPacket,
    pub(crate) scale: f64,
    next_id: u64,
    pub(crate) frame: u64,
    textures: FxHashMap<u64, Vec<CachedTexture>>,
    texture_keys: FxHashMap<TextureId, u64>,
    node_cache: super::node_records::NodeRecordCache,
    pub(crate) retired: Vec<TextureId>,
    pub(crate) group_cache: super::groups::GroupCache,
    pub(crate) image_cache: super::images::ImageCache,
    pub(crate) gpu_image_cache: super::image_records::GpuImageCache,
    /// Layers whose pixels are rasterized at the end of prepare, all in parallel (texture ids are
    /// assigned when they are queued, so records reference them like any upload)
    pending: Vec<PendingRaster>,
    /// Image cache keys queued this frame: a second request shares the first one's texture
    pending_keys: FxHashMap<String, TextureId>,
    /// Text composites queued this frame by quads hash (indices into `pending`)
    pending_text: FxHashMap<u64, Vec<usize>>,
    /// Image cache layers already uploaded, by their shared pixel allocation (the Arc is kept so
    /// the address cannot be reused): a cache hit finds its texture without hashing its bytes
    layer_textures: FxHashMap<usize, (std::sync::Arc<[u8]>, TextureId)>,
    /// This frame's exactly blended shadow parts: (node id, shadow index) -> device box (exact_shadow_boxes)
    exact_shadows: HashMap<(u64, usize), [f64; 4]>,
    /// Composited text layers by their quads (text::TextLayerCache)
    text_cache: super::text::TextLayerCache,
    /// The scene of the last packet prepared (its nodes, layers, header and fonts are shared, never
    /// copied) and the asset revision it saw: an identical next scene draws nothing new
    last_scene: Option<(SceneStore, u64)>,
    /// The retained display list: every node record drawn this frame, in draw order, and last frame's.
    /// A node drawn again in the same place of the order with the same context reuses its records
    /// without a cache lookup (drawn_list_hit); the node record cache serves the rest.
    drawn: Vec<DrawnNode>,
    drawn_before: Vec<DrawnNode>,
    drawn_cursor: usize,
    /// Node ids whose node differs from the last prepared scene's (added, removed or replaced), and
    /// whether the walk may take unchanged nodes' records by id
    changed_nodes: rustc_hash::FxHashSet<u64>,
    walk_retained: bool,
    /// Record texels of the last packet (the next one is allocated at that size)
    texel_capacity: usize,
    /// emit drops records that draw nothing (off by record-format tests only)
    pub(crate) cull_records: bool,
    /// Bytes of the resident layer textures (`textures`)
    texture_bytes: usize,
    /// No texture was retired since last frame's packet
    textures_stable: bool,
    /// Nodes with shadows, kept up to date from changed_nodes; exact_shadow_boxes is recomputed only when
    /// one of them, the draw order or the scale changes
    shadowed: rustc_hash::FxHashSet<u64>,
    shadowed_known: bool,
    shadows_layers: usize,
    /// Stroke coverage (and ring mask) by the stroke's geometry, without its colour and transparency: a
    /// stroke whose transparency or colour changes (a hover tween) is tinted from it (tint_stroke), not
    /// rasterized again. Least recently used entries leave past STROKE_COVERAGE_BUDGET bytes.
    stroke_coverage: FxHashMap<String, (std::sync::Arc<StrokeCoverage>, u64)>,
    stroke_coverage_bytes: usize,
    /// Coverage computed by this frame's queued stroke rasters, stored once they ran
    pending_coverage: Vec<(String, std::sync::Arc<std::sync::OnceLock<std::sync::Arc<StrokeCoverage>>>)>,
    /// A shadow's alpha bytes by its geometry, without its colour: a shadow whose colour animates is
    /// tinted from them, not rasterized again (kept like stroke_coverage, within SHADOW_ALPHA_BUDGET)
    shadow_alpha: FxHashMap<String, (std::sync::Arc<Vec<u8>>, u64)>,
    shadow_alpha_bytes: usize,
    /// Alpha computed by this frame's queued shadow rasters, stored once they ran
    pending_shadow_alpha: Vec<(String, std::sync::Arc<std::sync::OnceLock<std::sync::Arc<Vec<u8>>>>)>,
    /// The node draw_node_uncached is drawing (0: none), whose layer misses layer_misses tracks
    layer_node: u64,
    /// By node: the last frame one of its layers missed the image cache, and whether those misses go on
    /// probation (redrawn_layer)
    layer_misses: FxHashMap<u64, (u64, bool)>,
}
/// A stroke's coverage and ring mask (coverage::border_stroke_alpha_modes) for its layer's pixels
pub(crate) struct StrokeCoverage {
    coverage: Vec<f32>,
    ring: Vec<u8>,
}
impl StrokeCoverage {
    fn bytes(&self) -> usize {
        self.coverage.len() * 4 + self.ring.len() + 64
    }
}
const STROKE_COVERAGE_BUDGET: usize = 48 << 20;
const SHADOW_ALPHA_BUDGET: usize = 16 << 20;
/// A stroke layer's premultiplied pixels from its coverage, colour and vertex alpha byte (exactly the
/// stroke raster's: coverage::stroke_alpha_byte per pixel, then premultiply)
fn tint_stroke(coverage: &StrokeCoverage, rgb: [u8; 3], a: u8) -> Vec<u8> {
    // Premultiplied as it goes; a pixel without coverage (a ring's inside) stays transparent black
    let mut bytes = vec![0; coverage.coverage.len() * 4];
    for (k, p) in bytes.chunks_exact_mut(4).enumerate() {
        if coverage.coverage[k] <= 0. {
            continue;
        }
        let alpha = super::coverage::stroke_alpha_byte(coverage.coverage[k] as f64, a as f64, coverage.ring[k] != 0);
        for c in 0..3 {
            p[c] = super::geometry::mul_div255(rgb[c], alpha);
        }
        p[3] = alpha;
    }
    bytes
}
/// A node's records as drawn in a frame. `node` is the address of the scene's node: last_scene holds
/// that scene (and so the node) until these entries are replaced or cleared (reset_resources), so the
/// address identifies it.
struct DrawnNode {
    id: u64,
    node: usize,
    context: super::node_records::RecordContext,
    records: std::sync::Arc<super::node_records::RecordedNode>,
    /// Its records depend on the assets (node_records::asset_dependent): dropped when they change
    asset_dependent: bool,
}
thread_local! {
    /// The image cache's source for layers computed from geometry alone (one shared empty allocation)
    static NO_SOURCE: std::sync::Arc<[u8]> = std::sync::Arc::from(Vec::<u8>::new());
}
/// Layer-cache hits whose bytes differed from a fresh raster (RUR_VERIFY_LAYER_CACHE=1)
pub static LAYER_CACHE_MISMATCHES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// Layer-cache hits checked against a fresh raster (RUR_VERIFY_LAYER_CACHE=1)
pub static LAYER_CACHE_VERIFIED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// RUR_VERIFY_LAYER_CACHE=1: every cache hit is rasterized again where it is drawn now and compared with
/// the cached bytes (a layer keyed relative to its origin must equal the one rasterized elsewhere);
/// mismatches are counted and reported on stderr. A diagnostic: it costs a raster per hit.
fn verify_layer_hit(key: &str, cached: &[u8], raster: Box<dyn FnOnce() -> Vec<u8> + Send>) {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if !*ENABLED.get_or_init(|| std::env::var("RUR_VERIFY_LAYER_CACHE").as_deref() == Ok("1")) {
        return;
    }
    use std::sync::atomic::Ordering;
    LAYER_CACHE_VERIFIED.fetch_add(1, Ordering::Relaxed);
    let fresh = raster();
    if fresh[..] != cached[..] {
        let count = LAYER_CACHE_MISMATCHES.fetch_add(1, Ordering::Relaxed) + 1;
        if count <= 20 {
            let differing = fresh.iter().zip(cached).filter(|(a, b)| a != b).count();
            eprintln!(
                "rur-native: layer cache hit differs from a fresh raster ({differing} bytes): {}",
                key.chars().take(300).collect::<String>()
            );
        }
    }
}
pub(crate) struct PendingRaster {
    id: TextureId,
    width: u32,
    height: u32,
    format: TextureFormat,
    job: Box<dyn FnOnce() -> Vec<u8> + Send>,
    /// Stored in the image cache once rasterized: key, layer placement, premultiplied, source, on
    /// probation (ImageCache::store_with)
    image: Option<(String, i32, i32, bool, std::sync::Arc<[u8]>, bool)>,
    /// Stored in the text cache once rasterized
    text: Option<(super::text::TextStore, super::text::TextPixels)>,
}
impl Default for DrawPreparer {
    fn default() -> Self {
        Self::new()
    }
}
impl DrawPreparer {
    pub fn new() -> Self {
        Self {
            stats: DrawStats::default(),
            packet: RenderPacket::default(),
            scale: 1.,
            next_id: 1,
            frame: 0,
            textures: FxHashMap::default(),
            texture_keys: FxHashMap::default(),
            node_cache: Default::default(),
            retired: Vec::new(),
            group_cache: Default::default(),
            image_cache: Default::default(),
            gpu_image_cache: Default::default(),
            pending: Vec::new(),
            pending_keys: FxHashMap::default(),
            pending_text: FxHashMap::default(),
            layer_textures: FxHashMap::default(),
            exact_shadows: HashMap::new(),
            text_cache: Default::default(),
            last_scene: None,
            drawn: Vec::new(),
            drawn_before: Vec::new(),
            drawn_cursor: 0,
            changed_nodes: Default::default(),
            walk_retained: false,
            texel_capacity: 0,
            cull_records: true,
            texture_bytes: 0,
            textures_stable: false,
            shadowed: Default::default(),
            shadowed_known: false,
            shadows_layers: 0,
            stroke_coverage: Default::default(),
            stroke_coverage_bytes: 0,
            pending_coverage: Vec::new(),
            shadow_alpha: Default::default(),
            shadow_alpha_bytes: 0,
            pending_shadow_alpha: Vec::new(),
            layer_node: 0,
            layer_misses: Default::default(),
        }
    }
    pub fn last_stats(&self) -> &DrawStats {
        &self.stats
    }
    /// What the preparer keeps between frames, in bytes (the render service's /memory)
    pub fn memory(&self) -> serde_json::Value {
        serde_json::json!({
            "uploadedLayerCopies": self.texture_bytes,
            "nodeRecords": self.node_cache.bytes(),
            "groups": self.group_cache.resident_bytes(),
            "imageCache": self.image_cache.bytes(),
            "imageCacheStats": self.image_cache.stats(),
            "gpuImageCache": self.gpu_image_cache.bytes(),
            "textCache": self.text_cache.bytes(),
            "strokeCoverage": self.stroke_coverage_bytes,
            "shadowAlpha": self.shadow_alpha_bytes,
            "layerTextures": self.layer_textures.len(),
            "lastScene": self.last_scene.is_some(),
            "texelCapacity": self.texel_capacity * 16,
        })
    }
    pub fn reset_resources(&mut self) {
        self.retired
            .extend(self.textures.values().flatten().map(|v| v.id));
        self.textures.clear();
        self.texture_bytes = 0;
        self.texture_keys.clear();
        self.layer_textures.clear();
        self.node_cache.clear();
        self.group_cache.clear();
        self.image_cache = Default::default();
        self.text_cache.clear();
        self.stroke_coverage.clear();
        self.stroke_coverage_bytes = 0;
        self.pending_coverage.clear();
        self.shadow_alpha.clear();
        self.shadow_alpha_bytes = 0;
        self.pending_shadow_alpha.clear();
        self.layer_misses.clear();
        self.last_scene = None;
        self.drawn.clear();
        self.drawn_before.clear();
        self.walk_retained = false;
        self.shadowed_known = false;
        self.retired.extend(self.gpu_image_cache.clear());
    }
    pub fn prepare(
        &mut self,
        scene: &SceneStore,
        assets: &AssetStore,
    ) -> Result<RenderPacket, DrawError> {
        let prepare_started = Instant::now();
        let _prepare_scope = crate::microprofile::scope("Prepare scene");
        let _memory = crate::memtrace::scope(crate::memtrace::Tag::Prepare);
        self.stats = DrawStats::default();
        self.image_cache.reset_stats();
        let view = scene.viewport();
        // Nothing to draw anew: the same nodes, layers, header (viewport, background) and fonts as the
        // last packet, with the same assets. The renderer keeps that frame (RenderPacket::unchanged).
        let last = self.last_scene.take();
        if let Some((last, revision)) = &last {
            if self.retired.is_empty()
                && *revision == assets.render_revision()
                && last.shares_nodes(scene)
                && std::ptr::eq(last.layers(), scene.layers())
                && std::ptr::eq(last.header(), scene.header())
                && std::ptr::eq(last.fonts(), scene.fonts())
            {
                self.stats.unchanged = true;
                self.stats.total_ms = prepare_started.elapsed().as_secs_f64() * 1000.;
                self.last_scene = Some((scene.clone(), *revision));
                return Ok(RenderPacket {
                    width: (view.width * view.scale).round().max(1.) as u32,
                    height: (view.height * view.scale).round().max(1.) as u32,
                    unchanged: true,
                    ..Default::default()
                });
            }
        }
        // What changed since the last prepared scene, by node identity
        self.changed_nodes.clear();
        let diffed = match &last {
            Some((last, _)) => {
                if !last.shares_nodes(scene) {
                    let mut old = last.node_arcs().peekable();
                    let mut new = scene.node_arcs().peekable();
                    loop {
                        match (old.peek(), new.peek()) {
                            (None, None) => break,
                            (Some((a, _)), None) => {
                                self.changed_nodes.insert(*a);
                                old.next();
                            }
                            (None, Some((b, _))) => {
                                self.changed_nodes.insert(*b);
                                new.next();
                            }
                            (Some((a, x)), Some((b, y))) => {
                                if a < b {
                                    self.changed_nodes.insert(*a);
                                    old.next();
                                } else if b < a {
                                    self.changed_nodes.insert(*b);
                                    new.next();
                                } else {
                                    if !std::sync::Arc::ptr_eq(x, y) {
                                        self.changed_nodes.insert(*a);
                                    }
                                    old.next();
                                    new.next();
                                }
                            }
                        }
                    }
                }
                true
            }
            None => false,
        };
        // The previous scene lives until this frame is prepared (drawn_before holds its nodes' addresses),
        // then its own nodes are freed off this thread
        let previous_scene = last;
        let scale_same = self.scale == view.scale;
        self.scale = view.scale;
        self.frame += 1;
        let assets_changed = self
            .node_cache
            .begin_frame(scene.fonts(), assets.render_revision());
        self.drawn_before = std::mem::take(&mut self.drawn);
        self.text_cache.begin_frame(assets_changed);
        if assets_changed {
            // Text and image records may draw differently with the new assets; the others stay valid
            // (as in the node record cache, which keeps them too): a node record must not be lost (and its
            // layers rasterized again) when an unrelated image or glyph page arrives
            self.drawn_before.retain(|entry| !entry.asset_dependent);
        }
        self.drawn_cursor = 0;
        self.retired.extend(self.group_cache.begin_frame());
        let width = (view.width * view.scale).round().max(1.) as u32;
        let height = (view.height * view.scale).round().max(1.) as u32;
        let mut texels = Vec::with_capacity(self.texel_capacity);
        texels.extend_from_slice(&byte_float_texels());
        self.packet = RenderPacket {
            width,
            height,
            texels,
            retire_textures: std::mem::take(&mut self.retired),
            ..Default::default()
        };
        self.textures_stable = self.packet.retire_textures.is_empty() && !assets_changed;
        self.drawn.reserve(self.drawn_before.len());
        // Nodes with shadows; the exact shadow boxes again when one of them, the draw order or the scale changed
        let mut shadows_changed = !diffed || !self.shadowed_known;
        if shadows_changed {
            self.shadowed = scene
                .node_arcs()
                .filter(|(_, node)| !array(&node["shadows"]).is_empty())
                .map(|(id, _)| id)
                .collect();
        } else {
            for &id in &self.changed_nodes {
                let had = self.shadowed.contains(&id);
                let has = scene.node(id).is_some_and(|node| !array(&node["shadows"]).is_empty());
                shadows_changed |= had || has;
                if has {
                    self.shadowed.insert(id);
                } else {
                    self.shadowed.remove(&id);
                }
            }
        }
        self.shadowed_known = true;
        let layers_id = scene.layers() as *const Value as usize;
        if shadows_changed || layers_id != self.shadows_layers || !scale_same {
            self.exact_shadows = if self.shadowed.is_empty() {
                HashMap::new()
            } else {
                exact_shadow_boxes(scene, self.scale)
            };
        }
        self.shadows_layers = layers_id;
        // Unchanged nodes are drawn from last frame's records by id when nothing else shaped them: the
        // same assets and no exact shadow boxes (they key records); the scale is in the record context
        self.walk_retained = diffed && !assets_changed && self.exact_shadows.is_empty();
        self.stats.changed_nodes = self.changed_nodes.len() as u64;
        self.stats.assets_changed = assets_changed;
        self.stats.walk_retained = self.walk_retained;
        self.stats.exact_shadows = self.exact_shadows.len();
        let mut main = BuildContext::new(FRAME_TARGET, width, height, [0., 0.]);
        if let Some(background) = scene.header().get("background").filter(|v| v.is_array()) {
            let c = color(background, [0.; 3]).map(quantize);
            main.clear = Some([c[0], c[1], c[2], 255]);
        }
        let mut layers: Vec<_> = array(scene.layers()).iter().collect();
        layers.sort_by(|a, b| {
            layer_pass(a).cmp(&layer_pass(b)).then(number(&a["displayOrder"], 0.).total_cmp(&number(&b["displayOrder"], 0.)))
        });
        self.stats.setup_ms = prepare_started.elapsed().as_secs_f64() * 1000.;
        let walk_started = Instant::now();
        for layer in layers {
            if let Err(error) =
                self.draw_entries(&mut main, array(&layer["draw"]), scene, assets, None)
            {
                // A failed packet never reached the device. Cached upload IDs must not masquerade
                // as resident on the next successful frame.
                self.pending.clear();
                self.pending_keys.clear();
                self.pending_text.clear();
                self.pending_coverage.clear();
                self.reset_resources();
                self.retired
                    .extend(self.packet.retire_textures.iter().copied());
                self.stats.total_ms = prepare_started.elapsed().as_secs_f64() * 1000.;
                return Err(error);
            }
        }
        self.stats.walk_ms = walk_started.elapsed().as_secs_f64() * 1000.;
        self.run_pending(&main.batches);
        self.store_coverage();
        let finalize_started = Instant::now();
        self.finish_pass(main);
        let mut total: usize = self.texture_bytes;
        // Records reused from the display list do not touch their textures: before anything is
        // evicted, every texture this packet draws with is marked used this frame
        if total > 96 << 20 || self.gpu_image_cache.over_budget() {
            let frame = self.frame;
            for batch in self.packet.passes.iter().flat_map(|p| p.batches.iter()) {
                for id in batch
                    .image_slots
                    .iter()
                    .copied()
                    .chain([batch.glyphs, batch.layer, batch.lut])
                    .flatten()
                {
                    if let Some(key) = self.texture_keys.get(&id) {
                        if let Some(texture) = self.textures.get_mut(key).and_then(|v| v.iter_mut().find(|v| v.id == id)) {
                            texture.last = frame;
                        }
                    } else {
                        self.gpu_image_cache.touch(id, frame);
                    }
                }
            }
        }
        if total > 96 << 20 {
            // Down to 80 MB, so the next eviction is some frames away
            let frame = self.frame;
            let retired = &mut self.retired;
            self.textures.retain(|_, list| {
                list.retain(|v| {
                    if total > 80 << 20 && v.last < frame {
                        total -= v.bytes;
                        retired.push(v.id);
                        false
                    } else {
                        true
                    }
                });
                !list.is_empty()
            });
        }
        self.retired.extend(self.gpu_image_cache.evict(self.frame));
        for id in &self.retired {
            self.texture_keys.remove(id);
        }
        self.texture_bytes = total;
        // Layers the image cache released, or whose texture is gone (a stale entry only holds memory:
        // swept when textures were retired, else every 64 frames)
        if !self.retired.is_empty() || self.frame % 64 == 0 {
            let texture_keys = &self.texture_keys;
            self.layer_textures.retain(|_, (rgba, id)| {
                std::sync::Arc::strong_count(rgba) > 1 && texture_keys.contains_key(id)
            });
        }
        self.stats.record_texels = self.packet.texels.len();
        self.texel_capacity = self.packet.texels.len().next_power_of_two();
        self.stats.passes = self.packet.passes.len();
        self.stats.batches = self.packet.passes.iter().map(|p| p.batches.len()).sum();
        self.stats.instances = self
            .packet
            .passes
            .iter()
            .flat_map(|p| p.batches.iter())
            .map(|b| b.record_offsets.len())
            .sum();
        self.stats.texture_uploads = self.packet.textures.len();
        self.stats.texture_upload_bytes = self
            .packet
            .textures
            .iter()
            .flat_map(|t| t.levels.iter())
            .map(|l| l.bytes.len() as u64)
            .sum();
        self.stats.retained_texture_bytes = total;
        self.stats.retained_record_bytes = self.node_cache.bytes();
        self.stats.group_field_bytes = self.group_cache.resident_bytes();
        self.stats.image_cache_bytes = self.image_cache.bytes();
        self.stats.image_cache_stats = self.image_cache.stats();
        self.stats.source_texture_bytes = self.gpu_image_cache.bytes();
        self.stats.text_cache_bytes = self.text_cache.bytes();
        self.last_scene = Some((scene.clone(), assets.render_revision()));
        super::drop_later(previous_scene);
        self.stats.finalize_ms = finalize_started.elapsed().as_secs_f64() * 1000.;
        self.stats.total_ms = prepare_started.elapsed().as_secs_f64() * 1000.;
        self.stats.other_ms = (self.stats.total_ms - self.stats.category_ms()).max(0.);
        Ok(std::mem::take(&mut self.packet))
    }
    /// An image layer: a cache hit uploads (or finds) its bytes now, a miss is queued and stored in
    /// the image cache once rasterized.
    pub(crate) fn upload_image_layer(&mut self, planned: super::images::PlannedLayer) -> (i32, i32, u32, u32, TextureId) {
        match planned {
            super::images::PlannedLayer::Ready(layer) => {
                let id = self.upload_shared(layer.width, layer.height, &layer.rgba);
                (layer.x, layer.y, layer.width, layer.height, id)
            }
            super::images::PlannedLayer::Pending(pending) => {
                let placed = (pending.x, pending.y, pending.width, pending.height);
                if let Some(id) = self.pending_keys.get(&pending.key) {
                    return (placed.0, placed.1, placed.2, placed.3, *id);
                }
                let id = self.texture_id();
                self.pending_keys.insert(pending.key.clone(), id);
                self.pending.push(PendingRaster {
                    id,
                    width: pending.width,
                    height: pending.height,
                    format: TextureFormat::Rgba8,
                    job: pending.job,
                    image: Some((pending.key, pending.x, pending.y, pending.premultiplied, pending.source, pending.probation)),
                    text: None,
                });
                (placed.0, placed.1, placed.2, placed.3, id)
            }
        }
    }
    /// A layer kept in the image cache under `key` (content determined by the key): a hit reuses
    /// its texture, a miss queues `raster` with the frame's other layers.
    pub(crate) fn cached_layer(
        &mut self,
        key: String,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        raster: Box<dyn FnOnce() -> Vec<u8> + Send>,
    ) -> (i32, i32, u32, u32, TextureId) {
        self.cached_layer_with(key, x, y, width, height, raster, false)
    }
    /// cached_layer, a miss stored on probation when `probation` (a layer cheap to make again)
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn cached_layer_with(
        &mut self,
        key: String,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        raster: Box<dyn FnOnce() -> Vec<u8> + Send>,
        probation: bool,
    ) -> (i32, i32, u32, u32, TextureId) {
        let planned = match self.image_cache.cached(&key) {
            Some(hit) => {
                verify_layer_hit(&key, &hit.rgba, raster);
                super::images::PlannedLayer::Ready(hit)
            }
            None => super::images::PlannedLayer::Pending(super::images::PendingLayer {
                x: {
                    // RUR_DEBUG_LAYER_MISSES=1: every layer-cache miss on stderr (frame, size, key)
                    static LOG: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
                    if *LOG.get_or_init(|| std::env::var("RUR_DEBUG_LAYER_MISSES").as_deref() == Ok("1")) {
                        eprintln!("layer miss: frame {} {}x{} {}", self.frame, width, height, key.chars().take(400).collect::<String>());
                    }
                    x
                },
                y,
                width,
                height,
                premultiplied: true,
                key,
                source: NO_SOURCE.with(|s| s.clone()),
                job: raster,
                probation: probation || self.redrawn_layer(),
            }),
        };
        self.upload_image_layer(planned)
    }
    /// upload() for a layer whose bytes live in a shared allocation: the texture made from that
    /// allocation before is reused while it is resident (marked used this frame), else upload().
    pub(crate) fn upload_shared(&mut self, width: u32, height: u32, rgba: &std::sync::Arc<[u8]>) -> TextureId {
        let address = rgba.as_ptr() as usize;
        if let Some((kept, id)) = self.layer_textures.get(&address) {
            if std::sync::Arc::ptr_eq(kept, rgba) {
                let id = *id;
                let frame = self.frame;
                let resident = self.texture_keys.get(&id).and_then(|key| self.textures.get_mut(key)).and_then(|list| list.iter_mut().find(|v| v.id == id));
                if let Some(texture) = resident {
                    texture.last = frame;
                    self.stats.upload_cache_hits += 1;
                    return id;
                }
            }
        }
        let id = self.upload_arc(width, height, rgba.clone(), TextureFormat::Rgba8);
        self.layer_textures.insert(address, (rgba.clone(), id));
        id
    }
    /// Rasterizes the queued layers in parallel and registers them like upload() does.
    fn run_pending(&mut self, open: &[DrawBatch]) {
        self.pending_keys.clear();
        self.pending_text.clear();
        if self.pending.is_empty() {
            return;
        }
        let started = Instant::now();
        // Layers only dropped records (emit) asked for are never drawn: not rasterized
        let mut used: rustc_hash::FxHashSet<TextureId> = Default::default();
        for batch in open.iter().chain(self.packet.passes.iter().flat_map(|p| p.batches.iter())) {
            used.extend(
                batch
                    .image_slots
                    .iter()
                    .copied()
                    .chain([batch.glyphs, batch.layer, batch.lut])
                    .flatten(),
            );
        }
        for copy in self.packet.passes.iter().flat_map(|p| p.copies_after.iter()) {
            used.extend([copy.source, copy.destination]);
        }
        let before = self.pending.len();
        self.pending.retain(|p| used.contains(&p.id));
        for p in &self.pending {
            let kind = match (&p.text, p.image.as_ref().map(|image| image.0.split(':').next().unwrap_or(""))) {
                (Some(_), _) => 0,
                (_, Some("stroke") | Some("stroke-local")) => 1,
                (_, Some("fill") | Some("fill-local")) => 2,
                (_, Some("fill-straight") | Some("fill-straight-local")) => 3,
                (_, Some(k)) if k.contains("shadow") => 5,
                (_, Some(_)) => 4,
                (_, None) => 5,
            };
            self.stats.raster_counts[kind] += 1;
            self.stats.raster_pixels[kind] += u64::from(p.width) * u64::from(p.height);
        }
        self.stats.skipped_rasters += (before - self.pending.len()) as u64;
        if self.pending.is_empty() {
            self.stats.deferred_raster_ms += started.elapsed().as_secs_f64() * 1000.;
            return;
        }
        let _raster_scope = crate::microprofile::scope_detail("Rasterize layers (parallel)", self.pending.len() as u64);
        use rayon::prelude::*;
        let pixels: u64 = self.pending.iter().map(|p| u64::from(p.width) * u64::from(p.height)).sum();
        let serial = self.pending.len() == 1 || pixels < 32 * 1024;
        let raster = |p: PendingRaster| {
                let raster_started = Instant::now();
                let kind = match p.image.as_ref().map(|image| image.0.split(':').next().unwrap_or("")) {
                    Some("texture") => "Raster group image",
                    Some("shadow") => "Raster shadow",
                    Some("stroke") | Some("stroke-local") => "Raster stroke",
                    Some("fill") | Some("fill-local") | Some("fill-straight") | Some("fill-straight-local") => "Raster fill",
                    Some("gradborder-straight") | Some("gradborder-local") => "Raster gradient border",
                    _ => "Raster layer",
                };
                let kind = if p.text.is_some() { "Raster text" } else { kind };
                let _job_scope = crate::microprofile::scope_detail(kind, u64::from(p.width) * u64::from(p.height));
                let bytes = (p.job)();
                let elapsed = raster_started.elapsed();
                let hash = content_hash(p.width, p.height, &bytes);
                (p.id, p.width, p.height, p.format, p.image, p.text, bytes, hash, elapsed)
        };
        let pending = std::mem::take(&mut self.pending);
        let done: Vec<_> = if serial {
            pending.into_iter().map(raster).collect()
        } else {
            pending.into_par_iter().map(raster).collect()
        };
        for (id, width, height, format, image, text, bytes, key, elapsed) in done {
            self.stats.pixel_bytes_processed += bytes.len() as u64;
            // One allocation for the caches that keep these pixels and the upload
            let bytes: std::sync::Arc<[u8]> = bytes.into();
            if let Some((store, pixels)) = text {
                self.text_cache.store(store, &pixels, bytes.clone());
                self.layer_textures.insert(bytes.as_ptr() as usize, (bytes.clone(), id));
            }
            if let Some((cache_key, x, y, premultiplied, source, probation)) = image {
                let layer = super::images::ImageLayer {
                    x,
                    y,
                    width,
                    height,
                    rgba: bytes.clone(),
                    premultiplied,
                };
                let at = Instant::now().checked_sub(elapsed).unwrap_or_else(Instant::now);
                let stored = self.image_cache.store_with(cache_key, layer, source, at, probation);
                self.layer_textures.insert(stored.rgba.as_ptr() as usize, (stored.rgba, id));
            }
            self.texture_keys.insert(id, key);
            self.packet.textures.push(TextureUpload {
                id,
                revision: 1,
                format,
                levels: vec![TextureLevel {
                    width,
                    height,
                    bytes: bytes.clone().into(),
                }],
            });
            self.texture_bytes += bytes.len();
            self.textures.entry(key).or_default().push(CachedTexture {
                id,
                bytes: bytes.len(),
                last: self.frame,
                width,
                height,
                format,
                data: bytes,
            });
        }
        self.stats.deferred_raster_ms += started.elapsed().as_secs_f64() * 1000.;
    }
    /// Keeps the stroke coverage and shadow alpha this frame's rasters computed (stroke_coverage,
    /// shadow_alpha), within their budgets
    fn store_coverage(&mut self) {
        let frame = self.frame;
        for (key, slot) in std::mem::take(&mut self.pending_shadow_alpha) {
            let Some(alpha) = slot.get() else {
                continue;
            };
            if self.shadow_alpha.contains_key(&key) || alpha.len() > SHADOW_ALPHA_BUDGET / 4 {
                continue;
            }
            self.shadow_alpha_bytes += alpha.len();
            self.shadow_alpha.insert(key, (alpha.clone(), frame));
        }
        if self.shadow_alpha_bytes > SHADOW_ALPHA_BUDGET {
            let mut order: Vec<(u64, String)> =
                self.shadow_alpha.iter().map(|(key, (_, used))| (*used, key.clone())).collect();
            order.sort_unstable();
            for (_, key) in order {
                if self.shadow_alpha_bytes <= SHADOW_ALPHA_BUDGET * 3 / 4 {
                    break;
                }
                if let Some((alpha, _)) = self.shadow_alpha.remove(&key) {
                    self.shadow_alpha_bytes -= alpha.len();
                }
            }
        }
        let frame = self.frame;
        for (key, slot) in std::mem::take(&mut self.pending_coverage) {
            let Some(coverage) = slot.get() else {
                continue;
            };
            if self.stroke_coverage.contains_key(&key) || coverage.bytes() > STROKE_COVERAGE_BUDGET / 4 {
                continue;
            }
            self.stroke_coverage_bytes += coverage.bytes();
            self.stroke_coverage.insert(key, (coverage.clone(), frame));
        }
        if self.stroke_coverage_bytes > STROKE_COVERAGE_BUDGET {
            let mut order: Vec<(u64, String)> =
                self.stroke_coverage.iter().map(|(key, (_, used))| (*used, key.clone())).collect();
            order.sort_unstable();
            for (_, key) in order {
                if self.stroke_coverage_bytes <= STROKE_COVERAGE_BUDGET * 3 / 4 {
                    break;
                }
                if let Some((coverage, _)) = self.stroke_coverage.remove(&key) {
                    self.stroke_coverage_bytes -= coverage.bytes();
                }
            }
        }
    }
    /// A text layer composited with the frame's other deferred rasters, stored in the text cache once
    /// done; a composite of the same quads already queued this frame shares its texture
    fn queue_text(
        &mut self,
        layout: super::text::TextLayout,
        pixels: super::text::TextPixels,
        mut store: super::text::TextStore,
    ) -> TextureId {
        use super::text::TextLayerCache;
        let (hash, key) = TextLayerCache::pending_key(&mut store, &pixels);
        if let Some(indices) = self.pending_text.get(&hash) {
            for &index in indices {
                if let Some((queued, _)) = &self.pending[index].text {
                    if TextLayerCache::queued_key(queued).is_some_and(|(_, queued)| queued == key) {
                        return self.pending[index].id;
                    }
                }
            }
        }
        let id = self.texture_id();
        self.pending_text.entry(hash).or_default().push(self.pending.len());
        self.pending.push(PendingRaster {
            id,
            width: layout.width,
            height: layout.height,
            format: TextureFormat::Rgba8,
            job: pixels.job(),
            image: None,
            text: Some((store, pixels)),
        });
        id
    }
    pub(crate) fn texture_id(&mut self) -> TextureId {
        let id = TextureId(self.next_id);
        self.next_id += 1;
        id
    }
    pub(crate) fn upload(
        &mut self,
        width: u32,
        height: u32,
        bytes: Vec<u8>,
        format: TextureFormat,
    ) -> TextureId {
        match self.upload_hit(width, height, &bytes, format) {
            Ok(id) => id,
            Err(key) => self.upload_miss(width, height, bytes.into(), format, key),
        }
    }
    /// upload() of bytes already in a shared allocation (kept, not copied)
    pub(crate) fn upload_arc(
        &mut self,
        width: u32,
        height: u32,
        bytes: std::sync::Arc<[u8]>,
        format: TextureFormat,
    ) -> TextureId {
        match self.upload_hit(width, height, &bytes, format) {
            Ok(id) => id,
            Err(key) => self.upload_miss(width, height, bytes, format, key),
        }
    }
    /// The resident texture with exactly these bytes, marked used this frame; else their content key
    fn upload_hit(&mut self, width: u32, height: u32, bytes: &[u8], format: TextureFormat) -> Result<TextureId, u64> {
        let upload_started = Instant::now();
        let _upload_scope = crate::microprofile::scope_detail("Upload (hash)", bytes.len() as u64);
        self.stats.pixel_bytes_processed += bytes.len() as u64;
        let key = content_hash(width, height, bytes);
        if let Some(item) = self.textures.get_mut(&key).and_then(|items| {
            items.iter_mut().find(|v| {
                v.width == width && v.height == height && v.format == format && *v.data == *bytes
            })
        }) {
            item.last = self.frame;
            self.stats.upload_cache_hits += 1;
            self.stats.upload_ms += upload_started.elapsed().as_secs_f64() * 1000.;
            return Ok(item.id);
        }
        self.stats.upload_ms += upload_started.elapsed().as_secs_f64() * 1000.;
        Err(key)
    }
    fn upload_miss(&mut self, width: u32, height: u32, bytes: std::sync::Arc<[u8]>, format: TextureFormat, key: u64) -> TextureId {
        let upload_started = Instant::now();
        let id = self.texture_id();
        self.texture_keys.insert(id, key);
        self.packet.textures.push(TextureUpload {
            id,
            revision: 1,
            format,
            levels: vec![TextureLevel {
                width,
                height,
                bytes: bytes.clone().into(),
            }],
        });
        self.texture_bytes += bytes.len();
        self.textures.entry(key).or_default().push(CachedTexture {
            id,
            bytes: bytes.len(),
            last: self.frame,
            width,
            height,
            format,
            data: bytes,
        });
        self.stats.upload_ms += upload_started.elapsed().as_secs_f64() * 1000.;
        id
    }
    pub(crate) fn begin(
        &mut self,
        _ctx: &BuildContext,
        bounds: [f64; 4],
        clip: Option<[f64; 4]>,
        kind: u32,
    ) -> usize {
        let start = self.packet.texels.len();
        self.push(bounds);
        self.push(clip.unwrap_or(NO_CLIP));
        self.push([kind as f64, 0., 0., 0.]);
        start
    }
    pub(crate) fn push(&mut self, v: [f64; 4]) {
        self.packet.texels.push(v.map(|n| n as f32));
    }
    /// Whether a record with these bounds and clip would be dropped by emit (it draws nothing)
    pub(crate) fn culled(ctx: &BuildContext, bounds: [f64; 4], clip: Option<[f64; 4]>) -> bool {
        let b = bounds.map(|v| v as f32);
        let c = clip.unwrap_or(NO_CLIP).map(|v| v as f32);
        let (w, h) = (ctx.width as f32, ctx.height as f32);
        b[2] <= b[0]
            || b[3] <= b[1]
            || b[2] <= c[0]
            || b[0] >= c[2]
            || b[3] <= c[1]
            || b[1] >= c[3]
            || b[2] <= 0.
            || b[3] <= 0.
            || b[0] >= w
            || b[1] >= h
    }
    pub(crate) fn emit(&mut self, ctx: &mut BuildContext, start: usize, mut bindings: DrawBatch) {
        // A record none of whose pixels can pass its clip or lie on its target draws nothing (the
        // shader draws pixel centres inside the quad [x0, x1) x [y0, y1) that pass the clip test
        // clip.xy <= centre < clip.zw, both in these f32 values): dropped, with its texels
        let b = self.packet.texels[start];
        let c = self.packet.texels[start + 1];
        let (w, h) = (ctx.width as f32, ctx.height as f32);
        if self.cull_records
            && (b[2] <= b[0]
            || b[3] <= b[1]
            || b[2] <= c[0]
            || b[0] >= c[2]
            || b[3] <= c[1]
            || b[1] >= c[3]
            || b[2] <= 0.
            || b[3] <= 0.
            || b[0] >= w
            || b[1] >= h)
        {
            self.packet.texels.truncate(start);
            self.stats.culled_records += 1;
            return;
        }
        if let Some(last) = ctx.batches.last_mut() {
            if last.image_slots == bindings.image_slots
                && last.glyphs == bindings.glyphs
                && last.layer == bindings.layer
                && last.lut == bindings.lut
                && last.blend == bindings.blend
                && last.scissor == bindings.scissor
            {
                last.record_offsets.push(start as u32);
                return;
            }
        }
        bindings.record_offsets.push(start as u32);
        ctx.batches.push(bindings);
    }
    pub(crate) fn finish_pass(&mut self, ctx: BuildContext) {
        self.packet.passes.push(RenderPass {
            target: ctx.target,
            width: ctx.width,
            height: ctx.height,
            clear: ctx.clear,
            batches: ctx.batches,
            copies_after: vec![],
        });
    }
    pub(crate) fn draw_entries(
        &mut self,
        ctx: &mut BuildContext,
        entries: &[Value],
        scene: &SceneStore,
        assets: &AssetStore,
        skip_group: Option<u64>,
    ) -> Result<(), DrawError> {
        for entry in entries {
            if entry.is_object() {
                self.draw_group(ctx, entry, scene, assets)?;
                continue;
            }
            let id = entry.as_i64().unwrap_or(0);
            // A node the same as when last frame drew it here: its records again, without reading it
            if id > 0 && self.walk_retained && !self.changed_nodes.contains(&(id as u64)) {
                let context = super::node_records::RecordContext {
                    origin: ctx.origin,
                    target_size: [ctx.width, ctx.height],
                    scale: self.scale,
                    in_group: skip_group == Some(id as u64),
                    opaque: false,
                    part: super::node_records::NodePart::All,
                    space: None,
                    shadows: 0,
                };
                if let Some(index) = self.drawn_id_hit(id as u64, &context) {
                    let before = &self.drawn_before[index];
                    before.records.append_to(&mut self.packet.texels, &mut ctx.batches);
                    let again = DrawnNode {
                        id: before.id,
                        node: before.node,
                        context,
                        records: before.records.clone(),
                        asset_dependent: before.asset_dependent,
                    };
                    self.drawn.push(again);
                    self.stats.node_cache_hits += 1;
                    self.stats.display_list_hits += 1;
                    self.stats.retained_walk_hits += 1;
                    continue;
                }
            }
            let Some(owner) = scene.node_arc(id.unsigned_abs()) else {
                continue;
            };
            let node = owner.as_ref();
            if node["culled"] == true {
                continue;
            }
            if id < 0 {
                self.draw_post(ctx, node, assets)?
            } else {
                let owned_started = Instant::now();
                self.draw_owned(ctx, id as u64, owner, scene, assets, skip_group == Some(id as u64), "all")?;
                self.stats.draw_node_ms += owned_started.elapsed().as_secs_f64() * 1000.;
            }
        }
        Ok(())
    }
    pub(crate) fn draw_node(
        &mut self,
        ctx: &mut BuildContext,
        node: &Value,
        scene: &SceneStore,
        assets: &AssetStore,
        in_group: bool,
    ) -> Result<(), DrawError> {
        self.draw_node_parts(ctx, node, scene, assets, in_group, "all")
    }
    pub(crate) fn draw_node_parts(
        &mut self,
        ctx: &mut BuildContext,
        node: &Value,
        scene: &SceneStore,
        assets: &AssetStore,
        in_group: bool,
        parts: &str,
    ) -> Result<(), DrawError> {
        let id = node["id"].as_u64().unwrap_or(0);
        let owner = scene.node_arc(id).filter(|arc| std::ptr::eq(arc.as_ref(), node));
        let Some(owner) = owner else {
            return self.draw_node_uncached(ctx, node, scene, assets, in_group, parts);
        };
        self.draw_owned(ctx, id, owner, scene, assets, in_group, parts)
    }
    /// draw_node_parts for a node the scene holds (`owner`), through the node record cache
    fn draw_owned(
        &mut self,
        ctx: &mut BuildContext,
        id: u64,
        owner: &std::sync::Arc<Value>,
        scene: &SceneStore,
        assets: &AssetStore,
        in_group: bool,
        parts: &str,
    ) -> Result<(), DrawError> {
        use super::node_records::{NodePart, RecordContext, RecordedNode};
        let node = owner.as_ref();
        let Some(part) = NodePart::from_str(parts) else {
            return self.draw_node_uncached(ctx, node, scene, assets, in_group, parts);
        };
        let context = RecordContext {
            origin: ctx.origin,
            target_size: [ctx.width, ctx.height],
            scale: self.scale,
            in_group,
            opaque: false,
            part,
            space: None,
            shadows: if ctx.target == FRAME_TARGET && !in_group { self.exact_shadows_hash(node) } else { 0 },
        };
        let cache_started = Instant::now();
        let _cache_scope = crate::microprofile::scope("Draw node");
        // A node wholly outside its clip (a scrolled-out row) draws nothing: its records are empty, found
        // without a cache lookup or insert (a scroll moves hundreds of such rows every frame)
        if parts == "all"
            && !in_group
            && ctx.target == FRAME_TARGET
            && ctx.origin == [0., 0.]
            && node_offscreen(node, self.scale, ctx.width, ctx.height)
        {
            static EMPTY: std::sync::OnceLock<std::sync::Arc<RecordedNode>> = std::sync::OnceLock::new();
            let records = EMPTY
                .get_or_init(|| std::sync::Arc::new(RecordedNode::capture(&[], &[], 0).expect("empty records")))
                .clone();
            self.stats.offscreen_nodes += 1;
            self.drawn.push(DrawnNode {
                id,
                node: std::sync::Arc::as_ptr(owner) as usize,
                context,
                records,
                asset_dependent: false,
            });
            self.stats.node_cache_ms += cache_started.elapsed().as_secs_f64() * 1000.;
            return Ok(());
        }
        if let Some(records) = self.drawn_list_hit(owner, &context) {
            records.append_to(&mut self.packet.texels, &mut ctx.batches);
            self.drawn.push(DrawnNode {
                id,
                node: std::sync::Arc::as_ptr(owner) as usize,
                context,
                records,
                asset_dependent: super::node_records::asset_dependent(node, part),
            });
            self.stats.node_cache_hits += 1;
            self.stats.display_list_hits += 1;
            self.stats.node_cache_ms += cache_started.elapsed().as_secs_f64() * 1000.;
            return Ok(());
        }
        let keys = &self.texture_keys;
        let textures = &mut self.textures;
        let gpu_images = &mut self.gpu_image_cache;
        let frame = self.frame;
        if let Some(records) = self.node_cache.get(owner, &context, |id| {
            let Some(key) = keys.get(&id) else {
                return gpu_images.touch(id, frame);
            };
            let Some(item) = textures
                .get_mut(key)
                .and_then(|v| v.iter_mut().find(|v| v.id == id))
            else {
                return false;
            };
            item.last = frame;
            true
        }) {
            records.append_to(&mut self.packet.texels, &mut ctx.batches);
            self.drawn.push(DrawnNode {
                id,
                node: std::sync::Arc::as_ptr(owner) as usize,
                context,
                records,
                asset_dependent: super::node_records::asset_dependent(node, part),
            });
            self.stats.node_cache_hits += 1;
            self.stats.node_cache_ms += cache_started.elapsed().as_secs_f64() * 1000.;
            return Ok(());
        }
        self.stats.node_cache_misses += 1;
        self.stats.node_cache_ms += cache_started.elapsed().as_secs_f64() * 1000.;
        let base = self.packet.texels.len();
        let mut scratch = BuildContext::new(ctx.target, ctx.width, ctx.height, ctx.origin);
        let uncached_started = Instant::now();
        // A node wholly outside its clip (a scrolled-out row) draws nothing: its records are empty
        if parts == "all"
            && !in_group
            && ctx.target == FRAME_TARGET
            && ctx.origin == [0., 0.]
            && node_offscreen(node, self.scale, ctx.width, ctx.height)
        {
            self.stats.offscreen_nodes += 1;
        } else {
            self.draw_node_uncached(&mut scratch, node, scene, assets, in_group, parts)?;
        }
        self.stats.prepared_node_ms += uncached_started.elapsed().as_secs_f64() * 1000.;
        let capture_started = Instant::now();
        let records =
            RecordedNode::capture(&self.packet.texels[base..], &scratch.batches, base as u32)
                .map_err(anyhow::Error::msg)?;
        self.packet.texels.truncate(base);
        let records = std::sync::Arc::new(records);
        records.append_to(&mut self.packet.texels, &mut ctx.batches);
        self.stats.record_capture_ms += capture_started.elapsed().as_secs_f64() * 1000.;
        let insert_started = Instant::now();
        self.drawn.push(DrawnNode {
            id,
            node: std::sync::Arc::as_ptr(owner) as usize,
            context,
            records: records.clone(),
            asset_dependent: super::node_records::asset_dependent(node, part),
        });
        self.node_cache.insert(owner, &context, records);
        self.stats.record_insert_ms += insert_started.elapsed().as_secs_f64() * 1000.;
        Ok(())
    }
    /// drawn_list_hit by node id, for a node whose identity is unchanged since last frame's scene
    fn drawn_id_hit(&mut self, id: u64, context: &super::node_records::RecordContext) -> Option<usize> {
        const WINDOW: usize = 32;
        let end = (self.drawn_cursor + WINDOW).min(self.drawn_before.len());
        let index = (self.drawn_cursor..end).find(|&i| {
            let entry = &self.drawn_before[i];
            entry.id == id && entry.context == *context
        })?;
        let entry = &self.drawn_before[index];
        // Last frame's records use textures its packet kept (eviction spares them); only a texture
        // retired since can be missing
        let resident = self.textures_stable
            || entry
                .records
                .texture_ids
                .iter()
                .all(|id| self.texture_keys.contains_key(id) || self.gpu_image_cache.contains(*id));
        if !resident {
            return None;
        }
        self.drawn_cursor = index + 1;
        Some(index)
    }
    /// Last frame's records for `owner` drawn with `context`, found at (or a little after) the display
    /// list position this frame has reached; their textures must still be resident.
    fn drawn_list_hit(
        &mut self,
        owner: &std::sync::Arc<Value>,
        context: &super::node_records::RecordContext,
    ) -> Option<std::sync::Arc<super::node_records::RecordedNode>> {
        const WINDOW: usize = 32;
        let pointer = std::sync::Arc::as_ptr(owner);
        let end = (self.drawn_cursor + WINDOW).min(self.drawn_before.len());
        let index = (self.drawn_cursor..end).find(|&i| {
            let entry = &self.drawn_before[i];
            entry.node == pointer as usize && entry.context == *context
        })?;
        let entry = &self.drawn_before[index];
        let resident = entry
            .records
            .texture_ids
            .iter()
            .all(|id| self.texture_keys.contains_key(id) || self.gpu_image_cache.contains(*id));
        if !resident {
            return None;
        }
        self.drawn_cursor = index + 1;
        Some(entry.records.clone())
    }
    fn draw_node_uncached(
        &mut self,
        ctx: &mut BuildContext,
        node: &Value,
        scene: &SceneStore,
        assets: &AssetStore,
        in_group: bool,
        parts: &str,
    ) -> Result<(), DrawError> {
        let previous = std::mem::replace(&mut self.layer_node, node["id"].as_u64().unwrap_or(0));
        self.group_cache.fringe = node["fringe"] == true;
        let result = self.draw_node_body(ctx, node, scene, assets, in_group, parts);
        self.group_cache.fringe = false;
        self.layer_node = previous;
        result
    }
    /// Whether a layer missing the image cache now belongs to a node redrawn since its layers first missed
    /// (in an earlier frame): an animated node's layers (a gradient resized every frame, a stroke shaken to
    /// random places) are mostly one-offs, cached on probation so they do not push the reused layers out.
    /// A redrawn node's layer that does come back (a shake revisiting a place) is kept the next time it
    /// misses (ImageCache's ghosts).
    fn redrawn_layer(&mut self) -> bool {
        if self.layer_node == 0 {
            return false;
        }
        let frame = self.frame;
        if frame % 1024 == 0 {
            self.layer_misses.retain(|_, (last, _)| *last + 1024 >= frame);
        }
        let entry = self.layer_misses.entry(self.layer_node).or_insert((frame, false));
        if entry.0 != frame {
            *entry = (frame, true);
        }
        entry.1
    }
    fn draw_node_body(
        &mut self,
        ctx: &mut BuildContext,
        node: &Value,
        scene: &SceneStore,
        assets: &AssetStore,
        in_group: bool,
        parts: &str,
    ) -> Result<(), DrawError> {
        if node["culled"] == true {
            return Ok(());
        }
        self.stats.nodes_prepared += 1;
        if node["path2d"].is_object() {
            // (a Path2D's node holds its stroke only)
            if parts == "all" || parts == "over" {
                self.path2d(ctx, node);
            }
            return Ok(());
        }
        let physical = physical_rect(node, self.scale);
        let radii = if in_group {
            [0.; 4]
        } else {
            radius(&node["corner"], self.scale)
        };
        // A pure-translation matrix (a Rotation cancelled by an ancestor's) draws its fill and its image
        // like an axis-aligned node's, on snapped edges (images::translated_image_node): an edge half way
        // between two pixels rounds up, where a turned quad leaves its right and bottom tie out
        // (fixtures/image-probe-translated.luau, the 90 and 180 degree cells)
        let moved = if physical.snapped {
            None
        } else {
            super::images::translated_image_node(node)
        };
        if parts == "all" || parts == "under" {
            if !in_group {
                self.group_shadows(ctx, node, scene, assets)?;
            }
            match &moved {
                Some(moved) => {
                    self.background(ctx, moved, physical_rect(moved, self.scale), radii, in_group)?
                }
                None => self.background(ctx, node, physical, radii, in_group)?,
            }
        }
        if parts == "all" || parts == "image" {
            let image_started = Instant::now();
            let _image_scope = crate::microprofile::scope("Image");
            if node["image"].is_object() {
                self.stats.images += 1;
            }
            let (node, physical) = match &moved {
                Some(moved) => (moved, physical_rect(moved, self.scale)),
                None => (node, physical),
            };
            let r = physical.rect;
            if let Some(uri) = node["image"]["uri"].as_str() {
                if let Some(asset) = assets.image(uri) {
                    if !physical.snapped {
                        self.rotated_image_record(ctx, node, asset, physical, radii);
                    } else if !node["gradient"].is_object() {
                        self.snapped_image_records(ctx, node, asset, physical, radii)?;
                    } else {
                        let ir = super::images::ImageRect {
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
                        if let Some(layer) = super::images::prepare_image(
                            node,
                            asset,
                            ir,
                            radii,
                            node_transform(node, ctx.origin, self.scale),
                            self.scale,
                            &mut self.image_cache,
                        ) {
                            let id = self.upload(
                                layer.width,
                                layer.height,
                                layer.rgba.to_vec(),
                                TextureFormat::Rgba8,
                            );
                            let (tx, ty) = if physical.snapped {
                                (-ctx.origin[0], -ctx.origin[1])
                            } else {
                                (0., 0.)
                            };
                            self.image_layer(
                                ctx,
                                Rect::new(
                                    layer.x as f64 + tx,
                                    layer.y as f64 + ty,
                                    layer.width as f64,
                                    layer.height as f64,
                                ),
                                id,
                                clip_rect(node, ctx.origin, self.scale),
                            );
                        }
                    }
                }
            }
            self.stats.images_ms += image_started.elapsed().as_secs_f64() * 1000.;
        }
        if parts == "all" || parts == "over" {
            if node["text"].is_object() {
                let text_started = Instant::now();
                let _text_scope = crate::microprofile::scope("Text");
                self.stats.texts += 1;
                self.text_shadows(ctx, node, scene, assets)?;
                let (hits, node_hits, misses) =
                    (self.text_cache.hits, self.text_cache.node_hits, self.text_cache.misses);
                use super::text::TextPlan;
                let plan = match self.text_cache.plan(node, scene.fonts(), assets, self.scale) {
                    Ok(plan) => plan,
                    Err(_) if assets.pending_count() > 0 => TextPlan::Layers(Vec::new()),
                    Err(e) => return Err(e.into()),
                };
                self.stats.text_cache_hits += self.text_cache.hits - hits;
                self.stats.text_node_hits += self.text_cache.node_hits - node_hits;
                self.stats.text_cache_misses += self.text_cache.misses - misses;
                let layers: Vec<(super::text::TextQuad, TextureId)> = match plan {
                    TextPlan::Layers(quads) => quads
                        .into_iter()
                        .map(|q| {
                            // A cached layer is one shared allocation: its texture is found without hashing it
                            let id = self.upload_shared(q.width, q.height, &q.rgba);
                            (q, id)
                        })
                        .collect(),
                    TextPlan::Composite { layout, pixels, store } => {
                        // Drawn as layer() below (an unrotated text layer): not composited when that record
                        // would be dropped (the cache keeps its quads for when it is drawn)
                        let m = node_transform(node, ctx.origin, self.scale);
                        let at = Rect::new(layout.x, layout.y, layout.w, layout.h).translated(m[4], m[5]);
                        if m[..4] == [1., 0., 0., 1.]
                            && layout.w == layout.width as f64
                            && layout.h == layout.height as f64
                            && Self::culled(ctx, at.bounds(), text_clip_rect(node, ctx.origin, self.scale))
                        {
                            self.text_cache.defer(store, pixels);
                            Vec::new()
                        } else {
                            let id = self.queue_text(layout, pixels, store);
                            vec![(super::text::layer_quad(layout, NO_SOURCE.with(|s| s.clone())), id)]
                        }
                    }
                };
                for (q, id) in layers {
                    let mut m = node_transform(node, ctx.origin, self.scale);
                    if q.translation_only {
                        m = [1., 0., 0., 1., m[4], m[5]];
                    }
                    let clip = text_clip_rect(node, ctx.origin, self.scale);
                    let qr = Rect::new(q.x, q.y, q.w, q.h);
                    let selection: Vec<_> = q
                        .selection_rects
                        .iter()
                        .map(|r| [r[0] + m[4], r[1] + m[5], r[2] + m[4], r[3] + m[5]])
                        .collect();
                    if m == [1., 0., 0., 1., m[4], m[5]]
                        && q.opacity == 1.
                        && q.w == q.width as f64
                        && q.h == q.height as f64
                    {
                        let selection: Vec<_> = q
                            .selection_rects
                            .iter()
                            .map(|r| [r[0] + m[4], r[1] + m[5], r[2] + m[4], r[3] + m[5]])
                            .collect();
                        self.layer(
                            ctx,
                            qr.translated(m[4], m[5]),
                            id,
                            clip,
                            q.selection_mode,
                            &selection,
                        );
                    } else {
                        self.text_quad(
                            ctx,
                            qr,
                            q.width,
                            q.height,
                            id,
                            m,
                            clip,
                            q.opacity,
                            q.selection_mode,
                            &selection,
                        );
                    }
                }
                self.stats.text_ms += text_started.elapsed().as_secs_f64() * 1000.;
            }
            if !in_group {
                self.group_inset_shadows(ctx, node, scene, assets)?;
                self.group_strokes(ctx, node, scene, assets)?;
            }
        }
        Ok(())
    }
    /// A Path2D's stroke (path2d.rs): its coverage in its colour, as one layer over the box it covers. The layer
    /// is kept by what it shows relative to its own corner in whole pixels, so a path that moves by whole pixels
    /// (a scroll) reuses it.
    fn path2d(&mut self, ctx: &mut BuildContext, node: &Value) {
        let Some(stroke) = super::path2d::Stroke::of(node) else { return };
        let scale = self.scale;
        let mesh = stroke.mesh(scale);
        let Some([x0, y0, x1, y1]) = super::path2d::Stroke::extent(&mesh) else { return };
        // (a box no texture holds: a path far larger than any screen)
        let (width, height) = ((x1 - x0) as u64, (y1 - y0) as u64);
        if width > 16384 || height > 16384 {
            return;
        }
        let (width, height) = (width as u32, height as u32);
        let at = Rect::new(x0 as f64 - ctx.origin[0], y0 as f64 - ctx.origin[1], width as f64, height as f64);
        let clip = clip_rect(node, ctx.origin, scale);
        if Self::culled(ctx, at.bounds(), clip) {
            return;
        }
        let key = format!(
            "path2d:{:032x}:{}",
            super::path2d::Stroke::mesh_hash(&mesh, x0, y0),
            serde_json::to_string(&(stroke.thickness, stroke.color, [width, height])).unwrap_or_default()
        );
        let raster = move || {
            let alpha = stroke.coverage(&mesh, x0, y0, width as usize, height as usize);
            stroke.shade(&alpha)
        };
        let probation = u64::from(width) * u64::from(height) <= 65536;
        let (.., id) = self.cached_layer_with(key, at.x as i32, at.y as i32, width, height, Box::new(raster), probation);
        self.layer(ctx, at, id, clip, 0, &[]);
    }
    pub(crate) fn layer(
        &mut self,
        ctx: &mut BuildContext,
        r: Rect,
        id: TextureId,
        clip: Option<[f64; 4]>,
        mode: u8,
        selection: &[[f64; 4]],
    ) {
        self.layer_blend(ctx, r, id, clip, mode, selection, BlendMode::Premultiplied);
    }
    /// An image layer (prepare_image: straight bytes) blended like Studio, round((c A + D (255 - A)) / 255),
    /// one rounding (BlendMode::Image)
    pub(crate) fn image_layer(&mut self, ctx: &mut BuildContext, r: Rect, id: TextureId, clip: Option<[f64; 4]>) {
        self.layer_blend(ctx, r, id, clip, 0, &[], BlendMode::Image);
    }
    #[allow(clippy::too_many_arguments)]
    fn layer_blend(
        &mut self,
        ctx: &mut BuildContext,
        r: Rect,
        id: TextureId,
        clip: Option<[f64; 4]>,
        mode: u8,
        selection: &[[f64; 4]],
        blend: BlendMode,
    ) {
        if r.w <= 0. || r.h <= 0. {
            return;
        }
        let start = self.begin(ctx, r.bounds(), clip, 18);
        self.push([0.; 4]);
        self.selection(mode, selection);
        let mut b = DrawBatch {
            blend,
            ..DrawBatch::default()
        };
        b.image_slots[0] = Some(id);
        self.emit(ctx, start, b);
    }
    fn selection(&mut self, mode: u8, rects: &[[f64; 4]]) {
        self.push([mode as f64, rects.len() as f64, 0., 0.]);
        for r in rects {
            self.push(*r)
        }
    }
    fn text_quad(
        &mut self,
        ctx: &mut BuildContext,
        r: Rect,
        w: u32,
        h: u32,
        id: TextureId,
        m: Transform,
        clip: Option<[f64; 4]>,
        opacity: f64,
        mode: u8,
        selection: &[[f64; 4]],
    ) {
        let start = self.begin(ctx, transformed_bounds(m, r, 1.), clip, 15);
        self.push([4., 0., 0., 0.]);
        self.push([0., opacity * 255., 0., 0.]);
        self.selection(mode, selection);
        self.packet.texels[start + 2][2] = (self.packet.texels.len() - start) as f32;
        let inv = invert(m);
        let sx = w as f64 / r.w;
        let sy = h as f64 / r.h;
        let [x, y] = apply(m, r.x, r.y).map(f64::floor);
        self.push([inv[0] * sx, inv[1] * sy, inv[2] * sx, inv[3] * sy]);
        self.push([
            (inv[0] * x + inv[2] * y + inv[4] - r.x) * sx,
            (inv[1] * x + inv[3] * y + inv[5] - r.y) * sy,
            x,
            y,
        ]);
        self.push([w as f64, h as f64, 0., 0.]);
        let mut b = DrawBatch::default();
        b.image_slots[0] = Some(id);
        self.emit(ctx, start, b);
    }
    fn solid(&mut self, paint: [f32; 4]) {
        self.packet.texels.push([0., paint[0], paint[1], paint[2]]);
        self.packet.texels.push([paint[3], 0., 0., 0.]);
    }
    fn rect_paint(
        &mut self,
        ctx: &mut BuildContext,
        r: Rect,
        clip: Option<[f64; 4]>,
        paint: Paint,
        hole: Option<Rect>,
    ) {
        if r.w <= 0. || r.h <= 0. {
            return;
        }
        let start = self.begin(ctx, r.bounds(), clip, 1);
        self.packet.texels[start + 2][1] = if hole.is_some() { 1. } else { 0. };
        self.push(hole.map_or([0.; 4], Rect::bounds));
        paint.append(&mut self.packet.texels, start);
        self.emit(ctx, start, DrawBatch::default());
    }
    pub(crate) fn rect(
        &mut self,
        ctx: &mut BuildContext,
        mut r: Rect,
        clip: Option<[f64; 4]>,
        paint: [f32; 4],
        hole: Option<Rect>,
    ) {
        if r.w < 0. {
            r.x += r.w;
            r.w = -r.w
        }
        if r.h < 0. {
            r.y += r.h;
            r.h = -r.h
        }
        if r.w <= 0. || r.h <= 0. {
            return;
        }
        let start = self.begin(ctx, r.bounds(), clip, 1);
        self.packet.texels[start + 2][1] = if hole.is_some() { 1. } else { 0. };
        self.push(hole.map_or([0.; 4], Rect::bounds));
        self.solid(paint);
        self.emit(ctx, start, DrawBatch::default());
    }
    /// A turned frame under a UIGradient (Studio 0.747, fixtures/gradient-probe-turned.luau): the pixels whose
    /// centres lie in the turned rect, without a fringe, each shaded where it lies in the frame's own axes like
    /// an unturned one's: the gradient turns with its frame and keeps its steps, its table and the screen's
    /// dither. `parts` are local rects with the colour the gradient multiplies, a later one over an earlier
    /// (the border's outer rect, then the fill); None leaves the rect empty.
    #[allow(clippy::too_many_arguments)]
    fn turned_gradient_fill(
        &mut self,
        ctx: &mut BuildContext,
        gradient: &Value,
        raw: [f64; 4],
        m: Transform,
        clip: Option<[f64; 4]>,
        a: u8,
        parts: &[(Rect, Option<[f64; 3]>)],
    ) {
        let Some(&(outer, _)) = parts.first() else { return };
        let bounds = transformed_bounds(m, outer, 0.);
        let (x, y) = (bounds[0], bounds[1]);
        let (w, h) = ((bounds[2] - bounds[0]) as u32, (bounds[3] - bounds[1]) as u32);
        if w == 0 || h == 0 || Self::culled(ctx, bounds, clip) {
            return;
        }
        let meshes: Vec<(Rect, Option<super::gradients::GradientMesh>)> = parts
            .iter()
            .map(|(rect, colour)| {
                let mesh = colour.map(|c| {
                    super::gradients::gradient_mesh(gradient, raw, c.map(|v| quantize(v) as f64), a as f64, rect.bounds())
                });
                (*rect, mesh)
            })
            .collect();
        let key = format!(
            "fill-turned:{}",
            serde_json::to_string(&(gradient, raw, parts.iter().map(|(r, c)| (r.bounds(), c)).collect::<Vec<_>>(), a, m, [x, y]))
                .unwrap_or_default()
        );
        let to_local = invert(m);
        // Which part a pixel centre is in: the edge functions on the parts' grid corners (geometry.rs grid_inside)
        let origin = ctx.origin;
        // A border (an outer part around a later one) is four strips laid like a pinwheel, each on its own grid
        // corners: the left one holds the top left corner, the top one the top right, the right one the bottom
        // right, the bottom one the bottom left. A pixel centre on the seam of a strip and the part inside it can
        // be in neither (fixtures/group-probe-border.luau Turn2Banded, Turn20Banded: Studio draws nothing there)
        let pinwheel = |outer: Rect, inner: Rect| -> Vec<[[i64; 2]; 4]> {
            let (x0, y0, x1, y1) = (outer.x, outer.y, outer.x + outer.w, outer.y + outer.h);
            let (ix0, iy0, ix1, iy1) = (inner.x, inner.y, inner.x + inner.w, inner.y + inner.h);
            [
                Rect::new(x0, y0, ix0 - x0, iy1 - y0),
                Rect::new(ix0, y0, x1 - ix0, iy0 - y0),
                Rect::new(ix1, iy0, x1 - ix1, y1 - iy0),
                Rect::new(x0, iy1, ix1 - x0, y1 - iy1),
            ]
            .into_iter()
            .filter(|r| r.w > 0. && r.h > 0.)
            .map(|r| grid_quad(m, r, origin))
            .collect()
        };
        let quads: Vec<Vec<[[i64; 2]; 4]>> = parts
            .iter()
            .enumerate()
            .map(|(i, (rect, _))| match parts.get(i + 1) {
                Some((inner, _)) if i == 0 => pinwheel(*rect, *inner),
                _ => vec![grid_quad(m, *rect, origin)],
            })
            .collect();
        let raster = move || {
            let mut bytes = vec![0; w as usize * h as usize * 4];
            for (k, pixel) in bytes.chunks_exact_mut(4).enumerate() {
                let (px, py) = (x + (k % w as usize) as f64, y + (k / w as usize) as f64);
                let [lx, ly] = apply(to_local, px + 0.5, py + 0.5);
                let (ax, ay) = ((px + origin[0]).round() as i64, (py + origin[1]).round() as i64);
                let Some(index) = (0..meshes.len()).rev().find(|&i| quads[i].iter().any(|q| grid_inside(q, ax, ay))) else { continue };
                let (_, mesh) = &meshes[index];
                let Some(mesh) = mesh else { continue };
                let value = mesh.at_turned([lx, ly], px as i64, py as i64);
                for c in 0..4 {
                    pixel[c] = gpu8(value[c] as f64 / 255.);
                }
            }
            bytes
        };
        let (.., id) = self.cached_layer(key, x as i32, y as i32, w, h, Box::new(raster));
        self.image_layer(ctx, Rect::new(x, y, w as f64, h as f64), id, clip);
    }
    fn rotated_fill(
        &mut self,
        ctx: &mut BuildContext,
        r: Rect,
        radii: [f64; 4],
        m: Transform,
        clip: Option<[f64; 4]>,
        paint: Paint,
        hole: f64,
    ) {
        if r.w <= 0. || r.h <= 0. {
            return;
        }
        let bounds = transformed_bounds(m, r, 2.);
        let extent = transformed_extent(m, r);
        let start = self.begin(ctx, bounds, clip, 3);
        for t in local_texels(
            invert(m),
            [bounds[0], bounds[1]],
            [r.x + r.w / 2., r.y + r.h / 2.],
        ) {
            self.push(t)
        }
        let radii = radii.map(|v| v.max(0.).min(r.w / 2.).min(r.h / 2.));
        let quarter = !self.group_cache.fringe
            && ((m[1].abs() < 1e-9 && m[2].abs() < 1e-9) || (m[0].abs() < 1e-9 && m[3].abs() < 1e-9));
        let rounded = radii.iter().any(|v| *v > 0.);
        self.push([
            r.w / 2.,
            r.h / 2.,
            hole,
            ((quarter as u8)
                | ((rounded as u8) << 1)
                | (((matches!(paint, Paint::Gradient { .. }) && !rounded) as u8) << 2)) as f64,
        ]);
        // A turned quad as the rasterizer holds it: its corners on the 1/256 px grid, clockwise on the screen from
        // the local top left, in 1/256 px (two texels). A pixel whose centre is within a few thousandths of a
        // pixel of an edge is in or out by the sign of the edge function on these, top and left edges
        // included: where the coverage has a step there (no anti-aliasing under a UIGradient; half and up
        // inside a rounded Frame's straight edge, nothing outside), Studio's pixels follow it
        // (tools/studio/edge-probe-*.luau: 690 of 691 pixel centres within 0.004 px of an edge).
        let hard = matches!(paint, Paint::Gradient { .. }) && !rounded;
        let origin = ctx.origin;
        let grid = |x: f64, y: f64| {
            let p = apply(m, x, y);
            [((p[0] + origin[0]) * 256.).round() - origin[0] * 256., ((p[1] + origin[1]) * 256.).round() - origin[1] * 256.]
        };
        let flipped = m[0] * m[3] - m[1] * m[2] < 0.;
        let quad = |x0: f64, y0: f64, x1: f64, y1: f64| -> Option<[[f64; 4]; 2]> {
            let (tl, mut tr, br, mut bl) = (grid(x0, y0), grid(x1, y0), grid(x1, y1), grid(x0, y1));
            if flipped {
                std::mem::swap(&mut tr, &mut bl);
            }
            let all = [tl, tr, br, bl];
            // The shader's integer edge function holds 20 bits a difference
            let fits = all.iter().all(|c| c.iter().all(|v| v.abs() < 4_000_000.))
                && all.iter().all(|c| (c[0] - tl[0]).abs() < 500_000. && (c[1] - tl[1]).abs() < 500_000.);
            fits.then_some([[tl[0], tl[1], tr[0], tr[1]], [br[0], br[1], bl[0], bl[1]]])
        };
        let exact = if quarter { None } else { quad(r.x, r.y, r.x + r.w, r.y + r.h) };
        // A hard border is four quads laid like a pinwheel, each on its own grid corners
        let strips = if hard && hole > 0. && exact.is_some() {
            let (x0, y0, x1, y1) = (r.x, r.y, r.x + r.w, r.y + r.h);
            [
                quad(x0, y0, x0 + hole, y1 - hole),
                quad(x0 + hole, y0, x1, y0 + hole),
                quad(x1 - hole, y0 + hole, x1, y1),
                quad(x0, y1 - hole, x1 - hole, y1),
            ]
            .into_iter()
            .collect::<Option<Vec<_>>>()
        } else {
            None
        };
        let radii_at = self.packet.texels.len();
        self.push(radii);
        match exact {
            Some(corners) => {
                // Flag 8: texels 7 and 8 hold the quad's grid corners
                self.packet.texels[radii_at - 1][3] += 8.;
                self.push(corners[0]);
                self.push(corners[1]);
            }
            None => {
                self.push(extent);
                self.push(if hole > 0. {
                    transformed_extent(
                        m,
                        Rect::new(r.x + hole, r.y + hole, r.w - 2. * hole, r.h - 2. * hole),
                    )
                } else {
                    [0.; 4]
                });
            }
        }
        paint.append(&mut self.packet.texels, start);
        if let Some(strips) = strips {
            // (a hard fill has no radii: the first holds where the strips start)
            self.packet.texels[radii_at][0] = (self.packet.texels.len() - start) as f32;
            for strip in strips {
                self.push(strip[0]);
                self.push(strip[1]);
            }
        }
        self.emit(ctx, start, DrawBatch::default());
    }
    fn background(
        &mut self,
        ctx: &mut BuildContext,
        node: &Value,
        p: PhysicalRect,
        radii: [f64; 4],
        in_group: bool,
    ) -> Result<(), DrawError> {
        let start = Instant::now();
        let _scope = crate::microprofile::scope("Background");
        if node["bg"].is_object() {
            self.stats.backgrounds += 1;
        }
        let result = self.background_inner(ctx, node, p, radii, in_group, None);
        self.stats.background_ms += start.elapsed().as_secs_f64() * 1000.;
        result
    }
    /// The border of a CanvasGroup, drawn on the group's target over its composited texture. Measured on
    /// fixtures/group-probe-border.luau (Studio 0.747): the border lies on the pixels a Frame's border would,
    /// in every BorderMode, turned or not; it is as opaque as the background times the group's opacity
    /// (`opacity`, of 255), takes the group's UIGradient like a Frame's border and not its GroupColor3; the
    /// texture under it holds the fill over the whole rect and the children, which an Inset or Middle border
    /// covers. A UICorner leaves it out, as on a Frame.
    pub(crate) fn group_border(
        &mut self,
        ctx: &mut BuildContext,
        node: &Value,
        p: PhysicalRect,
        radii: [f64; 4],
        opacity: u8,
    ) -> Result<(), DrawError> {
        if number(&node["border"]["size"], 0.) <= 0. {
            return Ok(());
        }
        self.background_inner(ctx, node, p, radii, false, Some(opacity as f64 / 255.))
    }
    /// `ring`: only the border, its opacity scaled by the value (group_border)
    fn background_inner(
        &mut self,
        ctx: &mut BuildContext,
        node: &Value,
        p: PhysicalRect,
        radii: [f64; 4],
        in_group: bool,
        ring: Option<f64>,
    ) -> Result<(), DrawError> {
        if !node["bg"].is_object() {
            return Ok(());
        }
        let bg = &node["bg"];
        let mut colour = color(&bg["color"], [1.; 3]);
        if node["interaction"]["bg"].is_array() {
            colour = color(&node["interaction"]["bg"], [255.; 3]).map(|v| v / 255.)
        }
        let a = alpha8((1. - number(&bg["transparency"], 0.)) * ring.unwrap_or(1.));
        if a == 0 {
            return Ok(());
        }
        let m = node_transform(node, ctx.origin, self.scale);
        let clip = clip_rect(node, ctx.origin, self.scale);
        let r = p.rect;
        let gradient = if in_group {
            &Value::Null
        } else {
            &node["gradient"]
        };
        let paint_for = |c: [f64; 3], dither: [f64; 2], varying: bool| {
            if gradient.is_object() {
                Paint::gradient(
                    gradient,
                    p.raw,
                    c,
                    a as f64 / 255.,
                    invert(m),
                    dither,
                    varying,
                )
            } else {
                Paint::solid(c, a as f64 / 255.)
            }
        };
        let border = &node["border"];
        // Measured: BorderMode Middle / Inset shrink the fill by B / 2 / B (re-snapped with snap_device); the
        // border wraps the fill out to the snapped fill edges -/+ B, B = BorderSizePixel x DPI, and shares the
        // background's transparency. With a UIGradient the outer edges round ties down, ceil(v - 0.5)
        // (fixtures/gradient-edges.luau). Rotated frames: the fill first, then the border as a ring on top whose
        // inner edge fringes into the fill like its outer edge (fixtures/rotation.luau).
        // A CanvasGroup fills its whole rect in its texture; its border is drawn over the composite
        // (group_border)
        let border_size = if in_group { 0. } else { number(&border["size"], 0.) * self.scale };
        if border_size > 0. && !radii.iter().any(|v| *v > 0.) {
            let inset = if border["mode"] == "Inset" {
                border_size
            } else if border["mode"] == "Middle" {
                border_size / 2.
            } else {
                0.
            };
            let mut fill = Rect::new(r.x + inset, r.y + inset, r.w - 2. * inset, r.h - 2. * inset);
            if !p.snapped && gradient.is_object() && fill.w > 0. && fill.h > 0. {
                // (the border takes the gradient too, as on an unturned frame)
                let outer = Rect::new(
                    fill.x - border_size,
                    fill.y - border_size,
                    fill.w + 2. * border_size,
                    fill.h + 2. * border_size,
                );
                let raw = [p.raw.x, p.raw.y, p.raw.w, p.raw.h];
                let parts = [(outer, Some(color(&border["color"], [0.; 3]))), (fill, ring.is_none().then_some(colour))];
                self.turned_gradient_fill(ctx, gradient, raw, m, clip, a, &parts);
                return Ok(());
            }
            if !p.snapped {
                let fb = transformed_bounds(m, fill, 2.);
                if ring.is_none() {
                    self.rotated_fill(
                        ctx,
                        fill,
                        [0.; 4],
                        m,
                        clip,
                        paint_for(colour, [fb[0], fb[1]], false),
                        0.,
                    );
                }
                let outer = Rect::new(
                    fill.x - border_size,
                    fill.y - border_size,
                    fill.w + 2. * border_size,
                    fill.h + 2. * border_size,
                );
                let ob = transformed_bounds(m, outer, 2.);
                self.rotated_fill(
                    ctx,
                    outer,
                    [0.; 4],
                    m,
                    clip,
                    paint_for(color(&border["color"], [0.; 3]), [ob[0], ob[1]], false),
                    border_size,
                );
            } else {
                let s = self.scale;
                let snapped = |q: Rect| {
                    let l = snap_device(q.x, s);
                    let t = snap_device(q.y, s);
                    Rect::new(l, t, snap_device(q.x + q.w, s) - l, snap_device(q.y + q.h, s) - t)
                };
                if inset > 0. {
                    fill = snapped(fill)
                }
                let mut outer = snapped(Rect::new(
                    fill.x - border_size,
                    fill.y - border_size,
                    fill.w + 2. * border_size,
                    fill.h + 2. * border_size,
                ));
                if gradient.is_object() {
                    let l = (fill.x - border_size - 0.5).ceil();
                    let t = (fill.y - border_size - 0.5).ceil();
                    outer = Rect::new(
                        l,
                        t,
                        (fill.x + fill.w + border_size - 0.5).ceil() - l,
                        (fill.y + fill.h + border_size - 0.5).ceil() - t,
                    );
                }
                if gradient.is_object() {
                    // Per vertex on the border strips and the fill quad (gradients::border_gradient_pixels)
                    if outer.w <= 0. || outer.h <= 0. {
                        return Ok(());
                    }
                    let gradient = gradient.clone();
                    let raw = [p.raw.x, p.raw.y, p.raw.w, p.raw.h];
                    let border_colour = color(&border["color"], [0.; 3]).map(|v| quantize(v) as f64);
                    let fill_colour = colour.map(|v| quantize(v) as f64);
                    // Straight bytes blended like an image (BlendMode::Image): Studio blends a gradient in
                    // float with one rounding, round((c A + D (255 - A)) / 255)
                    let mut parts = super::gradients::border_gradient_meshes(
                        &gradient,
                        raw,
                        outer,
                        fill,
                        border_colour,
                        fill_colour,
                        a as f64,
                    );
                    if ring.is_some() && fill.w > 0. && fill.h > 0. {
                        // The quad of the fill is the last (border_gradient_meshes)
                        parts.pop();
                    }
                    // The pixels come from the quads' meshes alone (GradientMesh::field): keyed relative
                    // to the layer when every mesh is exact there, so a scrolled frame is rasterized once
                    let origin = [outer.x, outer.y];
                    let local: Option<Vec<_>> = parts
                        .iter()
                        .map(|(bounds, mesh)| {
                            let bounds = [
                                super::gradients::exact_difference(bounds[0], origin[0])?,
                                super::gradients::exact_difference(bounds[1], origin[1])?,
                                super::gradients::exact_difference(bounds[2], origin[0])?,
                                super::gradients::exact_difference(bounds[3], origin[1])?,
                            ];
                            Some((bounds, mesh.local(origin)?))
                        })
                        .collect();
                    let key = match local {
                        Some(local) => format!(
                            "gradborder-local:{}",
                            serde_json::to_string(&(local, [outer.w, outer.h], border_colour, fill_colour, a, ring.is_some()))
                                .unwrap_or_default()
                        ),
                        None => format!(
                            "gradborder-straight:{}",
                            serde_json::to_string(&(&gradient, raw, outer.bounds(), fill.bounds(), border_colour, fill_colour, a, ring.is_some()))
                                .unwrap_or_default()
                        ),
                    };
                    let raster = move || super::gradients::border_gradient_mesh_pixels(&parts, outer);
                    let (.., id) = self.cached_layer(
                        key,
                        outer.x as i32,
                        outer.y as i32,
                        outer.w as u32,
                        outer.h as u32,
                        Box::new(raster),
                    );
                    self.image_layer(ctx, outer.translated(m[4], m[5]), id, clip);
                    return Ok(());
                }
                let hole = if fill.w > 0. && fill.h > 0. {
                    Some(fill.translated(m[4], m[5]))
                } else {
                    None
                };
                self.rect_paint(
                    ctx,
                    outer.translated(m[4], m[5]),
                    clip,
                    paint_for(color(&border["color"], [0.; 3]), [0., 0.], false),
                    hole,
                );
                if ring.is_none() {
                    self.rect_paint(
                        ctx,
                        fill.translated(m[4], m[5]),
                        clip,
                        paint_for(colour, [0., 0.], true),
                        None,
                    );
                }
            }
            return Ok(());
        }
        if ring.is_some() {
            return Ok(());
        }
        if !p.snapped {
            let bounds = transformed_bounds(m, r, 2.);
            if gradient.is_object() && !radii.iter().any(|v| *v > 0.) && r.w > 0. && r.h > 0. {
                let raw = [p.raw.x, p.raw.y, p.raw.w, p.raw.h];
                self.turned_gradient_fill(ctx, gradient, raw, m, clip, a, &[(r, Some(colour))]);
                return Ok(());
            }
            self.rotated_fill(
                ctx,
                r,
                radii,
                m,
                clip,
                paint_for(colour, [bounds[0], bounds[1]], false),
                0.,
            );
            return Ok(());
        }
        if gradient.is_object() || radii.iter().any(|v| *v > 0.) {
            let clip_bounds = clip.unwrap_or(NO_CLIP);
            let crop = if m[4].fract() == 0. && m[5].fract() == 0. {
                let l = r.x.max((clip_bounds[0].max(0.) - m[4]).floor());
                let t = r.y.max((clip_bounds[1].max(0.) - m[5]).floor());
                let right = (r.x + r.w).min((clip_bounds[2].min(ctx.width as f64) - m[4]).ceil());
                let bottom = (r.y + r.h).min((clip_bounds[3].min(ctx.height as f64) - m[5]).ceil());
                Rect::new(l, t, (right - l).max(0.), (bottom - t).max(0.))
            } else {
                r
            };
            let w = crop.w as u32;
            let h = crop.h as u32;
            if w == 0 || h == 0 {
                return Ok(());
            }
            if Self::culled(ctx, crop.translated(m[4], m[5]).bounds(), clip) {
                return Ok(());
            }
            // A solid fill's pixel depends on its place in the rect only (corner_coverage_region_limited):
            // the whole rect's layer, drawn under the same clip, draws the crop's pixels, and is one layer
            // for every crop of it (a row scrolling across the clip edge, equal rows) unless far larger
            let full = r.w * r.h;
            let crop = if !gradient.is_object()
                && crop != r
                && r.w.fract() == 0.
                && r.h.fract() == 0.
                && (full <= 4. * crop.w * crop.h || full <= 65536.)
            {
                r
            } else {
                crop
            };
            let (w, h) = (crop.w as u32, crop.h as u32);
            let gradient = gradient.is_object().then(|| gradient.clone());
            let straight = gradient.is_some();
            let raw = [p.raw.x, p.raw.y, p.raw.w, p.raw.h];
            let origin = [(r.x + m[4]) as i32, (r.y + m[5]) as i32];
            let parity = [origin[0].rem_euclid(2), origin[1].rem_euclid(2)];
            // The gradient's triangulation and vertex values, from its absolute placement (the pixels come
            // from these alone: GradientMesh::field)
            let mesh = gradient.as_ref().map(|gradient| {
                super::gradients::gradient_mesh(
                    gradient,
                    raw,
                    colour.map(|v| quantize(v) as f64),
                    a as f64,
                    r.bounds(),
                )
            });
            // The layer's pixels depend on exactly these inputs; misses are rasterized with the
            // frame's other queued layers
            // Without a gradient the pixels depend on the rect's size, the crop within it and the
            // parity of its device origin (corner_coverage_region_limited's derivative quads) only;
            // with one, also on the gradient mesh relative to the rect (GradientMesh::local), so a
            // scrolled gradient row is rasterized once, not at every offset
            let local_mesh = mesh.as_ref().and_then(|mesh| mesh.local([r.x, r.y]));
            let key = if let Some(local) = local_mesh {
                format!(
                    "fill-straight-local:{}",
                    serde_json::to_string(&(
                        local,
                        [r.w, r.h],
                        [raw[2], raw[3]],
                        [crop.x - r.x, crop.y - r.y, crop.w, crop.h],
                        colour,
                        a,
                        radii,
                        parity
                    ))
                    .unwrap_or_default()
                )
            } else if gradient.is_none() {
                format!(
                    "fill-local:{}",
                    serde_json::to_string(&(
                        [r.w, r.h],
                        [raw[2], raw[3]],
                        [crop.x - r.x, crop.y - r.y, crop.w, crop.h],
                        colour,
                        a,
                        radii,
                        parity
                    ))
                    .unwrap_or_default()
                )
            } else {
                format!(
                    "fill-straight:{}",
                    serde_json::to_string(&(&gradient, raw, crop.bounds(), r.bounds(), colour, a, radii, origin))
                        .unwrap_or_default()
                )
            };
            let raster = move || {
                if gradient.is_none() {
                    // Corner radii clamp to half the unsnapped short side (coverage::corner_coverage_region_limited)
                    let limit = (raw[2].min(raw[3]) / 2.0).max(0.0);
                    return solid_fill_pixels(
                        [r.w as u32, r.h as u32],
                        radii,
                        limit,
                        origin,
                        [(crop.x - r.x) as i32, (crop.y - r.y) as i32],
                        [w, h],
                        colour,
                        a,
                    );
                }
                let (wu, hu) = (w as usize, h as usize);
                // A square-cornered fill of a gradient varying along one axis: its pixels are the shaded row
                // (column) of the field repeated
                if !radii.iter().any(|v| *v > 0.) {
                    if let Some(profile) = mesh.as_ref().and_then(|mesh| mesh.profile(crop.bounds())) {
                        let texel = |value: &[f32]| {
                            [0, 1, 2, 3].map(|c| gpu8(value[c] as f64 / 255.))
                        };
                        let mut bytes = vec![0; wu * hu * 4];
                        match profile {
                            super::gradients::Profile::Row(row) => {
                                let shaded: Vec<u8> = row.chunks_exact(4).flat_map(texel).collect();
                                for out in bytes.chunks_exact_mut(wu * 4) {
                                    out.copy_from_slice(&shaded);
                                }
                            }
                            super::gradients::Profile::Column(column) => {
                                for (out, value) in bytes.chunks_exact_mut(wu * 4).zip(column.chunks_exact(4)) {
                                    let pixel = texel(value);
                                    for target in out.chunks_exact_mut(4) {
                                        target.copy_from_slice(&pixel);
                                    }
                                }
                            }
                        }
                        return bytes;
                    }
                }
                let field = mesh.as_ref().map(|mesh| mesh.field(crop.bounds()));
                let coverage = if radii.iter().any(|v| *v > 0.) {
                    // Corner radii clamp to half the unsnapped short side (coverage::corner_coverage_region_limited)
                    let limit = (raw[2].min(raw[3]) / 2.0).max(0.0);
                    super::coverage::corner_coverage_region_limited(
                        r.w as u32,
                        r.h as u32,
                        radii,
                        limit,
                        origin[0],
                        origin[1],
                        (crop.x - r.x) as i32,
                        (crop.y - r.y) as i32,
                        w,
                        h,
                    )
                } else {
                    vec![1.; w as usize * h as usize]
                };
                let mut bytes = vec![0; wu * hu * 4];
                let shade = |k: usize, pixel: &mut [u8]| {
                    let alpha = field.as_ref().map_or(a as f64, |v| v[k * 4 + 3] as f64);
                    pixel[3] = gpu8(alpha / 255. * coverage[k]);
                    for c in 0..3 {
                        pixel[c] = field
                            .as_ref()
                            .map_or(quantize(colour[c]), |v| gpu8(v[k * 4 + c] as f64 / 255.));
                    }
                };
                // Equal inputs shade to equal bytes: a pixel (row) like the one before it, as a gradient varying
                // along one axis makes every pixel of a row or every row, is copied
                let same = |k: usize, before: usize, n: usize| {
                    coverage[k..k + n] == coverage[before..before + n]
                        && field.as_ref().is_none_or(|v| v[k * 4..(k + n) * 4] == v[before * 4..(before + n) * 4])
                };
                let shade_rows = |top: usize, rows: &mut [u8]| {
                    for j in 0..rows.len() / (wu * 4) {
                        let k = (top + j) * wu;
                        let (done, row) = rows.split_at_mut(j * wu * 4);
                        let row = &mut row[..wu * 4];
                        if j > 0 && same(k, k - wu, wu) {
                            row.copy_from_slice(&done[(j - 1) * wu * 4..]);
                            continue;
                        }
                        for i in 0..wu {
                            if i > 0 && same(k + i, k + i - 1, 1) {
                                row.copy_within((i - 1) * 4..i * 4, i * 4);
                            } else {
                                shade(k + i, &mut row[i * 4..i * 4 + 4]);
                            }
                        }
                    }
                };
                if super::sampling::parallel_rows(wu * hu) {
                    use rayon::prelude::*;
                    let band = (super::sampling::PARALLEL_PIXELS / wu).max(1);
                    bytes
                        .par_chunks_mut(band * wu * 4)
                        .enumerate()
                        .for_each(|(i, rows)| shade_rows(i * band, rows));
                } else {
                    shade_rows(0, &mut bytes);
                }
                // A gradient fill stays straight and is blended like an image (round 9, a history list's
                // stacked fades: Studio rounds once, in float; premultiplied bytes drifted a level per faint
                // layer); a solid rounded fill keeps the premultiplied layer
                if !straight {
                    premultiply(&mut bytes);
                }
                bytes
            };
            let (.., id) = self.cached_layer(key, crop.x as i32, crop.y as i32, w, h, Box::new(raster));
            if straight {
                self.image_layer(ctx, crop.translated(m[4], m[5]), id, clip);
            } else {
                self.layer(ctx, crop.translated(m[4], m[5]), id, clip, 0, &[]);
            }
        } else {
            self.rect(
                ctx,
                r.translated(m[4], m[5]),
                clip,
                solid_paint(colour, a as f64 / 255.),
                None,
            )
        }
        Ok(())
    }
    /// A hash of the node's exact shadow boxes for the node record key (0: none)
    fn exact_shadows_hash(&self, node: &Value) -> u64 {
        if self.exact_shadows.is_empty() {
            return 0;
        }
        let id = node["id"].as_u64().unwrap_or(0);
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        let mut any = false;
        for index in 0..array(&node["shadows"]).len() {
            if let Some(b) = self.exact_shadows.get(&(id, index)) {
                any = true;
                index.hash(&mut hasher);
                b.map(f64::to_bits).hash(&mut hasher);
            }
        }
        if any { hasher.finish() | 1 } else { 0 }
    }
    pub(crate) fn group_shadows(
        &mut self,
        ctx: &mut BuildContext,
        node: &Value,
        scene: &SceneStore,
        assets: &AssetStore,
    ) -> Result<(), DrawError> {
        let start = Instant::now();
        let _scope = crate::microprofile::scope("Shadows");
        self.stats.shadows += array(&node["shadows"]).len() as u64;
        let result = self.group_shadows_inner(ctx, node, scene, assets, false);
        self.stats.shadows_ms += start.elapsed().as_secs_f64() * 1000.;
        result
    }
    /// The node's UIShadows with Mode = Text: under its text, over its fill and image
    /// (fixtures/shadow-probe-text.luau). BlurRadius and Offset are whole pixels of the target, whatever the
    /// device scale and UIScale: a glow spans 2 x BlurRadius px each side of a glyph's edge
    /// (shadow-probe-text2.luau: 16.07 px for BlurRadius 8 at 1.5x, and under UIScale 2).
    fn text_shadows(&mut self, ctx: &mut BuildContext, node: &Value, scene: &SceneStore, assets: &AssetStore) -> Result<(), DrawError> {
        let mut shadows: Vec<&Value> = array(&node["shadows"])
            .iter()
            .filter(|s| s["mode"] == "Text" && number(&s["transparency"], 0.) < 1.)
            .collect();
        if shadows.is_empty() || !node["text"].is_object() {
            return Ok(());
        }
        shadows.sort_by(|a, b| number(&a["zIndex"], 0.).total_cmp(&number(&b["zIndex"], 0.)));
        for shadow in shadows {
            let rgb = color(&shadow["color"], [0.; 3]).map(|c| stop_byte(c) as f64);
            let opacity = (1. - number(&shadow["transparency"], 0.)).clamp(0., 1.);
            let width = 2. * number(&shadow["blur"], 0.).max(0.);
            let quads = match super::text::text_shadow(node, scene.fonts(), assets, self.scale, rgb, opacity, width) {
                Ok(quads) => quads,
                Err(_) if assets.pending_count() > 0 => Vec::new(),
                Err(e) => return Err(e.into()),
            };
            let by = [component(&shadow["offset"], 0, 0.).trunc(), component(&shadow["offset"], 1, 0.).trunc()];
            for q in quads {
                let id = self.upload_shared(q.width, q.height, &q.rgba);
                let mut m = node_transform(node, ctx.origin, self.scale);
                m[4] += by[0];
                m[5] += by[1];
                let clip = text_clip_rect(node, ctx.origin, self.scale);
                let qr = Rect::new(q.x, q.y, q.w, q.h);
                if m == [1., 0., 0., 1., m[4], m[5]] && q.opacity == 1. && q.w == q.width as f64 && q.h == q.height as f64 {
                    self.layer(ctx, qr.translated(m[4], m[5]), id, clip, 0, &[]);
                } else {
                    self.text_quad(ctx, qr, q.width, q.height, id, m, clip, q.opacity, 0, &[]);
                }
            }
        }
        Ok(())
    }
    /// The node's Inset UIShadows: over its fill, image and text (a CanvasGroup's: over its composite, not
    /// faded by it), under its strokes and children (fixtures/shadow-probe-inset2.luau)
    pub(crate) fn group_inset_shadows(
        &mut self,
        ctx: &mut BuildContext,
        node: &Value,
        scene: &SceneStore,
        assets: &AssetStore,
    ) -> Result<(), DrawError> {
        if !array(&node["shadows"]).iter().any(inset_shadow) {
            return Ok(());
        }
        let start = Instant::now();
        let _scope = crate::microprofile::scope("Shadows");
        let result = self.group_shadows_inner(ctx, node, scene, assets, true);
        self.stats.shadows_ms += start.elapsed().as_secs_f64() * 1000.;
        result
    }
    fn group_shadows_inner(
        &mut self,
        ctx: &mut BuildContext,
        node: &Value,
        _scene: &SceneStore,
        _assets: &AssetStore,
        inset: bool,
    ) -> Result<(), DrawError> {
        let p = physical_rect(node, self.scale);
        let r = p.rect;
        let m = node_transform(node, ctx.origin, self.scale);
        let radii = radius(&node["corner"], self.scale);
        let mut shadows: Vec<_> = array(&node["shadows"]).iter().enumerate().collect();
        // UIShadows draw below the element in ZIndex order (child order among equals), Inset ones over its
        // fill, image and text; without ShowBehindParent the frame's shape is cut out. Mode = Text draws nothing here:
        // it is the glow of a text object's glyphs, and nothing on any other object.
        // Among equals, the ones the frame cuts (ShowBehindParent = false) come after the others
        // (fixtures/shadow-probe-behind.luau: a cut first child lies over the second child's shadow)
        shadows.sort_by(|a, b| {
            number(&a.1["zIndex"], 0.)
                .total_cmp(&number(&b.1["zIndex"], 0.))
                .then((a.1["behind"] == false).cmp(&(b.1["behind"] == false)))
        });
        let node_id = node["id"].as_u64().unwrap_or(0);
        for (index, shadow) in shadows {
            let alpha = 1. - number(&shadow["transparency"], 0.);
            if alpha <= 0. || shadow["mode"] == "Text" || inset_shadow(shadow) != inset {
                continue;
            }
            if inset || shadow["behind"] == false {
                self.cut_shadow(ctx, node, shadow, p, m, radii, alpha, inset);
                continue;
            }
            // UIShadow geometry: the snapped element rect grown by Spread (the total per axis, not per side) and
            // moved by Offset; corner radii above 0 grow by half the smaller spread (min(sx, sy) / 2, never below
            // 0); sigma = BlurRadius x DPI / 2 (notes/images.md round 4, shadow-probe-d: spread, offset, colour)
            let sigma = number(&shadow["blur"], 0.).max(0.) * self.scale / 2.;
            let sx = component(&shadow["spread"], 0, 0.) * self.scale;
            let sy = component(&shadow["spread"], 1, 0.) * self.scale;
            let w = r.w + sx;
            let h = r.h + sy;
            if w <= 0. || h <= 0. {
                continue;
            }
            let cx = r.x + r.w / 2. + component(&shadow["offset"], 0, 0.) * self.scale;
            let cy = r.y + r.h / 2. + component(&shadow["offset"], 1, 0.) * self.scale;
            let radii = radii.map(|v| {
                if v > 0. {
                    (v + sx.min(sy) / 2.).max(0.)
                } else {
                    0.
                }
            });
            let pad = 3. * sigma;
            let x = (cx - w / 2. - pad).floor();
            let y = (cy - h / 2. - pad).floor();
            let width = ((cx + w / 2. + pad).ceil() - x) as u32;
            let height = ((cy + h / 2. + pad).ceil() - y) as u32;
            let rgb = color(&shadow["color"], [0.; 3]).map(stop_byte);
            // Blurred shadows through Studio's mesh + profile texture (coverage::shadow_mesh / shadow_alpha8,
            // measured), hard ones the rounded rect at floor(255 alpha) (coverage::ShadowField)
            let va = if sigma > 0. {
                super::coverage::shadow_vertex_alpha(w / 2., h / 2., sigma, alpha)
            } else {
                0
            };
            if sigma > 0. && va == 0 {
                continue;
            }
            let hard = (255. * alpha).floor().clamp(0., 255.) as u8;
            let scale = self.scale;
            // The layer's pixels depend on exactly these inputs; misses are rasterized with the
            // frame's other queued layers
            // lx = (x + px + 0.5) - cx is exact when cx is on a 1/1024 grid (every operand a small
            // multiple of 2^-10), and then equals (px + 0.5) - (cx - x): the layer is keyed and
            // rasterized relative to (x, y) and reused wherever the shadow moves by whole pixels
            let dyadic = |v: f64| (v * 1024.).fract() == 0. && v.abs() < 1e9;
            let (ox, oy) = if dyadic(cx) && dyadic(cy) { (x, y) } else { (0., 0.) };
            let (x, y, cx, cy) = (x - ox, y - oy, cx - ox, cy - oy);
            let geometry =
                serde_json::to_string(&(x, y, cx, cy, width, height, w, h, sigma, radii, scale, va, hard)).unwrap_or_default();
            let key = format!("shadow:{}", serde_json::to_string(&(&geometry, rgb)).unwrap_or_default());
            let (x, y) = (x + ox, y + oy);
            let (lx0, ly0) = (x - ox, y - oy);
            // The alpha depends on the geometry alone, the colour is copied in: a recoloured shadow (a tween)
            // is tinted from its kept alpha, and its straight and premultiplied layers share one
            let alpha = move || {
                let (x, y) = (lx0, ly0);
                let corners = (sigma > 0.).then(|| super::coverage::shadow_mesh(w, h, sigma, radii, scale));
                let fields = radii.map(|radius| super::coverage::ShadowField::polygonal(w, h, radius, scale));
                let mut alpha = vec![0; width as usize * height as usize];
                for py in 0..height {
                    for px in 0..width {
                        let lx = x + px as f64 + 0.5 - cx;
                        let ly = y + py as f64 + 0.5 - cy;
                        // Quadrants split at the first pixel centre with a non-negative offset (0 is right / bottom)
                        let quadrant = if ly < 0. {
                            if lx < 0. { 0 } else { 1 }
                        } else if lx < 0. {
                            3
                        } else {
                            2
                        };
                        let a = match &corners {
                            Some(corners) => super::coverage::shadow_alpha8(corners[quadrant].d(lx.abs(), ly.abs()), va),
                            None if fields[quadrant].at(lx, ly) > 0. => hard,
                            None => 0,
                        };
                        alpha[(py * width + px) as usize] = a;
                    }
                }
                alpha
            };
            let kept = match self.shadow_alpha.get_mut(&geometry) {
                Some((cached, used)) => {
                    *used = self.frame;
                    let slot = std::sync::OnceLock::new();
                    let _ = slot.set(cached.clone());
                    std::sync::Arc::new(slot)
                }
                None => {
                    let slot = std::sync::Arc::new(std::sync::OnceLock::new());
                    self.pending_shadow_alpha.push((geometry, slot.clone()));
                    slot
                }
            };
            let raster_for = move |straight: bool| {
                let kept = kept.clone();
                move || {
                    let alpha = kept.get_or_init(|| std::sync::Arc::new(alpha()));
                    let mut bytes = vec![0; alpha.len() * 4];
                    for (pixel, a) in bytes.chunks_exact_mut(4).zip(alpha.iter()) {
                        pixel[..3].copy_from_slice(&rgb);
                        pixel[3] = *a;
                    }
                    if !straight {
                        premultiply(&mut bytes);
                    }
                    bytes
                }
            };
            let rect = Rect::new(x, y, width as f64, height as f64);
            let clip = clip_rect(node, ctx.origin, self.scale);
            let translation = m[0] == 1. && m[1] == 0. && m[2] == 0. && m[3] == 1.;
            // The part in the shadow's exact box (exact_shadow_boxes: where three halos overlap, shadow-probe-p)
            // blends like an image (straight bytes, BlendMode::Image), the rest premultiplied; one draw per clip rect
            let exact = (translation && ctx.target == FRAME_TARGET)
                .then(|| self.exact_shadows.get(&(node_id, index)).copied())
                .flatten();
            if let Some(exact) = exact {
                let l = rect.translated(m[4], m[5]);
                let l = [l.x, l.y, l.x + l.w, l.y + l.h];
                let e = [l[0].max(exact[0]), l[1].max(exact[1]), l[2].min(exact[2]), l[3].min(exact[3])];
                if e[0] < e[2] && e[1] < e[3] {
                    let straight_key = format!("straight-{key}");
                    let straight_raster = raster_for(true);
                    let (.., straight_id) = self.cached_layer(straight_key, x as i32, y as i32, width, height, Box::new(straight_raster));
                    let (.., id) = self.cached_layer(key, x as i32, y as i32, width, height, Box::new(raster_for(false)));
                    let c = clip.unwrap_or(NO_CLIP_RECT);
                    let parts = [
                        (e, true),
                        ([l[0], l[1], l[2], e[1]], false),
                        ([l[0], e[3], l[2], l[3]], false),
                        ([l[0], e[1], e[0], e[3]], false),
                        ([e[2], e[1], l[2], e[3]], false),
                    ];
                    let target = rect.translated(m[4], m[5]);
                    for (b, straight) in parts {
                        let part = [b[0].max(c[0]), b[1].max(c[1]), b[2].min(c[2]), b[3].min(c[3])];
                        if part[0] < part[2] && part[1] < part[3] {
                            if straight {
                                self.image_layer(ctx, target, straight_id, Some(part));
                            } else {
                                self.layer(ctx, target, id, Some(part), 0, &[]);
                            }
                        }
                    }
                    continue;
                }
            }
            let (.., id) = self.cached_layer(key, x as i32, y as i32, width, height, Box::new(raster_for(false)));
            if translation {
                self.layer(ctx, rect.translated(m[4], m[5]), id, clip, 0, &[]);
            } else {
                self.text_quad(ctx, rect, width, height, id, m, clip, 1., 0, &[]);
            }
        }
        Ok(())
    }
    /// A UIShadow that the frame's shape cuts (Studio 0.747; fixtures/shadow-probe-inset.luau, -inset2,
    /// -behind, notes/images.md), rasterized in device space through the node's transform:
    /// - Inset: the outer shadow's mesh of the hole, read through the profile the other way round, inside the
    ///   frame only. The hole is the frame shrunk by Spread (the total per axis) and moved by Offset; its
    ///   corner radii above 0 shrink by half the smaller spread.
    /// - ShowBehindParent = false: the outer shadow, outside the frame only.
    /// The frame cuts with the rounded fill's mask (the distance to the rounded box over its coarse
    /// derivative, sharp frames too, so a pixel beside a corner can hold a little): a pixel is the profile
    /// times the mask's coverage, or times what the mask leaves.
    #[allow(clippy::too_many_arguments)]
    fn cut_shadow(
        &mut self,
        ctx: &mut BuildContext,
        node: &Value,
        shadow: &Value,
        p: PhysicalRect,
        m: Transform,
        radii: [f64; 4],
        alpha: f64,
        inset: bool,
    ) {
        let frame = p.rect;
        let scale = self.scale;
        let sigma = number(&shadow["blur"], 0.).max(0.) * scale / 2.;
        let sx = component(&shadow["spread"], 0, 0.) * scale;
        let sy = component(&shadow["spread"], 1, 0.) * scale;
        // The shadow's shape: the hole of an inner shadow, the grown frame of an outer one
        let (w, h) = if inset { (frame.w - sx, frame.h - sy) } else { (frame.w + sx, frame.h + sy) };
        let shaped = w > 0. && h > 0.;
        if !inset && !shaped {
            return;
        }
        let grow = if inset { -sx.min(sy) / 2. } else { sx.min(sy) / 2. };
        let shape_radii = radii.map(|v| if v > 0. { (v + grow).max(0.) } else { 0. });
        let cx = frame.x + frame.w / 2. + component(&shadow["offset"], 0, 0.) * scale;
        let cy = frame.y + frame.h / 2. + component(&shadow["offset"], 1, 0.) * scale;
        let va = if sigma > 0. && shaped {
            super::coverage::shadow_vertex_alpha(w / 2., h / 2., sigma, alpha)
        } else {
            (255. * alpha - 0.5).ceil().max(0.) as u32
        };
        if va == 0 {
            return;
        }
        let hard = (255. * alpha).floor().clamp(0., 255.) as u8;
        // What is drawn, in the node's space: the frame and a pixel around it, or the shadow's reach
        let pad = 3. * sigma;
        let local = if inset {
            Rect::new(frame.x, frame.y, frame.w, frame.h)
        } else {
            Rect::new(cx - w / 2. - pad, cy - h / 2. - pad, w + 2. * pad, h + 2. * pad)
        };
        // The layer is placed and rasterized in whole device pixels of the target; the translation's whole
        // part is left out of what it shows, so a shadow that moves by whole pixels keeps its layer
        let shift = [m[4].floor(), m[5].floor()];
        let about = [m[0], m[1], m[2], m[3], m[4] - shift[0], m[5] - shift[1]];
        let e = transformed_extent(about, local);
        let x = (e[0] - 1.).floor();
        let y = (e[1] - 1.).floor();
        let width = ((e[2] + 1.).ceil() - x) as u32;
        let height = ((e[3] + 1.).ceil() - y) as u32;
        if width == 0 || height == 0 || width > 16384 || height > 16384 {
            return;
        }
        let limit = (p.raw.w / 2.).min(p.raw.h / 2.);
        let rgb = color(&shadow["color"], [0.; 3]).map(stop_byte);
        // Whether the hole and its mesh lie wholly off the frame
        let apart = !shaped
            || (cx - (frame.x + frame.w / 2.)).abs() >= (frame.w + w) / 2. + pad
            || (cy - (frame.y + frame.h / 2.)).abs() >= (frame.h + h) / 2. + pad;
        // The mask's derivative quads sit on even pixels of the target
        let parity = [((x + shift[0]) as i64).rem_euclid(2), ((y + shift[1]) as i64).rem_euclid(2)];
        let dyadic = |v: f64| (v * 1024.).fract() == 0. && v.abs() < 1e9;
        let translation = about[..4] == [1., 0., 0., 1.];
        // (an unturned shadow is keyed about its layer's corner, as the outer shadows are)
        let (ox, oy) = if translation && [cx, cy, frame.x, frame.y].into_iter().all(dyadic) { (x, y) } else { (0., 0.) };
        let key = format!(
            "cut-shadow:{}",
            serde_json::to_string(&(
                (x - ox, y - oy, cx - ox, cy - oy, frame.x - ox, frame.y - oy, frame.w, frame.h),
                (width, height, w, h, sigma, radii, shape_radii, limit),
                (scale, va, hard, inset, parity, rgb, about)
            ))
            .unwrap_or_default()
        );
        let raster = move || {
            let (x, y, cx, cy) = (x - ox, y - oy, cx - ox, cy - oy);
            let (hw, hh) = (frame.w / 2., frame.h / 2.);
            let (fx, fy) = (frame.x - ox + hw, frame.y - oy + hh);
            let to_local = invert(about);
            let radii = radii.map(|r| r.max(0.).min(limit));
            // The signed distance to the frame's rounded box at a layer pixel's centre
            let sd = |i: i64, j: i64| {
                let [lx, ly] = apply(to_local, x + ox + i as f64 + 0.5, y + oy + j as f64 + 0.5);
                let (px, py) = (lx - ox - fx, ly - oy - fy);
                let r = radii[if py < 0. {
                    if px < 0. { 0 } else { 1 }
                } else if px < 0. {
                    3
                } else {
                    2
                }];
                let qx = px.abs() - hw + r;
                let qy = py.abs() - hh + r;
                qx.max(0.).hypot(qy.max(0.)) + qx.max(qy).min(0.) - r
            };
            let corners = (sigma > 0. && shaped).then(|| super::coverage::shadow_mesh(w, h, sigma, shape_radii, scale));
            let fields = shape_radii.map(|radius| super::coverage::ShadowField::polygonal(w, h, radius, scale));
            let mut bytes = vec![0; (width * height) as usize * 4];
            let mut quad_fw = vec![(i64::MIN, 0.0f64); width as usize + 2];
            for py in 0..height as i64 {
                for px in 0..width as i64 {
                    let d = sd(px, py);
                    let coverage = if d < -1.5 {
                        1.
                    } else if d > 1.5 {
                        0.
                    } else {
                        let qi = px - (px + parity[0]) % 2;
                        let qj = py - (py + parity[1]) % 2;
                        let slot = &mut quad_fw[(qi + 1) as usize];
                        let fw = if slot.0 == qj {
                            slot.1
                        } else {
                            let origin = sd(qi, qj);
                            let fw = (sd(qi + 1, qj) - origin).abs() + (sd(qi, qj + 1) - origin).abs();
                            *slot = (qj, fw);
                            fw
                        };
                        let t = if fw > 0. {
                            (0.5 - d / fw).clamp(0., 1.)
                        } else if d < 0. {
                            1.
                        } else {
                            0.
                        };
                        t * t * (3. - 2. * t)
                    };
                    let keep = if inset { coverage } else { 1. - coverage };
                    if keep <= 0. {
                        continue;
                    }
                    let [lx, ly] = apply(to_local, x + ox + px as f64 + 0.5, y + oy + py as f64 + 0.5);
                    // An inner shadow's pixel outside the frame (the mask leaves a little beside a corner) is
                    // drawn where the hole's mesh reaches, or when the mesh lies wholly off the frame: what
                    // fills the frame around the mesh ends on the frame's edges
                    let beside = inset && ((lx - ox - fx).abs() > hw || (ly - oy - fy).abs() > hh);
                    let (lx, ly) = (lx - ox - cx, ly - oy - cy);
                    if beside && !apart && !(lx.abs() < w / 2. + pad && ly.abs() < h / 2. + pad) {
                        continue;
                    }
                    let quadrant = if ly < 0. {
                        if lx < 0. { 0 } else { 1 }
                    } else if lx < 0. {
                        3
                    } else {
                        2
                    };
                    let a = match &corners {
                        Some(corners) => {
                            let d = corners[quadrant].d(lx.abs(), ly.abs());
                            if beside && !apart && d < -3. {
                                continue;
                            }
                            super::coverage::shadow_cut_alpha8(if inset { -d } else { d }, va, keep)
                        }
                        None => {
                            let inside = shaped && fields[quadrant].at(lx, ly) > 0.;
                            if inside != inset { (hard as f64 * keep + 1e-6).floor() as u8 } else { 0 }
                        }
                    };
                    if a == 0 {
                        continue;
                    }
                    let index = (py as u32 * width + px as u32) as usize * 4;
                    bytes[index..index + 3].copy_from_slice(&rgb);
                    bytes[index + 3] = a;
                }
            }
            premultiply(&mut bytes);
            bytes
        };
        let rect = Rect::new(x + shift[0], y + shift[1], width as f64, height as f64);
        let clip = clip_rect(node, ctx.origin, scale);
        let (.., id) = self.cached_layer(key, x as i32, y as i32, width, height, Box::new(raster));
        self.layer(ctx, rect, id, clip, 0, &[]);
    }
    pub(crate) fn group_strokes(
        &mut self,
        ctx: &mut BuildContext,
        node: &Value,
        scene: &SceneStore,
        assets: &AssetStore,
    ) -> Result<(), DrawError> {
        let start = Instant::now();
        let _scope = crate::microprofile::scope("Strokes");
        self.stats.strokes += array(&node["strokes"])
            .iter()
            .filter(|s| s["mode"] != "Contextual" || !node["text"].is_object())
            .count() as u64;
        let result = self.group_strokes_inner(ctx, node, scene, assets, None);
        self.stats.strokes_ms += start.elapsed().as_secs_f64() * 1000.;
        result
    }
    /// A node's border strokes drawn through `transform` (the screen -> texture
    /// scale of a resampled CanvasGroup) into its texture, under `clip` (whole texels).
    pub(crate) fn texture_strokes(
        &mut self,
        ctx: &mut BuildContext,
        node: &Value,
        scene: &SceneStore,
        assets: &AssetStore,
        transform: Transform,
        clip: Option<[f64; 4]>,
    ) -> Result<(), DrawError> {
        let start = Instant::now();
        let _scope = crate::microprofile::scope("Strokes (group texture)");
        self.stats.strokes += array(&node["strokes"])
            .iter()
            .filter(|s| s["mode"] != "Contextual" || !node["text"].is_object())
            .count() as u64;
        let result = self.group_strokes_inner(ctx, node, scene, assets, Some((transform, clip)));
        self.stats.strokes_ms += start.elapsed().as_secs_f64() * 1000.;
        result
    }
    fn group_strokes_inner(
        &mut self,
        ctx: &mut BuildContext,
        node: &Value,
        _scene: &SceneStore,
        _assets: &AssetStore,
        space: Option<(Transform, Option<[f64; 4]>)>,
    ) -> Result<(), DrawError> {
        let node_clip = match space {
            Some((_, clip)) => clip,
            None => clip_rect(node, ctx.origin, self.scale),
        };
        for stroke in array(&node["strokes"]) {
            if stroke["mode"] == "Contextual" && node["text"].is_object() {
                continue;
            }
            let alpha = 1. - number(&stroke["transparency"], 0.);
            // Device px in float32, as Studio multiplies: a thickness of 1 / 3 logical px is exactly 0.5 px at
            // 1.5x, an edge on the pixel centres, which the top-left rule then decides (fixtures/stroke-margin.luau)
            let thickness = (number(&stroke["thickness"], 0.) as f32 * self.scale as f32) as f64;
            if alpha <= 0. || thickness <= 0. {
                continue;
            }
            let physical = physical_rect(node, self.scale);
            let mut rect = physical.rect;
            // Corner radii clamp to half the unsnapped short side (coverage::border_stroke_alpha_limited)
            let mut limit = if physical.snapped {
                Some(physical.raw.w.min(physical.raw.h) / 2.)
            } else {
                None
            };
            let mut radii = radius(&node["corner"], self.scale);
            let mut position = stroke["position"].as_str().unwrap_or("Outer");
            let offset = number(&stroke["borderOffset"], 0.) * self.scale;
            if offset != 0. {
                let inner = offset
                    - if position == "Inner" {
                        thickness
                    } else if position == "Center" {
                        thickness / 2.
                    } else {
                        0.
                    };
                let grow = if inner >= 0. {
                    position = "Outer";
                    inner
                } else {
                    offset
                };
                rect = Rect::new(
                    rect.x - grow,
                    rect.y - grow,
                    rect.w + 2. * grow,
                    rect.h + 2. * grow,
                );
                if radii.iter().any(|v| *v > 0.) {
                    radii = radii.map(|v| (v + grow).max(0.));
                }
                limit = limit.map(|v| (v + grow).max(0.));
            }
            if rect.w <= 0. || rect.h <= 0. {
                continue;
            }
            let outward = if position == "Inner" {
                0.
            } else if position == "Center" {
                thickness / 2.
            } else {
                thickness
            };
            let m = match space {
                Some((transform, _)) => transform,
                None => node_transform(node, ctx.origin, self.scale),
            };
            let extent = transformed_extent(
                m,
                Rect::new(
                    rect.x - outward - 2.,
                    rect.y - outward - 2.,
                    rect.w + 2. * outward + 4.,
                    rect.h + 2. * outward + 4.,
                ),
            );
            // The stroke's device-space layer, 2 px past the stroke, starts on an even pixel so its coarse 2 x 2
            // derivative quads line up with the target's (notes/strokes.md), and is clamped to the canvas drawn
            // into (a CanvasGroup scratch wider than the screen lost the stroke: a wide window 99.84 -> 100.00%)
            let x = (extent[0].floor().max(-2.) / 2.).floor() * 2.;
            let y = (extent[1].floor().max(-2.) / 2.).floor() * 2.;
            let w = (extent[2].ceil().min(ctx.width as f64 + 2.) - x).max(0.) as u32;
            let h = (extent[3].ceil().min(ctx.height as f64 + 2.) - y).max(0.) as u32;
            if w == 0 || h == 0 {
                continue;
            }
            // Both branches draw one record over this box under node_clip: nothing to draw, nothing
            // to rasterize
            if Self::culled(ctx, [x, y, x + w as f64, y + h as f64], node_clip) {
                continue;
            }
            let rotated = m[1].abs() > 1e-9 || m[2].abs() > 1e-9;
            if rotated && stroke["gradient"].is_object() {
                let joins = stroke["joins"].as_str().unwrap_or("Round");
                let round = joins != "Miter" && joins != "Bevel";
                let bevel = joins == "Bevel";
                let inner = if position == "Inner" {
                    thickness
                } else if position == "Center" {
                    thickness / 2.
                } else {
                    0.
                };
                let radii = radii.map(|r| r.max(0.).min(rect.w / 2.).min(rect.h / 2.));
                let start = self.begin(
                    ctx,
                    [x, y, x + w as f64, y + h as f64],
                    node_clip,
                    5,
                );
                self.push(rect.bounds());
                self.push(radii);
                self.push([
                    outward,
                    inner,
                    self.scale,
                    ((round as u8) | ((bevel as u8) << 1) | (((position == "Inner") as u8) << 2))
                        as f64,
                ]);
                for texel in local_texels(invert(m), [x, y], [0., 0.]) {
                    self.push(texel)
                }
                for r in radii {
                    let sharp = r == 0.;
                    let mut mode = 0.;
                    let mut margin = self.scale / 10.0;
                    if r - inner > 0. && (round || position != "Inner") {
                        mode = 1.;
                    } else if !round && outward > 0. {
                        mode = 2.;
                        if !(sharp && bevel && position == "Center") {
                            margin = 0.;
                        }
                    } else if sharp && outward == 0. {
                        mode = 2.;
                        margin = 0.;
                    }
                    self.push([
                        mode,
                        margin,
                        r + outward + 0.5,
                        if sharp && position == "Center" {
                            outward
                        } else {
                            thickness
                        },
                    ]);
                }
                let rgb = color(&stroke["color"], [1.; 3]).map(quantize);
                let interior = inner
                    + radii.into_iter().fold(0., f64::max)
                    + 2.
                    + if bevel { inner } else { 0. };
                self.push([
                    interior,
                    alpha,
                    (u32::from(rgb[0]) * 65536 + u32::from(rgb[1]) * 256 + u32::from(rgb[2]))
                        as f64,
                    // The vertex alpha byte; the shader converts coverage per mode (stroke_alpha_byte)
                    alpha8(alpha) as f64,
                ]);
                self.push([1., 0., 0., 0.]);
                let outer = Rect::new(
                    rect.x - outward,
                    rect.y - outward,
                    rect.w + outward * 2.,
                    rect.h + outward * 2.,
                );
                Paint::gradient(
                    &stroke["gradient"],
                    outer,
                    color(&stroke["color"], [1.; 3]),
                    1.,
                    invert(m),
                    [x, y],
                    false,
                )
                .append(&mut self.packet.texels, start);
                self.emit(ctx, start, DrawBatch::default());
                continue;
            }
            let rgb = color(&stroke["color"], [1.; 3]).map(quantize);
            let limit = limit.unwrap_or((rect.w / 2.).min(rect.h / 2.));
            let position = position.to_string();
            let joins = stroke["joins"].as_str().unwrap_or("Round").to_string();
            let gradient = (stroke["gradient"].is_object() && !rotated).then(|| stroke["gradient"].clone());
            let scale = self.scale;
            let mesh = gradient.as_ref().map(|gradient| {
                let outer = [
                    rect.x - outward + m[4],
                    rect.y - outward + m[5],
                    rect.x + rect.w + outward + m[4],
                    rect.y + rect.h + outward + m[5],
                ];
                super::gradients::gradient_mesh(
                    gradient,
                    [outer[0], outer[1], outer[2] - outer[0], outer[3] - outer[1]],
                    rgb.map(|v| v as f64),
                    alpha8(alpha) as f64,
                    [outer[0] - 2., outer[1] - 2., outer[2] + 2., outer[3] + 2.],
                )
            });
            // Under a translation with every coordinate on a 1/1024 grid (snapped rects, the usual stroke
            // widths), border_stroke_alpha_modes computes exact differences of pixel centres and edges (the
            // rest are margins far from that grid), so its coverage depends on the rect relative to the
            // layer, not on where the layer is: a scrolled stroke is rasterized once, not at every offset.
            // A gradient adds its mesh relative to the layer (GradientMesh::local).
            let dyadic = |v: f64| (v * 1024.).fract() == 0. && v.abs() < 1e9;
            let relative = m[0] == 1.
                && m[1] == 0.
                && m[2] == 0.
                && m[3] == 1.
                && !rotated
                && [rect.x, rect.y, rect.w, rect.h, m[4], m[5], thickness, outward, x, y]
                    .into_iter()
                    .all(dyadic);
            let local_mesh = match &mesh {
                Some(mesh) if relative => mesh.local([x, y]).map(Some),
                Some(_) => None,
                None => Some(None),
            };
            // The layer's pixels depend on exactly these inputs; misses are rasterized with the
            // frame's other queued layers
            // The pixels depend on the transparency through its vertex alpha byte only
            let a8 = alpha8(alpha);
            let key = if let (true, Some(local_mesh)) = (relative, &local_mesh) {
                format!(
                    "stroke-local:{}",
                    serde_json::to_string(&(
                        [rect.x + m[4] - x, rect.y + m[5] - y, rect.w, rect.h],
                        radii,
                        limit,
                        thickness,
                        &position,
                        &joins,
                        [w, h],
                        scale,
                        outward,
                        rgb,
                        a8,
                        local_mesh
                    ))
                    .unwrap_or_default()
                )
            } else {
                format!(
                "stroke:{}",
                serde_json::to_string(&(
                    [rect.x, rect.y, rect.w, rect.h],
                    radii,
                    limit,
                    thickness,
                    &position,
                    &joins,
                    [x, y],
                    [w, h],
                    m,
                    rotated,
                    scale,
                    outward,
                    rgb,
                    a8,
                    &gradient
                ))
                .unwrap_or_default()
                )
            };
            let coverage = {
                let position = position.clone();
                let joins = joins.clone();
                move || {
                    let (coverage, ring) = super::coverage::border_stroke_alpha_modes(
                        rect,
                        radii,
                        limit,
                        thickness,
                        &position,
                        &joins,
                        [x as i32, y as i32],
                        [w, h],
                        invert(m),
                        rotated,
                        scale,
                    );
                    StrokeCoverage { coverage, ring }
                }
            };
            let coverage_key = if relative {
                format!(
                    "local:{}",
                    serde_json::to_string(&(
                        [rect.x + m[4] - x, rect.y + m[5] - y, rect.w, rect.h],
                        radii,
                        limit,
                        thickness,
                        &position,
                        &joins,
                        [w, h],
                        scale,
                        outward
                    ))
                    .unwrap_or_default()
                )
            } else {
                format!(
                    "absolute:{}",
                    serde_json::to_string(&(
                        [rect.x, rect.y, rect.w, rect.h],
                        radii,
                        limit,
                        thickness,
                        &position,
                        &joins,
                        [x, y],
                        [w, h],
                        m,
                        rotated,
                        scale,
                        outward
                    ))
                    .unwrap_or_default()
                )
            };
            let Some(mesh) = mesh else {
                // Without a gradient the layer is its coverage tinted with the colour and alpha byte: the
                // coverage is kept by the geometry (relative to the layer like the key), so a stroke whose
                // transparency or colour animates is tinted, not rasterized again. Its small tinted layers
                // are cached on probation (one-off tween steps must not push reused layers out).
                let raster: Box<dyn FnOnce() -> Vec<u8> + Send> = match self.stroke_coverage.get_mut(&coverage_key) {
                    Some((cached, used)) => {
                        *used = self.frame;
                        let cached = cached.clone();
                        Box::new(move || {
                            let bytes = tint_stroke(&cached, rgb, a8);
                            // RUR_VERIFY_LAYER_CACHE=1: the coverage kept is the coverage computed here
                            verify_layer_hit(&format!("stroke coverage {coverage_key}"), &bytes, Box::new(move || tint_stroke(&coverage(), rgb, a8)));
                            bytes
                        })
                    }
                    None => {
                        let slot = std::sync::Arc::new(std::sync::OnceLock::new());
                        self.pending_coverage.push((coverage_key, slot.clone()));
                        Box::new(move || {
                            let computed = std::sync::Arc::new(coverage());
                            let bytes = tint_stroke(&computed, rgb, a8);
                            let _ = slot.set(computed);
                            bytes
                        })
                    }
                };
                let probation = u64::from(w) * u64::from(h) <= 65536;
                let (.., id) = self.cached_layer_with(key, x as i32, y as i32, w, h, raster, probation);
                self.layer(ctx, Rect::new(x, y, w as f64, h as f64), id, node_clip, 0, &[]);
                continue;
            };
            // With one, the kept coverage too (the gradient animates: a rotating stroke's every frame is new),
            // shaded with the field where there is coverage (elsewhere the alpha byte is 0 whatever the field)
            let kept = match self.stroke_coverage.get_mut(&coverage_key) {
                Some((cached, used)) => {
                    *used = self.frame;
                    Ok(cached.clone())
                }
                None => {
                    let slot = std::sync::Arc::new(std::sync::OnceLock::new());
                    self.pending_coverage.push((coverage_key, slot.clone()));
                    Err(slot)
                }
            };
            let raster = move || {
                let computed = match kept {
                    Ok(cached) => cached,
                    Err(slot) => {
                        let computed = std::sync::Arc::new(coverage());
                        let _ = slot.set(computed.clone());
                        computed
                    }
                };
                let StrokeCoverage { coverage, ring } = &*computed;
                let field = mesh.field_where([x, y, x + w as f64, y + h as f64], |k| coverage[k] > 0.);
                let mut bytes = vec![0; w as usize * h as usize * 4];
                let shade = |k: usize, p: &mut [u8]| {
                    // No coverage: alpha byte 0, transparent black once premultiplied (left zero)
                    if coverage[k] <= 0. {
                        return;
                    }
                    // coverage::stroke_alpha_byte: the gradient's interpolated vertex alpha times coverage,
                    // per corner mode
                    p[3] = super::coverage::stroke_alpha_byte(coverage[k] as f64, field[k * 4 + 3] as f64, ring[k] != 0);
                    for c in 0..3 {
                        p[c] = gpu8(field[k * 4 + c] as f64 / 255.);
                    }
                };
                if super::sampling::parallel_rows(w as usize * h as usize) {
                    use rayon::prelude::*;
                    bytes.par_chunks_exact_mut(4).enumerate().for_each(|(k, p)| shade(k, p));
                } else {
                    for (k, p) in bytes.chunks_exact_mut(4).enumerate() {
                        shade(k, p);
                    }
                }
                premultiply(&mut bytes);
                bytes
            };
            // On probation: shading the kept coverage again is cheap, and an animated gradient's layers are one-offs
            let (.., id) = self.cached_layer_with(key, x as i32, y as i32, w, h, Box::new(raster), true);
            self.layer(
                ctx,
                Rect::new(x, y, w as f64, h as f64),
                id,
                node_clip,
                0,
                &[],
            );
        }
        Ok(())
    }
    pub(crate) fn draw_post(
        &mut self,
        ctx: &mut BuildContext,
        node: &Value,
        assets: &AssetStore,
    ) -> Result<(), DrawError> {
        let start = Instant::now();
        let _scope = crate::microprofile::scope("Post (text / images over)");
        if node["scroll"].is_object() {
            self.stats.scrollbars += 1;
        }
        let result = self.draw_post_inner(ctx, node, assets);
        self.stats.scrollbars_ms += start.elapsed().as_secs_f64() * 1000.;
        result
    }
    fn draw_post_inner(
        &mut self,
        ctx: &mut BuildContext,
        node: &Value,
        assets: &AssetStore,
    ) -> Result<(), DrawError> {
        let scroll = &node["scroll"];
        if !scroll.is_object() {
            return Ok(());
        }
        let t = (number(&scroll["thickness"], 0.) - 1e-6).ceil();
        if t <= 0. || alpha8(1. - number(&scroll["transparency"], 0.)) == 0 {
            return Ok(());
        }
        let [fx, fy, fw, fh] = std::array::from_fn(|i| component(&node["rect"], i, 0.));
        // The frame's border also runs between the window and each bar: a line BorderSizePixel thick in the
        // border's colour along the window's edge, across the whole frame (the window is that much smaller:
        // AbsoluteWindowSize is the frame less the bar and the border). Measured, Studio 0.747
        // (fixtures/layout-probe-scrollgap.luau: thickness 4 to 13, borders of 0, 1 and 3, a bar on the left,
        // one bar or both): none without a border.
        let border = number(&node["border"]["size"], 0.);
        let window: Option<[f64; 4]> = scroll["window"].is_array().then(|| std::array::from_fn(|i| component(&scroll["window"], i, 0.)));
        if let (true, Some([wx, wy, ww, wh]), true) = (border > 0., window, node["matrix"].is_null()) {
            let alpha = alpha8(1. - number(&node["bg"]["transparency"], 0.));
            let paint = solid_paint(color(&node["border"]["color"], [0.; 3]), alpha as f64 / 255.);
            let mut lines: Vec<[f64; 4]> = Vec::new();
            if scroll["vertical"] == true {
                let x = if scroll["verticalPosition"] == "Left" { wx - border } else { wx + ww };
                lines.push([x, fy, border, fh]);
            }
            if scroll["horizontal"] == true {
                lines.push([fx, wy + wh, fw, border]);
            }
            for rect in lines {
                let mut part = serde_json::json!({ "rect": rect });
                part["clip"] = node["clip"].clone();
                let p = physical_rect(&part, self.scale);
                let m = node_transform(&part, ctx.origin, self.scale);
                if alpha > 0 && p.snapped {
                    self.rect(ctx, p.rect.translated(m[4], m[5]), clip_rect(&part, ctx.origin, self.scale), paint, None);
                }
            }
        }
        for horizontal in [false, true] {
            if scroll[if horizontal { "horizontal" } else { "vertical" }] != true {
                continue;
            }
            let other = scroll[if horizontal { "vertical" } else { "horizontal" }] == true;
            let axis = if horizontal { 0 } else { 1 };
            // The thumb runs over the window's length (the frame less the other bar and the border beside it:
            // a bordered frame's thumb is that much shorter; the same probe)
            let visible = match window {
                Some(window) => window[axis + 2],
                None => (if horizontal { fw } else { fh }) - if other { t } else { 0. },
            };
            let canvas = component(&scroll["canvas"], axis + 2, 0.);
            // Measured, whole logical px: thumb = max(2t + 1, round(visible^2 / canvas)), offset
            // floor((visible - thumb) * CanvasPosition / (canvas - visible)) (fixtures/image-probe-scroll.luau:
            // e.g. 350 / 449 -> 273); a clamped thumb keeps a 1 logical px middle
            // (fixtures/image-probe-scroll-min.luau; max(2t, ...) missed every clamped bar)
            let thumb = visible.min((2. * t + 1.).max(snap(visible * (visible / canvas).min(1.))));
            let range = canvas - visible;
            let pos = component(
                &scroll["canvasPosition"],
                axis,
                component(&scroll["window"], axis, 0.) - component(&scroll["canvas"], axis, 0.),
            );
            let progress = if range > 0. {
                (pos / range).clamp(0., 1.)
            } else {
                0.
            };
            let offset = ((visible - thumb) * progress).floor();
            let x = if horizontal {
                fx + offset
            } else if scroll["verticalPosition"] == "Left" {
                fx
            } else {
                fx + fw - t
            };
            let y = if horizontal { fy + fh - t } else { fy + offset };
            let middle = (thumb - 2. * t).max(0.);
            for (key, start, length) in [
                ("topImage", 0., t),
                ("midImage", t, middle),
                ("bottomImage", thumb - t, t),
            ] {
                let Some(uri) = scroll[key].as_str() else {
                    continue;
                };
                if length <= 0. {
                    continue;
                }
                // Horizontal parts: the vertical artwork on a quad turned a quarter turn counter-clockwise
                // about its centre (artwork top -> left), drawn like a rotated ImageLabel: the unsnapped quad,
                // pixel centres on its left / top edge in and on its right / bottom edge out (ties round down: a
                // thumb ending at 499.5 device px ends at column 499; fixtures/image-probe-hscroll.luau)
                let (rect, turn) = if horizontal {
                    let (px, py, pw, ph) = (x + start, y, length, t);
                    let (cx, cy) = (px + pw / 2., py + ph / 2.);
                    (
                        [cx - ph / 2., cy - pw / 2., ph, pw],
                        Some([0., -1., 1., 0., cx - cy, cx + cy]),
                    )
                } else {
                    ([x, y + start, t, length], None)
                };
                // The @2x file of a built-in texture when it exists, at 1x DPI too (notes/images.md: the 1x
                // caps left 3-8 level differences on every thumb end)
                let hi =
                    if uri.starts_with("rbxasset://") && uri.ends_with(".png") {
                        Some(format!("{}@2x.png", &uri[..uri.len() - 4]))
                    } else {
                        None
                    };
                let (source, density) =
                    if let Some(asset) = hi.as_deref().and_then(|u| assets.image(u)) {
                        (asset, 2.)
                    } else if let Some(asset) = assets.image(uri) {
                        (asset, 1.)
                    } else {
                        continue;
                    };
                let mut part = serde_json::json!({"rect":rect,"image":{"uri":source.id.0,"density":density,"color":scroll["color"],"transparency":scroll["transparency"],"scaleType":"Stretch"}});
                part["clip"] = node["clip"].clone();
                part["matrix"] = node["matrix"].clone();
                if let Some(turn) = turn {
                    let base: Transform = if node["matrix"].is_array() {
                        std::array::from_fn(|i| component(&node["matrix"], i, IDENTITY[i]))
                    } else {
                        IDENTITY
                    };
                    part["matrix"] = serde_json::json!(multiply(base, turn));
                }
                let p = physical_rect(&part, self.scale);
                let r = p.rect;
                let ir = super::images::ImageRect {
                    x: r.x,
                    y: r.y,
                    w: r.w,
                    h: r.h,
                    raw: [p.raw.x, p.raw.y, p.raw.w, p.raw.h],
                    snapped: p.snapped,
                };
                if let Some(layer) = super::images::prepare_image(
                    &part,
                    &source,
                    ir,
                    [0.; 4],
                    node_transform(&part, ctx.origin, self.scale),
                    self.scale,
                    &mut self.image_cache,
                ) {
                    let id = self.upload(
                        layer.width,
                        layer.height,
                        layer.rgba.to_vec(),
                        TextureFormat::Rgba8,
                    );
                    let (tx, ty) = if p.snapped {
                        (-ctx.origin[0], -ctx.origin[1])
                    } else {
                        (0., 0.)
                    };
                    self.image_layer(
                        ctx,
                        Rect::new(
                            layer.x as f64 + tx,
                            layer.y as f64 + ty,
                            layer.width as f64,
                            layer.height as f64,
                        ),
                        id,
                        clip_rect(&part, ctx.origin, self.scale),
                    );
                }
            }
        }
        Ok(())
    }
}

/// No clip (the shaders' NO_CLIP rect)
const NO_CLIP_RECT: [f64; 4] = [-1e9, -1e9, 1e9, 1e9];

/// A solid (rounded) fill's premultiplied layer: pixels [x0, x0 + w) x [y0, y0 + h) of a `size` rect whose
/// coverage is corner_coverage_region_limited's (1 without radii), each pixel (colour, gpu8(a / 255 x
/// coverage)) premultiplied. Coverage is exactly 1 more than the largest radius + 2 px inside every edge
/// (the signed distance there is below -1.25): those pixels are one repeated value, and the coverage is
/// computed along the edges only.
fn solid_fill_pixels(
    size: [u32; 2],
    radii: [f64; 4],
    limit: f64,
    origin: [i32; 2],
    at: [i32; 2],
    extent: [u32; 2],
    colour: [f64; 3],
    a: u8,
) -> Vec<u8> {
    let rgb = colour.map(quantize);
    let pixel = |coverage: f64| {
        let alpha = gpu8(a as f64 / 255. * coverage);
        [mul_div255(rgb[0], alpha), mul_div255(rgb[1], alpha), mul_div255(rgb[2], alpha), alpha]
    };
    let [w, h] = extent;
    let mut bytes = pixel(1.).repeat(w as usize * h as usize);
    if !radii.iter().any(|v| *v > 0.) || w == 0 || h == 0 {
        return bytes;
    }
    let [rw, rh] = size.map(|v| v as i32);
    let band = radii.iter().fold(0f64, |m, r| m.max(*r)).ceil().min(1e6) as i32 + 3;
    // Rect rows [0, band) and [rh - band, rh), and the columns [0, band), [rw - band, rw) of the rows between
    let top = band.min(rh);
    let bottom = (rh - band).max(top);
    let left = band.min(rw);
    let right = (rw - band).max(left);
    let regions = [
        (0, 0, rw, top),
        (0, bottom, rw, rh),
        (0, top, left, bottom),
        (right, top, rw, bottom),
    ];
    let [x0, y0] = at;
    let (x1, y1) = (x0 + w as i32, y0 + h as i32);
    for (l, t, r, b) in regions {
        let (l, t, r, b) = (l.max(x0), t.max(y0), r.min(x1), b.min(y1));
        if l >= r || t >= b {
            continue;
        }
        let coverage = super::coverage::corner_coverage_region_limited(
            size[0],
            size[1],
            radii,
            limit,
            origin[0],
            origin[1],
            l,
            t,
            (r - l) as u32,
            (b - t) as u32,
        );
        let pw = (r - l) as usize;
        for (row, line) in coverage.chunks_exact(pw).enumerate() {
            let y = (t - y0) as usize + row;
            for (column, value) in line.iter().enumerate() {
                let x = (l - x0) as usize + column;
                let k = (y * w as usize + x) * 4;
                bytes[k..k + 4].copy_from_slice(&pixel(*value));
            }
        }
    }
    bytes
}

/// The texture dedupe key of a layer's bytes (upload / run_pending; equal keys are confirmed on the bytes)
fn content_hash(width: u32, height: u32, bytes: &[u8]) -> u64 {
    let mut hasher = rustc_hash::FxHasher::default();
    hasher.write_u32(width);
    hasher.write_u32(height);
    hasher.write_usize(bytes.len());
    let mut words = bytes.chunks_exact(8);
    for word in &mut words {
        hasher.write_u64(u64::from_le_bytes(word.try_into().unwrap()));
    }
    for byte in words.remainder() {
        hasher.write_u8(*byte);
    }
    hasher.finish()
}

/// Whether an unrotated node on the frame target draws nothing because everything it can draw lies
/// outside its clip (or the target): every record draw_node_uncached makes for it is clipped by
/// clip_rect(node) (text by text_clip_rect, inside it) and lies within node_extent. False when unsure.
fn node_offscreen(node: &Value, scale: f64, width: u32, height: u32) -> bool {
    if node["matrix"].is_array() || node["selection"].is_object() || node["caret"].is_object() {
        return false;
    }
    let (w, h) = (width as f64, height as f64);
    let c = clip_rect(node, [0., 0.], scale).unwrap_or([0., 0., w, h]);
    let c = [c[0].max(0.), c[1].max(0.), c[2].min(w), c[3].min(h)];
    if c[2] <= c[0] || c[3] <= c[1] {
        return true;
    }
    // Most nodes are plainly visible: their own rect meets the clip
    let v = &node["rect"];
    let [x, y, rw, rh] = std::array::from_fn(|i| component(v, i, 0.) * scale);
    let [x0, x1] = [x.min(x + rw), x.max(x + rw)];
    let [y0, y1] = [y.min(y + rh), y.max(y + rh)];
    if x1 > c[0] && x0 < c[2] && y1 > c[1] && y0 < c[3] {
        return false;
    }
    let Some(e) = node_extent(node, scale) else {
        return false;
    };
    e[2] <= c[0] || e[0] >= c[2] || e[3] <= c[1] || e[1] >= c[3]
}
/// A generous bound (device px) on what draw_node_uncached draws for an unrotated node: its rect (and
/// absolute position) grown by its border, strokes, shadows and anti-aliasing, and its text lines grown by
/// three glyph sizes and their strokes. None when a value is not finite.
fn node_extent(node: &Value, scale: f64) -> Option<[f64; 4]> {
    let v = &node["rect"];
    let [x, y, rw, rh] = std::array::from_fn(|i| component(v, i, 0.));
    let dev = |a: f64, b: f64| {
        let (p, q) = (a * scale, (a + b) * scale);
        [p.min(q), p.max(q)]
    };
    let [x0, x1] = dev(x, rw);
    let [y0, y1] = dev(y, rh);
    let mut e = [x0, y0, x1, y1];
    let mut union = |b: [f64; 4]| {
        e = [e[0].min(b[0]), e[1].min(b[1]), e[2].max(b[2]), e[3].max(b[3])];
    };
    if let Some(a) = node["absolute"].as_array().filter(|a| a.len() >= 2) {
        let [ax0, ax1] = dev(number(&a[0], x), rw);
        let [ay0, ay1] = dev(number(&a[1], y), rh);
        union([ax0, ay0, ax1, ay1]);
    }
    let r = physical_rect(node, scale).rect;
    union([r.x.min(r.x + r.w), r.y.min(r.y + r.h), r.x.max(r.x + r.w), r.y.max(r.y + r.h)]);
    let mut pad: f64 = 4. + number(&node["border"]["size"], 0.).abs() * scale;
    let mut stroke_pad: f64 = 0.;
    for stroke in array(&node["strokes"]) {
        let grow = (number(&stroke["thickness"], 0.).abs() + number(&stroke["borderOffset"], 0.).abs()) * scale;
        stroke_pad = stroke_pad.max(grow);
    }
    pad += stroke_pad + 4.;
    for shadow in array(&node["shadows"]) {
        let sigma = number(&shadow["blur"], 0.).abs() * scale / 2.;
        let reach = (component(&shadow["offset"], 0, 0.).abs()
            + component(&shadow["offset"], 1, 0.).abs()
            + component(&shadow["spread"], 0, 0.).abs()
            + component(&shadow["spread"], 1, 0.).abs())
            * scale
            + 3. * sigma
            + 8.;
        pad = pad.max(reach + 8.);
    }
    let text = &node["text"];
    if text.is_object() {
        let text_pad = 8. + stroke_pad + 2. * number(&text["stroke"]["thickness"], 1.).abs().max(1.) * scale;
        let mut boxes = Vec::new();
        if let Some(c) = text["content"].as_array().filter(|c| c.len() >= 4) {
            let (cx, cy) = (number(&c[0], 0.), number(&c[1], 0.));
            let (cw, ch) = (number(&c[2], 0.), number(&c[3], 0.));
            boxes.push([cx.min(cx + cw), cy.min(cy + ch), cx.max(cx + cw), cy.max(cy + ch), text_pad]);
        }
        for line in array(&text["lines"]) {
            let mut ppem: f64 = 0.;
            let mut reach: f64 = 0.;
            let mut back: f64 = 0.;
            for glyph in array(&line["glyphs"]) {
                ppem = ppem.max(component(glyph, 3, 0.).abs());
                reach = reach.max(component(glyph, 2, 0.));
                back = back.min(component(glyph, 2, 0.));
            }
            let xs = ["x", "px", "ux"].map(|k| line.get(k).and_then(Value::as_f64));
            let left = xs.iter().flatten().fold(f64::INFINITY, |a, b| a.min(*b));
            let right = xs.iter().flatten().fold(f64::NEG_INFINITY, |a, b| a.max(*b));
            if !left.is_finite() {
                return None;
            }
            let top = number(&line["top"], 0.);
            let lw = number(&line["width"], 0.).abs().max(reach);
            let lh = number(&line["height"], 0.).abs();
            boxes.push([left + back, top, right + lw, top + lh, text_pad + 3. * ppem]);
        }
        for [bx0, by0, bx1, by1, grow] in boxes {
            union([bx0 - grow, by0 - grow, bx1 + grow, by1 + grow]);
        }
    }
    let e = [e[0] - pad, e[1] - pad, e[2] + pad, e[3] + pad];
    e.iter().all(|v| v.is_finite()).then_some(e)
}

/// Whether a UIShadow is drawn inside its frame: Inset, which the glow of Mode = Text does not take
fn inset_shadow(shadow: &Value) -> bool {
    shadow["inset"] == true && shadow["mode"] != "Text"
}

/// A shadow's layer box [x0, y0, x1, y1) in device pixels: the spread rect about its offset centre padded by
/// 3 sigma (sigma = BlurRadius * DPI / 2, the profile's [-3, 3] sigma span), whole pixels outward; None when
/// the shadow draws nothing
fn shadow_layer_box(r: Rect, shadow: &Value, scale: f64) -> Option<[f64; 4]> {
    let alpha = 1. - number(&shadow["transparency"], 0.);
    if alpha <= 0. || inset_shadow(shadow) || shadow["mode"] == "Text" {
        return None;
    }
    let sigma = (number(&shadow["blur"], 0.) * scale / 2.).max(0.);
    let w = r.w + component(&shadow["spread"], 0, 0.) * scale;
    let h = r.h + component(&shadow["spread"], 1, 0.) * scale;
    if !(w > 0. && h > 0.) {
        return None;
    }
    if sigma > 0. && super::coverage::shadow_vertex_alpha(w / 2., h / 2., sigma, alpha) == 0 {
        return None;
    }
    let cx = r.x + r.w / 2. + component(&shadow["offset"], 0, 0.) * scale;
    let cy = r.y + r.h / 2. + component(&shadow["offset"], 1, 0.) * scale;
    let pad = 3. * sigma;
    let b = [(cx - w / 2. - pad).floor(), (cy - h / 2. - pad).floor(), (cx + w / 2. + pad).ceil(), (cy + h / 2. + pad).ceil()];
    (b[2] > b[0] && b[3] > b[1]).then_some(b)
}

/// The screen-level shadows whose layer box meets two earlier ones' boxes where those also meet each other,
/// each with the box around those triple overlaps (blended straight there, BlendMode::Image: three overlapping
/// halos drift 3-4 levels under the premultiplied blend, shadow-probe-p)
fn exact_shadow_boxes(scene: &SceneStore, scale: f64) -> HashMap<(u64, usize), [f64; 4]> {
    let mut exact = HashMap::new();
    let mut boxes: Vec<[f64; 4]> = Vec::new();
    let meet = |a: [f64; 4], b: [f64; 4]| [a[0].max(b[0]), a[1].max(b[1]), a[2].min(b[2]), a[3].min(b[3])];
    let empty = |b: [f64; 4]| b[0] >= b[2] || b[1] >= b[3];
    let mut visit = |node: &Value, group: bool| {
        let shadows = array(&node["shadows"]);
        if node["culled"] == true || node["matrix"].is_array() || shadows.is_empty() {
            return;
        }
        let rect = physical_rect(node, scale).rect;
        let id = node["id"].as_u64().unwrap_or(0);
        let mut list: Vec<_> = shadows.iter().enumerate().collect();
        if !group {
            list.sort_by(|a, b| number(&a.1["zIndex"], 0.).total_cmp(&number(&b.1["zIndex"], 0.)));
        }
        for (index, shadow) in list {
            let Some(b) = shadow_layer_box(rect, shadow, scale) else {
                continue;
            };
            let hits: Vec<_> = boxes.iter().map(|o| meet(*o, b)).filter(|h| !empty(*h)).collect();
            let mut around: Option<[f64; 4]> = None;
            for i in 0..hits.len() {
                for j in i + 1..hits.len() {
                    let t = meet(hits[i], hits[j]);
                    if empty(t) {
                        continue;
                    }
                    around = Some(match around {
                        Some(a) => [a[0].min(t[0]), a[1].min(t[1]), a[2].max(t[2]), a[3].max(t[3])],
                        None => t,
                    });
                }
            }
            if let Some(a) = around {
                exact.insert((id, index), a);
            }
            boxes.push(b);
        }
    };
    let mut layers: Vec<_> = array(scene.layers()).iter().collect();
    layers.sort_by(|a, b| layer_pass(a).cmp(&layer_pass(b)).then(number(&a["displayOrder"], 0.).total_cmp(&number(&b["displayOrder"], 0.))));
    for layer in layers {
        for entry in array(&layer["draw"]) {
            if entry.is_object() {
                if let Some(node) = crate::scene::node_id(&entry["group"]).and_then(|id| scene.node(id)) {
                    visit(node, true);
                }
            } else if let Some(id) = entry.as_i64().filter(|id| *id > 0) {
                if let Some(node) = scene.node(id as u64) {
                    visit(node, false);
                }
            }
        }
    }
    exact
}
