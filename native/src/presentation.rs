//! Display-rate scroll prediction. The runtime keeps layout, script callbacks and content; between
//! its replies this moves only CanvasPosition and its descendants' translation over the retained
//! scene nodes (clip owners from the retained exporter), with Studio's wheel spring
//! (engine/input/scrolling.rs, notes/interaction.md "Mouse wheel").
//! Scene geometry is in logical pixels; text/selection geometry is in physical pixels.
use crate::scene::{NodeId, SceneStore, array, layer_pass, node_id};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Weak};

pub type Position = [f64; 2];
type Rect = [f64; 4];
const SPRING_FREQUENCY: f64 = 9.0 * std::f64::consts::PI;
fn f32_round(value: f64) -> f64 {
    (value as f32) as f64
}
fn number(value: Option<&Value>, fallback: f64) -> f64 {
    value
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite())
        .unwrap_or(fallback)
}
fn pair(value: Option<&Value>) -> Option<Position> {
    let a = value?.as_array()?;
    if a.len() != 2 {
        return None;
    }
    let x = a[0].as_f64()?;
    let y = a[1].as_f64()?;
    (x.is_finite() && y.is_finite()).then_some([x, y])
}

fn rect(value: Option<&Value>) -> Option<Rect> {
    let a = value?.as_array()?;
    if a.len() < 4 {
        return None;
    }
    let r = [
        a[0].as_f64()?,
        a[1].as_f64()?,
        a[2].as_f64()?,
        a[3].as_f64()?,
    ];
    r.iter().all(|v| v.is_finite()).then_some(r)
}
fn owners(value: Option<&Value>) -> Option<Vec<NodeId>> {
    array(value?).ok()?.iter().map(node_id).collect()
}
/// The ids owners() parses, without collecting them (none when it would be None)
fn owner_ids(value: Option<&Value>) -> impl Iterator<Item = NodeId> + '_ {
    value
        .and_then(|value| array(value).ok())
        .filter(|items| items.iter().all(|item| node_id(item).is_some()))
        .unwrap_or(&[])
        .iter()
        .filter_map(node_id)
}
/// Whether owners() parses the value (Some)
fn owners_listed(value: Option<&Value>) -> bool {
    value
        .and_then(|value| array(value).ok())
        .is_some_and(|items| items.iter().all(|item| node_id(item).is_some()))
}
fn clamp(value: f64, maximum: f64) -> f64 {
    value.min(maximum).max(0.0)
}
fn bounded(value: f64, minimum: f64, maximum: f64) -> f64 {
    value.min(maximum).max(minimum)
}
/// The canvas offset a CanvasPosition draws at: whole physical pixels, half up (the layout's measured
/// rule, engine/layout/pass.rs). Predicted motion moves nodes and already-snapped text by the
/// difference of these offsets, never by a fractional CanvasPosition delta.
fn offset(position: Position, scale: f64) -> Position {
    position.map(|v| (v * scale + 0.5).floor() / scale)
}
fn contains(r: Rect, x: f64, y: f64) -> bool {
    x >= r[0] && y >= r[1] && x < r[0] + r[2] && y < r[1] + r[3]
}
fn shift(mut r: Rect, dx: f64, dy: f64) -> Rect {
    r[0] = f32_round(r[0] + dx);
    r[1] = f32_round(r[1] + dy);
    r
}
fn intersect(a: Option<Rect>, b: Rect) -> Rect {
    let Some(a) = a else {
        return b;
    };
    let x = a[0].max(b[0]);
    let y = a[1].max(b[1]);
    [
        x,
        y,
        ((a[0] + a[2]).min(b[0] + b[2]) - x).max(0.0),
        ((a[1] + a[3]).min(b[1] + b[3]) - y).max(0.0),
    ]
}
/// A clip owner's clip rect: its scroll window (else its rect), moved to `absolute` when the export
/// gives one. Ancestor rotations that cancel out leave the owner axis-aligned but translated; `rect`
/// alone would clip at the unrotated place.
fn clip_rect(node: &Value) -> Result<Rect, String> {
    let base = rect(node.get("rect")).ok_or("clip owner has no rect")?;
    let mut r = rect(node.pointer("/scroll/window")).unwrap_or(base);
    if let Some(absolute) = pair(node.get("absolute")) {
        r[0] = absolute[0] + r[0] - base[0];
        r[1] = absolute[1] + r[1] - base[1];
    }
    Ok(r)
}

struct Translations {
    offsets: rustc_hash::FxHashMap<NodeId, Position>,
    movement: rustc_hash::FxHashMap<NodeId, Position>,
}
impl Translations {
    fn new(scene: &SceneStore, positions: &HashMap<NodeId, Position>) -> Result<Self, String> {
        let scale = scene.viewport().scale;
        let mut result = Self {
            offsets: Default::default(),
            movement: Default::default(),
        };
        for (&id, &position) in positions {
            let Some(node) = scene.node(id) else {
                continue;
            };
            let Some(scroll) = node.get("scroll") else {
                continue;
            };
            if node.get("matrix").is_some_and(|v| !v.is_null())
                || !position.iter().all(|v| v.is_finite())
            {
                continue;
            }
            let before = pair(scroll.get("scrollOffset"))
                .or_else(|| pair(scroll.get("canvasPosition")).map(|p| offset(p, scale)))
                .ok_or("scroll has no canvas position")?;
            let after = offset(position, scale);
            result
                .offsets
                .insert(id, [before[0] - after[0], before[1] - after[1]]);
        }
        result.movement.reserve(scene.node_count());
        let limit = scene.node_count();
        for (id, node) in scene.ordered_arcs() {
            result.resolve(id, Some(node), scene, 0, limit)?;
        }
        Ok(result)
    }
    /// The movement of `id` (whose node is `node` when known), memoized; an ancestry deeper than the
    /// scene has nodes is a cycle
    fn resolve(
        &mut self,
        id: NodeId,
        node: Option<&Value>,
        scene: &SceneStore,
        depth: usize,
        limit: usize,
    ) -> Result<Position, String> {
        if let Some(value) = self.movement.get(&id) {
            return Ok(*value);
        }
        if depth > limit {
            return Err("cyclic retained scroll ancestry".into());
        }
        let parent = node
            .or_else(|| scene.node(id))
            .and_then(|node| node.get("parent"))
            .and_then(node_id);
        // A parent already resolved is in the scene (only scene nodes are); else it is looked up
        let inherited = match parent {
            Some(parent) => match self.movement.get(&parent) {
                Some(value) => Some(*value),
                None => match scene.node(parent) {
                    Some(parent_node) => Some(self.resolve(parent, Some(parent_node), scene, depth + 1, limit)?),
                    None => None,
                },
            },
            None => None,
        };
        let (inherited, own) = match (parent, inherited) {
            (Some(parent), Some(inherited)) => (inherited, self.offsets.get(&parent).copied().unwrap_or([0.0; 2])),
            _ => ([0.0; 2], [0.0; 2]),
        };
        let result = [inherited[0] + own[0], inherited[1] + own[1]];
        self.movement.insert(id, result);
        Ok(result)
    }
    fn at(&self, id: NodeId) -> Position {
        self.movement.get(&id).copied().unwrap_or([0.0; 2])
    }
}

