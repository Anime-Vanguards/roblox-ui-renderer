//! Retained authoritative scene packets. Unknown export fields remain intact.
use serde_json::{Map, Value};
use std::{borrow::Cow, collections::BTreeMap, sync::Arc};

pub type NodeId = u64;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    pub width: f64,
    pub height: f64,
    pub scale: f64,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            width: 1200.0,
            height: 900.0,
            scale: 1.5,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ApplyOutcome {
    pub changed: bool,
    pub reset: bool,
    pub changed_nodes: Vec<NodeId>,
    pub removed_nodes: Vec<NodeId>,
    pub metadata_changed: bool,
}

#[derive(Clone, Debug)]
pub struct SceneStore {
    nodes: Arc<BTreeMap<NodeId, Arc<Value>>>,
    order: Arc<Vec<NodeId>>,
    header: Arc<Map<String, Value>>,
    layers: Arc<Value>,
    fonts: Arc<Value>,
    images: Arc<Vec<Value>>,
    pub revision: u64,
}

impl Default for SceneStore {
    fn default() -> Self {
        Self {
            nodes: Arc::new(BTreeMap::new()),
            order: Arc::new(Vec::new()),
            header: Arc::new(Map::new()),
            layers: Arc::new(Value::Array(Vec::new())),
            fonts: Arc::new(Value::Array(Vec::new())),
            images: Arc::new(Vec::new()),
            revision: 0,
        }
    }
}

/// Drops replaced node values on a background thread (freeing a scroll frame's moved glyph runs
/// costs about as much as building them); inline when the thread cannot be started.
fn retire(values: Vec<Value>) {
    use std::sync::{Mutex, OnceLock, mpsc};
    static DROPPER: OnceLock<Option<Mutex<mpsc::Sender<Vec<Value>>>>> = OnceLock::new();
    if values.is_empty() {
        return;
    }
    let sender = DROPPER.get_or_init(|| {
        let (sender, receiver) = mpsc::channel::<Vec<Value>>();
        std::thread::Builder::new()
            .name("scene-dropper".into())
            .spawn(move || while let Ok(values) = receiver.recv() { drop(values) })
            .ok()
            .map(|_| Mutex::new(sender))
    });
    if let Some(sender) = sender {
        if let Ok(sender) = sender.lock() {
            let _ = sender.send(values);
        }
    }
}

/// A runtime/session.luau NodePatch rather than a whole node
fn is_patch(node: &Value) -> bool {
    node.get("patch").and_then(Value::as_bool) == Some(true)
}

/// Luau's serializer emits `{}` for an empty array.
pub(crate) fn array(value: &Value) -> Result<&[Value], String> {
    match value {
        Value::Array(items) => Ok(items),
        Value::Object(items) if items.is_empty() => Ok(&[]),
        _ => Err("expected an array or empty Luau table".into()),
    }
}

/// A layer's pass: Roblox draws the PlayerGui pass (`playerGui: true`, exported by `--all-layers`)
/// under CoreGui's, whatever their DisplayOrders; layers sort by pass, then DisplayOrder.
pub(crate) fn layer_pass(layer: &Value) -> u8 {
    if layer.get("playerGui").and_then(Value::as_bool) == Some(true) { 0 } else { 1 }
}

pub(crate) fn node_id(value: &Value) -> Option<NodeId> {
    value.as_u64().or_else(|| {
        value
            .as_f64()
            .filter(|n| {
                n.is_finite() && *n >= 0.0 && n.fract() == 0.0 && *n <= 9_007_199_254_740_991.0
            })
            .map(|n| n as u64)
    })
}

