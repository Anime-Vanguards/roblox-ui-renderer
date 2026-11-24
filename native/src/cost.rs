//! `rur cost` (`rur-native cost`): deterministic Roblox UI render-cost pass.
//!
//! Counts what Roblox's 2D UI renderer does for a screen (or a saved `scene.json`) without a GPU
//! device, rasterization, or host wall-clock timing fields:
//! - Roblox `Pass2d` draw calls, batch breaks (by cause: layerPass, renderTarget, clip, shader,
//!   texture, rotation), non-glyph quads by kind, glyph quads (fill + legacy 4-offset TextStroke +
//!   contextual UIStroke), and clipped screen / offscreen overdraw in viewport multiples.
//! - `CanvasGroup` offscreen render-target allocations using exact Studio rectangular RGBA8
//!   dimensions (`render::geometry::group_texture_dims`: float32 logical size, 2048 px side cap,
//!   2048 x 1024 px area cap, skipping culled, empty, and fully transparent groups), RT switches,
//!   and nesting depth.
//! - Live-DOM hidden-in-shown accounting (`Visible = false` roots and all descendant GuiObjects and
//!   instances under enabled ScreenGuis), separated from disabled ScreenGuis (`Enabled = false`).
//! - Deterministic `Layout.Stats` counters, calibrated desktop layout CPU ms (Studio 0.744 client
//!   slopes from `engine/layout/install.rs`), anchored `phone-mid` layout CPU ms with uncertainty
//!   band, and explicit refusals (`status: "refused"`, `ms: null`) for uncalibrated draw-pass CPU
//!   and mobile GPU UI milliseconds.
//! - `rur cost compare <before> <after>` deterministic A/B diff (`rrr-ab/1`).

use std::{
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
};

use serde_json::{Map, Value, json};

use crate::{
    engine::{
        self, Engine, InstanceId, K, NameId,
        layout::Stats as LayoutStats,
        services,
        value::Value as EngineValue,
    },
    render::geometry::{
        Rect, alpha8, clip_rect, device_span, group_texture_dims, node_transform, physical_rect,
        text_clip_rect, transformed_extent,
    },
    scene::{SceneStore, layer_pass, node_id},
    story_args::{Args, Context, Failure},
};

/// Recorded Studio 0.744 client lazy-layout calibration (`native/src/engine/layout/install.rs`).
pub const STUDIO_LAYOUT_PASS_MS: f64 = 2.3e-3;
pub const STUDIO_LAYOUT_OBJECT_MS: f64 = 0.312e-3;
pub const STUDIO_SCROLL_PASS_MS: f64 = 62.0e-3;

/// Recorded live frame-weighted mobile mid-tier UI main-thread scaling ratio and uncertainty band.
pub const PHONE_UI_MAIN_RATIO: f64 = 4.09 * 0.77;
pub const PHONE_UI_MAIN_LOW: f64 = 2.65 * 0.77;
pub const PHONE_UI_MAIN_HIGH: f64 = 8.0 * 0.77;

const MAX_OBJECTS_REPORTED: usize = 200;
const MAX_HIDDEN_SUBTREES_REPORTED: usize = 100;

/// Per-root hidden-in-shown counts collected from the live DOM under an enabled LayerCollector.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RootHidden {
    pub roots: u64,
    pub gui_objects: u64,
    pub instances: u64,
}

/// One top-most `Visible = false` GuiObject inside an enabled ScreenGui.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HiddenSubtree {
    pub id: u64,
    pub name: String,
    pub path: String,
    pub class: String,
    pub screen_gui: String,
    pub gui_objects: u64,
    pub instances: u64,
}

/// One `Enabled = false` LayerCollector found in PlayerGui or CoreGui (separated from `hiddenInShown`).
#[derive(Clone, Debug, PartialEq)]
pub struct DisabledGui {
    pub id: u64,
    pub name: String,
    pub path: String,
    pub pass: &'static str,
    pub display_order: f64,
    pub gui_objects: u64,
    pub instances: u64,
}

/// Live-DOM hidden-in-shown and disabled ScreenGui summary collected before engine teardown.
#[derive(Clone, Debug, Default)]
pub struct DomHiddenSummary {
    /// True when walked from the live DOM; false when derived only from a saved `scene.json`
    /// (which drops descendants below `Visible = false` nodes).
    pub complete: bool,
    pub roots: u64,
    pub gui_objects: u64,
    pub instances: u64,
    pub disabled_screen_guis: u64,
    pub disabled_screen_gui_instances: u64,
    pub by_root: BTreeMap<u64, RootHidden>,
    pub disabled_guis: Vec<DisabledGui>,
    pub subtrees: Vec<HiddenSubtree>,
}

/// Walks the live DOM under enabled exported roots to count `Visible = false` subtrees (all
/// descendants, including non-GuiObject instances) and separately inventories disabled ScreenGuis
/// under PlayerGui and CoreGui.
pub fn collect_dom_hidden(engine: &Engine, roots: &[InstanceId]) -> DomHiddenSummary {
    let reflection = &engine.reflection;
    let layer_collector = reflection.class_id("LayerCollector");
    let gui_object = reflection.class_id("GuiObject");
    let gui_base = reflection.class_id("GuiBase2d");
    let world_bound = [reflection.class_id("SurfaceGuiBase"), reflection.class_id("BillboardGui")];
    let game = engine.world().game;
    let core_gui = services::get_service(engine, game, "CoreGui").ok();
    let state = engine.state.borrow();
    let dom = &state.dom;
    let player_gui = engine
        .world()
        .local_player
        .zip(reflection.class_id("PlayerGui"))
        .and_then(|(player, pg)| dom.find_child_of_class(player, pg));

    let is_class = |id: InstanceId, target: Option<engine::ClassId>| -> bool {
        target.is_some_and(|c| reflection.class(dom.at(id).class).is_a(c))
    };

    fn count_subtree(
        dom: &engine::dom::Dom,
        reflection: &engine::Reflection,
        gui_object: Option<engine::ClassId>,
        root: InstanceId,
    ) -> (u64, u64) {
        let mut gui_objects = 0u64;
        let mut instances = 0u64;
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            let inst = dom.at(id);
            instances += 1;
            if gui_object.is_some_and(|c| reflection.class(inst.class).is_a(c)) {
                gui_objects += 1;
            }
            for &child in inst.children.iter().rev() {
                stack.push(child);
            }
        }
        (gui_objects, instances)
    }

    let mut summary = DomHiddenSummary {
        complete: true,
        ..Default::default()
    };
    let mut seen_enabled = HashSet::new();
    let mut seen_disabled = HashSet::new();

    for &root in roots {
        if dom.get(root).is_none() {
            continue;
        }
        let enabled = !matches!(dom.raw_get(root, K::Enabled as NameId), Some(EngineValue::Bool(false)));
        if !enabled {
            if seen_disabled.insert(root) {
                let (go, insts) = count_subtree(dom, reflection, gui_object, root);
                let display_order = dom
                    .raw_get(root, K::DisplayOrder as NameId)
                    .and_then(EngineValue::as_number)
                    .unwrap_or(0.0);
                let in_player_gui = player_gui.is_some_and(|pg| dom.is_descendant_of(root, pg));
                summary.disabled_screen_guis += 1;
                summary.disabled_screen_gui_instances += insts;
                summary.disabled_guis.push(DisabledGui {
                    id: u64::from(dom.at(root).serial),
                    name: dom.at(root).name.to_string(),
                    path: dom.full_name(reflection, root),
                    pass: if in_player_gui { "PlayerGui" } else { "CoreGui" },
                    display_order,
                    gui_objects: go,
                    instances: insts,
                });
            }
            continue;
        }
        if !seen_enabled.insert(root) {
            continue;
        }
        let root_serial = u64::from(dom.at(root).serial);
        let sg_name = dom.at(root).name.to_string();
        let entry = summary.by_root.entry(root_serial).or_default();
        let mut stack: Vec<InstanceId> = dom.at(root).children.iter().rev().copied().collect();
        while let Some(child) = stack.pop() {
            if is_class(child, layer_collector) {
                continue;
            }
            if is_class(child, gui_object) {
                let visible = !matches!(dom.raw_get(child, K::Visible as NameId), Some(EngineValue::Bool(false)));
                if !visible {
                    let (go, insts) = count_subtree(dom, reflection, gui_object, child);
                    entry.roots += 1;
                    entry.gui_objects += go;
                    entry.instances += insts;
                    summary.roots += 1;
                    summary.gui_objects += go;
                    summary.instances += insts;
                    summary.subtrees.push(HiddenSubtree {
                        id: u64::from(dom.at(child).serial),
                        name: dom.at(child).name.to_string(),
                        path: dom.full_name(reflection, child),
                        class: reflection.class(dom.at(child).class).name.to_string(),
                        screen_gui: sg_name.clone(),
                        gui_objects: go,
                        instances: insts,
                    });
                    continue;
                }
            }
            for &grand in dom.at(child).children.iter().rev() {
                stack.push(grand);
            }
        }
    }

    // Inventory disabled LayerCollectors under PlayerGui and CoreGui (separated from hiddenInShown).
    for (container, pass) in [(player_gui, "PlayerGui"), (core_gui, "CoreGui")] {
        let Some(container) = container else { continue };
        let mut stack: Vec<InstanceId> = dom.at(container).children.iter().rev().copied().collect();
        while let Some(id) = stack.pop() {
            if is_class(id, layer_collector) {
                let in_world = world_bound.iter().any(|&wb| is_class(id, wb));
                let enabled = !matches!(dom.raw_get(id, K::Enabled as NameId), Some(EngineValue::Bool(false)));
                if !in_world && !enabled && seen_disabled.insert(id) {
                    let (go, insts) = count_subtree(dom, reflection, gui_object, id);
                    let display_order = dom
                        .raw_get(id, K::DisplayOrder as NameId)
                        .and_then(EngineValue::as_number)
                        .unwrap_or(0.0);
                    summary.disabled_screen_guis += 1;
                    summary.disabled_screen_gui_instances += insts;
                    summary.disabled_guis.push(DisabledGui {
                        id: u64::from(dom.at(id).serial),
                        name: dom.at(id).name.to_string(),
                        path: dom.full_name(reflection, id),
                        pass,
                        display_order,
                        gui_objects: go,
                        instances: insts,
                    });
                }
            } else if !is_class(id, gui_base) {
                for &child in dom.at(id).children.iter().rev() {
                    stack.push(child);
                }
            }
        }
    }

    summary.subtrees.sort_by(|a, b| {
        b.gui_objects
            .cmp(&a.gui_objects)
            .then_with(|| b.instances.cmp(&a.instances))
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.id.cmp(&b.id))
    });
    summary.disabled_guis.sort_by(|a, b| {
        a.pass
            .cmp(b.pass)
            .then_with(|| a.display_order.total_cmp(&b.display_order))
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.id.cmp(&b.id))
    });
    summary
}

/// Fallback hidden summary when analyzing a saved `scene.json` without a live DOM.
fn scene_only_hidden(scene: &SceneStore) -> DomHiddenSummary {
    let mut summary = DomHiddenSummary {
        complete: false,
        ..Default::default()
    };
    let mut root_names: BTreeMap<u64, String> = BTreeMap::new();
    let mut parent_of: BTreeMap<u64, u64> = BTreeMap::new();
    let mut disabled_root_ids: HashSet<u64> = HashSet::new();

    if let Some(layers) = scene.layers().as_array() {
        for layer in layers {
            if let Some(rid) = layer.get("root").and_then(node_id) {
                if let Some(node) = scene.node(rid) {
                    let name = node["name"].as_str().unwrap_or("?").to_string();
                    root_names.insert(rid, name.clone());
                    if node["enabled"].as_bool() == Some(false) {
                        disabled_root_ids.insert(rid);
                        summary.disabled_screen_guis += 1;
                        summary.disabled_screen_gui_instances += 1;
                        summary.disabled_guis.push(DisabledGui {
                            id: rid,
                            name,
                            path: node["path"].as_str().unwrap_or("?").to_string(),
                            pass: if layer_pass(layer) == 0 { "PlayerGui" } else { "CoreGui" },
                            display_order: layer["displayOrder"].as_f64().unwrap_or(0.0),
                            gui_objects: 0,
                            instances: 1,
                        });
                    }
                }
            }
        }
    }

    for node in scene.ordered_nodes() {
        if let (Some(id), Some(parent)) = (node.get("id").and_then(node_id), node.get("parent").and_then(node_id)) {
            parent_of.insert(id, parent);
        }
    }

    for node in scene.ordered_nodes() {
        if node["visible"].as_bool() != Some(false) {
            continue;
        }
        let Some(id) = node.get("id").and_then(node_id) else { continue };
        let mut cursor = id;
        while let Some(&p) = parent_of.get(&cursor) {
            cursor = p;
        }
        if disabled_root_ids.contains(&cursor) {
            continue;
        }
        let sg_name = root_names.get(&cursor).cloned().unwrap_or_else(|| "?".to_string());
        let entry = summary.by_root.entry(cursor).or_default();
        entry.roots += 1;
        entry.gui_objects += 1;
        entry.instances += 1;
        summary.roots += 1;
        summary.gui_objects += 1;
        summary.instances += 1;
        summary.subtrees.push(HiddenSubtree {
            id,
            name: node["name"].as_str().unwrap_or("?").to_string(),
            path: node["path"].as_str().unwrap_or("?").to_string(),
            class: node["class"].as_str().unwrap_or("?").to_string(),
            screen_gui: sg_name,
            gui_objects: 1,
            instances: 1,
        });
    }
    summary.subtrees.sort_by(|a, b| a.path.cmp(&b.path).then_with(|| a.id.cmp(&b.id)));
    summary
}