fn move_rect(node: &mut Value, key: &str, dx: f64, dy: f64) {
    if let Some(r) = node
        .get_mut(key)
        .and_then(Value::as_array_mut)
        .filter(|r| r.len() >= 2)
    {
        r[0] = json!(f32_round(number(r.first(), 0.0) + dx));
        r[1] = json!(f32_round(number(r.get(1), 0.0) + dy));
    }
}
fn move_text(text: &mut Value, dx: f64, dy: f64, scale: f64) {
    let Some(content) = rect(text.get("content")).map(|r| shift(r, dx, dy)) else {
        return;
    };
    let rich = text["rich"].as_bool().unwrap_or(false);
    let align = text["xAlign"].as_str().unwrap_or("").to_owned();
    let round3 = |v: f64| (v * 1000.0 + 0.5).floor() / 1000.0;
    let snap = |v: f64| (round3(v) + 0.5).floor();
    text["content"] = json!(content);
    if let Some(lines) = text.get_mut("lines").and_then(Value::as_array_mut) {
        for line in lines {
            let width = number(line.get("width"), 0.0);
            // The engine's pen rule (engine/text/layout.rs line_pen): a rich centred line whose width
            // / DPI is inexact takes floor((w - 1) / 2)
            let half = if rich && !crate::engine::text::layout::divides_exactly(width, scale) {
                ((width - 1.0) / 2.0).floor()
            } else {
                (width / 2.0).floor()
            };
            let pen = |snap: &dyn Fn(f64) -> f64| match align.as_str() {
                "Left" => snap(content[0]),
                "Right" => snap(content[0] + content[2]) - width,
                _ => snap(content[0] + content[2] / 2.0) - half,
            };
            let px = pen(&snap);
            let ux = round3(pen(&|v| v));
            line["x"] = json!(round3(number(line.get("x"), 0.0) + dx));
            line["top"] = json!(round3(number(line.get("top"), 0.0) + dy));
            line["px"] = json!(px);
            if let Some(map) = line.as_object_mut() {
                if ux != px {
                    map.insert("ux".into(), json!(ux));
                } else {
                    map.remove("ux");
                }
            }
        }
    }
}