impl SceneStore {
    pub fn node(&self, id: NodeId) -> Option<&Value> {
        self.nodes.get(&id).map(Arc::as_ref)
    }
    pub fn node_arc(&self, id: NodeId) -> Option<&Arc<Value>> {
        self.nodes.get(&id)
    }
    pub fn ordered_nodes(&self) -> impl Iterator<Item = &Value> {
        self.order.iter().filter_map(|id| self.node(*id))
    }
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }
    pub fn header(&self) -> &Map<String, Value> {
        &self.header
    }
    pub fn layers(&self) -> &Value {
        &self.layers
    }
    pub fn fonts(&self) -> &Value {
        &self.fonts
    }
    pub fn images(&self) -> &[Value] {
        &self.images
    }
    pub(crate) fn images_arc(&self) -> &Arc<Vec<Value>> {
        &self.images
    }
    pub(crate) fn fonts_arc(&self) -> &Arc<Value> {
        &self.fonts
    }
    pub fn viewport(&self) -> Viewport {
        let d = Viewport::default();
        let v = self.header.get("viewport");
        let positive = |key: &str, fallback: f64| {
            v.and_then(|v| v.get(key))
                .and_then(Value::as_f64)
                .filter(|n| n.is_finite() && *n > 0.0)
                .unwrap_or(fallback)
        };
        Viewport {
            width: positive("width", d.width),
            height: positive("height", d.height),
            scale: positive("scale", d.scale),
        }
    }

    /// Accepts direct full scenes, `{scene: ...}`, and ready/frame delta replies.
    /// Validation precedes mutation, so a malformed packet cannot partially replace a frame.
    pub fn apply(&mut self, reply: &Value) -> Result<ApplyOutcome, String> {
        let _memory = crate::memtrace::scope(crate::memtrace::Tag::Scene);
        if let Some(full) = reply.get("scene") {
            return self.replace(full);
        }
        if reply.get("viewport").is_some() && reply.get("nodes").is_some() {
            return self.replace(reply);
        }
        let Some(delta) = reply.get("delta") else {
            return Ok(ApplyOutcome::default());
        };
        let object = delta.as_object().ok_or("scene delta is not an object")?;
        let nodes = match object.get("nodes") {
            Some(nodes) => array(nodes)?.iter().map(Cow::Borrowed).collect(),
            None => Vec::new(),
        };
        self.apply_delta(object, nodes)
    }

    /// `apply` for a reply the caller no longer needs the nodes of: changed nodes move into the
    /// scene instead of being copied. Other reply fields are left in place.
    pub fn apply_owned(&mut self, reply: &mut Value) -> Result<ApplyOutcome, String> {
        let _memory = crate::memtrace::scope(crate::memtrace::Tag::Scene);
        if reply.get("scene").is_some()
            || (reply.get("viewport").is_some() && reply.get("nodes").is_some())
            || !reply.pointer("/delta/nodes").is_some_and(Value::is_array)
        {
            return self.apply(reply);
        }
        let Some(Value::Array(nodes)) = reply.pointer_mut("/delta/nodes").map(std::mem::take)
        else {
            unreachable!("checked above");
        };
        let object = reply["delta"]
            .as_object()
            .ok_or("scene delta is not an object")?;
        self.apply_delta(object, nodes.into_iter().map(Cow::Owned).collect())
    }

    fn apply_delta(
        &mut self,
        object: &Map<String, Value>,
        nodes: Vec<Cow<'_, Value>>,
    ) -> Result<ApplyOutcome, String> {
        let mut changed = Vec::with_capacity(nodes.len());
        for node in nodes {
            let id = node
                .get("id")
                .and_then(node_id)
                .ok_or("scene node has no integer id")?;
            if !node.is_object() {
                return Err("scene node is not an object".into());
            }
            if is_patch(&node) && !self.nodes.contains_key(&id) {
                return Err(format!("patch for unknown scene node {id}"));
            }
            changed.push((id, node));
        }
        let mut removed = Vec::new();
        if let Some(ids) = object.get("removed") {
            for id in array(ids)? {
                removed.push(node_id(id).ok_or("removed node has no integer id")?);
            }
        }
        for key in ["layers", "fonts", "images"] {
            if let Some(value) = object.get(key) {
                array(value)?;
            }
        }
        if object.get("header").is_some_and(|value| !value.is_object()) {
            return Err("scene header is not an object".into());
        }
        // The Rust engine's retained export (engine/export/retained.rs) sends only nodes it rebuilt
        // (whole nodes, or patches of their moving keys): applied without comparing them with the
        // held ones.
        let rebuilt = object.get("retained").and_then(Value::as_bool) == Some(true);
        let mut outcome = ApplyOutcome::default();
        // Values replaced by patches (moved text glyph runs, rects): freed off the calling thread
        let mut retired = Vec::new();
        for (id, node) in changed {
            // runtime/session.luau NodePatch: the changed keys of a node the store already holds (checked
            // above), and `unset`, the keys it no longer has. A changed node is always a new Arc: draw
            // records and presentation caches key on node identity, so it is never merged in place.
            if is_patch(&node) {
                let patch = node.as_object().expect("checked above");
                let stored = self.nodes.get(&id).expect("checked above");
                let differs = rebuilt || patch.iter().any(|(key, value)| {
                    key != "patch" && key != "unset" && stored.get(key) != Some(value)
                }) || patch.get("unset").and_then(Value::as_array).is_some_and(|keys| {
                    keys.iter().filter_map(Value::as_str).any(|key| stored.get(key).is_some())
                });
                if !differs {
                    continue;
                }
                // The stored value moves out unless a presented frame still holds it; the node records'
                // Weak keeps the old allocation alive, so the new Arc never reuses its address
                static PLACEHOLDER: std::sync::OnceLock<Arc<Value>> = std::sync::OnceLock::new();
                let placeholder = PLACEHOLDER.get_or_init(|| Arc::new(Value::Null)).clone();
                let slot = Arc::make_mut(&mut self.nodes).get_mut(&id).expect("checked above");
                let old = std::mem::replace(slot, placeholder);
                let mut patch = match node {
                    Cow::Owned(Value::Object(map)) => map,
                    other => other.as_object().expect("checked above").clone(),
                };
                // A node a presented frame still holds is copied without the values the patch
                // replaces or unsets (a moved label's old glyph runs are not cloned only to be
                // dropped); the result is the same node
                let mut merged = Arc::try_unwrap(old).unwrap_or_else(|shared| {
                    let unset: Vec<&str> = match patch.get("unset") {
                        Some(Value::Array(keys)) => keys.iter().filter_map(Value::as_str).collect(),
                        _ => Vec::new(),
                    };
                    let kept = shared.as_object().map(|stored| {
                        stored
                            .iter()
                            .filter(|(key, _)| {
                                let replaced = key.as_str() != "patch" && key.as_str() != "unset" && patch.contains_key(key.as_str());
                                !replaced && !unset.contains(&key.as_str())
                            })
                            .map(|(key, value)| (key.clone(), value.clone()))
                            .collect::<Map<String, Value>>()
                    });
                    kept.map(Value::Object).unwrap_or_else(|| shared.as_ref().clone())
                });
                let target = merged.as_object_mut().ok_or("scene node is not an object")?;
                // Values move into the stored node; what they replace (and the emptied patch) is retired
                for (key, value) in patch.iter_mut() {
                    if key == "patch" || key == "unset" {
                        continue;
                    }
                    let value = std::mem::take(value);
                    match target.get_mut(key.as_str()) {
                        Some(slot) => retired.push(std::mem::replace(slot, value)),
                        None => {
                            target.insert(key.clone(), value);
                        }
                    }
                }
                if let Some(Value::Array(keys)) = patch.get("unset") {
                    for key in keys.iter().filter_map(Value::as_str) {
                        if let Some(old) = target.remove(key) {
                            retired.push(old);
                        }
                    }
                }
                retired.push(Value::Object(patch));
                *Arc::make_mut(&mut self.nodes).get_mut(&id).expect("checked above") = Arc::new(merged);
                outcome.changed_nodes.push(id);
                continue;
            }
            if !rebuilt && self.node(id) == Some(node.as_ref()) {
                continue;
            }
            if !self.nodes.contains_key(&id) {
                Arc::make_mut(&mut self.order).push(id);
            }
            if let Some(old) = Arc::make_mut(&mut self.nodes).insert(id, Arc::new(node.into_owned())) {
                if let Ok(value) = Arc::try_unwrap(old) {
                    retired.push(value);
                }
            }
            outcome.changed_nodes.push(id);
        }
        retire(retired);
        for id in removed {
            if Arc::make_mut(&mut self.nodes).remove(&id).is_some() {
                outcome.removed_nodes.push(id);
            }
        }
        if !outcome.removed_nodes.is_empty() {
            Arc::make_mut(&mut self.order).retain(|id| self.nodes.contains_key(id));
        }
        for (key, destination) in [("layers", &mut self.layers), ("fonts", &mut self.fonts)] {
            if let Some(value) = object.get(key) {
                if destination.as_ref() != value {
                    *destination = Arc::new(value.clone());
                    outcome.metadata_changed = true;
                }
            }
        }
        if let Some(images) = object.get("images") {
            for image in array(images)? {
                if !self.images.contains(image) {
                    if let Some(slot) = image.as_str().and_then(crate::assets::viewport_slot) {
                        Arc::make_mut(&mut self.images).retain(|old| old.as_str().and_then(crate::assets::viewport_slot) != Some(slot));
                    }
                    Arc::make_mut(&mut self.images).push(image.clone());
                    outcome.metadata_changed = true;
                }
            }
        }
        if let Some(header) = object.get("header").and_then(Value::as_object) {
            let mut next = self.header.as_ref().clone();
            // These optional fields belong to a complete delta header, not a merge patch.
            for key in [
                "viewport",
                "background",
                "status",
                "failure",
                "presentation",
            ] {
                next.remove(key);
            }
            next.extend(header.clone());
            if next != *self.header {
                self.header = Arc::new(next);
                outcome.metadata_changed = true;
            }
        }
        outcome.changed = outcome.metadata_changed
            || !outcome.changed_nodes.is_empty()
            || !outcome.removed_nodes.is_empty();
        if outcome.changed {
            self.revision += 1;
        }
        Ok(outcome)
    }

    fn replace(&mut self, full: &Value) -> Result<ApplyOutcome, String> {
        let object = full.as_object().ok_or("scene is not an object")?;
        let nodes = array(object.get("nodes").ok_or("scene has no nodes")?)?;
        let mut next = Self::default();
        for node in nodes {
            let id = node
                .get("id")
                .and_then(node_id)
                .ok_or("scene node has no integer id")?;
            if next.nodes.contains_key(&id) {
                return Err(format!("duplicate scene node {id}"));
            }
            Arc::make_mut(&mut next.order).push(id);
            let retained = self
                .node_arc(id)
                .filter(|old| old.as_ref() == node)
                .cloned()
                .unwrap_or_else(|| Arc::new(node.clone()));
            Arc::make_mut(&mut next.nodes).insert(id, retained);
        }
        let mut header = object.clone();
        header.remove("nodes");
        for key in ["layers", "fonts", "images"] {
            if let Some(value) = header.remove(key) {
                array(&value)?;
                match key {
                    "layers" => next.layers = Arc::new(value),
                    "fonts" => next.fonts = Arc::new(value),
                    _ => next.images = Arc::new(array(&value)?.to_vec()),
                }
            }
        }
        next.header = Arc::new(header);
        next.revision = self.revision + 1;
        let outcome = ApplyOutcome {
            changed: true,
            reset: true,
            changed_nodes: next.order.as_ref().clone(),
            removed_nodes: self
                .order
                .iter()
                .filter(|id| !next.nodes.contains_key(id))
                .copied()
                .collect(),
            metadata_changed: true,
        };
        *self = next;
        Ok(outcome)
    }

    /// Serialization/debug boundary only; rendering should borrow retained nodes directly.
    pub fn to_value(&self) -> Value {
        let mut scene = self.header.as_ref().clone();
        scene.insert(
            "nodes".into(),
            Value::Array(self.ordered_nodes().cloned().collect()),
        );
        scene.insert("layers".into(), self.layers.as_ref().clone());
        scene.insert("fonts".into(), self.fonts.as_ref().clone());
        scene.insert("images".into(), Value::Array(self.images.as_ref().clone()));
        Value::Object(scene)
    }

    pub(crate) fn replace_projected_arc(&mut self, id: NodeId, node: Arc<Value>) {
        Arc::make_mut(&mut self.nodes).insert(id, node);
    }
    /// Nodes in draw-list order with their ids
    pub(crate) fn ordered_arcs(&self) -> impl Iterator<Item = (NodeId, &Arc<Value>)> {
        self.order.iter().filter_map(|id| self.nodes.get(id).map(|node| (*id, node)))
    }
    pub(crate) fn shares_nodes(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.nodes, &other.nodes)
    }
    /// The draw order (a new Arc whenever a node is added or removed while another holder shares it)
    pub(crate) fn order_arc(&self) -> &Arc<Vec<NodeId>> {
        &self.order
    }
    /// This store's nodes with `scene`'s order, header, layers, fonts, images and revision
    pub(crate) fn with_scene_metadata(&self, scene: &Self) -> Self {
        Self {
            nodes: self.nodes.clone(),
            order: scene.order.clone(),
            header: scene.header.clone(),
            layers: scene.layers.clone(),
            fonts: scene.fonts.clone(),
            images: scene.images.clone(),
            revision: scene.revision,
        }
    }
    /// Nodes in id order (the draw preparer diffs consecutive scenes by node identity)
    pub(crate) fn node_arcs(&self) -> impl Iterator<Item = (NodeId, &Arc<Value>)> {
        self.nodes.iter().map(|(id, node)| (*id, node))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn node_patches_merge_changed_keys_and_unset_removed_ones() {
        let mut scene = SceneStore::default();
        scene
            .apply(&json!({"scene":{"viewport":{"width":10,"height":10,"scale":1},"nodes":[
                {"id":1,"rect":[0,0,5,5],"bg":{"color":[1,0,0]},"clip":[0,0,9,9]},
                {"id":2,"rect":[1,1,2,2]}
            ],"layers":[],"fonts":[],"images":[]}}))
            .unwrap();
        let untouched = scene.node_arc(2).unwrap().clone();
        let outcome = scene
            .apply(&json!({"delta":{"nodes":[{"id":1,"patch":true,"rect":[3,0,5,5],"unset":["clip"]}]}}))
            .unwrap();
        assert_eq!(outcome.changed_nodes, vec![1]);
        assert_eq!(scene.node(1).unwrap(), &json!({"id":1,"rect":[3,0,5,5],"bg":{"color":[1,0,0]}}));
        assert!(Arc::ptr_eq(&untouched, scene.node_arc(2).unwrap()));
        // A patch that changes nothing is not a change
        let outcome = scene
            .apply(&json!({"delta":{"nodes":[{"id":1,"patch":true,"rect":[3,0,5,5]}]}}))
            .unwrap();
        assert!(outcome.changed_nodes.is_empty());
        // A patch for a node the store lacks fails before anything changes
        let before = scene.to_value();
        assert!(
            scene
                .apply(&json!({"delta":{"nodes":[{"id":1,"patch":true,"rect":[9,9,9,9]},{"id":7,"patch":true}]}}))
                .is_err()
        );
        assert_eq!(scene.to_value(), before);
    }

    #[test]
    fn sparse_replies_match_full_scene_and_keep_node_identity() {
        let a = json!({"id": 1, "rect": [0,0,20,20], "future": {"keep": true}});
        let b = json!({"id": 2, "rect": [1,2,3,4]});
        let full = json!({"viewport":{"width":1200,"height":900,"scale":1.5},"nodes":[a,b],"layers":[{"draw":[1,2]}],"fonts":[],"images":["asset:a"],"futureHeader":7});
        let mut scene = SceneStore::default();
        scene.apply(&json!({"scene":full})).unwrap();
        let stable = scene.node_arc(1).unwrap().clone();
        scene
            .apply(&json!({"type":"frame","unchanged":true}))
            .unwrap();
        scene.apply(&json!({"delta":{"nodes":[{"id":3,"rect":[5,6,7,8]}],"removed":[2],"layers":[{"draw":[1,3]}],"images":["asset:a","asset:b"]}})).unwrap();
        assert!(Arc::ptr_eq(&stable, scene.node_arc(1).unwrap()));
        assert_eq!(
            scene.to_value(),
            json!({"viewport":{"width":1200,"height":900,"scale":1.5},"nodes":[a,{"id":3,"rect":[5,6,7,8]}],"layers":[{"draw":[1,3]}],"fonts":[],"images":["asset:a","asset:b"],"futureHeader":7})
        );
        let before = scene.to_value();
        assert!(
            scene
                .apply(&json!({"delta":{"nodes":[{"id":5},{"id":"bad"}],"removed":[1]}}))
                .is_err()
        );
        assert_eq!(scene.to_value(), before);
    }

    #[test]
    fn empty_luau_arrays_and_header_omissions() {
        let mut scene = SceneStore::default();
        scene.apply(&json!({"delta":{"nodes":{},"removed":{},"layers":{},"fonts":{},"images":{},"header":{"status":"error","failure":"old"}}})).unwrap();
        scene
            .apply(&json!({"delta":{"header":{"status":"ok"}}}))
            .unwrap();
        assert_eq!(scene.node_count(), 0);
        assert!(scene.header().get("failure").is_none());
    }

    #[test]
    fn runtime_rotation_exports_agree_through_sparse_and_full_paths() {
        let cases: Value =
            serde_json::from_str(include_str!("../tests/fixtures/rotation-projection.json"))
                .unwrap();
        for case in cases.as_array().unwrap() {
            let mut sparse = SceneStore::default();
            sparse.apply(&case["initial"]).unwrap();
            let mut full = SceneStore::default();
            full.apply(&case["authoritative"]).unwrap();
            let changed: Vec<_> = full
                .ordered_nodes()
                .filter(|node| sparse.node(node_id(&node["id"]).unwrap()) != Some(*node))
                .cloned()
                .collect();
            let removed: Vec<_> = sparse
                .ordered_nodes()
                .filter_map(|node| {
                    let id = node_id(&node["id"]).unwrap();
                    full.node(id).is_none().then_some(id)
                })
                .collect();
            sparse.apply(&json!({"type":"frame","delta":{"nodes":changed,"removed":removed,
                "layers":full.layers(),"fonts":full.fonts(),"images":full.images(),"header":full.header()}})).unwrap();
            assert_eq!(sparse.to_value(), full.to_value());
        }
    }
}