/// Axis-aligned device rect of a node ignoring rotation matrices (used for CanvasGroup offscreen targets).
fn flat_physical_rect(node: &Value, scale: f64) -> Rect {
    let rect = &node["rect"];
    let x = rect.get(0).and_then(Value::as_f64).unwrap_or(0.0);
    let y = rect.get(1).and_then(Value::as_f64).unwrap_or(0.0);
    let w = rect.get(2).and_then(Value::as_f64).unwrap_or(0.0);
    let h = rect.get(3).and_then(Value::as_f64).unwrap_or(0.0);
    let [l, r] = device_span(x, w, scale);
    let [t, b] = device_span(y, h, scale);
    Rect::new(l, t, r - l, b - t)
}

fn has_corner(node: &Value) -> bool {
    match node.get("corner") {
        Some(Value::Number(n)) => n.as_f64().is_some_and(|v| v > 0.0),
        Some(Value::Array(items)) => items.iter().any(|v| v.as_f64().is_some_and(|n| n > 0.0)),
        _ => false,
    }
}

fn is_rotated(node: &Value) -> bool {
    let Some(m) = node.get("matrix").and_then(Value::as_array) else { return false };
    if m.len() < 6 {
        return false;
    }
    let m0 = m[0].as_f64().unwrap_or(1.0);
    let m1 = m[1].as_f64().unwrap_or(0.0);
    let m2 = m[2].as_f64().unwrap_or(0.0);
    let m3 = m[3].as_f64().unwrap_or(1.0);
    (m0 - 1.0).abs() > 1e-6 || m1.abs() > 1e-6 || m2.abs() > 1e-6 || (m3 - 1.0).abs() > 1e-6
}

fn quad_bounds_device(node: &Value, rect: Rect, scale: f64) -> [f64; 4] {
    if node.get("matrix").and_then(Value::as_array).is_some_and(|m| m.len() >= 6) {
        let m = node_transform(node, [0.0, 0.0], scale);
        transformed_extent(m, rect)
    } else {
        rect.bounds()
    }
}

fn intersect_area(bounds: [f64; 4], clip: Option<[f64; 4]>, target: [f64; 4]) -> f64 {
    let (mut cx0, mut cy0, mut cx1, mut cy1) = (target[0], target[1], target[2], target[3]);
    if let Some(c) = clip {
        cx0 = cx0.max(c[0]);
        cy0 = cy0.max(c[1]);
        cx1 = cx1.min(c[2]);
        cy1 = cy1.min(c[3]);
    }
    let ix0 = bounds[0].max(cx0);
    let iy0 = bounds[1].max(cy0);
    let ix1 = bounds[2].min(cx1);
    let iy1 = bounds[3].min(cy1);
    if ix1 <= ix0 || iy1 <= iy0 {
        0.0
    } else {
        (ix1 - ix0) * (iy1 - iy0)
    }
}

fn scissor_key(clip: Option<[f64; 4]>) -> Option<[i64; 4]> {
    clip.map(|c| [c[0].round() as i64, c[1].round() as i64, c[2].round() as i64, c[3].round() as i64])
}

fn round6(value: f64) -> f64 {
    if !value.is_finite() {
        return 0.0;
    }
    let rounded = (value * 1_000_000.0).round() / 1_000_000.0;
    if rounded == 0.0 { 0.0 } else { rounded }
}

fn round9(value: f64) -> f64 {
    if !value.is_finite() {
        return 0.0;
    }
    let rounded = (value * 1_000_000_000.0).round() / 1_000_000_000.0;
    if rounded == 0.0 { 0.0 } else { rounded }
}

/// Deterministic key-sorted JSON conversion (independent of Map insertion order).
pub fn sort_json(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<(String, Value)> = map.into_iter().collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            let mut sorted = Map::with_capacity(entries.len());
            for (k, v) in entries {
                sorted.insert(k, sort_json(v));
            }
            Value::Object(sorted)
        }
        Value::Array(items) => Value::Array(items.into_iter().map(sort_json).collect()),
        other => other,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BatchState {
    pass_id: u8,
    shader: &'static str,
    texture: Option<String>,
    scissor: Option<[i64; 4]>,
    rotated: bool,
}

impl BatchState {
    fn break_cause(prev: &BatchState, next: &BatchState) -> &'static str {
        if prev.pass_id != next.pass_id {
            "layerPass"
        } else if (prev.shader == "group_composite" || next.shader == "group_composite")
            && (prev.shader != next.shader || prev.texture != next.texture)
        {
            "renderTarget"
        } else if prev.scissor != next.scissor {
            "clip"
        } else if prev.shader != next.shader {
            "shader"
        } else if prev.texture != next.texture {
            "texture"
        } else {
            "rotation"
        }
    }
}

#[derive(Default)]
struct ScreenGuiAccum {
    id: u64,
    name: String,
    path: String,
    pass: &'static str,
    display_order: f64,
    enabled: bool,
    drawn_nodes: u64,
    quads: u64,
    glyph_quads: u64,
    draw_calls: u64,
    batch_breaks: u64,
    canvas_group_targets: u64,
    canvas_rt_bytes: u64,
    canvas_rt_switches: u64,
    screen_overdraw_px: f64,
    offscreen_overdraw_px: f64,
    hidden_roots: u64,
    hidden_gui_objects: u64,
    hidden_instances: u64,
    disabled_gui_objects: u64,
    disabled_instances: u64,
}

#[derive(Default)]
struct ObjectAccum {
    id: u64,
    name: String,
    path: String,
    class: String,
    screen_gui: String,
    quads: u64,
    glyph_quads: u64,
    batch_breaks: u64,
    canvas_rt_bytes: u64,
    screen_overdraw_px: f64,
    offscreen_overdraw_px: f64,
}

#[derive(Default)]
struct TotalsAccum {
    screen_guis: u64,
    total_nodes: u64,
    drawn_nodes: u64,
    quads: u64,
    quads_by_kind: BTreeMap<&'static str, u64>,
    glyph_quads: u64,
    fill_glyph_quads: u64,
    stroke_glyph_quads: u64,
    draw_calls: u64,
    batch_breaks: u64,
    batch_breaks_by_cause: BTreeMap<&'static str, u64>,
    canvas_groups: u64,
    canvas_group_targets: u64,
    canvas_rt_bytes: u64,
    canvas_rt_switches: u64,
    max_canvas_group_depth: u64,
    screen_overdraw_px: f64,
    offscreen_overdraw_px: f64,
}

struct Analyzer<'a> {
    scene: &'a SceneStore,
    scale: f64,
    totals: TotalsAccum,
    last_state_by_target: BTreeMap<u64, BatchState>,
    objects: BTreeMap<u64, ObjectAccum>,
}

impl<'a> Analyzer<'a> {
    fn new(scene: &'a SceneStore, scale: f64) -> Self {
        let mut totals = TotalsAccum {
            total_nodes: scene.node_count() as u64,
            ..Default::default()
        };
        for cause in ["layerPass", "renderTarget", "clip", "shader", "texture", "rotation"] {
            totals.batch_breaks_by_cause.insert(cause, 0);
        }
        Self {
            scene,
            scale,
            totals,
            last_state_by_target: BTreeMap::new(),
            objects: BTreeMap::new(),
        }
    }