/// What a node's projected value is made of besides its source node: its movement, a scroller's own
/// offset and predicted position, and the clip / culled corrections (Some: the value written). Equal
/// inputs make an equal node, so ProjectionMemo hands back last frame's Arc (the draw preparer keys its
/// retained records on node identity).
#[derive(Clone, Copy, PartialEq, Eq)]
struct ProjectionKey {
    moved: bool,
    movement: [u64; 2],
    own: [u64; 2],
    position: Option<[u64; 2]>,
    clip: Option<Option<[u64; 4]>>,
    culled: Option<bool>,
}
/// The projected nodes of the last projection, by id: a node whose source node (the same Arc) and
/// ProjectionKey are unchanged keeps its projected Arc instead of being copied and moved again. Source
/// nodes are held weakly: the scene store moves a patched node's value out of its Arc when nothing else
/// holds it (a Weak keeps the allocation, so the address still identifies the node).
#[derive(Default)]
pub struct ProjectionMemo {
    scale: u64,
    nodes: rustc_hash::FxHashMap<NodeId, (Weak<Value>, ProjectionKey, Arc<Value>)>,
    /// The last projection (project_incremental starts from it)
    state: Option<ProjectionState>,
    /// Projections made by project_incremental
    pub incremental: u64,
}
thread_local! {
    // Nodes the last project_scroll_scene on this thread gave new values (copied, moved or corrected)
    static PROJECTED_NODES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
/// The rect / absolute a moved copy holds (move_rect): x and y moved in float32, the rest unchanged
fn moved_values(value: Option<&Value>, dx: f64, dy: f64) -> Option<Vec<f64>> {
    let a = value?.as_array()?;
    if a.len() < 2 {
        return a.iter().map(Value::as_f64).collect();
    }
    let mut out = Vec::with_capacity(a.len());
    out.push(f32_round(number(a.first(), 0.0) + dx));
    out.push(f32_round(number(a.get(1), 0.0) + dy));
    for v in &a[2..] {
        out.push(v.as_f64()?);
    }
    Some(out)
}
fn finite_rect(values: Option<Vec<f64>>) -> Option<Rect> {
    let v = values?;
    if v.len() < 4 {
        return None;
    }
    let r = [v[0], v[1], v[2], v[3]];
    r.iter().all(|v| v.is_finite()).then_some(r)
}
fn finite_pair(values: Option<Vec<f64>>) -> Option<Position> {
    let v = values?;
    (v.len() == 2 && v.iter().all(|v| v.is_finite())).then(|| [v[0], v[1]])
}
/// The node's rect and absolute as its projection holds them (moved by (dx, dy) when `moved`)
fn projected_rect(node: &Value, moved: bool, dx: f64, dy: f64) -> (Option<Rect>, Option<Position>) {
    if !moved {
        return (rect(node.get("rect")), pair(node.get("absolute")));
    }
    (
        finite_rect(moved_values(node.get("rect"), dx, dy)),
        finite_pair(moved_values(node.get("absolute"), dx, dy)),
    )
}
/// clip_rect of the node as its projection holds it (the scroll window moved like its rect)
fn projected_clip_rect(node: &Value, moved: bool, dx: f64, dy: f64) -> Result<Rect, String> {
    if !moved {
        return clip_rect(node);
    }
    let (base, absolute) = projected_rect(node, true, dx, dy);
    let base = base.ok_or("clip owner has no rect")?;
    let window = node.get("scroll").and_then(|scroll| finite_rect(moved_values(scroll.get("window"), dx, dy)));
    let mut r = window.unwrap_or(base);
    if let Some(absolute) = absolute {
        r[0] = absolute[0] + r[0] - base[0];
        r[1] = absolute[1] + r[1] - base[1];
    }
    Ok(r)
}
/// The moved copy of `node` (dx, dy: its movement; own: a scroller's own offset; position: its predicted
/// CanvasPosition), with the clip / culled corrections applied
fn project_node(
    node: &Value,
    key: &ProjectionKey,
    [dx, dy]: Position,
    own: Position,
    position: Option<Position>,
    clip: Option<Option<Rect>>,
    scale: f64,
) -> Value {
    let mut next = node.clone();
    if key.moved {
        move_rect(&mut next, "rect", dx, dy);
        move_rect(&mut next, "absolute", dx, dy);
        if let Some(matrix) = node
            .get("matrix")
            .and_then(Value::as_array)
            .filter(|m| m.len() == 6)
        {
            let m: Vec<f64> = matrix.iter().map(|v| number(Some(v), 0.0)).collect();
            next["matrix"] = json!([
                m[0],
                m[1],
                m[2],
                m[3],
                m[4] + dx - m[0] * dx - m[2] * dy,
                m[5] + dy - m[1] * dx - m[3] * dy
            ]);
        }
        if let Some(text) = next.get_mut("text") {
            move_text(text, dx * scale, dy * scale, scale);
        }
        if let Some(rects) = next
            .pointer_mut("/selection/rects")
            .and_then(Value::as_array_mut)
        {
            for r in rects {
                if let Some(value) = rect(Some(r)) {
                    *r = json!(shift(value, dx * scale, dy * scale));
                }
            }
        }
        if let Some(caret) = next.get_mut("caret") {
            caret["x"] = json!(number(caret.get("x"), 0.0) + dx * scale);
            caret["y"] = json!(number(caret.get("y"), 0.0) + dy * scale);
        }
        if let Some(scroll) = next.get_mut("scroll") {
            move_rect(scroll, "window", dx, dy);
            move_rect(scroll, "canvas", dx + own[0], dy + own[1]);
            if let Some(position) = position {
                scroll["canvasPosition"] = json!(position);
                scroll["scrollOffset"] = json!(offset(position, scale));
            }
        }
    }
    if let Some(clip) = clip {
        if let Some(clip) = clip {
            next["clip"] = json!(clip);
        } else if let Some(map) = next.as_object_mut() {
            map.remove("clip");
        }
    }
    if let Some(culled) = key.culled {
        if culled {
            next["culled"] = Value::Bool(true);
        } else if let Some(map) = next.as_object_mut() {
            map.remove("culled");
        }
    }
    next
}

/// Moves the nodes under each predicted scroller by whole-device-pixel offsets, keeping the export's
/// float32 placement and text pen snapping, and recomputes clips and rotation culling from the moved
/// owners; the authoritative scene is not modified.
pub fn project_scroll_scene(
    scene: &SceneStore,
    positions: &HashMap<NodeId, Position>,
) -> Result<SceneStore, String> {
    project_scroll_scene_memo(scene, positions, &mut ProjectionMemo::default())
}
/// project_scroll_scene reusing `memo` (and keeping this projection in it): a projection with the same
/// scale and predicted positions as the last one, whose scene changed in nodes that are not scrollers
/// and kept their parents and clip owners, recomputes only those nodes and the nodes clipped by those
/// of them whose projected clip rect changed (project_incremental); any other projects every node,
/// reusing the memo's projected value of each node whose source node and inputs are unchanged. Either
/// way an unchanged node keeps its identity (the draw preparer keys its retained records on it), and
/// the result is the projection of every node, as project_scroll_scene computes it.
pub fn project_scroll_scene_memo(
    scene: &SceneStore,
    positions: &HashMap<NodeId, Position>,
    memo: &mut ProjectionMemo,
) -> Result<SceneStore, String> {
    PROJECTED_NODES.with(|count| count.set(0));
    if positions.is_empty() {
        return Ok(scene.clone());
    }
    let scale = scene.viewport().scale;
    if let Some(projected) = project_incremental(scene, positions, memo, scale) {
        return Ok(projected);
    }
    // The last projection's nodes are freed off this thread
    if let Some(state) = memo.state.take() {
        crate::render::drop_later(state);
    }
    let translations = Translations::new(scene, positions)?;
    let needs_projection = translations.offsets.iter().any(|(id, d)| {
        *d != [0.0; 2]
            || pair(
                scene
                    .node(*id)
                    .and_then(|n| n.pointer("/scroll/canvasPosition")),
            ) != positions.get(id).copied()
    });
    if !needs_projection {
        return Ok(scene.clone());
    }
    if memo.scale != scale.to_bits() {
        memo.nodes.clear();
        memo.scale = scale.to_bits();
    }
    use rayon::prelude::*;
    // Nodes are projected independently; the first error in scene order is reported
    fn first_error<T>(results: Vec<Result<T, String>>) -> Result<Vec<T>, String> {
        results.into_iter().collect()
    }
    let ordered: Vec<(NodeId, &Arc<Value>)> = scene.ordered_arcs().collect();
    let movements: Vec<Movement> = first_error(
        ordered
            .iter()
            .map(|&(id, node)| movement(id, node, &translations, positions))
            .collect(),
    )?;
    // The clip owners' clip rects as projected (moved, before any correction), once per owner, and the
    // nodes that list each owner
    let index: rustc_hash::FxHashMap<NodeId, usize> = ordered
        .iter()
        .enumerate()
        .map(|(i, (id, _))| (*id, i))
        .collect();
    let owner_clip = |id: NodeId| -> Option<Result<Rect, String>> {
        match index.get(&id) {
            Some(&i) => {
                let m = &movements[i];
                Some(projected_clip_rect(ordered[i].1, m.moved, m.delta[0], m.delta[1]))
            }
            None => scene.node(id).map(clip_rect),
        }
    };
    let mut owner_rects: OwnerRects = Default::default();
    for (_, node) in &ordered {
        for id in listed_owners(node) {
            owner_rects.entry(id).or_insert_with(|| owner_clip(id));
        }
    }
    let projected_nodes: Vec<Projected> = first_error(
        ordered
            .par_iter()
            .zip(movements.par_iter())
            .map(|(&(id, source), m)| project_one(id, source, m, &owner_rects, &memo.nodes, scale))
            .collect(),
    )?;
    let mut projected = scene.clone();
    let mut nodes = rustc_hash::FxHashMap::default();
    let mut created = 0;
    for ((id, source), value) in ordered.iter().zip(projected_nodes) {
        let (key, value) = match value {
            Projected::Source => continue,
            Projected::Kept(key, value) => (key, value),
            Projected::New(key, value) => {
                created += 1;
                (key, Arc::new(value))
            }
        };
        projected.replace_projected_arc(*id, value.clone());
        nodes.insert(*id, (Arc::downgrade(source), key, value));
    }
    PROJECTED_NODES.with(|count| count.set(created));
    // Last frame's projected nodes not projected again are freed off this thread
    crate::render::drop_later(std::mem::replace(&mut memo.nodes, nodes));
    memo.state = Some(ProjectionState {
        positions: positions.clone(),
        viewport: scene.viewport(),
        order: scene.order_arc().clone(),
        sources: scene.node_arcs().map(|(id, node)| (id, Arc::downgrade(node))).collect(),
        projected: projected.clone(),
        translations,
        owner_rects,
    });
    Ok(projected)
}

/// What a node contributes to other nodes' projections, as the projection reads it: its parent (their
/// movement), whether it is a scroller, and the owners it lists (who is clipped and culled by whom)
#[derive(PartialEq)]
struct Structure {
    parent: Option<NodeId>,
    scroller: bool,
    clip_owners: Option<Vec<NodeId>>,
    cull_owners: Option<Option<Vec<NodeId>>>,
}
fn structure(node: &Value) -> Structure {
    Structure {
        parent: node.get("parent").and_then(node_id),
        scroller: node.get("scroll").is_some(),
        clip_owners: owners(node.get("clipOwners")),
        cull_owners: node.get("rotationCull").map(|cull| owners(cull.get("owners"))),
    }
}

/// A node's movement in a projection: (dx, dy), a scroller's own offset and predicted position, and
/// whether the projection moves it at all
struct Movement {
    delta: Position,
    own: Position,
    position: Option<Position>,
    moved: bool,
}
fn movement(
    id: NodeId,
    node: &Value,
    translations: &Translations,
    positions: &HashMap<NodeId, Position>,
) -> Result<Movement, String> {
    let [dx, dy] = translations.at(id);
    let own = translations.offsets.get(&id).copied().unwrap_or([0.0; 2]);
    let position = translations
        .offsets
        .contains_key(&id)
        .then(|| positions[&id]);
    let moved = !(dx == 0.0
        && dy == 0.0
        && (position.is_none() || position == pair(node.pointer("/scroll/canvasPosition"))));
    if (dx != 0.0 || dy != 0.0) && !owners_listed(node.get("clipOwners")) {
        return Err("retained scroll clip ownership missing".to_string());
    }
    Ok(Movement { delta: [dx, dy], own, position, moved })
}
/// The clip and rotation-cull owners a node lists
fn listed_owners(node: &Value) -> impl Iterator<Item = NodeId> + '_ {
    owner_ids(node.get("clipOwners")).chain(owner_ids(node.get("rotationCull").and_then(|cull| cull.get("owners"))))
}
type OwnerRects = rustc_hash::FxHashMap<NodeId, Option<Result<Rect, String>>>;
/// A node's projected value: the source node itself, the memo's (unchanged inputs), or a new copy
enum Projected {
    Source,
    Kept(ProjectionKey, Arc<Value>),
    New(ProjectionKey, Value),
}
/// One node's projection: its clip / cull corrections from the projected owners, then its value
fn project_one(
    id: NodeId,
    source: &Arc<Value>,
    m: &Movement,
    owner_rects: &OwnerRects,
    memo: &rustc_hash::FxHashMap<NodeId, (Weak<Value>, ProjectionKey, Arc<Value>)>,
    scale: f64,
) -> Result<Projected, String> {
    let owner_rect = |id: NodeId, missing: &str| -> Result<Rect, String> {
        match owner_rects.get(&id) {
            Some(Some(rect)) => rect.clone(),
            _ => Err(missing.to_string()),
        }
    };
    let mut clip_fix = None;
    let mut culled = None;
    if owners_listed(source.get("clipOwners")) {
        let mut clip = None;
        for owner in owner_ids(source.get("clipOwners")) {
            clip = Some(intersect(clip, owner_rect(owner, "retained scroll clip owner missing")?));
        }
        if rect(source.get("clip")) != clip {
            clip_fix = Some(clip);
        }
        if let Some(cull) = source.get("rotationCull") {
            if !owners_listed(cull.get("owners")) {
                return Err("retained rotation cull ownership missing".into());
            }
            let mut cull_clip = None;
            for owner in owner_ids(cull.get("owners")) {
                cull_clip = Some(intersect(cull_clip, owner_rect(owner, "retained rotation cull owner missing")?));
            }
            let (r, absolute) = projected_rect(source, m.moved, m.delta[0], m.delta[1]);
            let r = r.ok_or("rotation cull node has no rect")?;
            let a = absolute.unwrap_or([r[0], r[1]]);
            let outside = cull_clip.is_some_and(|c| {
                c[2] <= 0.0
                    || c[3] <= 0.0
                    || a[0] + r[2] <= c[0]
                    || a[1] + r[3] <= c[1]
                    || a[0] >= c[0] + c[2]
                    || a[1] >= c[1] + c[3]
            });
            let value = cull["baseCulled"].as_bool().unwrap_or(false) || outside;
            if source["culled"].as_bool().unwrap_or(false) != value {
                culled = Some(value);
            }
        }
    }
    if !m.moved && clip_fix.is_none() && culled.is_none() {
        return Ok(Projected::Source);
    }
    let bits2 = |p: Position| p.map(f64::to_bits);
    let key = ProjectionKey {
        moved: m.moved,
        movement: bits2(m.delta),
        own: bits2(m.own),
        position: m.position.map(bits2),
        clip: clip_fix.map(|clip| clip.map(|c| c.map(f64::to_bits))),
        culled,
    };
    if let Some((kept_source, kept_key, kept)) = memo.get(&id) {
        if std::ptr::eq(kept_source.as_ptr(), Arc::as_ptr(source)) && *kept_key == key {
            return Ok(Projected::Kept(key, kept.clone()));
        }
    }
    Ok(Projected::New(
        key,
        project_node(source, &key, m.delta, m.own, m.position, clip_fix, scale),
    ))
}
/// Whether two owner clip rects (or their errors) are the same
fn same_rect(a: &Option<Result<Rect, String>>, b: &Option<Result<Rect, String>>) -> bool {
    match (a, b) {
        (Some(Ok(a)), Some(Ok(b))) => a.map(f64::to_bits) == b.map(f64::to_bits),
        (Some(Err(a)), Some(Err(b))) => a == b,
        (None, None) => true,
        _ => false,
    }
}

