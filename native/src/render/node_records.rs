//! Retained leaf-node packets, equivalent to GpuRenderer.nodeRecords / stillValid.
//! Pixel resources remain owned by DrawPreparer; every hit revalidates and touches them.
use super::packet::{DrawBatch, TextureId};
use serde_json::Value;
use rustc_hash::FxHashMap;
use std::sync::{Arc, Weak};

const BYTE_BUDGET: usize = 96 * 1024 * 1024;
const ENTRY_BUDGET: usize = 8192;

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum NodePart {
    All,
    Under,
    Image,
    Over,
}
impl NodePart {
    pub fn from_str(part: &str) -> Option<Self> {
        match part {
            "all" => Some(Self::All),
            "under" => Some(Self::Under),
            "image" => Some(Self::Image),
            "over" => Some(Self::Over),
            _ => None,
        }
    }
}

/// Whether a node's records for `part` depend on the assets (images, glyph pages). Matches
/// draw_node_uncached's actual branches, including empty packets prepared while a glyph or image is
/// still loading.
pub fn asset_dependent(node: &Value, part: NodePart) -> bool {
    match part {
        NodePart::All => node["image"].is_object() || node["text"].is_object(),
        NodePart::Image => node["image"].is_object(),
        NodePart::Over => node["text"].is_object(),
        NodePart::Under => false,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RecordContext {
    pub origin: [f64; 2],
    pub target_size: [u32; 2],
    pub scale: f64,
    pub in_group: bool,
    pub opaque: bool,
    pub part: NodePart,
    pub space: Option<[f64; 6]>,
    /// Hash of the node's exactly blended shadow boxes (DrawPreparer::exact_shadows), 0 without any
    pub shadows: u64,
}
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    pointer: usize,
    origin: [u64; 2],
    target_size: [u32; 2],
    scale: u64,
    in_group: bool,
    opaque: bool,
    part: NodePart,
    space: Option<[u64; 6]>,
    shadows: u64,
}
impl RecordContext {
    fn key(self, node: &Arc<Value>) -> Key {
        Key {
            pointer: Arc::as_ptr(node) as usize,
            origin: self.origin.map(f64::to_bits),
            target_size: self.target_size,
            scale: self.scale.to_bits(),
            in_group: self.in_group,
            opaque: self.opaque,
            part: self.part,
            space: self.space.map(|s| s.map(f64::to_bits)),
            shadows: self.shadows,
        }
    }
}

#[derive(Clone, Debug)]
pub struct RecordedNode {
    /// Record-local offsets inside each header stay unchanged when this block moves.
    pub texels: Vec<[f32; 4]>,
    pub batches: Vec<DrawBatch>,
    pub texture_ids: Vec<TextureId>,
    bytes: usize,
}
impl RecordedNode {
    /// `batches` must come from an isolated scratch BuildContext, never the preceding node's
    /// merged batch. `base` is the original start of the supplied contiguous texel slice.
    pub fn capture(texels: &[[f32; 4]], batches: &[DrawBatch], base: u32) -> Result<Self, String> {
        let end = (base as usize)
            .checked_add(texels.len())
            .ok_or("record block extent overflow")?;
        let mut batches = batches.to_vec();
        let mut texture_ids = Vec::new();
        for batch in &mut batches {
            for offset in &mut batch.record_offsets {
                if *offset < base || (*offset as usize).checked_add(3).is_none_or(|v| v >= end) {
                    return Err("record batch references outside the captured node".into());
                }
                *offset -= base;
            }
            texture_ids.extend(batch.image_slots.iter().copied().flatten());
            texture_ids.extend([batch.glyphs, batch.layer, batch.lut].into_iter().flatten());
        }
        texture_ids.sort_by_key(|id| id.0);
        texture_ids.dedup();
        let texels = texels.to_vec();
        let bytes = texels.capacity() * std::mem::size_of::<[f32; 4]>()
            + batches.capacity() * std::mem::size_of::<DrawBatch>()
            + batches
                .iter()
                .map(|b| b.record_offsets.capacity() * std::mem::size_of::<u32>())
                .sum::<usize>()
            + texture_ids.capacity() * std::mem::size_of::<TextureId>()
            + 256;
        Ok(Self {
            texels,
            batches,
            texture_ids,
            bytes,
        })
    }
    pub fn bytes(&self) -> usize {
        self.bytes
    }
    /// Rebase only batch record offsets. Shader block offsets are relative to their record.
    pub fn append_to(&self, texels: &mut Vec<[f32; 4]>, batches: &mut Vec<DrawBatch>) {
        let base =
            u32::try_from(texels.len()).expect("draw record arena exceeds u32 address space");
        texels.extend_from_slice(&self.texels);
        let rebased = |offset: &u32| {
            offset
                .checked_add(base)
                .expect("draw record offset overflow")
        };
        for batch in &self.batches {
            if let Some(last) = batches.last_mut() {
                if compatible(last, batch) {
                    last.record_offsets.extend(batch.record_offsets.iter().map(rebased));
                    continue;
                }
            }
            batches.push(DrawBatch {
                record_offsets: batch.record_offsets.iter().map(rebased).collect(),
                image_slots: batch.image_slots,
                glyphs: batch.glyphs,
                layer: batch.layer,
                lut: batch.lut,
                blend: batch.blend,
                scissor: batch.scissor,
            });
        }
    }
}
fn compatible(a: &DrawBatch, b: &DrawBatch) -> bool {
    a.image_slots == b.image_slots
        && a.glyphs == b.glyphs
        && a.layer == b.layer
        && a.lut == b.lut
        && a.blend == b.blend
        && a.scissor == b.scissor
}
struct Entry {
    node: Weak<Value>,
    records: Arc<RecordedNode>,
    used: u64,
    asset_dependent: bool,
}
/// Least recently used entries go first once a budget is exceeded (down to 7/8 of it, so eviction
/// runs rarely; a hit only stamps its entry).
pub struct NodeRecordCache {
    entries: FxHashMap<Key, Entry>,
    fonts: Value,
    asset_revision: Option<u64>,
    frame: u64,
    bytes: usize,
    pub hits: u64,
    pub misses: u64,
}
impl Default for NodeRecordCache {
    fn default() -> Self {
        Self {
            entries: FxHashMap::default(),
            fonts: Value::Null,
            asset_revision: None,
            frame: 0,
            bytes: 0,
            hits: 0,
            misses: 0,
        }
    }
}
impl NodeRecordCache {
    pub fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
    }
    pub fn bytes(&self) -> usize {
        self.bytes
    }
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    /// Asset revision must advance for replacement, eviction and glyph arrival, including
    /// same-sized content. A byte count is not a valid asset generation identifier.
    /// Returns whether the assets changed (asset-dependent records were dropped)
    pub fn begin_frame(&mut self, fonts: &Value, asset_revision: u64) -> bool {
        let assets_changed = self.asset_revision != Some(asset_revision) || self.fonts != *fonts;
        // Moving scene nodes die in cohorts. Sweep them out once the cache is half full (rather than
        // every frame: a dead entry never hits, its Weak keeps its address from being reused)
        if !assets_changed && self.entries.len() <= ENTRY_BUDGET / 2 {
            return false;
        }
        self.entries.retain(|_, entry| {
            if entry.node.strong_count() == 0 || (assets_changed && entry.asset_dependent) {
                self.bytes -= entry.records.bytes();
                false
            } else {
                true
            }
        });
        if assets_changed {
            self.fonts = fonts.clone();
            self.asset_revision = Some(asset_revision);
        }
        assets_changed
    }
    pub fn get(
        &mut self,
        node: &Arc<Value>,
        context: &RecordContext,
        mut touch: impl FnMut(TextureId) -> bool,
    ) -> Option<Arc<RecordedNode>> {
        let key = context.key(node);
        let Some(entry) = self.entries.get_mut(&key) else {
            self.misses += 1;
            return None;
        };
        // The pointer is only a key while the cached node lives (a dead node's address can be reused)
        let valid = std::ptr::eq(entry.node.as_ptr(), Arc::as_ptr(node))
            && entry.node.strong_count() > 0
            && entry.records.texture_ids.iter().copied().all(&mut touch);
        if !valid {
            let old = self.entries.remove(&key).unwrap();
            self.bytes -= old.records.bytes();
            self.misses += 1;
            return None;
        }
        self.frame += 1;
        entry.used = self.frame;
        self.hits += 1;
        Some(entry.records.clone())
    }
    pub fn insert(
        &mut self,
        node: &Arc<Value>,
        context: &RecordContext,
        records: Arc<RecordedNode>,
    ) {
        let key = context.key(node);
        if let Some(old) = self.entries.remove(&key) {
            self.bytes -= old.records.bytes();
        }
        if records.bytes() > BYTE_BUDGET {
            return;
        }
        if self.bytes + records.bytes() > BYTE_BUDGET || self.entries.len() >= ENTRY_BUDGET {
            // Dead nodes first, then the least recently used down to half the budgets
            let bytes = &mut self.bytes;
            self.entries.retain(|_, entry| {
                let live = entry.node.strong_count() > 0;
                if !live {
                    *bytes -= entry.records.bytes();
                }
                live
            });
            if self.bytes + records.bytes() > BYTE_BUDGET / 2 || self.entries.len() >= ENTRY_BUDGET / 2 {
                let mut order: Vec<(u64, Key)> = self.entries.iter().map(|(k, e)| (e.used, *k)).collect();
                order.sort_unstable_by_key(|(used, _)| *used);
                for (_, oldest) in order {
                    if self.bytes + records.bytes() <= BYTE_BUDGET / 2
                        && self.entries.len() < ENTRY_BUDGET / 2
                    {
                        break;
                    }
                    self.bytes -= self.entries.remove(&oldest).unwrap().records.bytes();
                }
            }
        }
        self.bytes += records.bytes();
        self.frame += 1;
        self.entries.insert(
            key,
            Entry {
                node: Arc::downgrade(node),
                records,
                used: self.frame,
                asset_dependent: asset_dependent(node, context.part),
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn context() -> RecordContext {
        RecordContext {
            origin: [0.0; 2],
            target_size: [800, 600],
            scale: 1.5,
            in_group: false,
            opaque: false,
            part: NodePart::All,
            space: None,
            shadows: 0,
        }
    }
    fn records(id: u64) -> Arc<RecordedNode> {
        let mut batch = DrawBatch::default();
        batch.record_offsets.push(64);
        batch.image_slots[0] = Some(TextureId(id));
        Arc::new(RecordedNode::capture(&[[1.0, 2.0, 3.0, 4.0]; 6], &[batch], 64).unwrap())
    }
    #[test]
    fn capture_rebases_only_batch_offsets() {
        let record = records(9);
        let mut texels = vec![[0.0; 4]; 100];
        let mut batches = Vec::new();
        record.append_to(&mut texels, &mut batches);
        assert_eq!(batches[0].record_offsets, [100]);
        assert_eq!(&texels[100..], &record.texels);
        record.append_to(&mut texels, &mut batches);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].record_offsets, [100, 106]);
        let bad = DrawBatch {
            record_offsets: vec![63],
            ..Default::default()
        };
        assert!(RecordedNode::capture(&[[0.0; 4]; 6], &[bad], 64).is_err());
    }
    #[test]
    fn invalidation_tracks_context_identity_and_exact_assets() {
        let node = Arc::new(serde_json::json!({"id":1,"text":{}}));
        let same_value = Arc::new((*node).clone());
        let fonts = serde_json::json!([{"file":"font.ttf"}]);
        let mut cache = NodeRecordCache::default();
        let ctx = context();
        cache.begin_frame(&fonts, 1);
        cache.insert(&node, &ctx, records(9));
        assert!(cache.get(&node, &ctx, |_| true).is_some());
        assert!(cache.get(&same_value, &ctx, |_| true).is_none());
        for changed in [
            RecordContext {
                origin: [1.0, 0.0],
                ..ctx
            },
            RecordContext { scale: 2.0, ..ctx },
            RecordContext {
                part: NodePart::Over,
                ..ctx
            },
            RecordContext {
                target_size: [900, 600],
                ..ctx
            },
            RecordContext {
                in_group: true,
                ..ctx
            },
            RecordContext {
                space: Some([1.0; 6]),
                ..ctx
            },
        ] {
            assert!(cache.get(&node, &changed, |_| true).is_none());
        }
        cache.begin_frame(&fonts, 2);
        assert!(cache.is_empty());
        cache.insert(&node, &ctx, records(9));
        cache.begin_frame(&serde_json::json!([{"file":"other.ttf"}]), 2);
        assert!(cache.is_empty());
    }
    #[test]
    fn asset_arrival_preserves_shapes_and_under_records() {
        let shape = Arc::new(serde_json::json!({"id":1}));
        let mixed = Arc::new(serde_json::json!({"id":2,"text":{},"image":{}}));
        let image = Arc::new(serde_json::json!({"id":3,"image":{}}));
        let mut cache = NodeRecordCache::default();
        let all = context();
        let under = RecordContext {
            part: NodePart::Under,
            ..all
        };
        let over = RecordContext {
            part: NodePart::Over,
            ..all
        };
        let image_part = RecordContext {
            part: NodePart::Image,
            ..all
        };
        for revision in 1..=3 {
            cache.begin_frame(&serde_json::json!([revision]), revision);
            if revision > 1 {
                assert_eq!(cache.len(), 3);
                assert_eq!(cache.bytes(), records(9).bytes() * 3);
                for (node, ctx) in [(&shape, &all), (&mixed, &under), (&image, &over)] {
                    assert!(cache.get(node, ctx, |_| true).is_some());
                }
                for ctx in [&all, &over, &image_part] {
                    assert!(cache.get(&mixed, ctx, |_| true).is_none());
                }
            }
            for (node, ctx) in [
                (&shape, &all),
                (&mixed, &under),
                (&image, &over),
                (&mixed, &all),
                (&mixed, &over),
                (&mixed, &image_part),
            ] {
                cache.insert(node, ctx, records(9));
            }
        }
        assert!(cache.get(&shape, &all, |_| false).is_none());
    }
    #[test]
    fn retired_textures_invalidate_and_hits_touch_dependencies() {
        let node = Arc::new(serde_json::json!({"id":1}));
        let mut cache = NodeRecordCache::default();
        cache.begin_frame(&Value::Null, 1);
        let ctx = context();
        cache.insert(&node, &ctx, records(9));
        let mut touched = Vec::new();
        assert!(
            cache
                .get(&node, &ctx, |id| {
                    touched.push(id);
                    true
                })
                .is_some()
        );
        assert_eq!(touched, [TextureId(9)]);
        assert!(cache.get(&node, &ctx, |_| false).is_none());
        assert!(cache.is_empty());
        assert_eq!(cache.bytes(), 0);
    }
    #[test]
    fn cache_does_not_retain_scene_nodes_or_exceed_entry_limit() {
        let mut cache = NodeRecordCache::default();
        cache.begin_frame(&Value::Null, 1);
        let ctx = context();
        let record = records(1);
        let mut nodes = Vec::new();
        for id in 0..ENTRY_BUDGET + 1 {
            cache.begin_frame(&Value::Null, 1);
            let node = Arc::new(serde_json::json!({"id":id}));
            cache.insert(&node, &ctx, record.clone());
            nodes.push(node);
        }
        assert!(cache.len() <= ENTRY_BUDGET);
        assert!(cache.get(&nodes[0], &ctx, |_| true).is_none());
        assert!(cache.get(nodes.last().unwrap(), &ctx, |_| true).is_some());
        assert_eq!(Arc::strong_count(nodes.last().unwrap()), 1);
        drop(nodes);
        cache.insert(&Arc::new(Value::Null), &ctx, record);
        assert!(cache.bytes() <= BYTE_BUDGET);
    }
    #[test]
    fn ordered_eviction_preserves_recent_hits_and_prunes_dead_nodes() {
        let mut cache = NodeRecordCache::default();
        let ctx = context();
        let record = records(1);
        let nodes: Vec<_> = (0..ENTRY_BUDGET)
            .map(|id| Arc::new(serde_json::json!({"id":id})))
            .collect();
        for node in &nodes {
            cache.insert(node, &ctx, record.clone());
        }
        assert!(cache.get(&nodes[0], &ctx, |_| true).is_some());
        let new = Arc::new(serde_json::json!({"id":"new"}));
        cache.insert(&new, &ctx, record.clone());
        assert!(cache.get(&nodes[0], &ctx, |_| true).is_some());
        assert!(cache.get(&nodes[1], &ctx, |_| true).is_none());
        drop(nodes);
        cache.begin_frame(&Value::Null, 0);
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.bytes(), record.bytes());
        cache.clear();
        assert!(cache.is_empty());
    }
}