    fn object_entry(&mut self, node: &Value, nid: u64, sg_name: &str) -> &mut ObjectAccum {
        self.objects.entry(nid).or_insert_with(|| ObjectAccum {
            id: nid,
            name: node["name"].as_str().unwrap_or("?").to_string(),
            path: node["path"].as_str().unwrap_or("?").to_string(),
            class: node["class"].as_str().unwrap_or("?").to_string(),
            screen_gui: sg_name.to_string(),
            ..Default::default()
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn record_batch(
        &mut self,
        sg: &mut ScreenGuiAccum,
        node: &Value,
        nid: u64,
        target_id: u64,
        state: BatchState,
        area_px: f64,
    ) {
        match self.last_state_by_target.get(&target_id) {
            None => {
                self.totals.draw_calls += 1;
                sg.draw_calls += 1;
            }
            Some(prev) if *prev != state => {
                let cause = BatchState::break_cause(prev, &state);
                self.totals.draw_calls += 1;
                self.totals.batch_breaks += 1;
                *self.totals.batch_breaks_by_cause.entry(cause).or_insert(0) += 1;
                sg.draw_calls += 1;
                sg.batch_breaks += 1;
                self.object_entry(node, nid, &sg.name).batch_breaks += 1;
            }
            Some(_) => {}
        }
        self.last_state_by_target.insert(target_id, state);
        if target_id == 0 {
            self.totals.screen_overdraw_px += area_px;
            sg.screen_overdraw_px += area_px;
            self.object_entry(node, nid, &sg.name).screen_overdraw_px += area_px;
        } else {
            self.totals.offscreen_overdraw_px += area_px;
            sg.offscreen_overdraw_px += area_px;
            self.object_entry(node, nid, &sg.name).offscreen_overdraw_px += area_px;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_quads(
        &mut self,
        sg: &mut ScreenGuiAccum,
        node: &Value,
        nid: u64,
        target_id: u64,
        pass_id: u8,
        quad_kind: &'static str,
        shader: &'static str,
        texture: Option<String>,
        clip: Option<[f64; 4]>,
        rotated: bool,
        quad_count: u64,
        area_px: f64,
    ) {
        if quad_count == 0 {
            return;
        }
        let state = BatchState {
            pass_id,
            shader,
            texture,
            scissor: scissor_key(clip),
            rotated,
        };
        self.record_batch(sg, node, nid, target_id, state, area_px);
        self.totals.quads += quad_count;
        *self.totals.quads_by_kind.entry(quad_kind).or_insert(0) += quad_count;
        sg.quads += quad_count;
        self.object_entry(node, nid, &sg.name).quads += quad_count;
    }

    fn draw_node(
        &mut self,
        sg: &mut ScreenGuiAccum,
        node: &Value,
        nid: u64,
        target_id: u64,
        target_bounds: [f64; 4],
        pass_id: u8,
        in_group: bool,
    ) {
        if node["culled"].as_bool() == Some(true) || node["visible"].as_bool() == Some(false) {
            return;
        }
        let mut drawn_any = false;
        let rot = is_rotated(node);

        // Path2D
        if let Some(p2d) = node.get("path2d").and_then(Value::as_object) {
            let segs = p2d.get("segments").and_then(Value::as_array).map_or(0, Vec::len) as u64;
            if segs > 0 {
                let phys = physical_rect(node, self.scale).rect;
                let clip = clip_rect(node, [0.0, 0.0], self.scale);
                let area = intersect_area(phys.bounds(), clip, target_bounds);
                if area > 0.0 {
                    self.emit_quads(sg, node, nid, target_id, pass_id, "path2d", "path2d", None, clip, false, segs, area);
                    drawn_any = true;
                }
            }
            if drawn_any {
                self.totals.drawn_nodes += 1;
                sg.drawn_nodes += 1;
            }
            return;
        }

        let phys = physical_rect(node, self.scale).rect;
        let bounds = quad_bounds_device(node, phys, self.scale);
        let clip = clip_rect(node, [0.0, 0.0], self.scale);
        let clipped_area = intersect_area(bounds, clip, target_bounds);
        let corner = if in_group { false } else { has_corner(node) };
        let gradient = if in_group { false } else { node.get("gradient").is_some_and(Value::is_object) };
        let has_text = node.get("text").is_some_and(Value::is_object);

        // Outer shadows
        if !in_group {
            if let Some(shadows) = node.get("shadows").and_then(Value::as_array) {
                for sh in shadows {
                    if sh.is_object()
                        && sh["inset"].as_bool() != Some(true)
                        && alpha8(1.0 - sh["transparency"].as_f64().unwrap_or(0.0)) > 0
                        && phys.w > 0.0
                        && phys.h > 0.0
                    {
                        self.emit_quads(sg, node, nid, target_id, pass_id, "shadow", "shadow", None, clip, rot, 1, clipped_area);
                        drawn_any = true;
                    }
                }
            }
        }

        // Background + Border
        if let Some(bg) = node.get("bg").and_then(Value::as_object) {
            let a = alpha8(1.0 - bg.get("transparency").and_then(Value::as_f64).unwrap_or(0.0));
            if a > 0 && phys.w > 0.0 && phys.h > 0.0 && clipped_area > 0.0 {
                let shader = if corner || gradient { "styled_fill" } else { "solid" };
                self.emit_quads(sg, node, nid, target_id, pass_id, shader, shader, None, clip, rot, 1, clipped_area);
                drawn_any = true;
                if !in_group && !corner {
                    if let Some(border) = node.get("border").and_then(Value::as_object) {
                        if border.get("size").and_then(Value::as_f64).unwrap_or(0.0) > 0.0 {
                            self.emit_quads(sg, node, nid, target_id, pass_id, "border", shader, None, clip, rot, 1, 0.0);
                        }
                    }
                }
            }
        }

        // Image
        if let Some(img) = node.get("image").and_then(Value::as_object) {
            if let Some(uri) = img.get("uri").and_then(Value::as_str).filter(|u| !u.is_empty()) {
                let a = alpha8(1.0 - img.get("transparency").and_then(Value::as_f64).unwrap_or(0.0));
                if a > 0 && phys.w > 0.0 && phys.h > 0.0 && clipped_area > 0.0 {
                    let shader = if corner || gradient { "styled_image" } else { "image" };
                    self.emit_quads(
                        sg,
                        node,
                        nid,
                        target_id,
                        pass_id,
                        shader,
                        shader,
                        Some(uri.to_string()),
                        clip,
                        rot,
                        1,
                        clipped_area,
                    );
                    drawn_any = true;
                }
            }
        }

        // Text
        if let Some(text) = node.get("text").and_then(Value::as_object) {
            if let Some(lines) = text.get("lines").and_then(Value::as_array).filter(|l| !l.is_empty()) {
                let a = alpha8(1.0 - text.get("transparency").and_then(Value::as_f64).unwrap_or(0.0));
                let tclip = text_clip_rect(node, [0.0, 0.0], self.scale);
                let mut glyphs = 0u64;
                let mut line_area = 0.0f64;
                for line in lines {
                    let Some(line_obj) = line.as_object() else { continue };
                    let Some(g) = line_obj.get("glyphs").and_then(Value::as_array).filter(|g| !g.is_empty()) else {
                        continue;
                    };
                    let drawn_g = match line_obj.get("shown").and_then(Value::as_u64) {
                        Some(shown) => shown + line_obj.get("ellipsis").and_then(Value::as_u64).unwrap_or(0),
                        None => g.len() as u64,
                    };
                    if drawn_g == 0 {
                        continue;
                    }
                    let lx = line_obj.get("x").and_then(Value::as_f64).unwrap_or(0.0);
                    let lt = line_obj.get("top").and_then(Value::as_f64).unwrap_or(0.0);
                    let lw = line_obj.get("width").and_then(Value::as_f64).unwrap_or(0.0);
                    let lh = line_obj.get("height").and_then(Value::as_f64).unwrap_or(0.0);
                    let la = intersect_area([lx, lt, lx + lw, lt + lh], tclip, target_bounds);
                    if la > 0.0 || rot {
                        glyphs += drawn_g;
                        line_area += la;
                    }
                }
                if a > 0 && glyphs > 0 {
                    let mut stroke_glyphs = 0u64;
                    if let Some(tstroke) = text.get("stroke").and_then(Value::as_object) {
                        let st_a = alpha8(1.0 - tstroke.get("transparency").and_then(Value::as_f64).unwrap_or(1.0));
                        if st_a > 0 {
                            stroke_glyphs += 4 * glyphs;
                        }
                    }
                    if let Some(strokes) = node.get("strokes").and_then(Value::as_array) {
                        for st in strokes {
                            let mode = st.get("mode").or_else(|| st.get("applyMode")).and_then(Value::as_str);
                            if mode == Some("Contextual")
                                && st["thickness"].as_f64().unwrap_or(0.0) > 0.0
                                && alpha8(1.0 - st["transparency"].as_f64().unwrap_or(0.0)) > 0
                            {
                                stroke_glyphs += glyphs;
                            }
                        }
                    }
                    let total_g = glyphs + stroke_glyphs;
                    self.totals.fill_glyph_quads += glyphs;
                    self.totals.stroke_glyph_quads += stroke_glyphs;
                    self.totals.glyph_quads += total_g;
                    sg.glyph_quads += total_g;
                    self.object_entry(node, nid, &sg.name).glyph_quads += total_g;
                    let state = BatchState {
                        pass_id,
                        shader: "text",
                        texture: Some("font-atlas".to_string()),
                        scissor: scissor_key(tclip),
                        rotated: rot,
                    };
                    self.record_batch(sg, node, nid, target_id, state, line_area);
                    drawn_any = true;
                }
            }
        }

        // Inset shadows + border UIStrokes
        if !in_group {
            if let Some(shadows) = node.get("shadows").and_then(Value::as_array) {
                for sh in shadows {
                    if sh.is_object()
                        && sh["inset"].as_bool() == Some(true)
                        && alpha8(1.0 - sh["transparency"].as_f64().unwrap_or(0.0)) > 0
                        && phys.w > 0.0
                        && phys.h > 0.0
                        && clipped_area > 0.0
                    {
                        self.emit_quads(sg, node, nid, target_id, pass_id, "shadow", "shadow", None, clip, rot, 1, clipped_area);
                        drawn_any = true;
                    }
                }
            }
            if let Some(strokes) = node.get("strokes").and_then(Value::as_array) {
                for st in strokes {
                    let mode = st.get("mode").or_else(|| st.get("applyMode")).and_then(Value::as_str);
                    let is_text_stroke = has_text && mode == Some("Contextual");
                    if !is_text_stroke
                        && st["thickness"].as_f64().unwrap_or(0.0) > 0.0
                        && alpha8(1.0 - st["transparency"].as_f64().unwrap_or(0.0)) > 0
                        && phys.w > 0.0
                        && phys.h > 0.0
                    {
                        self.emit_quads(sg, node, nid, target_id, pass_id, "stroke", "stroke", None, clip, rot, 1, 0.0);
                        drawn_any = true;
                    }
                }
            }
        }

        if drawn_any {
            self.totals.drawn_nodes += 1;
            sg.drawn_nodes += 1;
        }
    }

    fn draw_post(&mut self, sg: &mut ScreenGuiAccum, node: &Value, nid: u64, target_id: u64, pass_id: u8) {
        let Some(scroll) = node.get("scroll").and_then(Value::as_object) else { return };
        let thickness = (scroll.get("thickness").and_then(Value::as_f64).unwrap_or(0.0) - 1e-6).ceil() as i64;
        if thickness <= 0 || alpha8(1.0 - scroll.get("transparency").and_then(Value::as_f64).unwrap_or(0.0)) == 0 {
            return;
        }
        let clip = clip_rect(node, [0.0, 0.0], self.scale);
        let rot = is_rotated(node);
        for horiz in [false, true] {
            let axis_key = if horiz { "horizontal" } else { "vertical" };
            if scroll.get(axis_key).and_then(Value::as_bool) != Some(true) {
                continue;
            }
            for key in ["topImage", "midImage", "bottomImage"] {
                if let Some(uri) = scroll.get(key).and_then(Value::as_str).filter(|u| !u.is_empty()) {
                    self.emit_quads(
                        sg,
                        node,
                        nid,
                        target_id,
                        pass_id,
                        "scrollbar",
                        "image",
                        Some(uri.to_string()),
                        clip,
                        rot,
                        1,
                        0.0,
                    );
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn walk_entries(
        &mut self,
        sg: &mut ScreenGuiAccum,
        entries: &[Value],
        target_id: u64,
        target_bounds: [f64; 4],
        pass_id: u8,
        depth: u64,
        skip_group: Option<u64>,
    ) {
        for entry in entries {
            if let Some(obj) = entry.as_object() {
                if let Some(gid) = obj.get("group").and_then(node_id) {
                    let Some(gnode) = self.scene.node(gid) else { continue };
                    if gnode["culled"].as_bool() == Some(true) || gnode["visible"].as_bool() == Some(false) {
                        continue;
                    }
                    self.totals.canvas_groups += 1;
                    let grp_transparency = gnode
                        .get("group")
                        .and_then(|g| g.get("transparency"))
                        .and_then(Value::as_f64)
                        .unwrap_or(0.0);
                    if alpha8(1.0 - grp_transparency) == 0 {
                        continue;
                    }
                    let flat = flat_physical_rect(gnode, self.scale);
                    if flat.w <= 0.0 || flat.h <= 0.0 {
                        continue;
                    }
                    let (nw, nh) = group_texture_dims(flat.x, flat.y, flat.w, flat.h, self.scale);
                    let rt_bytes = u64::from(nw) * u64::from(nh) * 4;
                    let new_depth = depth + 1;
                    self.totals.canvas_group_targets += 1;
                    self.totals.canvas_rt_bytes += rt_bytes;
                    self.totals.canvas_rt_switches += 2;
                    self.totals.max_canvas_group_depth = self.totals.max_canvas_group_depth.max(new_depth);
                    sg.canvas_group_targets += 1;
                    sg.canvas_rt_bytes += rt_bytes;
                    sg.canvas_rt_switches += 2;
                    self.object_entry(gnode, gid, &sg.name).canvas_rt_bytes += rt_bytes;

                    // Recurse into offscreen CanvasGroup render target
                    let rt_bounds = flat.bounds();
                    self.last_state_by_target.remove(&gid);
                    if let Some(sub_draw) = obj.get("draw").and_then(Value::as_array) {
                        self.walk_entries(sg, sub_draw, gid, rt_bounds, pass_id, new_depth, Some(gid));
                    }

                    // Composite offscreen RT onto parent target
                    let phys = physical_rect(gnode, self.scale).rect;
                    let bounds = quad_bounds_device(gnode, phys, self.scale);
                    let clip = clip_rect(gnode, [0.0, 0.0], self.scale);
                    let area = intersect_area(bounds, clip, target_bounds);
                    if area > 0.0 {
                        self.emit_quads(
                            sg,
                            gnode,
                            gid,
                            target_id,
                            pass_id,
                            "group_composite",
                            "group_composite",
                            Some(format!("group:{gid}")),
                            clip,
                            is_rotated(gnode),
                            1,
                            area,
                        );
                        self.totals.drawn_nodes += 1;
                        sg.drawn_nodes += 1;
                    }
                    continue;
                }
            }

            let raw = entry
                .as_i64()
                .or_else(|| entry.as_f64().filter(|v| v.is_finite() && v.fract() == 0.0).map(|v| v as i64));
            let Some(raw) = raw else { continue };
            let nid = raw.unsigned_abs();
            let Some(node) = self.scene.node(nid) else { continue };
            if node["culled"].as_bool() == Some(true) || node["visible"].as_bool() == Some(false) {
                continue;
            }
            if raw < 0 {
                self.draw_post(sg, node, nid, target_id, pass_id);
            } else {
                self.draw_node(sg, node, nid, target_id, target_bounds, pass_id, skip_group == Some(nid));
            }
        }
    }
}

pub fn layout_stats_json(stats: LayoutStats) -> Value {
    json!({
        "passes": stats.passes,
        "computed": stats.computed,
        "memo": stats.memo,
        "measures": stats.measures,
        "stored": stats.stored,
        "skipped": stats.skipped,
        "moved": stats.moved,
        "texts": stats.texts,
        "scrollMoved": stats.scroll_moved,
    })
}

pub fn desktop_layout_cpu_ms(stats: LayoutStats) -> f64 {
    stats.passes as f64 * STUDIO_LAYOUT_PASS_MS
        + (stats.stored + stats.moved) as f64 * STUDIO_LAYOUT_OBJECT_MS
        + if stats.scroll_moved > 0 { STUDIO_SCROLL_PASS_MS } else { 0.0 }
}

pub fn predicted_json(stats: LayoutStats) -> Value {
    let desk_ms = desktop_layout_cpu_ms(stats);
    let draw_refusal = json!({
        "status": "refused",
        "basis": "uncalibrated",
        "ms": Value::Null,
        "reason": "No recorded per-element CPU calibration for Roblox Pass2d / fillGuiVertices / TextRender.",
        "settledBy": "Regress quad, glyph, and batch-break counts against recorded Roblox MicroProfiler UI pass captures."
    });
    let gpu_refusal = json!({
        "status": "refused",
        "basis": "uncalibrated",
        "ms": Value::Null,
        "reason": "No recorded mobile GPU UI fill-rate or offscreen render-target calibration backing gpuUiMs.",
        "settledBy": "Regress overdraw viewports and CanvasGroup RT bytes against recorded GPU timing captures on target devices."
    });
    json!({
        "desktop-ref": {
            "layoutCpuMs": {
                "status": "calibrated",
                "basis": "measured",
                "ms": round9(desk_ms),
                "source": "Studio 0.744 client lazy-layout calibration (native/src/engine/layout/install.rs: 2.3 us/pass + 0.312 us/(stored+moved) + 62 us/scroll pass)"
            },
            "drawPassCpuMs": draw_refusal,
            "gpuUiMs": gpu_refusal
        },
        "phone-mid": {
            "layoutCpuMs": {
                "status": "calibrated",
                "basis": "anchored",
                "ms": round9(desk_ms * PHONE_UI_MAIN_RATIO),
                "ratio": PHONE_UI_MAIN_RATIO,
                "bandMs": [round9(desk_ms * PHONE_UI_MAIN_LOW), round9(desk_ms * PHONE_UI_MAIN_HIGH)],
                "source": "Anchored to desktop-ref via recorded mobile mid-tier UI main-thread ratio 3.1493x (band 2.0405x..6.16x)"
            },
            "drawPassCpuMs": draw_refusal,
            "gpuUiMs": gpu_refusal
        }
    })
}

pub fn unmodeled_list() -> Value {
    json!([
        "draw_pass_cpu_ms: Pass2d / fillGuiVertices / TextRender per-element CPU slopes are uncalibrated (refused in predicted.*.drawPassCpuMs)",
        "gpu_ui_ms: mobile GPU fill-rate, tile-binning, and CanvasGroup offscreen RT bandwidth costs are uncalibrated (refused in predicted.*.gpuUiMs)",
        "thermal_throttling_dvfs: device clock scaling and sustained thermal throttling are not modeled",
        "luau_script_cpu_ms: Luau script work is measured separately with --profile work",
        "viewport_frame_3d_pass: 3D rendering inside ViewportFrames is excluded from 2D UI draw counts"
    ])
}

/// Analyzes one `SceneStore` deterministically into the canonical `rur-cost/1` JSON report.
/// Contains zero wall-clock fields or host-specific output paths.
pub fn analyze_scene(
    scene: &SceneStore,
    layout: LayoutStats,
    dom_hidden: Option<&DomHiddenSummary>,
) -> Value {
    let vp = scene.viewport();
    let phys_w = (vp.width * vp.scale).round().max(1.0) as u32;
    let phys_h = (vp.height * vp.scale).round().max(1.0) as u32;
    let vp_area = (f64::from(phys_w) * f64::from(phys_h)).max(1.0);
    let screen_bounds = [0.0, 0.0, f64::from(phys_w), f64::from(phys_h)];

    let fallback_hidden;
    let hidden = match dom_hidden {
        Some(h) => h,
        None => {
            fallback_hidden = scene_only_hidden(scene);
            &fallback_hidden
        }
    };

    let layers = scene.layers().as_array().map(Vec::as_slice).unwrap_or(&[]);
    let mut indexed_layers: Vec<(usize, &Value)> = layers.iter().enumerate().collect();
    indexed_layers.sort_by(|(ia, a), (ib, b)| {
        layer_pass(a)
            .cmp(&layer_pass(b))
            .then_with(|| {
                let da = a["displayOrder"].as_f64().unwrap_or(0.0);
                let db = b["displayOrder"].as_f64().unwrap_or(0.0);
                da.total_cmp(&db)
            })
            .then_with(|| ia.cmp(ib))
    });

    let mut analyzer = Analyzer::new(scene, vp.scale);
    let mut screen_guis: Vec<ScreenGuiAccum> = Vec::new();
    let mut listed_ids: HashSet<u64> = HashSet::new();

    for (_, layer) in indexed_layers {
        let root_id = layer.get("root").and_then(node_id);
        let root_node = root_id.and_then(|id| scene.node(id));
        if root_node.is_some_and(|n| n["enabled"].as_bool() == Some(false)) {
            continue;
        }
        let pass_id = layer_pass(layer);
        let sg_id = root_id.unwrap_or(0);
        listed_ids.insert(sg_id);
        let sg_name = root_node
            .and_then(|n| n["name"].as_str())
            .unwrap_or("?")
            .to_string();
        let sg_path = root_node
            .and_then(|n| n["path"].as_str())
            .unwrap_or("?")
            .to_string();
        let display_order = layer["displayOrder"].as_f64().unwrap_or(0.0);
        let root_hidden = root_id.and_then(|id| hidden.by_root.get(&id)).cloned().unwrap_or_default();

        analyzer.totals.screen_guis += 1;
        let mut sg = ScreenGuiAccum {
            id: sg_id,
            name: sg_name,
            path: sg_path,
            pass: if pass_id == 0 { "PlayerGui" } else { "CoreGui" },
            display_order,
            enabled: true,
            hidden_roots: root_hidden.roots,
            hidden_gui_objects: root_hidden.gui_objects,
            hidden_instances: root_hidden.instances,
            ..Default::default()
        };
        if let Some(draw_list) = layer.get("draw").and_then(Value::as_array) {
            analyzer.walk_entries(&mut sg, draw_list, 0, screen_bounds, pass_id, 0, None);
        }
        screen_guis.push(sg);
    }

    for dg in &hidden.disabled_guis {
        if listed_ids.insert(dg.id) {
            screen_guis.push(ScreenGuiAccum {
                id: dg.id,
                name: dg.name.clone(),
                path: dg.path.clone(),
                pass: dg.pass,
                display_order: dg.display_order,
                enabled: false,
                disabled_gui_objects: dg.gui_objects,
                disabled_instances: dg.instances,
                ..Default::default()
            });
        }
    }

    let totals = &analyzer.totals;
    let totals_json = json!({
        "screenGuis": totals.screen_guis,
        "disabledScreenGuis": hidden.disabled_screen_guis,
        "disabledScreenGuiInstances": hidden.disabled_screen_gui_instances,
        "totalNodes": totals.total_nodes,
        "drawnNodes": totals.drawn_nodes,
        "quads": totals.quads,
        "quadsByKind": totals.quads_by_kind,
        "glyphQuads": totals.glyph_quads,
        "fillGlyphQuads": totals.fill_glyph_quads,
        "strokeGlyphQuads": totals.stroke_glyph_quads,
        "drawCalls": totals.draw_calls,
        "batchBreaks": totals.batch_breaks,
        "batchBreaksByCause": totals.batch_breaks_by_cause,
        "canvasGroups": totals.canvas_groups,
        "canvasGroupTargets": totals.canvas_group_targets,
        "canvasRtBytes": totals.canvas_rt_bytes,
        "canvasRtSwitches": totals.canvas_rt_switches,
        "maxCanvasGroupDepth": totals.max_canvas_group_depth,
        "screenOverdrawViewports": round6(totals.screen_overdraw_px / vp_area),
        "offscreenOverdrawViewports": round6(totals.offscreen_overdraw_px / vp_area),
        "totalOverdrawViewports": round6((totals.screen_overdraw_px + totals.offscreen_overdraw_px) / vp_area),
        "hiddenInShown": {
            "complete": hidden.complete,
            "roots": hidden.roots,
            "guiObjects": hidden.gui_objects,
            "instances": hidden.instances,
        }
    });

    let screen_guis_json: Vec<Value> = screen_guis
        .iter()
        .map(|sg| {
            let mut obj = Map::new();
            obj.insert("id".into(), json!(sg.id));
            obj.insert("name".into(), json!(sg.name));
            obj.insert("path".into(), json!(sg.path));
            obj.insert("pass".into(), json!(sg.pass));
            obj.insert("displayOrder".into(), json!(sg.display_order));
            obj.insert("enabled".into(), json!(sg.enabled));
            obj.insert("drawnNodes".into(), json!(sg.drawn_nodes));
            obj.insert("quads".into(), json!(sg.quads));
            obj.insert("glyphQuads".into(), json!(sg.glyph_quads));
            obj.insert("drawCalls".into(), json!(sg.draw_calls));
            obj.insert("batchBreaks".into(), json!(sg.batch_breaks));
            obj.insert("canvasGroupTargets".into(), json!(sg.canvas_group_targets));
            obj.insert("canvasRtBytes".into(), json!(sg.canvas_rt_bytes));
            obj.insert("canvasRtSwitches".into(), json!(sg.canvas_rt_switches));
            obj.insert("screenOverdrawViewports".into(), json!(round6(sg.screen_overdraw_px / vp_area)));
            obj.insert("offscreenOverdrawViewports".into(), json!(round6(sg.offscreen_overdraw_px / vp_area)));
            obj.insert("hiddenRoots".into(), json!(sg.hidden_roots));
            obj.insert("hiddenGuiObjects".into(), json!(sg.hidden_gui_objects));
            obj.insert("hiddenInstances".into(), json!(sg.hidden_instances));
            if !sg.enabled {
                obj.insert("disabledGuiObjects".into(), json!(sg.disabled_gui_objects));
                obj.insert("disabledInstances".into(), json!(sg.disabled_instances));
            }
            Value::Object(obj)
        })
        .collect();

    let mut objects: Vec<ObjectAccum> = analyzer
        .objects
        .into_values()
        .filter(|o| o.quads > 0 || o.glyph_quads > 0 || o.canvas_rt_bytes > 0 || o.batch_breaks > 0)
        .collect();
    objects.sort_by(|a, b| {
        b.canvas_rt_bytes
            .cmp(&a.canvas_rt_bytes)
            .then_with(|| (b.screen_overdraw_px + b.offscreen_overdraw_px).total_cmp(&(a.screen_overdraw_px + a.offscreen_overdraw_px)))
            .then_with(|| (b.quads + b.glyph_quads).cmp(&(a.quads + a.glyph_quads)))
            .then_with(|| b.batch_breaks.cmp(&a.batch_breaks))
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.id.cmp(&b.id))
    });
    objects.truncate(MAX_OBJECTS_REPORTED);

    let objects_json: Vec<Value> = objects
        .iter()
        .map(|o| {
            json!({
                "id": o.id,
                "name": o.name,
                "path": o.path,
                "class": o.class,
                "screenGui": o.screen_gui,
                "quads": o.quads,
                "glyphQuads": o.glyph_quads,
                "batchBreaks": o.batch_breaks,
                "canvasRtBytes": o.canvas_rt_bytes,
                "screenOverdrawViewports": round6(o.screen_overdraw_px / vp_area),
                "offscreenOverdrawViewports": round6(o.offscreen_overdraw_px / vp_area),
            })
        })
        .collect();

    let hidden_subtrees_json: Vec<Value> = hidden
        .subtrees
        .iter()
        .take(MAX_HIDDEN_SUBTREES_REPORTED)
        .map(|h| {
            json!({
                "id": h.id,
                "name": h.name,
                "path": h.path,
                "class": h.class,
                "screenGui": h.screen_gui,
                "guiObjects": h.gui_objects,
                "instances": h.instances,
            })
        })
        .collect();

    let status = scene
        .header()
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("ok");

    sort_json(json!({
        "schema": "rur-cost/1",
        "status": status,
        "mode": "single",
        "input": if dom_hidden.is_some() { "live" } else { "scene" },
        "viewport": {
            "width": vp.width,
            "height": vp.height,
            "scale": vp.scale,
            "physicalWidth": phys_w,
            "physicalHeight": phys_h,
        },
        "totals": totals_json,
        "layout": layout_stats_json(layout),
        "screenGuis": screen_guis_json,
        "objects": objects_json,
        "hiddenSubtrees": hidden_subtrees_json,
        "predicted": predicted_json(layout),
        "fidelity": {
            "canvasGroupRt": "exact_studio_dimensions (rectangular float32 logical size, 2048 side cap, 2048x1024 area cap, culled/empty/transparent skipped)",
            "batchBreaks": "roblox_pass2d_state_model (layerPass, renderTarget, clip, shader, texture, rotation)",
            "hiddenInShown": if hidden.complete {
                "live_dom_traversal"
            } else {
                "scene_json_only_incomplete (run without --scene for full hidden subtree counts)"
            },
            "determinism": "zero_wall_clock_fields"
        },
        "unmodeled": unmodeled_list(),
    }))
}

fn describe_non_cost_report(value: &Value) -> String {
    let Some(obj) = value.as_object() else {
        return "not a JSON object (expected a rur-cost/1 cost report)".into();
    };
    if let Some(schema) = obj.get("schema").and_then(Value::as_str) {
        if schema == "rrr-ab/1" {
            return "a rur cost compare (rrr-ab/1) diff report, not a rur-cost/1 cost report".into();
        }
        return format!("schema \"{schema}\", not a rur-cost/1 cost report");
    }
    if obj.get("nodes").is_some_and(Value::is_array) && obj.get("layers").is_some_and(Value::is_array) {
        return "a scene.json export, not a rur-cost/1 cost report (run `rur cost --scene <file>` first)".into();
    }
    if obj.get("units").is_some_and(Value::is_string) && obj.get("nodes").is_some_and(Value::is_array) {
        return "a sequence trace.json file, not a rur-cost/1 cost report".into();
    }
    if obj.contains_key("renderErrors") || obj.contains_key("framesDir") || obj.contains_key("png") {
        return "a summary.json file, not a rur-cost/1 cost report".into();
    }
    "a JSON object without \"schema\": \"rur-cost/1\" (not a rur cost report)".into()
}

/// Validates that `value` is a well-formed `rur-cost/1` cost report before comparison.
pub fn validate_cost_report(value: &Value, role: &str, path: Option<&Path>) -> Result<(), String> {
    let prefix = match path {
        Some(p) => format!("{role} ({})", p.display()),
        None => role.to_string(),
    };
    if value.get("schema").and_then(Value::as_str) != Some("rur-cost/1") {
        return Err(format!("{prefix}: {}", describe_non_cost_report(value)));
    }
    let Some(mode) = value.get("mode").and_then(Value::as_str) else {
        return Err(format!("{prefix}: malformed rur-cost/1 report (missing string \"mode\")"));
    };
    if !matches!(mode, "single" | "sequence") {
        return Err(format!("{prefix}: malformed rur-cost/1 report (unknown mode \"{mode}\")"));
    }
    if value.get("status").and_then(Value::as_str).is_none() {
        return Err(format!("{prefix}: malformed rur-cost/1 report (missing string \"status\")"));
    }
    let vp = value.get("viewport").and_then(Value::as_object);
    let vp_ok = vp.is_some_and(|v| {
        v.get("width").and_then(Value::as_f64).is_some_and(f64::is_finite)
            && v.get("height").and_then(Value::as_f64).is_some_and(f64::is_finite)
            && v.get("scale").and_then(Value::as_f64).is_some_and(f64::is_finite)
    });
    if !vp_ok {
        return Err(format!("{prefix}: malformed rur-cost/1 report (missing numeric viewport width/height/scale)"));
    }
    if !value.get("totals").is_some_and(Value::is_object) {
        return Err(format!("{prefix}: malformed rur-cost/1 report (missing object \"totals\")"));
    }
    if value.pointer("/totals/hiddenInShown/complete").and_then(Value::as_bool).is_none() {
        return Err(format!("{prefix}: malformed rur-cost/1 report (missing boolean \"totals.hiddenInShown.complete\")"));
    }
    Ok(())
}

struct EquivalenceMismatch {
    field: &'static str,
    before: Value,
    after: Value,
    reason: String,
    remedy: String,
}

fn check_compare_equivalence(before: &Value, after: &Value) -> Vec<EquivalenceMismatch> {
    let mut mismatches = Vec::new();

    let b_status = before["status"].as_str().unwrap_or("");
    let a_status = after["status"].as_str().unwrap_or("");
    if b_status != a_status {
        mismatches.push(EquivalenceMismatch {
            field: "status",
            before: json!(b_status),
            after: json!(a_status),
            reason: format!("status differs (before=\"{b_status}\", after=\"{a_status}\")"),
            remedy: "Fix the failing run so both reports have status \"ok\" before comparing.".into(),
        });
    } else if b_status != "ok" {
        mismatches.push(EquivalenceMismatch {
            field: "status",
            before: json!(b_status),
            after: json!(a_status),
            reason: format!("cannot compare reports with non-ok status (\"{b_status}\")"),
            remedy: "Fix the failing runs so both reports have status \"ok\" before comparing.".into(),
        });
    }

    let b_mode = before["mode"].as_str().unwrap_or("");
    let a_mode = after["mode"].as_str().unwrap_or("");
    if b_mode != a_mode {
        mismatches.push(EquivalenceMismatch {
            field: "mode",
            before: json!(b_mode),
            after: json!(a_mode),
            reason: format!("mode differs (before=\"{b_mode}\", after=\"{a_mode}\")"),
            remedy: "Compare two single-moment reports (`rur cost ...`) or two matching sequence reports (`rur cost --from ... --frames ... --fps ...`).".into(),
        });
    } else if b_mode == "sequence" {
        if let (Some(bf), Some(af)) = (
            before.pointer("/sequence/summary/frameCount").and_then(Value::as_u64),
            after.pointer("/sequence/summary/frameCount").and_then(Value::as_u64),
        ) {
            if bf != af {
                mismatches.push(EquivalenceMismatch {
                    field: "sequence.frameCount",
                    before: json!(bf),
                    after: json!(af),
                    reason: format!("sequence frameCount differs (before={bf}, after={af})"),
                    remedy: "Re-run both sequence arms with the same --from, --frames, and --fps.".into(),
                });
            }
        }
        if let (Some(bfps), Some(afps)) = (
            before.pointer("/sequence/summary/fps").and_then(Value::as_f64),
            after.pointer("/sequence/summary/fps").and_then(Value::as_f64),
        ) {
            if (bfps - afps).abs() > 1e-6 {
                mismatches.push(EquivalenceMismatch {
                    field: "sequence.fps",
                    before: json!(bfps),
                    after: json!(afps),
                    reason: format!("sequence fps differs (before={bfps}, after={afps})"),
                    remedy: "Re-run both sequence arms with the same --fps.".into(),
                });
            }
        }
    }

    let b_complete = before.pointer("/totals/hiddenInShown/complete").and_then(Value::as_bool).unwrap_or(false);
    let a_complete = after.pointer("/totals/hiddenInShown/complete").and_then(Value::as_bool).unwrap_or(false);
    let b_input = before.get("input").and_then(Value::as_str).unwrap_or(if b_complete { "live" } else { "scene" });
    let a_input = after.get("input").and_then(Value::as_str).unwrap_or(if a_complete { "live" } else { "scene" });
    if b_input != a_input || b_complete != a_complete {
        mismatches.push(EquivalenceMismatch {
            field: "hiddenInShown.complete",
            before: json!({"input": b_input, "complete": b_complete}),
            after: json!({"input": a_input, "complete": a_complete}),
            reason: format!(
                "input mode / hiddenInShown.complete differs (before: input=\"{b_input}\" complete={b_complete}, after: input=\"{a_input}\" complete={a_complete})"
            ),
            remedy: "Run both arms live (`rur cost --story/--fixture/--module/--model ...`) or both from saved scenes (`rur cost --scene ...`).".into(),
        });
    }

    let bw = before.pointer("/viewport/width").and_then(Value::as_f64).unwrap_or(0.0);
    let bh = before.pointer("/viewport/height").and_then(Value::as_f64).unwrap_or(0.0);
    let aw = after.pointer("/viewport/width").and_then(Value::as_f64).unwrap_or(0.0);
    let ah = after.pointer("/viewport/height").and_then(Value::as_f64).unwrap_or(0.0);
    if (bw - aw).abs() > 1e-6 || (bh - ah).abs() > 1e-6 {
        mismatches.push(EquivalenceMismatch {
            field: "viewport",
            before: json!(format!("{bw}x{bh}")),
            after: json!(format!("{aw}x{ah}")),
            reason: format!("viewport differs (before={bw}x{bh}, after={aw}x{ah})"),
            remedy: "Re-run both arms with the same --viewport WxH.".into(),
        });
    }

    let bs = before.pointer("/viewport/scale").and_then(Value::as_f64).unwrap_or(0.0);
    let as_ = after.pointer("/viewport/scale").and_then(Value::as_f64).unwrap_or(0.0);
    if (bs - as_).abs() > 1e-6 {
        mismatches.push(EquivalenceMismatch {
            field: "scale",
            before: json!(bs),
            after: json!(as_),
            reason: format!("scale differs (before={bs}, after={as_})"),
            remedy: "Re-run both arms with the same --scale S.".into(),
        });
    }

    mismatches
}

/// Deterministic A/B comparison between two `rur-cost/1` reports (`rrr-ab/1`).
/// Rejects non-cost-report inputs with `Err` and refuses incomparable arms with `verdict: "refused"`.
pub fn compare_reports(before: &Value, after: &Value) -> Result<Value, String> {
    validate_cost_report(before, "before", None)?;
    validate_cost_report(after, "after", None)?;

    let checks = json!(["schema", "status", "mode", "input", "hiddenInShown.complete", "viewport", "scale"]);
    let mismatches = check_compare_equivalence(before, after);
    if !mismatches.is_empty() {
        let reasons: Vec<String> = mismatches.iter().map(|m| m.reason.clone()).collect();
        let mut remedies: Vec<String> = Vec::new();
        for m in &mismatches {
            if !remedies.contains(&m.remedy) {
                remedies.push(m.remedy.clone());
            }
        }
        let items: Vec<Value> = mismatches
            .iter()
            .map(|m| {
                json!({
                    "field": m.field,
                    "before": m.before,
                    "after": m.after,
                    "reason": m.reason,
                    "remedy": m.remedy,
                })
            })
            .collect();
        return Ok(sort_json(json!({
            "schema": "rrr-ab/1",
            "verdict": "refused",
            "equivalence": {
                "passed": false,
                "checks": checks,
                "mismatches": items,
            },
            "reason": reasons.join("; "),
            "remedy": remedies.join(" "),
        })));
    }
    const COUNT_KEYS: [&str; 12] = [
        "screenGuis",
        "disabledScreenGuis",
        "drawnNodes",
        "quads",
        "glyphQuads",
        "drawCalls",
        "batchBreaks",
        "canvasGroupTargets",
        "canvasRtBytes",
        "canvasRtSwitches",
        "screenOverdrawViewports",
        "offscreenOverdrawViewports",
    ];
    const VERDICT_KEYS: [&str; 7] = [
        "drawCalls",
        "batchBreaks",
        "quads",
        "glyphQuads",
        "canvasRtBytes",
        "screenOverdrawViewports",
        "offscreenOverdrawViewports",
    ];
    const LAYOUT_KEYS: [&str; 5] = ["passes", "computed", "stored", "moved", "scrollMoved"];

    let mut before_counts = Map::new();
    let mut after_counts = Map::new();
    let mut delta_counts = Map::new();
    let mut any_improved = false;
    let mut any_regressed = false;

    for key in COUNT_KEYS {
        let b = before.pointer(&format!("/totals/{key}")).and_then(Value::as_f64).unwrap_or(0.0);
        let a = after.pointer(&format!("/totals/{key}")).and_then(Value::as_f64).unwrap_or(0.0);
        let d = round6(a - b);
        before_counts.insert(key.into(), json!(b));
        after_counts.insert(key.into(), json!(a));
        delta_counts.insert(key.into(), json!(d));
        if VERDICT_KEYS.contains(&key) {
            if d < -1e-9 {
                any_improved = true;
            } else if d > 1e-9 {
                any_regressed = true;
            }
        }
    }

    let b_hid = before.pointer("/totals/hiddenInShown/guiObjects").and_then(Value::as_f64).unwrap_or(0.0);
    let a_hid = after.pointer("/totals/hiddenInShown/guiObjects").and_then(Value::as_f64).unwrap_or(0.0);
    let d_hid = round6(a_hid - b_hid);
    before_counts.insert("hiddenGuiObjects".into(), json!(b_hid));
    after_counts.insert("hiddenGuiObjects".into(), json!(a_hid));
    delta_counts.insert("hiddenGuiObjects".into(), json!(d_hid));
    if d_hid < -1e-9 {
        any_improved = true;
    } else if d_hid > 1e-9 {
        any_regressed = true;
    }

    let mut before_layout = Map::new();
    let mut after_layout = Map::new();
    let mut delta_layout = Map::new();
    for key in LAYOUT_KEYS {
        let b = before.pointer(&format!("/layout/{key}")).and_then(Value::as_f64).unwrap_or(0.0);
        let a = after.pointer(&format!("/layout/{key}")).and_then(Value::as_f64).unwrap_or(0.0);
        let d = round6(a - b);
        before_layout.insert(key.into(), json!(b));
        after_layout.insert(key.into(), json!(a));
        delta_layout.insert(key.into(), json!(d));
        if matches!(key, "stored" | "moved" | "scrollMoved") {
            if d < -1e-9 {
                any_improved = true;
            } else if d > 1e-9 {
                any_regressed = true;
            }
        }
    }

    let verdict = match (any_improved, any_regressed) {
        (true, false) => "improved",
        (false, true) => "regressed",
        (true, true) => "mixed",
        (false, false) => "unchanged",
    };

    let layout_delta = |tier: &str| {
        let b = before.pointer(&format!("/predicted/{tier}/layoutCpuMs/ms")).and_then(Value::as_f64);
        let a = after.pointer(&format!("/predicted/{tier}/layoutCpuMs/ms")).and_then(Value::as_f64);
        match (b, a) {
            (Some(b), Some(a)) => json!({
                "beforeMs": round9(b),
                "afterMs": round9(a),
                "deltaMs": round9(a - b),
            }),
            _ => json!({"beforeMs": Value::Null, "afterMs": Value::Null, "deltaMs": Value::Null}),
        }
    };

    // Per-ScreenGui movers
    let mut by_name: BTreeMap<String, (Option<&Value>, Option<&Value>)> = BTreeMap::new();
    for item in before.get("screenGuis").and_then(Value::as_array).into_iter().flatten() {
        if let Some(name) = item["name"].as_str() {
            by_name.entry(name.to_string()).or_default().0 = Some(item);
        }
    }
    for item in after.get("screenGuis").and_then(Value::as_array).into_iter().flatten() {
        if let Some(name) = item["name"].as_str() {
            by_name.entry(name.to_string()).or_default().1 = Some(item);
        }
    }
    let mut movers: Vec<Value> = Vec::new();
    for (name, (b, a)) in by_name {
        let num = |v: Option<&Value>, k: &str| v.and_then(|o| o[k].as_f64()).unwrap_or(0.0);
        let d_quads = num(a, "quads") - num(b, "quads");
        let d_glyphs = num(a, "glyphQuads") - num(b, "glyphQuads");
        let d_breaks = num(a, "batchBreaks") - num(b, "batchBreaks");
        let d_rt = num(a, "canvasRtBytes") - num(b, "canvasRtBytes");
        let d_ov = round6(num(a, "screenOverdrawViewports") - num(b, "screenOverdrawViewports"));
        let d_hid = num(a, "hiddenGuiObjects") - num(b, "hiddenGuiObjects");
        if d_quads != 0.0 || d_glyphs != 0.0 || d_breaks != 0.0 || d_rt != 0.0 || d_ov != 0.0 || d_hid != 0.0 {
            movers.push(json!({
                "name": name,
                "deltaQuads": d_quads,
                "deltaGlyphQuads": d_glyphs,
                "deltaBatchBreaks": d_breaks,
                "deltaCanvasRtBytes": d_rt,
                "deltaScreenOverdrawViewports": d_ov,
                "deltaHiddenGuiObjects": d_hid,
            }));
        }
    }

    Ok(sort_json(json!({
        "schema": "rrr-ab/1",
        "verdict": verdict,
        "equivalence": {
            "passed": true,
            "checks": checks,
        },
        "measurementNoise": 0.0,
        "counts": {
            "before": before_counts,
            "after": after_counts,
            "delta": delta_counts,
        },
        "layout": {
            "before": before_layout,
            "after": after_layout,
            "delta": delta_layout,
        },
        "predicted": {
            "desktop-ref": { "layoutCpuMs": layout_delta("desktop-ref") },
            "phone-mid": { "layoutCpuMs": layout_delta("phone-mid") },
        },
        "screenGuiMovers": movers,
        "unmodeled": unmodeled_list(),
    })))
}

/// Writes a deterministic sorted JSON cost report to `path`.
pub fn write_report(path: &Path, report: &Value) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(report).map_err(|e| e.to_string())?;
    std::fs::write(path, format!("{text}\n")).map_err(|e| format!("{}: {e}", path.display()))
}

struct FrameMetrics {
    draw_calls: f64,
    batch_breaks: f64,
    quads: f64,
    glyph_quads: f64,
    canvas_group_targets: f64,
    canvas_rt_bytes: f64,
    screen_overdraw: f64,
    offscreen_overdraw: f64,
    layout: LayoutStats,
}

/// Accumulates per-frame deterministic UI cost and incremental `Layout.Stats` churn across a
/// multi-frame sequence (`rur cost --from/--to/--frames/--duration/--fps` and `rur sequence --cost`).
pub struct SequenceAccumulator {
    fps: f64,
    frames: Vec<Value>,
    metrics: Vec<FrameMetrics>,
    times: Vec<f64>,
    last_dom_hidden: Option<DomHiddenSummary>,
    last_report: Option<Value>,
    unchanged_frames: u32,
    active_frames: u32,
}

impl SequenceAccumulator {
    pub fn new(fps: f64) -> Self {
        Self {
            fps,
            frames: Vec::new(),
            metrics: Vec::new(),
            times: Vec::new(),
            last_dom_hidden: None,
            last_report: None,
            unchanged_frames: 0,
            active_frames: 0,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_frame(
        &mut self,
        index: u32,
        time: f64,
        unchanged: bool,
        changed_nodes: usize,
        removed_nodes: usize,
        scene: &SceneStore,
        layout: LayoutStats,
        dom_hidden: Option<DomHiddenSummary>,
    ) {
        if dom_hidden.is_some() {
            self.last_dom_hidden = dom_hidden;
        }
        let report = if !unchanged || self.last_report.is_none() {
            let computed = analyze_scene(scene, layout, self.last_dom_hidden.as_ref());
            self.last_report = Some(computed.clone());
            computed
        } else {
            self.last_report.as_ref().expect("checked above").clone()
        };
        if unchanged {
            self.unchanged_frames += 1;
        } else {
            self.active_frames += 1;
        }
        let totals = &report["totals"];
        self.metrics.push(FrameMetrics {
            draw_calls: totals["drawCalls"].as_f64().unwrap_or(0.0),
            batch_breaks: totals["batchBreaks"].as_f64().unwrap_or(0.0),
            quads: totals["quads"].as_f64().unwrap_or(0.0),
            glyph_quads: totals["glyphQuads"].as_f64().unwrap_or(0.0),
            canvas_group_targets: totals["canvasGroupTargets"].as_f64().unwrap_or(0.0),
            canvas_rt_bytes: totals["canvasRtBytes"].as_f64().unwrap_or(0.0),
            screen_overdraw: totals["screenOverdrawViewports"].as_f64().unwrap_or(0.0),
            offscreen_overdraw: totals["offscreenOverdrawViewports"].as_f64().unwrap_or(0.0),
            layout,
        });
        self.times.push(time);
        self.frames.push(json!({
            "index": index,
            "time": round9(time),
            "unchanged": unchanged,
            "changedNodes": changed_nodes,
            "removedNodes": removed_nodes,
            "totals": totals,
            "layout": layout_stats_json(layout),
            "predicted": predicted_json(layout),
        }));
    }

    pub fn finish(self, status_override: Option<&str>) -> Value {
        let count = self.metrics.len();
        let agg = |extract: fn(&FrameMetrics) -> f64| -> (f64, f64, f64) {
            if count == 0 {
                return (0.0, 0.0, 0.0);
            }
            let mut vals: Vec<f64> = self.metrics.iter().map(extract).collect();
            let mean = vals.iter().sum::<f64>() / vals.len() as f64;
            vals.sort_by(f64::total_cmp);
            let peak = *vals.last().unwrap_or(&0.0);
            let p95_idx = ((vals.len() as f64 * 0.95).ceil() as usize).saturating_sub(1).min(vals.len() - 1);
            let p95 = vals[p95_idx];
            (round6(mean), round6(peak), round6(p95))
        };

        let fields: [(&str, fn(&FrameMetrics) -> f64); 12] = [
            ("drawCalls", |m| m.draw_calls),
            ("batchBreaks", |m| m.batch_breaks),
            ("quads", |m| m.quads),
            ("glyphQuads", |m| m.glyph_quads),
            ("canvasGroupTargets", |m| m.canvas_group_targets),
            ("canvasRtBytes", |m| m.canvas_rt_bytes),
            ("screenOverdrawViewports", |m| m.screen_overdraw),
            ("offscreenOverdrawViewports", |m| m.offscreen_overdraw),
            ("layoutPasses", |m| m.layout.passes as f64),
            ("layoutStored", |m| m.layout.stored as f64),
            ("layoutMoved", |m| m.layout.moved as f64),
            ("layoutScrollMoved", |m| m.layout.scroll_moved as f64),
        ];
        let mut mean_map = Map::new();
        let mut peak_map = Map::new();
        let mut p95_map = Map::new();
        for (key, f) in fields {
            let (m, pk, p95) = agg(f);
            mean_map.insert(key.into(), json!(m));
            peak_map.insert(key.into(), json!(pk));
            p95_map.insert(key.into(), json!(p95));
        }

        let steady_slice = if count > 1 { &self.metrics[1..] } else { &self.metrics[..] };
        let steady_len = steady_slice.len().max(1) as f64;
        let steady_passes = steady_slice.iter().map(|m| m.layout.passes as f64).sum::<f64>() / steady_len;
        let steady_computed = steady_slice.iter().map(|m| m.layout.computed as f64).sum::<f64>() / steady_len;
        let steady_stored = steady_slice.iter().map(|m| m.layout.stored as f64).sum::<f64>() / steady_len;
        let steady_moved = steady_slice.iter().map(|m| m.layout.moved as f64).sum::<f64>() / steady_len;
        let steady_scroll = steady_slice.iter().map(|m| m.layout.scroll_moved as f64).sum::<f64>() / steady_len;
        let steady_desk_ms = steady_slice.iter().map(|m| desktop_layout_cpu_ms(m.layout)).sum::<f64>() / steady_len;

        let mut base = self.last_report.unwrap_or_else(|| {
            analyze_scene(&SceneStore::default(), LayoutStats::default(), None)
        });
        let last_layout = self.metrics.last().map(|m| m.layout).unwrap_or_default();
        if let Some(obj) = base.as_object_mut() {
            obj.insert("mode".into(), json!("sequence"));
            if let Some(status) = status_override {
                obj.insert("status".into(), json!(status));
            }
            obj.insert("layout".into(), layout_stats_json(last_layout));
            obj.insert("predicted".into(), predicted_json(last_layout));
            obj.insert(
                "sequence".into(),
                json!({
                    "summary": {
                        "frameCount": count,
                        "fps": self.fps,
                        "from": round9(self.times.first().copied().unwrap_or(0.0)),
                        "to": round9(self.times.last().copied().unwrap_or(0.0)),
                        "activeFrames": self.active_frames,
                        "unchangedFrames": self.unchanged_frames,
                        "steadyStateFrames": steady_slice.len(),
                        "mean": mean_map,
                        "peak": peak_map,
                        "p95": p95_map,
                        "steadyStateLayoutMean": {
                            "passes": round6(steady_passes),
                            "computed": round6(steady_computed),
                            "stored": round6(steady_stored),
                            "moved": round6(steady_moved),
                            "scrollMoved": round6(steady_scroll),
                        },
                        "steadyStateLayoutCpuMs": {
                            "desktop-ref": round9(steady_desk_ms),
                            "phone-mid": round9(steady_desk_ms * PHONE_UI_MAIN_RATIO),
                            "phone-mid-band": [
                                round9(steady_desk_ms * PHONE_UI_MAIN_LOW),
                                round9(steady_desk_ms * PHONE_UI_MAIN_HIGH),
                            ],
                        },
                    },
                    "frames": self.frames,
                }),
            );
        }
        sort_json(base)
    }
}

/// Runs a CPU-only multi-frame cost sequence (no GPU device, PNG encoding, or video encoding).
pub fn run_sequence_cost(
    engine_args: &engine::cli::Args,
    spec: &engine::cli::SequenceSpec,
    repo: &Path,
) -> Result<Value, String> {
    let mut spec = spec.clone();
    spec.collect_cost = true;
    let mut resolver = crate::resolve::Resolver::new(repo.to_path_buf(), engine_args.offline);
    let mut frames_rx = crate::sequence::start_fresh(engine_args, &spec)?;
    let recv_first = |rx: &std::sync::mpsc::Receiver<engine::cli::SequenceMessage>| match rx.recv() {
        Ok(engine::cli::SequenceMessage::Frame(frame)) => Ok(frame),
        Ok(engine::cli::SequenceMessage::End(Err(error))) => Err(error),
        Ok(engine::cli::SequenceMessage::End(Ok(_))) | Err(_) => Err("the sequence ended before its first frame".into()),
    };
    let mut first = recv_first(&frames_rx)?;
    for _ in 0..3 {
        let diagnostics = &first.reply["diagnostics"];
        if engine_args.offline || !diagnostics["missingFontAssets"].as_array().is_some_and(|fonts| !fonts.is_empty()) {
            break;
        }
        if !resolver.fetch_fonts(diagnostics).unwrap_or(false) {
            break;
        }
        drop(frames_rx);
        frames_rx = crate::sequence::start_fresh(engine_args, &spec)?;
        first = recv_first(&frames_rx)?;
    }

    let mut scene = SceneStore::default();
    let mut acc = SequenceAccumulator::new(spec.fps);
    let mut next = Some(first);
    let mut ready_status: Option<String> = None;
    let end = loop {
        let frame = match next.take() {
            Some(frame) => frame,
            None => match frames_rx.recv() {
                Ok(engine::cli::SequenceMessage::Frame(frame)) => frame,
                Ok(engine::cli::SequenceMessage::End(Ok(report))) => break report,
                Ok(engine::cli::SequenceMessage::End(Err(error))) => return Err(error),
                Err(_) => return Err("the sequence engine stopped without finishing".into()),
            },
        };
        let engine::cli::SequenceFrame { index, time, mut reply, layout, dom_hidden } = frame;
        if reply["type"] == "error" {
            return Err(format!("frame {index}: {}", reply["message"].as_str().unwrap_or("the engine failed")));
        }
        let unchanged = index > 0 && reply["unchanged"] == true;
        let outcome = scene.apply_owned(&mut reply).map_err(|e| format!("frame {index}: {e}"))?;
        if index == 0 {
            ready_status = reply.pointer("/delta/header/status").and_then(Value::as_str).map(str::to_string);
        }
        acc.record_frame(
            index,
            time,
            unchanged,
            outcome.changed_nodes.len(),
            outcome.removed_nodes.len(),
            &scene,
            layout,
            dom_hidden,
        );
    };
    let status = end["status"]
        .as_str()
        .map(str::to_string)
        .or(ready_status)
        .unwrap_or_else(|| "ok".into());
    Ok(acc.finish(Some(&status)))
}

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.2} MiB ({bytes} B)", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KiB ({bytes} B)", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

fn print_summary(report: &Value, dest: &Path) {
    let t = &report["totals"];
    let l = &report["layout"];
    let rt_bytes = t["canvasRtBytes"].as_u64().unwrap_or(0);
    if let Some(seq) = report.pointer("/sequence/summary") {
        let frames = seq["frameCount"].as_u64().unwrap_or(0);
        let fps = seq["fps"].as_f64().unwrap_or(60.0);
        let steady = &seq["steadyStateLayoutMean"];
        let desk_ms = seq.pointer("/steadyStateLayoutCpuMs/desktop-ref").and_then(Value::as_f64).unwrap_or(0.0);
        let phone_ms = seq.pointer("/steadyStateLayoutCpuMs/phone-mid").and_then(Value::as_f64).unwrap_or(0.0);
        eprintln!(
            "rur cost: {} ({} frames @ {} fps) | draws={} breaks={} | quads={} glyphs={} | rt={} ({} targets) | overdraw={:.3}x screen / {:.3}x rt | steady layout={:.2}p/{:.1}s/{:.1}m (desk {:.4} ms, phone {:.4} ms; draw/gpu refused) -> {}",
            report["status"].as_str().unwrap_or("?"),
            frames,
            fps,
            t["drawCalls"].as_u64().unwrap_or(0),
            t["batchBreaks"].as_u64().unwrap_or(0),
            t["quads"].as_u64().unwrap_or(0),
            t["glyphQuads"].as_u64().unwrap_or(0),
            format_bytes(rt_bytes),
            t["canvasGroupTargets"].as_u64().unwrap_or(0),
            t["screenOverdrawViewports"].as_f64().unwrap_or(0.0),
            t["offscreenOverdrawViewports"].as_f64().unwrap_or(0.0),
            steady["passes"].as_f64().unwrap_or(0.0),
            steady["stored"].as_f64().unwrap_or(0.0),
            steady["moved"].as_f64().unwrap_or(0.0),
            desk_ms,
            phone_ms,
            dest.display(),
        );
        return;
    }
    let desk_ms = report.pointer("/predicted/desktop-ref/layoutCpuMs/ms").and_then(Value::as_f64).unwrap_or(0.0);
    let phone_ms = report.pointer("/predicted/phone-mid/layoutCpuMs/ms").and_then(Value::as_f64).unwrap_or(0.0);
    eprintln!(
        "rur cost: {} | guis={} (disabled={}) | draws={} breaks={} | quads={} glyphs={} | rt={} ({} targets, {} switches) | overdraw={:.3}x screen / {:.3}x rt | hidden={} roots / {} guiObjects | layout={}p/{}s/{}m (desk {:.4} ms, phone {:.4} ms; draw/gpu refused) -> {}",
        report["status"].as_str().unwrap_or("?"),
        t["screenGuis"].as_u64().unwrap_or(0),
        t["disabledScreenGuis"].as_u64().unwrap_or(0),
        t["drawCalls"].as_u64().unwrap_or(0),
        t["batchBreaks"].as_u64().unwrap_or(0),
        t["quads"].as_u64().unwrap_or(0),
        t["glyphQuads"].as_u64().unwrap_or(0),
        format_bytes(rt_bytes),
        t["canvasGroupTargets"].as_u64().unwrap_or(0),
        t["canvasRtSwitches"].as_u64().unwrap_or(0),
        t["screenOverdrawViewports"].as_f64().unwrap_or(0.0),
        t["offscreenOverdrawViewports"].as_f64().unwrap_or(0.0),
        t["hiddenInShown"]["roots"].as_u64().unwrap_or(0),
        t["hiddenInShown"]["guiObjects"].as_u64().unwrap_or(0),
        l["passes"].as_u64().unwrap_or(0),
        l["stored"].as_u64().unwrap_or(0),
        l["moved"].as_u64().unwrap_or(0),
        desk_ms,
        phone_ms,
        dest.display(),
    );
}

fn resolve_cost_file(path: &Path) -> PathBuf {
    if path.is_dir() {
        path.join("cost.json")
    } else {
        path.to_path_buf()
    }
}

fn read_json_file(path: &Path) -> Result<Value, Failure> {
    let text = std::fs::read_to_string(path).map_err(|e| Failure::Error(format!("{}: {e}", path.display())))?;
    serde_json::from_str(&text).map_err(|e| Failure::Error(format!("{}: invalid JSON: {e}", path.display())))
}

fn write_json_file(path: &Path, value: &Value) -> Result<(), Failure> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Failure::Error(format!("{}: {e}", parent.display())))?;
    }
    let text = serde_json::to_string_pretty(value).map_err(|e| Failure::Error(e.to_string()))?;
    std::fs::write(path, format!("{text}\n")).map_err(|e| Failure::Error(format!("{}: {e}", path.display())))
}

fn resolve_output_file(context: &Context, args: &Args, default_name: &str, filename: &str) -> PathBuf {
    match args.value("--out") {
        Some(out) => {
            let resolved = context.resolve(&out);
            if resolved.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("json")) {
                resolved
            } else {
                resolved.join(filename)
            }
        }
        None => context.render_out_dir(args, default_name).join(filename),
    }
}

fn build_engine_args(context: &Context, args: &Args) -> Result<engine::cli::Args, Failure> {
    let mut run_args = args.clone();
    if run_args.present("--viewport") || run_args.present("--viewport-from") {
        if let Some(vp) = context.resolve_viewport(&run_args)? {
            run_args.set_value("--viewport", vp);
        }
        run_args.remove("--viewport-from");
    }
    run_args.remove("--snapshot-dir");
    if let Some(dir) = context.snapshot_dir(args) {
        run_args.set_value("--snapshot-dir", dir.display().to_string());
    }
    let mut project = None;
    if let Some(reference) = args.value("--git-ref") {
        let mut git_ref = crate::gitref::GitRef::new(&reference, &context.state);
        let roots = context.project_roots(&run_args);
        let (mapped, fragment) = context.apply_git_ref(&run_args, &roots, &mut git_ref)?;
        run_args = mapped;
        project = Some(fragment);
    }
    let engine_argv = context.runtime_args(&run_args, project)?;
    let mut engine_args = engine::cli::parse(&engine_argv).map_err(Failure::Usage)?;
    engine_args.offline = args.truthy("--offline");
    Ok(engine_args)
}

const HELP: &str = "\
rur cost — deterministic Roblox UI render-cost accounting

Usage:
  rur cost --story <file.story.luau> | --module <file> | --model <file.rbxm> | --fixture <file>
           [--out <dir|file.json>] [--viewport WxH] [--scale S] [--time T] [--all-layers]
           [--from S] [--to S | --frames N | --duration S] [--fps N]
           [--mode edit|play] [--device desktop|touch|gamepad] [--topbar-inset px]
           [--controls <json|file>] [--actions <json|file>] [--compare <before>] [--json]
  rur cost --scene <scene.json> [--out <dir|file.json>] [--json]
  rur cost compare <before> <after> [--out <dir|file.json>] [--json]

Outputs:
  <out>/cost.json    deterministic JSON (zero wall-clock fields): Roblox Pass2d draw calls,
                     batch breaks by cause, quads & glyph quads, clipped overdraw, exact Studio
                     CanvasGroup RT bytes & switches, live-DOM hidden-in-shown subtrees, layout
                     pass counters (and per-frame churn in sequence mode), calibrated desktop /
                     phone-mid layout CPU ms, and explicit refusals for uncalibrated draw-pass
                     CPU and mobile GPU UI ms.
";

fn run_cli(argv: &[String]) -> Result<(Value, bool, bool), Failure> {
    let (context, args) = Context::new(argv)?;

    if args.positional.first().map(String::as_str) == Some("compare") {
        if args.positional.len() < 3 {
            return Err(Failure::Usage("cost compare needs <before> <after>".into()));
        }
        let before_path = resolve_cost_file(&context.resolve(&args.positional[1]));
        let after_path = resolve_cost_file(&context.resolve(&args.positional[2]));
        let before = read_json_file(&before_path)?;
        let after = read_json_file(&after_path)?;
        validate_cost_report(&before, "before", Some(&before_path)).map_err(Failure::Error)?;
        validate_cost_report(&after, "after", Some(&after_path)).map_err(Failure::Error)?;
        let diff = compare_reports(&before, &after).map_err(Failure::Error)?;
        let refused = diff["verdict"].as_str() == Some("refused");
        if args.value("--out").is_some() {
            let out = resolve_output_file(&context, &args, "cost-compare", "compare.json");
            write_json_file(&out, &diff)?;
            if refused {
                eprintln!(
                    "rur cost compare: refused — {} | Remedy: {} -> {}",
                    diff["reason"].as_str().unwrap_or("incomparable reports"),
                    diff["remedy"].as_str().unwrap_or(""),
                    out.display()
                );
            } else {
                eprintln!("rur cost compare: {} -> {}", diff["verdict"].as_str().unwrap_or("?"), out.display());
            }
        } else if refused {
            eprintln!(
                "rur cost compare: refused — {} | Remedy: {}",
                diff["reason"].as_str().unwrap_or("incomparable reports"),
                diff["remedy"].as_str().unwrap_or("")
            );
        } else {
            eprintln!("rur cost compare: {}", diff["verdict"].as_str().unwrap_or("?"));
        }
        return Ok((diff, args.truthy("--json"), refused));
    }

    let is_sequence = ["--from", "--to", "--frames", "--duration", "--fps"].iter().any(|k| args.present(k));
    if is_sequence && args.present("--time") {
        return Err(Failure::Usage("cost sequence mode takes --from and --to (or --frames / --duration) with --fps, not --time".into()));
    }
    if is_sequence && args.present("--scene") {
        return Err(Failure::Usage("cost sequence mode runs a story, module, model, or fixture, not a saved --scene".into()));
    }

    let (report, default_name) = if is_sequence {
        let source = ["--story", "--module", "--model", "--fixture"]
            .iter()
            .find_map(|key| args.value(key))
            .ok_or_else(|| Failure::Usage("cost sequence mode needs --story, --module, --model, or --fixture".into()))?;
        let mut seq_opts = Map::new();
        for key in ["from", "to", "frames", "duration", "fps"] {
            if let Some(val) = args.value(&format!("--{key}")) {
                seq_opts.insert(key.into(), Value::String(val));
            }
        }
        seq_opts.insert("cost".into(), Value::Bool(true));
        let (spec, _) = crate::sequence::parse_options(&Value::Object(seq_opts), args.truthy("--offline"))
            .map_err(Failure::Usage)?;
        let engine_args = build_engine_args(&context, &args)?;
        let report = run_sequence_cost(&engine_args, &spec, &context.repo).map_err(Failure::Error)?;
        let name = format!("{}-cost", crate::story_args::source_name(&source, false));
        (report, name)
    } else if let Some(scene_arg) = args.value("--scene") {
        let scene_path = context.resolve(&scene_arg);
        let value = read_json_file(&scene_path)?;
        let mut store = SceneStore::default();
        store.apply(&value).map_err(Failure::Error)?;
        let report = analyze_scene(&store, LayoutStats::default(), None);
        let name = format!("{}-cost", crate::story_args::source_name(&scene_arg, true));
        (report, name)
    } else {
        let source = ["--story", "--module", "--model", "--fixture"]
            .iter()
            .find_map(|key| args.value(key))
            .ok_or_else(|| Failure::Usage("cost needs --story, --module, --model, --fixture, --scene, or `compare <before> <after>`".into()))?;
        let engine_args = build_engine_args(&context, &args)?;
        let mut resolver = crate::resolve::Resolver::new(context.repo.clone(), engine_args.offline);
        let mut run = engine::cli::run_once_with_cost(&engine_args).map_err(Failure::Error)?;
        for _ in 0..3 {
            if engine_args.offline {
                break;
            }
            let diagnostics = &run.scene.scene["diagnostics"];
            if !diagnostics["missingFontAssets"].as_array().is_some_and(|fonts| !fonts.is_empty()) {
                break;
            }
            if !resolver.fetch_fonts(diagnostics).unwrap_or(false) {
                break;
            }
            run = engine::cli::run_once_with_cost(&engine_args).map_err(Failure::Error)?;
        }
        let mut store = SceneStore::default();
        store.apply(&run.scene.scene).map_err(Failure::Error)?;
        let report = analyze_scene(&store, run.layout, Some(&run.dom_hidden));
        let name = format!("{}-cost", crate::story_args::source_name(&source, false));
        (report, name)
    };

    let dest = resolve_output_file(&context, &args, &default_name, "cost.json");
    write_json_file(&dest, &report)?;
    print_summary(&report, &dest);

    let failed = report["status"].as_str() == Some("error");
    if let Some(before_arg) = args.value("--compare") {
        let before_path = resolve_cost_file(&context.resolve(&before_arg));
        let before = read_json_file(&before_path)?;
        validate_cost_report(&before, "before", Some(&before_path)).map_err(Failure::Error)?;
        let diff = compare_reports(&before, &report).map_err(Failure::Error)?;
        let refused = diff["verdict"].as_str() == Some("refused");
        if let Some(parent) = dest.parent() {
            let cmp_path = parent.join("compare.json");
            write_json_file(&cmp_path, &diff)?;
            if refused {
                eprintln!(
                    "rur cost compare: refused — {} | Remedy: {} -> {}",
                    diff["reason"].as_str().unwrap_or("incomparable reports"),
                    diff["remedy"].as_str().unwrap_or(""),
                    cmp_path.display()
                );
            } else {
                eprintln!("rur cost compare: {} -> {}", diff["verdict"].as_str().unwrap_or("?"), cmp_path.display());
            }
        }
        return Ok((
            sort_json(json!({ "cost": report, "comparison": diff })),
            args.truthy("--json"),
            failed || refused,
        ));
    }

    Ok((report, args.truthy("--json"), failed))
}

/// Entrypoint for `rur-native cost ...`.
pub fn cli(argv: &[String]) -> i32 {
    if argv.iter().any(|a| a == "--help" || a == "-h") {
        print!("{HELP}");
        return 0;
    }
    match run_cli(argv) {
        Ok((value, compact, failed)) => {
            let out = if compact {
                serde_json::to_string(&value)
            } else {
                serde_json::to_string_pretty(&value)
            };
            if let Ok(text) = out {
                println!("{text}");
            }
            if failed { 2 } else { 0 }
        }
        Err(failure) => {
            eprintln!("rur cost: {}", failure.message());
            failure.code()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store_of(scene: Value) -> SceneStore {
        let mut store = SceneStore::default();
        store.apply(&scene).unwrap();
        store
    }

    #[test]
    fn batch_breaks_by_cause_and_additive_screen_guis() {
        let scene = store_of(json!({
            "viewport": {"width": 1000, "height": 500, "scale": 1.0},
            "status": "ok",
            "nodes": [
                {"id": 1, "name": "HudA", "path": "PlayerGui.HudA", "class": "ScreenGui", "rect": [0, 0, 1000, 500], "enabled": true},
                {"id": 2, "name": "S1", "path": "PlayerGui.HudA.S1", "class": "Frame", "rect": [0, 0, 100, 100], "bg": {"color": [1, 1, 1], "transparency": 0}},
                {"id": 3, "name": "S2", "path": "PlayerGui.HudA.S2", "class": "Frame", "rect": [120, 0, 100, 100], "bg": {"color": [1, 0, 0], "transparency": 0}},
                {"id": 4, "name": "Img1", "path": "PlayerGui.HudA.Img1", "class": "ImageLabel", "rect": [0, 120, 100, 100], "bg": {"color": [1, 1, 1], "transparency": 1}, "image": {"uri": "rbxasset://a.png", "transparency": 0}},
                {"id": 5, "name": "Img2", "path": "PlayerGui.HudA.Img2", "class": "ImageLabel", "rect": [120, 120, 100, 100], "bg": {"color": [1, 1, 1], "transparency": 1}, "image": {"uri": "rbxasset://a.png", "transparency": 0}},
                {"id": 6, "name": "Img3", "path": "PlayerGui.HudA.Img3", "class": "ImageLabel", "rect": [240, 120, 100, 100], "bg": {"color": [1, 1, 1], "transparency": 1}, "image": {"uri": "rbxasset://b.png", "transparency": 0}},
                {"id": 7, "name": "Clipped", "path": "PlayerGui.HudA.Clipped", "class": "Frame", "rect": [0, 250, 200, 100], "clip": [0, 250, 100, 100], "bg": {"color": [0, 1, 0], "transparency": 0}}
            ],
            "layers": [{"root": 1, "playerGui": true, "displayOrder": 0, "draw": [2, 3, 4, 5, 6, 7]}],
            "fonts": [],
            "images": []
        }));
        let report = analyze_scene(&scene, LayoutStats::default(), None);
        let t = &report["totals"];
        // S1+S2 share 1 solid batch -> shader break to Img1 -> Img2 shares Img1 -> texture break to Img3 -> clip break to Clipped
        assert_eq!(t["quads"], 6);
        assert_eq!(t["drawCalls"], 4);
        assert_eq!(t["batchBreaks"], 3);
        assert_eq!(t["batchBreaksByCause"]["shader"], 1);
        assert_eq!(t["batchBreaksByCause"]["texture"], 1);
        assert_eq!(t["batchBreaksByCause"]["clip"], 1);
        // Clipped frame (200x100) clipped to (100x100) -> 10,000 px instead of 20,000 px
        // Total screen area = 6 * 10,000 = 60,000 out of 500,000 = 0.12
        assert_eq!(t["screenOverdrawViewports"], 0.12);
    }

    #[test]
    fn canvas_group_studio_caps_and_transparent_skip() {
        let scene = store_of(json!({
            "viewport": {"width": 1920, "height": 1080, "scale": 1.5},
            "status": "ok",
            "nodes": [
                {"id": 1, "name": "G", "path": "G", "class": "ScreenGui", "rect": [0, 0, 1920, 1080], "enabled": true},
                {"id": 10, "name": "Wide", "path": "G.Wide", "class": "CanvasGroup", "rect": [0, 0, 2800, 50], "bg": {"color": [1, 1, 1], "transparency": 0}, "group": {"transparency": 0}},
                {"id": 11, "name": "Invis", "path": "G.Invis", "class": "CanvasGroup", "rect": [0, 100, 200, 100], "bg": {"color": [1, 1, 1], "transparency": 0}, "group": {"transparency": 1}}
            ],
            "layers": [{"root": 1, "playerGui": true, "displayOrder": 0, "draw": [
                {"group": 10, "draw": [10]},
                {"group": 11, "draw": [11]}
            ]}],
            "fonts": [],
            "images": []
        }));
        let report = analyze_scene(&scene, LayoutStats::default(), None);
        let t = &report["totals"];
        assert_eq!(t["canvasGroups"], 2);
        assert_eq!(t["canvasGroupTargets"], 1);
        // 2800x50 logical at 1.5x = 4200x75 device px -> Studio caps to (2049, 38) -> 2049*38*4 = 311,448 B
        assert_eq!(t["canvasRtBytes"], 2049 * 38 * 4);
        assert_eq!(t["canvasRtSwitches"], 2);
        assert_eq!(t["maxCanvasGroupDepth"], 1);
    }

    #[test]
    fn text_contextual_stroke_vs_frame_border_stroke() {
        let scene = store_of(json!({
            "viewport": {"width": 400, "height": 200, "scale": 1.0},
            "status": "ok",
            "nodes": [
                {"id": 1, "name": "G", "path": "G", "class": "ScreenGui", "rect": [0, 0, 400, 200], "enabled": true},
                {
                    "id": 2, "name": "Lbl", "path": "G.Lbl", "class": "TextLabel",
                    "rect": [10, 10, 100, 30], "bg": {"color": [0, 0, 0], "transparency": 1},
                    "text": {
                        "transparency": 0,
                        "stroke": {"transparency": 0},
                        "lines": [{"x": 10, "top": 10, "width": 50, "height": 20, "glyphs": [[0,1,0,14],[0,2,10,14],[0,3,20,14]]}]
                    },
                    "strokes": [{"mode": "Contextual", "thickness": 2, "transparency": 0}]
                },
                {
                    "id": 3, "name": "Box", "path": "G.Box", "class": "Frame",
                    "rect": [10, 60, 50, 50], "bg": {"color": [1, 1, 1], "transparency": 0},
                    "strokes": [{"mode": "Contextual", "thickness": 2, "transparency": 0}]
                }
            ],
            "layers": [{"root": 1, "playerGui": true, "displayOrder": 0, "draw": [2, 3]}],
            "fonts": [],
            "images": []
        }));
        let report = analyze_scene(&scene, LayoutStats::default(), None);
        let t = &report["totals"];
        // TextLabel: 3 fill + 12 legacy stroke + 3 contextual UIStroke = 18 glyph quads; 0 border UIStroke quads
        assert_eq!(t["fillGlyphQuads"], 3);
        assert_eq!(t["strokeGlyphQuads"], 15);
        assert_eq!(t["glyphQuads"], 18);
        // Frame: 1 solid fill + 1 border UIStroke (Contextual on Frame applies to Border) = 2 quads
        assert_eq!(t["quads"], 2);
        assert_eq!(t["quadsByKind"]["stroke"], 1);
    }

    #[test]
    fn calibration_and_refusal_contract() {
        let stats = LayoutStats {
            passes: 2,
            stored: 10,
            moved: 5,
            scroll_moved: 0,
            ..Default::default()
        };
        let pred = predicted_json(stats);
        let expected_desk = 2.0 * STUDIO_LAYOUT_PASS_MS + 15.0 * STUDIO_LAYOUT_OBJECT_MS;
        assert!((pred["desktop-ref"]["layoutCpuMs"]["ms"].as_f64().unwrap() - expected_desk).abs() < 1e-9);
        assert_eq!(pred["phone-mid"]["layoutCpuMs"]["basis"], "anchored");
        assert!((pred["phone-mid"]["layoutCpuMs"]["ms"].as_f64().unwrap() - expected_desk * PHONE_UI_MAIN_RATIO).abs() < 1e-9);
        for tier in ["desktop-ref", "phone-mid"] {
            for metric in ["drawPassCpuMs", "gpuUiMs"] {
                assert_eq!(pred[tier][metric]["status"], "refused");
                assert!(pred[tier][metric]["ms"].is_null());
                assert!(pred[tier][metric]["reason"].as_str().unwrap().len() >= 12);
                assert!(pred[tier][metric]["settledBy"].as_str().unwrap().len() >= 12);
            }
        }
    }

    fn sample_cost_report(
        mode: &str,
        input: &str,
        status: &str,
        width: f64,
        height: f64,
        scale: f64,
        complete: bool,
        quads: u64,
        canvas_rt_bytes: u64,
        hidden_gui: u64,
    ) -> Value {
        json!({
            "schema": "rur-cost/1",
            "status": status,
            "mode": mode,
            "input": input,
            "viewport": {
                "width": width,
                "height": height,
                "scale": scale,
                "physicalWidth": (width * scale).round() as u32,
                "physicalHeight": (height * scale).round() as u32,
            },
            "totals": {
                "screenGuis": 1, "disabledScreenGuis": 0, "drawnNodes": quads, "quads": quads,
                "glyphQuads": 20, "drawCalls": 6, "batchBreaks": 5,
                "canvasGroupTargets": if canvas_rt_bytes > 0 { 1 } else { 0 },
                "canvasRtBytes": canvas_rt_bytes, "canvasRtSwitches": if canvas_rt_bytes > 0 { 2 } else { 0 },
                "screenOverdrawViewports": 0.5, "offscreenOverdrawViewports": 0.2,
                "hiddenInShown": {"complete": complete, "roots": if hidden_gui > 0 { 1 } else { 0 }, "guiObjects": hidden_gui, "instances": hidden_gui}
            },
            "layout": {"passes": 1, "computed": quads, "stored": quads, "moved": 0, "scrollMoved": 0},
            "predicted": {
                "desktop-ref": {"layoutCpuMs": {"ms": 0.005}},
                "phone-mid": {"layoutCpuMs": {"ms": 0.015}}
            },
            "screenGuis": [{"name": "Hud", "path": "PlayerGui.Hud", "quads": quads, "glyphQuads": 20, "batchBreaks": 5, "canvasRtBytes": canvas_rt_bytes, "screenOverdrawViewports": 0.5, "hiddenGuiObjects": hidden_gui}]
        })
    }

    #[test]
    fn compare_reports_classifies_improvements_and_regressions() {
        let before = sample_cost_report("single", "live", "ok", 1920.0, 1080.0, 1.5, true, 10, 400_000, 10);
        let after = sample_cost_report("single", "live", "ok", 1920.0, 1080.0, 1.5, true, 8, 200_000, 0);
        let diff = compare_reports(&before, &after).unwrap();
        assert_eq!(diff["schema"], "rrr-ab/1");
        assert_eq!(diff["verdict"], "improved");
        assert_eq!(diff["equivalence"]["passed"], true);
        assert_eq!(diff["measurementNoise"], 0.0);
        assert_eq!(diff["counts"]["delta"]["canvasRtBytes"], -200_000.0);
        assert_eq!(diff["counts"]["delta"]["hiddenGuiObjects"], -10.0);
        assert_eq!(diff["screenGuiMovers"].as_array().unwrap().len(), 1);

        let regressed = compare_reports(&after, &before).unwrap();
        assert_eq!(regressed["verdict"], "regressed");
        assert_eq!(regressed["equivalence"]["passed"], true);
        assert_eq!(regressed["counts"]["delta"]["canvasRtBytes"], 200_000.0);
    }

    #[test]
    fn compare_reports_rejects_non_cost_files_and_refuses_incomparable_arms() {
        let live_single = sample_cost_report("single", "live", "ok", 1920.0, 1080.0, 1.5, true, 10, 0, 6);
        let scene_single = sample_cost_report("single", "scene", "ok", 1920.0, 1080.0, 1.5, false, 10, 0, 2);
        let live_seq = sample_cost_report("sequence", "live", "ok", 1920.0, 1080.0, 1.5, true, 10, 0, 6);
        let diff_vp = sample_cost_report("single", "live", "ok", 1280.0, 720.0, 1.5, true, 10, 0, 6);
        let diff_scale = sample_cost_report("single", "live", "ok", 1920.0, 1080.0, 1.0, true, 10, 0, 6);
        let err_rep = sample_cost_report("single", "live", "error", 1920.0, 1080.0, 1.5, true, 0, 0, 0);

        // 1. scene.json passed as a cost report -> error naming scene.json
        let scene_json = json!({"viewport": {"width": 800, "height": 600, "scale": 1}, "nodes": [], "layers": []});
        let err = compare_reports(&scene_json, &live_single).unwrap_err();
        assert!(err.contains("scene.json"), "expected scene.json error, got: {err}");

        // 2. compare diff report (rrr-ab/1) or arbitrary JSON -> error naming what it is
        let cmp_json = json!({"schema": "rrr-ab/1", "verdict": "unchanged"});
        let err_cmp = compare_reports(&live_single, &cmp_json).unwrap_err();
        assert!(err_cmp.contains("rrr-ab/1"), "expected rrr-ab/1 error, got: {err_cmp}");
        let err_obj = compare_reports(&json!({"hello": 1}), &live_single).unwrap_err();
        assert!(err_obj.contains("rur-cost/1"), "expected missing schema error, got: {err_obj}");

        // 3. Live vs --scene -> refused on hiddenInShown.complete / input
        let ref_ls = compare_reports(&live_single, &scene_single).unwrap();
        assert_eq!(ref_ls["verdict"], "refused");
        assert_eq!(ref_ls["equivalence"]["passed"], false);
        assert!(ref_ls["reason"].as_str().unwrap().contains("hiddenInShown.complete"));
        assert!(ref_ls["remedy"].as_str().unwrap().contains("--scene"));

        // 4. Single vs sequence -> refused on mode
        let ref_ss = compare_reports(&live_single, &live_seq).unwrap();
        assert_eq!(ref_ss["verdict"], "refused");
        assert!(ref_ss["reason"].as_str().unwrap().contains("mode"));
        assert!(ref_ss["remedy"].as_str().unwrap().contains("--from"));

        // 5. Viewport mismatch -> refused on viewport
        let ref_vp = compare_reports(&live_single, &diff_vp).unwrap();
        assert_eq!(ref_vp["verdict"], "refused");
        assert!(ref_vp["reason"].as_str().unwrap().contains("viewport"));
        assert!(ref_vp["remedy"].as_str().unwrap().contains("--viewport"));

        // 6. Scale mismatch -> refused on scale
        let ref_sc = compare_reports(&live_single, &diff_scale).unwrap();
        assert_eq!(ref_sc["verdict"], "refused");
        assert!(ref_sc["reason"].as_str().unwrap().contains("scale"));
        assert!(ref_sc["remedy"].as_str().unwrap().contains("--scale"));

        // 7. Status mismatch or both error -> refused on status
        let ref_st = compare_reports(&live_single, &err_rep).unwrap();
        assert_eq!(ref_st["verdict"], "refused");
        assert!(ref_st["reason"].as_str().unwrap().contains("status"));
        let ref_both_err = compare_reports(&err_rep, &err_rep).unwrap();
        assert_eq!(ref_both_err["verdict"], "refused");
        assert!(ref_both_err["reason"].as_str().unwrap().contains("non-ok"));
    }

    #[test]
    fn sequence_cost_zero_churn_on_static_and_matches_single_moment_on_anim() {
        let repo = crate::engine::default_repo();
        let root = repo.join("tools/tests/oracle-root");
        let make_args = |fixture: &Path, extra: &[&str]| {
            let mut argv: Vec<String> = vec![
                "--repo".into(), repo.to_string_lossy().into_owned(),
                "--roots".into(), root.to_string_lossy().into_owned(),
                "--fixture".into(), fixture.to_string_lossy().into_owned(),
                "--viewport".into(), "800x520".into(),
                "--no-snapshots".into(),
            ];
            argv.extend(extra.iter().map(|s| s.to_string()));
            engine::cli::parse(&argv).unwrap()
        };

        // 1. Static fixture (corners.luau): frame 0 lays out; frames 1..4 have 0 layout churn
        let static_args = make_args(&repo.join("fixtures/corners.luau"), &[]);
        let static_spec = engine::cli::SequenceSpec { from: 0.0, frames: 5, fps: 60.0, collect_cost: true };
        let static_rep = run_sequence_cost(&static_args, &static_spec, &repo).unwrap();
        let s_frames = static_rep["sequence"]["frames"].as_array().unwrap();
        assert_eq!(s_frames.len(), 5);
        assert!(s_frames[0]["layout"]["passes"].as_u64().unwrap() >= 1);
        assert!(s_frames[0]["layout"]["stored"].as_u64().unwrap() >= 20);
        for f in &s_frames[1..] {
            assert_eq!(f["layout"]["passes"].as_u64().unwrap(), 0, "static frame {} had passes", f["index"]);
            assert_eq!(f["layout"]["stored"].as_u64().unwrap(), 0, "static frame {} had stored", f["index"]);
            assert_eq!(f["layout"]["moved"].as_u64().unwrap(), 0, "static frame {} had moved", f["index"]);
        }

        // 2. Animated fixture (anim-tweens.luau): positive churn on frames 1..3 AND exact single-moment parity
        let anim_path = repo.join("fixtures/anim-tweens.luau");
        let anim_args = make_args(&anim_path, &[]);
        let anim_spec = engine::cli::SequenceSpec { from: 0.5, frames: 4, fps: 30.0, collect_cost: true };
        let anim_rep = run_sequence_cost(&anim_args, &anim_spec, &repo).unwrap();
        let a_frames = anim_rep["sequence"]["frames"].as_array().unwrap();
        assert_eq!(a_frames.len(), 4);
        for (i, af) in a_frames.iter().enumerate() {
            if i > 0 {
                assert!(af["layout"]["passes"].as_u64().unwrap() >= 1, "anim frame {i} had 0 passes");
                let touched = af["layout"]["stored"].as_u64().unwrap() + af["layout"]["moved"].as_u64().unwrap();
                assert!(touched > 0, "anim frame {i} had 0 layout churn");
            }
            let t = 0.5 + i as f64 / 30.0;
            let single_run = engine::cli::run_once_with_cost(&make_args(&anim_path, &["--time", &format!("{t:.9}")])).unwrap();
            let mut store = SceneStore::default();
            store.apply(&single_run.scene.scene).unwrap();
            let single_rep = analyze_scene(&store, single_run.layout, Some(&single_run.dom_hidden));
            for key in ["drawnNodes", "quads", "glyphQuads", "drawCalls", "batchBreaks", "canvasGroupTargets", "canvasRtBytes", "screenOverdrawViewports"] {
                assert_eq!(af["totals"][key], single_rep["totals"][key], "frame {i} mismatch on {key}");
            }
        }
    }
}