/// The last full projection's state and result, for project_incremental
pub struct ProjectionState {
    positions: HashMap<NodeId, Position>,
    viewport: crate::scene::Viewport,
    /// The source scene's draw order (a node added or removed replaces it)
    order: Arc<Vec<NodeId>>,
    /// The source scene's nodes in id order, held weakly (see ProjectionMemo)
    sources: Vec<(NodeId, Weak<Value>)>,
    projected: SceneStore,
    translations: Translations,
    owner_rects: OwnerRects,
}

/// The projection of `scene` from the last one when only nodes changed that cannot move anything else:
/// the same scale, positions and draw order (no node added or removed), and every changed node keeps its
/// parent and listed owners and is not a scroller (so every node's movement is the same). Those nodes
/// are projected again, and so are the nodes clipped or culled by an owner among them whose projected
/// clip rect changed. None: project every node instead.
fn project_incremental(
    scene: &SceneStore,
    positions: &HashMap<NodeId, Position>,
    memo: &mut ProjectionMemo,
    scale: f64,
) -> Option<SceneStore> {
    let state = memo.state.as_mut()?;
    if memo.scale != scale.to_bits()
        || state.positions != *positions
        || !Arc::ptr_eq(&state.order, scene.order_arc())
        || state.viewport != scene.viewport()
        || state.sources.len() != scene.node_count()
    {
        return None;
    }
    let mut changed = Vec::new();
    for ((id, node), (old_id, old_node)) in scene.node_arcs().zip(state.sources.iter_mut()) {
        if *old_id != id {
            return None;
        }
        if std::ptr::eq(old_node.as_ptr(), Arc::as_ptr(node)) {
            continue;
        }
        // A scroller or a node with another parent moves others; another owner list changes who is clipped.
        // The old node is compared while something still holds it (the frame drawn last holds the nodes it
        // did not move); else every node is projected.
        let structure_now = structure(node);
        if structure_now.scroller
            || state.translations.offsets.contains_key(&id)
            || old_node.upgrade().is_none_or(|old| structure(&old) != structure_now)
        {
            return None;
        }
        *old_node = Arc::downgrade(node);
        changed.push((id, node.clone()));
    }
    let movement_of = |id: NodeId, node: &Value| movement(id, node, &state.translations, positions);
    // Owners among the changed nodes whose projected clip rect changed: the nodes listing them too
    let mut dirty: Vec<(NodeId, Arc<Value>)> = changed.clone();
    let mut owner_updates = Vec::new();
    for (id, node) in &changed {
        let Some(old) = state.owner_rects.get(id) else {
            continue;
        };
        let m = movement_of(*id, node).ok()?;
        let new = Some(projected_clip_rect(node, m.moved, m.delta[0], m.delta[1]));
        if !same_rect(old, &new) {
            owner_updates.push((*id, new));
            // The nodes it clips or culls (rare: a scan of the scene)
            for (lister, node) in scene.ordered_arcs() {
                if listed_owners(node).any(|owner| owner == *id) && !changed.iter().any(|(c, _)| *c == lister) {
                    dirty.push((lister, node.clone()));
                }
            }
        }
    }
    for (id, rect) in owner_updates {
        state.owner_rects.insert(id, rect);
    }
    dirty.sort_by_key(|(id, _)| *id);
    dirty.dedup_by_key(|(id, _)| *id);
    let mut projected = state.projected.with_scene_metadata(scene);
    let mut created = 0;
    for (id, node) in dirty {
        let m = movement_of(id, &node).ok()?;
        match project_one(id, &node, &m, &state.owner_rects, &memo.nodes, scale).ok()? {
            Projected::Source => {
                projected.replace_projected_arc(id, node);
                if let Some(old) = memo.nodes.remove(&id) {
                    crate::render::drop_later(old);
                }
            }
            Projected::Kept(_, value) => projected.replace_projected_arc(id, value),
            Projected::New(key, value) => {
                created += 1;
                let value = Arc::new(value);
                projected.replace_projected_arc(id, value.clone());
                if let Some(old) = memo.nodes.insert(id, (Arc::downgrade(&node), key, value)) {
                    crate::render::drop_later(old);
                }
            }
        }
    }
    PROJECTED_NODES.with(|count| count.set(created));
    state.projected = projected.clone();
    memo.incremental += 1;
    Some(projected)
}

#[derive(Clone, Debug)]
pub struct ScrollState {
    pub id: NodeId,
    pub epoch: u64,
    pub owned: bool,
    pub position: Position,
    pub authoritative: Position,
    pub velocity: Position,
    pub target: Position,
    pub range: Position,
    pub enabled: bool,
    pub zero_sensitive: bool,
    pub direction: String,
    pub constrained: bool,
    pub constraint_reason: Option<&'static str>,
}

pub struct ScrollPresentation {
    pub states: BTreeMap<NodeId, ScrollState>,
    pub max_prediction: f64,
    pub spring_frequency: f64,
    pub sequence: u64,
    pub acknowledged: u64,
    pub fallback: Option<String>,
    response_seconds: Option<f64>,
    ranks: HashMap<NodeId, u64>,
    scale: f64,
    display_revision: u64,
    projection: Option<(u64, HashMap<NodeId, Position>, SceneStore)>,
    /// The last projection's nodes (project_scroll_scene_memo)
    memo: ProjectionMemo,
    /// Nodes the last project() gave new values (0 when it drew the authoritative scene or the last projection)
    pub projected_nodes: usize,
}
/// Prediction stays within 200 logical px of the latest authoritative CanvasPosition: rows further
/// out may not be instantiated in the retained scene yet.
impl Default for ScrollPresentation {
    fn default() -> Self {
        Self::new(200.0, SPRING_FREQUENCY)
    }
}
impl ScrollPresentation {
    pub fn new(max_prediction: f64, spring_frequency: f64) -> Self {
        let frequency = if spring_frequency.is_finite()
            && (spring_frequency as f32).is_finite()
            && (spring_frequency as f32) > 0.0
        {
            spring_frequency
        } else {
            SPRING_FREQUENCY
        };
        Self {
            states: BTreeMap::new(),
            max_prediction,
            spring_frequency: frequency,
            sequence: 0,
            acknowledged: 0,
            fallback: None,
            response_seconds: None,
            ranks: HashMap::new(),
            scale: 1.0,
            display_revision: 0,
            projection: None,
            memo: ProjectionMemo::default(),
            projected_nodes: 0,
        }
    }
    pub fn reset(&mut self) {
        *self = Self::new(self.max_prediction, self.spring_frequency);
    }
    pub fn update(&mut self, scene: &SceneStore, ack: Option<&Value>) -> bool {
        let _memory = crate::memtrace::scope(crate::memtrace::Tag::Presentation);
        let previous: HashMap<_, _> = self
            .states
            .iter()
            .map(|(&id, s)| {
                (
                    id,
                    (
                        s.owned,
                        offset(
                            if s.owned { s.position } else { s.authoritative },
                            self.scale,
                        ),
                    ),
                )
            })
            .collect();
        self.scale = scene.viewport().scale;
        self.ranks.clear();
        fn ranks(entries: &Value, output: &mut HashMap<NodeId, u64>, rank: &mut u64) {
            for entry in array(entries).unwrap_or(&[]) {
                if let Some(group) = entry.get("group").and_then(node_id) {
                    output.insert(group, *rank);
                    *rank += 1;
                    if let Some(draw) = entry.get("draw") {
                        ranks(draw, output, rank);
                    }
                } else if let Some(id) = node_id(entry).filter(|id| *id > 0) {
                    output.insert(id, *rank);
                    *rank += 1;
                }
            }
        }
        let mut layers: Vec<_> = array(scene.layers()).unwrap_or(&[]).iter().collect();
        layers.sort_by(|a, b| {
            layer_pass(a).cmp(&layer_pass(b)).then(number(a.get("displayOrder"), 0.0).total_cmp(&number(b.get("displayOrder"), 0.0)))
        });
        let mut rank = 0;
        for layer in layers {
            if let Some(draw) = layer.get("draw") {
                ranks(draw, &mut self.ranks, &mut rank);
            }
        }
        let valid_ack = ack.filter(|a| {
            a["version"].as_u64() == Some(1)
                && node_id(&a["seq"]).is_some_and(|seq| seq >= self.acknowledged)
        });
        let mut replies = HashMap::new();
        if let Some(ack) = valid_ack {
            self.acknowledged = node_id(&ack["seq"]).unwrap();
            for item in ack
                .get("scrolls")
                .and_then(|a| array(a).ok())
                .unwrap_or(&[])
            {
                if let Some(id) = item.get("id").and_then(node_id) {
                    replies.insert(id, item);
                }
            }
        }
        let mut alive = HashSet::new();
        for node in scene.ordered_nodes() {
            let Some(scroll) = node.get("scroll") else {
                continue;
            };
            if node.get("matrix").is_some_and(|v| !v.is_null())
                || owners(node.get("clipOwners")).is_none()
            {
                continue;
            }
            let Some(authoritative) = pair(scroll.get("canvasPosition")) else {
                continue;
            };
            let id = node_id(&node["id"]).unwrap();
            let reply = replies.get(&id).copied();
            let Some(range) = pair(reply.and_then(|r| r.get("range")))
                .or_else(|| pair(scroll.pointer("/presentation/range")))
                .or_else(|| pair(scroll.get("maxRange")))
            else {
                continue;
            };
            alive.insert(id);
            let epoch = reply
                .and_then(|r| r.get("epoch"))
                .or_else(|| scroll.get("epoch"))
                .and_then(node_id)
                .unwrap_or(0);
            let enabled = reply
                .and_then(|r| r.get("enabled"))
                .and_then(Value::as_bool)
                != Some(false)
                && scroll["scrollingEnabled"].as_bool() != Some(false)
                && scroll
                    .pointer("/presentation/enabled")
                    .and_then(Value::as_bool)
                    != Some(false);
            let direction = reply
                .and_then(|r| r.get("direction"))
                .or_else(|| scroll.pointer("/presentation/direction"))
                .or_else(|| scroll.get("direction"))
                .and_then(Value::as_str)
                .unwrap_or("XY")
                .to_owned();
            // A newer epoch (a runtime script write, reset or clamp) or a scroller the client does not
            // own takes the runtime's pose, velocity and target: programmatic scroll changes win over
            // pending predicted motion. Same-epoch replies keep the local target.
            if self
                .states
                .get(&id)
                .is_none_or(|s| epoch > s.epoch || !s.owned)
            {
                self.states.insert(
                    id,
                    ScrollState {
                        id,
                        epoch,
                        owned: false,
                        position: pair(reply.and_then(|r| r.get("position")))
                            .unwrap_or(authoritative),
                        velocity: pair(reply.and_then(|r| r.get("velocity"))).unwrap_or([0.0; 2]),
                        target: pair(reply.and_then(|r| r.get("target"))).unwrap_or(authoritative),
                        authoritative,
                        range,
                        enabled,
                        zero_sensitive: false,
                        direction: direction.clone(),
                        constrained: false,
                        constraint_reason: None,
                    },
                );
            }
            let state = self.states.get_mut(&id).unwrap();
            state.range = range;
            state.authoritative = authoritative;
            state.enabled = enabled;
            state.zero_sensitive = scroll
                .pointer("/presentation/zeroSensitive")
                .and_then(Value::as_bool)
                == Some(true);
            state.direction = direction;
            if !enabled {
                state.owned = false;
            }
            for axis in 0..2 {
                state.position[axis] = clamp(state.position[axis], range[axis]);
                state.target[axis] = clamp(state.target[axis], range[axis]);
            }
        }
        self.states.retain(|id, _| alive.contains(id));
        previous.len() != self.states.len()
            || self.states.iter().any(|(id, s)| {
                previous.get(id)
                    != Some(&(
                        s.owned,
                        offset(
                            if s.owned { s.position } else { s.authoritative },
                            self.scale,
                        ),
                    ))
            })
    }
    /// Records one request-to-scene interval (exponential average, new sample weight 1/4). Two
    /// snapshots can be in the pipeline (the scene shown and the request being computed), so long
    /// gestures are paced to 0.9 * max_prediction / (2 * max(interval, 1/120 s)) px/s: the content
    /// the runtime can supply.
    pub fn observe_response(&mut self, seconds: f64) {
        if seconds.is_finite() && seconds > 0.0 {
            self.response_seconds = Some(
                self.response_seconds
                    .map(|old| old * 0.75 + seconds * 0.25)
                    .unwrap_or(seconds),
            );
        }
    }
    fn positions(&self) -> HashMap<NodeId, Position> {
        self.states
            .values()
            .filter(|s| s.owned)
            .map(|s| (s.id, s.position))
            .collect()
    }
    fn wheel_candidates(&self, scene: &SceneStore, x: f64, y: f64) -> Result<Vec<NodeId>, String> {
        let translations = Translations::new(scene, &self.positions())?;
        let mut candidates = Vec::new();
        for (&id, state) in &self.states {
            let Some(node) = scene.node(id) else {
                continue;
            };
            let [dx, dy] = translations.at(id);
            let Some(r) = rect(node.get("rect")) else {
                continue;
            };
            if !state.enabled
                || node["visible"].as_bool() == Some(false)
                || node["culled"].as_bool() == Some(true)
                || !contains(shift(r, dx, dy), x, y)
            {
                continue;
            }
            let ids =
                owners(node.get("clipOwners")).ok_or("retained scroll clip ownership missing")?;
            let mut clip = None;
            for owner in ids {
                let [ox, oy] = translations.at(owner);
                clip = Some(intersect(
                    clip,
                    shift(
                        clip_rect(
                            scene
                                .node(owner)
                                .ok_or("retained scroll clip owner missing")?,
                        )?,
                        ox,
                        oy,
                    ),
                ));
            }
            if clip.is_none_or(|r| contains(r, x, y)) {
                candidates.push(id);
            }
        }
        candidates.sort_by_key(|id| std::cmp::Reverse(self.ranks.get(id).copied()));
        Ok(candidates)
    }
    /// Returns the predicted recipient. Always forward the wheel event to the runtime too.
    pub fn wheel(&mut self, scene: &SceneStore, x: f64, y: f64, notches: f64) -> Option<NodeId> {
        if !x.is_finite() || !y.is_finite() || !notches.is_finite() || notches == 0.0 {
            return None;
        }
        let candidates = match self.wheel_candidates(scene, x, y) {
            Ok(v) => v,
            Err(e) => {
                self.release_with_error(e);
                return None;
            }
        };
        for id in candidates {
            let state = self.states.get_mut(&id).unwrap();
            let target = clamp(state.target[1] - notches * 100.0, state.range[1]);
            if (target - state.target[1]).abs() <= 1e-6 {
                continue;
            }
            // ScrollingDirection X frames take a vertical wheel without moving (measured): no
            // prediction, the runtime handles it.
            if state.direction == "X" {
                return None;
            }
            if state.zero_sensitive && target == 0.0 && state.target[0] == 0.0 {
                state.owned = false;
                return None;
            }
            state.target[1] = target;
            state.owned = true;
            return Some(id);
        }
        None
    }
    pub fn advance(&mut self, scene: &SceneStore, dt: f64) -> bool {
        let _memory = crate::memtrace::scope(crate::memtrace::Tag::Presentation);
        if !dt.is_finite() || dt <= 0.0 {
            return false;
        }
        let dt = f32_round(dt);
        let scale = scene.viewport().scale;
        let mut moved = false;
        for state in self.states.values_mut().filter(|s| s.owned && s.enabled) {
            let before = offset(state.position, scale);
            state.constrained = false;
            state.constraint_reason = None;
            for axis in 0..2 {
                let previous = state.position[axis];
                let minimum =
                    0.0_f64.max(previous.min(state.authoritative[axis] - self.max_prediction));
                let maximum = state.range[axis]
                    .min(previous.max(state.authoritative[axis] + self.max_prediction));
                let reachable = bounded(state.target[axis], minimum, maximum);
                let limited = reachable != state.target[axis];
                state.constrained |= limited;
                let paced = limited && self.response_seconds.is_some();
                if limited {
                    state.constraint_reason = Some(if paced {
                        "response-pacing"
                    } else {
                        "retained-content-boundary"
                    });
                }
                let target = if paced { state.target[axis] } else { reachable };
                // Before any response timing exists, a target beyond the retained content approaches
                // that boundary conservatively, at omega min(28/s, the spring's).
                let omega = f32_round(if limited && !paced {
                    28.0_f64.min(self.spring_frequency)
                } else {
                    self.spring_frequency
                });
                // The spring itself approaches the reachable target: clamping only its position
                // accumulated velocity toward unseen content, then jumped to each new buffer edge.
                // Same float32 grouping as scrolling::step_axis (Studio rounds every operation;
                // regrouping the position term algebraically loses parity).
                let displacement = f32_round(previous - target);
                let omega_dt = f32_round(omega * dt);
                let decay = f32_round((-omega_dt).exp());
                let blend = f32_round(state.velocity[axis] + f32_round(omega * displacement));
                let term = f32_round(
                    f32_round(displacement * f32_round(1.0 + omega_dt))
                        + f32_round(state.velocity[axis] * dt),
                );
                state.position[axis] = f32_round(target + f32_round(term * decay));
                state.velocity[axis] = f32_round(
                    f32_round(state.velocity[axis] - f32_round(omega * f32_round(blend * dt)))
                        * decay,
                );
                if paced {
                    let speed = self.max_prediction * 0.9
                        / (2.0 * (1.0 / 120.0_f64).max(self.response_seconds.unwrap()));
                    let distance = state.position[axis] - previous;
                    if distance.abs() > speed * dt {
                        state.position[axis] = previous + distance.signum() * speed * dt;
                        state.velocity[axis] = distance.signum() * speed;
                    }
                    state.velocity[axis] = bounded(state.velocity[axis], -speed, speed);
                }
                if (target - state.position[axis]).abs() < 1.0 {
                    state.position[axis] = target;
                    state.velocity[axis] = 0.0;
                }
                // A delayed same-epoch reply can move the authoritative window behind the displayed
                // pose. Hold there until it catches up: the bound must not create motion the other way.
                let bounded_position = bounded(state.position[axis], minimum, maximum);
                if bounded_position != state.position[axis] {
                    state.constrained = true;
                    state.constraint_reason = Some("retained-content-boundary");
                }
                if (state.position[axis] > maximum && state.velocity[axis] > 0.0)
                    || (state.position[axis] < minimum && state.velocity[axis] < 0.0)
                {
                    state.velocity[axis] = 0.0;
                }
                state.position[axis] = bounded_position;
            }
            moved |= before != offset(state.position, scale);
        }
        moved
    }
    pub fn snapshot(&mut self) -> Value {
        self.snapshot_at(None)
    }
    /// The ownership packet sent with an event. With `presented` (the `inspect()` of the last drawn
    /// frame), same-epoch owned scrollers report the drawn position and velocity, so pointer events
    /// hit-test what the user saw; the pending wheel target is always current.
    pub fn snapshot_at(&mut self, presented: Option<&Value>) -> Value {
        self.sequence += 1;
        let captured: HashMap<_, _> = presented
            .and_then(|v| v.get("scrolls"))
            .and_then(|v| array(v).ok())
            .unwrap_or(&[])
            .iter()
            .filter_map(|s| Some((node_id(&s["id"])?, s)))
            .collect();
        let scrolls:Vec<_>=self.states.values().filter(|s|s.owned).map(|s|{
            let before=captured.get(&s.id).filter(|v|node_id(&v["epoch"])==Some(s.epoch));
            let previous=before.and_then(|v|Some((pair(v.get("position"))?,pair(v.get("velocity"))?)));
            let (position,velocity)=previous.unwrap_or((s.position,s.velocity));
            json!({"id":s.id,"position":position,"velocity":velocity,"target":s.target,"epoch":s.epoch})
        }).collect();
        json!({"version":1,"seq":self.sequence,"scrolls":scrolls})
    }
    pub fn inspect(&self) -> Value {
        let scrolls:Vec<_>=self.states.values().map(|s|json!({"id":s.id,"epoch":s.epoch,"owned":s.owned,"position":s.position,"authoritative":s.authoritative,"target":s.target,"velocity":s.velocity})).collect();
        let active = self
            .states
            .values()
            .find(|s| s.owned && s.position != s.target)
            .or_else(|| self.states.values().find(|s| s.owned));
        let constrained = self.states.values().any(|s| s.owned && s.constrained);
        let reason = self
            .states
            .values()
            .filter(|s| s.owned)
            .filter_map(|s| s.constraint_reason)
            .last();
        let mode = if constrained {
            "transport-limited"
        } else if self.spring_frequency == SPRING_FREQUENCY {
            "studio-model"
        } else {
            "custom-spring"
        };
        json!({"motionMode":mode,"constraintReason":reason,"sequence":self.sequence,"acknowledged":self.acknowledged,"scrolls":scrolls,"active":active.map(|s|s.id),"presentedScrollY":active.map(|s|offset(s.position,self.scale)[1]),"logicalScrollY":active.map(|s|s.authoritative[1]),"fallback":self.fallback})
    }
    fn release_with_error(&mut self, error: String) {
        self.fallback = Some(error);
        for state in self.states.values_mut() {
            state.owned = false;
        }
    }
    pub fn project(&mut self, scene: &SceneStore) -> SceneStore {
        let _memory = crate::memtrace::scope(crate::memtrace::Tag::Presentation);
        self.projected_nodes = 0;
        let positions = self.positions();
        if let Some((revision, previous, projected)) = &self.projection {
            if *revision == scene.revision && *previous == positions {
                return projected.clone();
            }
        }
        match project_scroll_scene_memo(scene, &positions, &mut self.memo) {
            Ok(mut projected) => {
                self.projected_nodes = PROJECTED_NODES.with(|count| count.get());
                self.fallback = None;
                if !projected.shares_nodes(scene) {
                    self.display_revision = self.display_revision.wrapping_add(1);
                    projected.revision = (1_u64 << 63) | self.display_revision;
                }
                // The superseded projection's moved nodes are freed off this thread
                crate::render::drop_later(self.projection.replace((scene.revision, positions, projected.clone())));
                projected
            }
            Err(error) => {
                self.projection = None;
                self.release_with_error(error);
                scene.clone()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moved_rich_centred_lines_follow_the_engine_pen_at_every_scale() {
        // Width 8 (not a multiple of 3): inexact over 1.5 (half 3), exact over 1 and 2 (half 4)
        for (scale, px) in [(1.0, 46.0), (2.0, 46.0), (1.5, 47.0)] {
            let mut text = json!({"rich": true, "xAlign": "Center", "content": [0.0, 0.0, 100.0, 20.0],
                "lines": [{"x": 46.0, "top": 0.0, "width": 8.0, "px": 46.0}]});
            move_text(&mut text, 0.0, 0.0, scale);
            assert_eq!(text["lines"][0]["px"], json!(px), "scale {scale}");
        }
    }

    fn scene(value: &Value) -> SceneStore {
        let mut scene = SceneStore::default();
        scene.apply(value).unwrap();
        scene
    }

    fn scroller() -> SceneStore {
        scene(
            &json!({"viewport":{"width":400,"height":300,"scale":1.5},"layers":[{"draw":[1]}],
            "nodes":[{"id":1,"rect":[0,0,400,300],"clipOwners":[],"scroll":{
                "window":[0,0,400,300],"canvas":[0,0,400,2000],"canvasPosition":[0,0],"maxRange":[0,1700],"direction":"Y","epoch":0}}]}),
        )
    }

    #[test]
    fn recorded_studio_wheel_coordinates_are_exact() {
        // Studio's recorded wheel trajectories (tools/interaction/experiments/scroll-physics.mjs).
        let reference: Value = serde_json::from_str(include_str!(
            "../../tools/interaction/reference/scroll-physics.json"
        ))
        .unwrap();
        let mut coordinates = 0;
        for test in reference["cases"].as_array().unwrap() {
            let actions = test["actions"].as_array().unwrap();
            if actions
                .iter()
                .any(|a| a.get("enabled").is_some() || a["clear"].as_bool() == Some(true))
            {
                continue;
            }
            let window = pair(test.get("window")).unwrap();
            let canvas = pair(test.get("canvas")).unwrap();
            let first = &test["samples"][0];
            let mut range = [
                (canvas[0] - window[0]).max(0.0),
                (canvas[1] - window[1]).max(0.0),
            ];
            let mut value = json!({"viewport":{"width":window[0],"height":window[1],"scale":1},"layers":[{"draw":[1]}],
                "nodes":[{"id":1,"rect":[0,0,window[0],window[1]],"clipOwners":[],"scroll":{
                    "window":[0,0,window[0],window[1]],"canvas":[0,-first["y"].as_f64().unwrap(),canvas[0],canvas[1]],
                    "canvasPosition":[first["x"],first["y"]],"maxRange":range,"scrollingEnabled":true,"direction":"Y"}}]});
            let mut source = scene(&value);
            let mut motion = ScrollPresentation::new(f64::INFINITY, SPRING_FREQUENCY);
            motion.update(&source, None);
            let mut epoch = 0;
            for (index, sample) in test["samples"].as_array().unwrap().iter().enumerate() {
                motion.advance(&source, sample["dt"].as_f64().unwrap());
                let state = &motion.states[&1];
                assert_eq!(
                    state.position,
                    [sample["x"].as_f64().unwrap(), sample["y"].as_f64().unwrap()],
                    "{} frame {}",
                    test["name"],
                    index + 1
                );
                assert!(!state.constrained);
                coordinates += 2;
                for action in actions
                    .iter()
                    .filter(|a| a["frame"].as_u64() == Some((index + 1) as u64))
                {
                    if let Some(wheel) = action["wheel"].as_f64() {
                        motion.wheel(&source, 10.0, 10.0, wheel);
                    }
                    if action["reset"].as_bool() == Some(true)
                        || action.get("position").is_some()
                        || action.get("canvas").is_some()
                    {
                        let state = &motion.states[&1];
                        let mut position = [
                            state.position[0],
                            action["position"].as_f64().unwrap_or(state.position[1]),
                        ];
                        if let Some(canvas_y) = action["canvas"].as_f64() {
                            let canvas_y = canvas_y * test["scale"].as_f64().unwrap();
                            range[1] = (canvas_y - window[1]).max(0.0);
                            position[1] = position[1].min(range[1]);
                            value["nodes"][0]["scroll"]["maxRange"] = json!(range);
                            value["nodes"][0]["scroll"]["canvas"] =
                                json!([0.0, -position[1], canvas[0], canvas_y]);
                        }
                        epoch += 1;
                        value["nodes"][0]["scroll"]["canvasPosition"] = json!(position);
                        value["nodes"][0]["scroll"]["epoch"] = json!(epoch);
                        source.apply(&value).unwrap();
                        motion.update(&source,Some(&json!({"version":1,"seq":index+1,"scrolls":[{"id":1,"epoch":epoch,"position":position,"target":position,"velocity":[0,0],"range":range,"enabled":true}]})));
                        assert_eq!(motion.snapshot()["scrolls"], json!([]));
                        assert_eq!(motion.states[&1].target, position);
                        assert_eq!(motion.states[&1].velocity, [0.0; 2]);
                    }
                }
            }
        }
        assert!(coordinates > 100, "must exercise recorded trajectories");
    }

    #[test]
    fn pending_targets_survive_ack_and_pointer_uses_last_drawn_pose() {
        let mut source = scroller();
        let mut motion = ScrollPresentation::default();
        motion.update(&source, None);
        assert_eq!(motion.wheel(&source, 10.0, 10.0, -1.0), Some(1));
        motion.advance(&source, 1.0 / 120.0);
        let drawn = motion.inspect();
        motion.advance(&source, 1.0 / 120.0);
        motion.wheel(&source, 10.0, 10.0, -1.0);
        let snapshot = motion.snapshot_at(Some(&drawn));
        assert_eq!(
            snapshot["scrolls"][0]["position"],
            drawn["scrolls"][0]["position"]
        );
        assert_eq!(snapshot["scrolls"][0]["target"], json!([0.0, 200.0]));
        let before = motion.states[&1].position;
        motion.update(&source,Some(&json!({"version":1,"seq":1,"scrolls":[{"id":1,"epoch":0,"position":[0,1],"target":[0,100],"range":[0,1700]}]})));
        assert_eq!(motion.states[&1].position, before);
        assert_eq!(motion.states[&1].target, [0.0, 200.0]);
        let mut node = source.node(1).unwrap().clone();
        node["scroll"]["epoch"] = json!(1);
        node["scroll"]["canvasPosition"] = json!([0, 7]);
        source.apply(&json!({"delta":{"nodes":[node]}})).unwrap();
        motion.update(&source, None);
        assert!(!motion.states[&1].owned);
        assert_eq!(motion.states[&1].position, [0.0, 7.0]);
    }

    #[test]
    fn prediction_stays_inside_retained_range_and_delayed_ack_cannot_reverse_it() {
        let source = scroller();
        let mut motion = ScrollPresentation::default();
        motion.update(&source, None);
        motion.wheel(&source, 10.0, 10.0, -20.0);
        motion.observe_response(0.07);
        let mut previous = 0.0;
        for _ in 0..240 {
            motion.advance(&source, 1.0 / 120.0);
            let y = motion.states[&1].position[1];
            assert!(y >= previous && y <= 200.0);
            previous = y;
        }
        assert_eq!(previous, 200.0);
        motion.states.get_mut(&1).unwrap().authoritative[1] = -100.0;
        motion.advance(&source, 1.0 / 120.0);
        assert_eq!(motion.states[&1].position[1], previous);
        assert_eq!(
            motion.states[&1].constraint_reason,
            Some("retained-content-boundary")
        );
    }

    #[test]
    fn memoized_projection_matches_a_fresh_one_and_keeps_unchanged_nodes() {
        let cases: Value =
            serde_json::from_str(include_str!("../tests/fixtures/rotation-projection.json"))
                .unwrap();
        let mut incremental = 0;
        for item in cases.as_array().unwrap() {
            let mut source = scene(&item["initial"]);
            let positions = HashMap::from([(
                node_id(&item["outer"]).unwrap(),
                [0.0, item["target"].as_f64().unwrap()],
            )]);
            let mut memo = ProjectionMemo::default();
            let mut previous = project_scroll_scene_memo(&source, &positions, &mut memo).unwrap();
            assert_eq!(previous.to_value(), project_scroll_scene(&source, &positions).unwrap().to_value());
            let ids: Vec<NodeId> = source
                .ordered_nodes()
                .filter(|node| node.get("scroll").is_none())
                .map(|node| node_id(&node["id"]).unwrap())
                .collect();
            // Every node that is not a scroller changes in turn: a property the projection copies, or its
            // rect (an owner's clip rect then changes the nodes it clips)
            for (k, id) in ids.iter().enumerate() {
                let mut node = source.node(*id).unwrap().clone();
                if k % 2 == 0 {
                    node["probe"] = json!(k);
                } else if let Some(r) = node.get_mut("rect").and_then(Value::as_array_mut) {
                    r[1] = json!(r[1].as_f64().unwrap() + 3.0);
                }
                source.apply(&json!({"delta":{"nodes":[node]}})).unwrap();
                let projected = project_scroll_scene_memo(&source, &positions, &mut memo).unwrap();
                assert_eq!(
                    projected.to_value(),
                    project_scroll_scene(&source, &positions).unwrap().to_value(),
                    "{} after changing node {id}",
                    item["name"]
                );
                // A node that did not change keeps its projected value (its identity)
                let kept = source
                    .node_arcs()
                    .filter(|(other, _)| other != id)
                    .filter(|(other, _)| {
                        Arc::ptr_eq(projected.node_arc(*other).unwrap(), previous.node_arc(*other).unwrap())
                    })
                    .count();
                assert!(kept + 1 >= source.node_count() / 2, "{kept} of {} kept", source.node_count());
                previous = projected;
            }
            incremental += memo.incremental;
        }
        assert!(incremental > 0);
    }

    #[test]
    fn retained_projection_matches_runtime_rotated_clips_and_culling() {
        // Generated unchanged by tools/tests/rotation_projection.luau; canonical exports are the oracle.
        let cases: Value =
            serde_json::from_str(include_str!("../tests/fixtures/rotation-projection.json"))
                .unwrap();
        let mut transitions = 0;
        for item in cases.as_array().unwrap() {
            let source = scene(&item["initial"]);
            let original = source.to_value();
            let projected = project_scroll_scene(
                &source,
                &HashMap::from([(
                    node_id(&item["outer"]).unwrap(),
                    [0.0, item["target"].as_f64().unwrap()],
                )]),
            )
            .unwrap();
            assert_eq!(
                source.to_value(),
                original,
                "projection must preserve the authoritative scene"
            );
            for expected in item["authoritative"]["nodes"].as_array().unwrap() {
                let id = node_id(&expected["id"]).unwrap();
                let actual = projected.node(id).unwrap();
                assert_eq!(actual.get("rotationCull"), expected.get("rotationCull"));
                for field in ["/rect", "/absolute", "/matrix", "/clip", "/scroll/window"] {
                    match (actual.pointer(field), expected.pointer(field)) {
                        (Some(a), Some(b)) => {
                            let a = a.as_array().unwrap();
                            let b = b.as_array().unwrap();
                            assert_eq!(a.len(), b.len());
                            for (a, b) in a.iter().zip(b) {
                                assert!(
                                    (a.as_f64().unwrap() - b.as_f64().unwrap()).abs() <= 1e-4,
                                    "{} {}: {} != {}",
                                    expected["name"],
                                    field,
                                    a,
                                    b
                                );
                            }
                        }
                        (None, None) => {}
                        _ => panic!("{} {} presence mismatch", expected["name"], field),
                    }
                }
                assert_eq!(
                    actual["culled"].as_bool().unwrap_or(false),
                    expected["culled"].as_bool().unwrap_or(false)
                );
                if item["modern"].as_bool() == Some(false)
                    && expected.get("rotationCull").is_some()
                    && source.node(id).unwrap()["culled"]
                        .as_bool()
                        .unwrap_or(false)
                        != expected["culled"].as_bool().unwrap_or(false)
                {
                    transitions += 1;
                }
            }
        }
        assert!(transitions >= 2, "must reveal and hide rotated edges");
    }
}
