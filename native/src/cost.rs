//! `rur cost` (`rur-native cost`): deterministic Roblox UI render-cost pass.
//!
//! Counts deterministic UI work for a screen (or a saved `scene.json`) without a GPU device,
//! rasterization, or host wall-clock timing fields:
//! - Non-glyph quads by kind, glyph quads (fill + legacy 4-offset TextStroke + contextual
//!   UIStroke), and clipped screen / offscreen overdraw in viewport multiples.
//! - An unmeasured state-transition model for draw calls and batch breaks (by cause: layerPass,
//!   renderTarget, clip, shader, texture, rotation) — distinct from rur's uber-shader, informational
//!   only (not calibrated against a client; client UI image atlasing is unmodeled).
//! - `CanvasGroup` offscreen render-target allocations using exact Studio rectangular RGBA8
//!   dimensions (`render::geometry::group_texture_dims`: float32 logical size, 2048 px side cap,
//!   2048 x 1024 px area cap, skipping culled, empty, and fully transparent groups), RT switches,
//!   and nesting depth.
//! - Live-DOM hidden-in-shown accounting (`Visible = false` roots and all descendant GuiObjects and
//!   instances under enabled ScreenGuis), separated from disabled ScreenGuis (`Enabled = false`).
//! - Deterministic `Layout.Stats` counters, extrapolated desktop layout CPU ms (Studio 0.744 client
//!   lazy-layout slopes from `engine/layout/install.rs`), derived `phone-mid` layout CPU ms range
//!   (3.9x..6.3x central, 2.55x..12.32x full band from whole-UI captured ratio * assumed capture
//!   factor * live frame weight), and explicit refusals (`status: "refused"`, `ms: null`) for
//!   uncalibrated draw-pass CPU and mobile GPU UI milliseconds.
//! - `rur cost compare <before> <after>` deterministic A/B diff (`rrr-ab/1`).

use std::{
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
};

use serde_json::{Map, Value, json};

use crate::{
    engine::{
        self, Engine, GuiEngine, InstanceId, K, NameId,
        layout::Stats as LayoutStats,
        services,
        value::Value as EngineValue,
    },
    render::geometry::{
        IDENTITY, Rect, Transform, alpha8, apply, clip_rect, component, device_span,
        group_texture_dims, invert, multiply, physical_rect, text_clip_rect,
    },
    scene::{SceneStore, layer_pass, node_id},
    story_args::{Args, Context, Failure},
};

/// Recorded Studio 0.744 client lazy-layout calibration (`native/src/engine/layout/install.rs`).
pub const STUDIO_LAYOUT_PASS_MS: f64 = 2.3e-3;
pub const STUDIO_LAYOUT_OBJECT_MS: f64 = 0.312e-3;
pub const STUDIO_SCROLL_PASS_MS: f64 = 62.0e-3;

/// Derived live frame-weighted mobile mid-tier UI main-thread scaling factors for uncaptured
/// desktop layout timings:
/// - Whole-UI-pass captured phone/desktop ratio: central 4.09 (range 2.65..8.0)
/// - Assumed profiler capture-overhead factor (unmeasured for layout): 1.25..2.0
/// - Live frame weight: 0.77
/// Central derived range: 4.09 * 1.25 * 0.77 = 3.936625 (~3.9x) .. 4.09 * 2.0 * 0.77 = 6.2986 (~6.3x).
/// Full uncertainty band: 2.65 * 1.25 * 0.77 = 2.550625 (~2.55x) .. 8.0 * 2.0 * 0.77 = 12.32 (~12.32x).
pub const PHONE_UI_CAPTURED_RATIO_CENTRAL: f64 = 4.09;
pub const PHONE_UI_CAPTURED_RATIO_LOW: f64 = 2.65;
pub const PHONE_UI_CAPTURED_RATIO_HIGH: f64 = 8.0;
pub const PHONE_CAPTURE_FACTOR_LOW: f64 = 1.25;
pub const PHONE_CAPTURE_FACTOR_HIGH: f64 = 2.0;
pub const PHONE_UI_LIVE_WEIGHT: f64 = 0.77;

pub const PHONE_UI_MAIN_RATIO_LOW: f64 =
    PHONE_UI_CAPTURED_RATIO_CENTRAL * PHONE_CAPTURE_FACTOR_LOW * PHONE_UI_LIVE_WEIGHT;
pub const PHONE_UI_MAIN_RATIO_HIGH: f64 =
    PHONE_UI_CAPTURED_RATIO_CENTRAL * PHONE_CAPTURE_FACTOR_HIGH * PHONE_UI_LIVE_WEIGHT;
pub const PHONE_UI_MAIN_LOW: f64 =
    PHONE_UI_CAPTURED_RATIO_LOW * PHONE_CAPTURE_FACTOR_LOW * PHONE_UI_LIVE_WEIGHT;
pub const PHONE_UI_MAIN_HIGH: f64 =
    PHONE_UI_CAPTURED_RATIO_HIGH * PHONE_CAPTURE_FACTOR_HIGH * PHONE_UI_LIVE_WEIGHT;

const MAX_OBJECTS_REPORTED: usize = 200;
const MAX_HIDDEN_SUBTREES_REPORTED: usize = 100;
const MAX_LAYOUT_DRIVERS_REPORTED: usize = 50;

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

/// Live-DOM hidden-in-shown, disabled ScreenGui, and interior clip geometry summary collected
/// before engine teardown.
#[derive(Clone, Debug, Default)]
pub struct DomHiddenSummary {
    /// True when walked from the live DOM; false when derived only from a saved `scene.json`
    /// (which drops descendants below `Visible = false` nodes).
    pub complete: bool,
    /// True when the live DOM walk saw zero mounted user instances outside the empty host
    /// (`CoreGui.RURStory` / `StoryTarget`), zero hidden GuiObjects, and zero disabled ScreenGuis.
    pub mounted_empty: bool,
    pub roots: u64,
    pub gui_objects: u64,
    pub instances: u64,
    pub disabled_screen_guis: u64,
    pub disabled_screen_gui_instances: u64,
    pub by_root: BTreeMap<u64, RootHidden>,
    pub disabled_guis: Vec<DisabledGui>,
    pub subtrees: Vec<HiddenSubtree>,
    /// Logical clip rect `[x, y, w, h]` of each visible unrotated clipping `GuiObject`
    /// (`ClipsDescendants == true` or `CanvasGroup`) under enabled roots, matching `engine/export.rs`.
    pub clip_rects: BTreeMap<u64, [f64; 4]>,
    /// Live-DOM parent serial for each visited instance under enabled roots.
    pub parent_of: BTreeMap<u64, u64>,
}

/// Walks the live DOM under enabled exported roots to count `Visible = false` subtrees (all
/// descendants, including non-GuiObject instances) and separately inventories disabled ScreenGuis
/// under PlayerGui and CoreGui.
pub fn collect_dom_hidden(engine: &Engine, roots: &[InstanceId]) -> DomHiddenSummary {
    let reflection = &engine.reflection;
    let layer_collector = reflection.class_id("LayerCollector");
    let gui_object = reflection.class_id("GuiObject");
    let canvas_group = reflection.class_id("CanvasGroup");
    let gui_base = reflection.class_id("GuiBase2d");
    let world_bound = [reflection.class_id("SurfaceGuiBase"), reflection.class_id("BillboardGui")];
    let game = engine.world().game;
    let core_gui = services::get_service(engine, game, "CoreGui").ok();
    let state = engine.state.borrow();
    let dom = &state.dom;
    let layout_guard = engine.layout().borrow();
    let layout_ref = layout_guard.as_ref();
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
    let mut mounted_user_instances = 0u64;

    for &root in roots {
        if dom.get(root).is_none() {
            continue;
        }
        let is_story_host = dom.at(root).name.to_string() == "RURStory"
            && core_gui.is_some_and(|cg| dom.at(root).parent == Some(cg));
        if is_story_host {
            for &child in &dom.at(root).children {
                if dom.at(child).name.to_string() == "StoryTarget" {
                    mounted_user_instances += dom.at(child).children.len() as u64;
                } else {
                    mounted_user_instances += 1;
                }
            }
        } else {
            mounted_user_instances += 1;
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
        let mut stack: Vec<(InstanceId, u64)> = dom
            .at(root)
            .children
            .iter()
            .rev()
            .map(|&child| (child, root_serial))
            .collect();
        while let Some((child, parent_serial)) = stack.pop() {
            let child_serial = u64::from(dom.at(child).serial);
            summary.parent_of.insert(child_serial, parent_serial);
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
                        id: child_serial,
                        name: dom.at(child).name.to_string(),
                        path: dom.full_name(reflection, child),
                        class: reflection.class(dom.at(child).class).name.to_string(),
                        screen_gui: sg_name.clone(),
                        gui_objects: go,
                        instances: insts,
                    });
                    continue;
                }
                let clips = matches!(dom.raw_get(child, K::ClipsDescendants as NameId), Some(EngineValue::Bool(true)))
                    || is_class(child, canvas_group);
                if clips {
                    if let Some(rec) = layout_ref.and_then(|l| l.record(child)) {
                        if rec.rotation == 0.0 {
                            let (ax, ay) = engine::layout::absolute_position(rec);
                            let rect = match rec.window {
                                Some(w) => [ax + w.x - rec.x, ay + w.y - rec.y, w.w, w.h],
                                None => [ax, ay, rec.w, rec.h],
                            };
                            summary.clip_rects.insert(child_serial, rect);
                        }
                    }
                }
            }
            for &grand in dom.at(child).children.iter().rev() {
                stack.push((grand, child_serial));
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
    summary.mounted_empty = mounted_user_instances == 0
        && summary.gui_objects == 0
        && summary.disabled_screen_guis == 0;
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

/// Returns the device-space affine transform carrying `node`'s local device rect into the current
/// render target, or `None` when axis-aligned (identity). Inside a rotated `CanvasGroup`,
/// `target_unturn` holds `invert(group_screen_matrix)` in logical units so descendants are
/// measured in the group's own flat texture coordinates, matching `render::groups::localized`.
fn effective_node_transform(
    node: &Value,
    scale: f64,
    target_unturn: Option<Transform>,
) -> Option<Transform> {
    let has_matrix = node.get("matrix").and_then(Value::as_array).is_some_and(|m| m.len() >= 6);
    let mut m = match (target_unturn, has_matrix) {
        (None, false) => return None,
        (None, true) => std::array::from_fn(|i| component(&node["matrix"], i, IDENTITY[i])),
        (Some(inv), _) => {
            let mat: Transform = if has_matrix {
                std::array::from_fn(|i| component(&node["matrix"], i, IDENTITY[i]))
            } else {
                IDENTITY
            };
            multiply(inv, mat)
        }
    };
    if (0..4).all(|i| (m[i] - IDENTITY[i]).abs() < 1e-6) && m[4].abs() < 1e-3 && m[5].abs() < 1e-3 {
        return None;
    }
    m[4] *= scale;
    m[5] *= scale;
    Some(m)
}

fn clip_half_plane(
    src: &[[f64; 2]; 16],
    len: usize,
    axis: usize,
    bound: f64,
    keep_ge: bool,
) -> ([[f64; 2]; 16], usize) {
    let mut dst = [[0.0; 2]; 16];
    if len == 0 {
        return (dst, 0);
    }
    let mut out = 0usize;
    let inside = |p: [f64; 2]| if keep_ge { p[axis] >= bound } else { p[axis] <= bound };
    let mut prev = src[len - 1];
    let mut prev_in = inside(prev);
    for &curr in &src[..len] {
        let curr_in = inside(curr);
        if curr_in != prev_in {
            let d = curr[axis] - prev[axis];
            if d.abs() > 1e-12 {
                let t = ((bound - prev[axis]) / d).clamp(0.0, 1.0);
                let other = 1 - axis;
                let mut pt = [0.0; 2];
                pt[axis] = bound;
                pt[other] = prev[other] + t * (curr[other] - prev[other]);
                if out < dst.len() {
                    dst[out] = pt;
                    out += 1;
                }
            }
        }
        if curr_in && out < dst.len() {
            dst[out] = curr;
            out += 1;
        }
        prev = curr;
        prev_in = curr_in;
    }
    (dst, out)
}

/// Exact area (in target device pixels) of `rect`, optionally transformed by `xform`, clipped to
/// `clip ∩ target`. Axis-aligned rects take a direct intersection fast path; rotated quads use
/// Sutherland-Hodgman polygon clipping against the four half-planes followed by shoelace area so
/// rotation preserves true covered area instead of inflating to an axis-aligned bounding box.
fn clipped_quad_area(
    rect: Rect,
    xform: Option<Transform>,
    clip: Option<[f64; 4]>,
    target: [f64; 4],
) -> f64 {
    let (mut cx0, mut cy0, mut cx1, mut cy1) = (target[0], target[1], target[2], target[3]);
    if let Some(c) = clip {
        cx0 = cx0.max(c[0]);
        cy0 = cy0.max(c[1]);
        cx1 = cx1.min(c[2]);
        cy1 = cy1.min(c[3]);
    }
    if cx1 <= cx0 || cy1 <= cy0 || rect.w <= 0.0 || rect.h <= 0.0 {
        return 0.0;
    }
    let Some(m) = xform else {
        let ix0 = rect.x.max(cx0);
        let iy0 = rect.y.max(cy0);
        let ix1 = (rect.x + rect.w).min(cx1);
        let iy1 = (rect.y + rect.h).min(cy1);
        return if ix1 <= ix0 || iy1 <= iy0 { 0.0 } else { (ix1 - ix0) * (iy1 - iy0) };
    };
    let mut poly = [[0.0f64; 2]; 16];
    poly[0] = apply(m, rect.x, rect.y);
    poly[1] = apply(m, rect.x + rect.w, rect.y);
    poly[2] = apply(m, rect.x + rect.w, rect.y + rect.h);
    poly[3] = apply(m, rect.x, rect.y + rect.h);
    let (poly, len) = clip_half_plane(&poly, 4, 0, cx0, true);
    let (poly, len) = clip_half_plane(&poly, len, 0, cx1, false);
    let (poly, len) = clip_half_plane(&poly, len, 1, cy0, true);
    let (poly, len) = clip_half_plane(&poly, len, 1, cy1, false);
    if len < 3 {
        return 0.0;
    }
    let mut twice = 0.0;
    for i in 0..len {
        let a = poly[i];
        let b = poly[(i + 1) % len];
        twice += a[0] * b[1] - b[0] * a[1];
    }
    0.5 * twice.abs()
}

/// Effective scissor state within `target_bounds`. A clip rect that covers the entire render
/// target changes no scissor state and normalizes to `None`.
fn effective_scissor_key(clip: Option<[f64; 4]>, target_bounds: [f64; 4]) -> Option<[i64; 4]> {
    let c = clip?;
    let x0 = c[0].max(target_bounds[0]).round() as i64;
    let y0 = c[1].max(target_bounds[1]).round() as i64;
    let x1 = c[2].min(target_bounds[2]).round() as i64;
    let y1 = c[3].min(target_bounds[3]).round() as i64;
    let tx0 = target_bounds[0].round() as i64;
    let ty0 = target_bounds[1].round() as i64;
    let tx1 = target_bounds[2].round() as i64;
    let ty1 = target_bounds[3].round() as i64;
    if x0 <= tx0 && y0 <= ty0 && x1 >= tx1 && y1 >= ty1 {
        None
    } else {
        Some([x0, y0, x1, y1])
    }
}

/// Strips a `CanvasGroup`'s inherited ancestor clip from a descendant's exported clip rect when
/// drawing into the group's offscreen texture (`render::groups::texture_clip_node`, Studio rule:
/// a group's background and descendants draw into the group's texture clipped only by the group's
/// bounds and interior clipping frames; the group's ancestor clip applies only on composite).
fn strip_group_ancestor_clip(
    clip: Option<[f64; 4]>,
    group_ancestor_clip: Option<[f64; 4]>,
    target_bounds: [f64; 4],
) -> Option<[f64; 4]> {
    let mut c = clip?;
    let Some(g) = group_ancestor_clip else {
        return Some(c);
    };
    if (c[0] - g[0]).abs() <= 1e-6 && g[0] > target_bounds[0] {
        c[0] = target_bounds[0];
    }
    if (c[2] - g[2]).abs() <= 1e-6 && g[2] < target_bounds[2] {
        c[2] = target_bounds[2];
    }
    if (c[1] - g[1]).abs() <= 1e-6 && g[1] > target_bounds[1] {
        c[1] = target_bounds[1];
    }
    if (c[3] - g[3]).abs() <= 1e-6 && g[3] < target_bounds[3] {
        c[3] = target_bounds[3];
    }
    Some(c)
}

fn effective_text_clip(
    node: &Value,
    scale: f64,
    in_group: bool,
    target_id: u64,
    node_clip: Option<[f64; 4]>,
) -> Option<[f64; 4]> {
    if in_group {
        return None;
    }
    if target_id == 0 {
        return text_clip_rect(node, [0.0, 0.0], scale);
    }
    if node["text"]["clip"] != Value::Bool(true) {
        return node_clip;
    }
    let rect = &node["rect"];
    let num = |v: &Value| v.as_f64().filter(|n| n.is_finite()).unwrap_or(0.0);
    let (x, y) = match node["absolute"].as_array() {
        Some(a) if a.len() >= 2 => (num(&a[0]), num(&a[1])),
        _ => (num(&rect[0]), num(&rect[1])),
    };
    let [l, r] = device_span(x, num(&rect[2]), scale);
    let [t, b] = device_span(y, num(&rect[3]), scale);
    let own = [l, t, l.max(r), t.max(b)];
    Some(match node_clip {
        None => own,
        Some(c) => {
            let (x0, y0) = (own[0].max(c[0]), own[1].max(c[1]));
            [x0, y0, own[2].min(c[2]).max(x0), own[3].min(c[3]).max(y0)]
        }
    })
}

fn round4(value: f64) -> f64 {
    if !value.is_finite() {
        return 0.0;
    }
    let rounded = (value * 10_000.0).round() / 10_000.0;
    if rounded == 0.0 { 0.0 } else { rounded }
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
    sibling_index: u32,
    path_count: u32,
    pos: [f64; 2],
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
    /// Live-DOM `(clip_rects, parent_of)` when analyzing a live run (`None` for `--scene`).
    dom_clips: Option<(&'a BTreeMap<u64, [f64; 4]>, &'a BTreeMap<u64, u64>)>,
    /// 1-based ordinal of each node among all nodes sharing its `path` in scene order.
    path_indices: BTreeMap<u64, u32>,
    /// Total count of nodes sharing each `path` in the scene.
    path_counts: BTreeMap<String, u32>,
    totals: TotalsAccum,
    last_state_by_target: BTreeMap<u64, BatchState>,
    objects: BTreeMap<u64, ObjectAccum>,
}

impl<'a> Analyzer<'a> {
    fn new(scene: &'a SceneStore, scale: f64, dom_hidden: Option<&'a DomHiddenSummary>) -> Self {
        let mut totals = TotalsAccum {
            total_nodes: scene.node_count() as u64,
            ..Default::default()
        };
        for cause in ["layerPass", "renderTarget", "clip", "shader", "texture", "rotation"] {
            totals.batch_breaks_by_cause.insert(cause, 0);
        }
        let mut path_counts: BTreeMap<String, u32> = BTreeMap::new();
        let mut path_indices: BTreeMap<u64, u32> = BTreeMap::new();
        for node in scene.ordered_nodes() {
            if let (Some(id), Some(path)) = (node.get("id").and_then(node_id), node["path"].as_str()) {
                let count = path_counts.entry(path.to_string()).or_insert(0);
                *count += 1;
                path_indices.insert(id, *count);
            }
        }
        Self {
            scene,
            scale,
            dom_clips: dom_hidden.filter(|h| h.complete).map(|h| (&h.clip_rects, &h.parent_of)),
            path_indices,
            path_counts,
            totals,
            last_state_by_target: BTreeMap::new(),
            objects: BTreeMap::new(),
        }
    }

    /// Resolves the active clip rect for `node` (`nid`) when drawing onto `target_id`.
    /// On screen (`target_id == 0`), the exported scene clip already holds all ancestors.
    /// Inside a `CanvasGroup` (`target_id != 0`), Studio clips only by interior clipping ancestors
    /// strictly between `target_id` and `nid` (never by ancestors outside `target_id`).
    fn effective_node_clip(
        &self,
        node: &Value,
        nid: u64,
        target_id: u64,
        target_bounds: [f64; 4],
        in_group: bool,
        group_ancestor_clip: Option<[f64; 4]>,
    ) -> Option<[f64; 4]> {
        if in_group {
            return None;
        }
        if target_id == 0 {
            return clip_rect(node, [0.0, 0.0], self.scale);
        }
        if let Some((clip_rects, parent_of)) = self.dom_clips {
            let mut cursor = nid;
            let mut acc: Option<[f64; 4]> = None;
            while let Some(&parent) = parent_of.get(&cursor) {
                if parent == target_id {
                    break;
                }
                if let Some(&r) = clip_rects.get(&parent) {
                    let (mut x1, mut y1) = (r[0], r[1]);
                    let (mut x2, mut y2) = (r[0] + r[2], r[1] + r[3]);
                    if let Some(c) = acc {
                        x1 = x1.max(c[0]);
                        y1 = y1.max(c[1]);
                        x2 = x2.min(c[0] + c[2]);
                        y2 = y2.min(c[1] + c[3]);
                    }
                    acc = Some([
                        f64::from(x1 as f32),
                        f64::from(y1 as f32),
                        f64::from((x2 - x1).max(0.0) as f32),
                        f64::from((y2 - y1).max(0.0) as f32),
                    ]);
                }
                cursor = parent;
            }
            return acc.map(|c| {
                let [l, r] = device_span(c[0], c[2], self.scale);
                let [t, b] = device_span(c[1], c[3], self.scale);
                [l, t, l.max(r), t.max(b)]
            });
        }
        strip_group_ancestor_clip(clip_rect(node, [0.0, 0.0], self.scale), group_ancestor_clip, target_bounds)
    }

    fn object_entry(&mut self, node: &Value, nid: u64, sg_name: &str) -> &mut ObjectAccum {
        let path_indices = &self.path_indices;
        let path_counts = &self.path_counts;
        self.objects.entry(nid).or_insert_with(|| {
            let path = node["path"].as_str().unwrap_or("?").to_string();
            let rect = &node["rect"];
            let x = rect.get(0).and_then(Value::as_f64).unwrap_or(0.0);
            let y = rect.get(1).and_then(Value::as_f64).unwrap_or(0.0);
            ObjectAccum {
                id: nid,
                name: node["name"].as_str().unwrap_or("?").to_string(),
                sibling_index: path_indices.get(&nid).copied().unwrap_or(1),
                path_count: path_counts.get(&path).copied().unwrap_or(1),
                pos: [x, y],
                path,
                class: node["class"].as_str().unwrap_or("?").to_string(),
                screen_gui: sg_name.to_string(),
                ..Default::default()
            }
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn record_batch(
        &mut self,
        sg: &mut ScreenGuiAccum,
        node: &Value,
        nid: u64,
        target_id: u64,
        target_area_scale: f64,
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
        let scaled_area = area_px * target_area_scale;
        if target_id == 0 {
            self.totals.screen_overdraw_px += scaled_area;
            sg.screen_overdraw_px += scaled_area;
            self.object_entry(node, nid, &sg.name).screen_overdraw_px += scaled_area;
        } else {
            self.totals.offscreen_overdraw_px += scaled_area;
            sg.offscreen_overdraw_px += scaled_area;
            self.object_entry(node, nid, &sg.name).offscreen_overdraw_px += scaled_area;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_quads(
        &mut self,
        sg: &mut ScreenGuiAccum,
        node: &Value,
        nid: u64,
        target_id: u64,
        target_bounds: [f64; 4],
        target_area_scale: f64,
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
            scissor: effective_scissor_key(clip, target_bounds),
            rotated,
        };
        self.record_batch(sg, node, nid, target_id, target_area_scale, state, area_px);
        self.totals.quads += quad_count;
        *self.totals.quads_by_kind.entry(quad_kind).or_insert(0) += quad_count;
        sg.quads += quad_count;
        self.object_entry(node, nid, &sg.name).quads += quad_count;
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_node(
        &mut self,
        sg: &mut ScreenGuiAccum,
        node: &Value,
        nid: u64,
        target_id: u64,
        target_bounds: [f64; 4],
        target_area_scale: f64,
        target_unturn: Option<Transform>,
        pass_id: u8,
        in_group: bool,
        group_ancestor_clip: Option<[f64; 4]>,
    ) {
        if node["culled"].as_bool() == Some(true) || node["visible"].as_bool() == Some(false) {
            return;
        }
        let mut drawn_any = false;
        let rot = is_rotated(node);
        let xform = if in_group { None } else { effective_node_transform(node, self.scale, target_unturn) };

        // Path2D
        if let Some(p2d) = node.get("path2d").and_then(Value::as_object) {
            let segs = p2d.get("segments").and_then(Value::as_array).map_or(0, Vec::len) as u64;
            if segs > 0 {
                let phys = if in_group { flat_physical_rect(node, self.scale) } else { physical_rect(node, self.scale).rect };
                let clip = self.effective_node_clip(node, nid, target_id, target_bounds, in_group, group_ancestor_clip);
                let area = clipped_quad_area(phys, xform, clip, target_bounds);
                if area > 0.0 {
                    self.emit_quads(sg, node, nid, target_id, target_bounds, target_area_scale, pass_id, "path2d", "path2d", None, clip, false, segs, area);
                    drawn_any = true;
                }
            }
            if drawn_any {
                self.totals.drawn_nodes += 1;
                sg.drawn_nodes += 1;
            }
            return;
        }

        let phys = if in_group { flat_physical_rect(node, self.scale) } else { physical_rect(node, self.scale).rect };
        let clip = self.effective_node_clip(node, nid, target_id, target_bounds, in_group, group_ancestor_clip);
        let clipped_area = clipped_quad_area(phys, xform, clip, target_bounds);
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
                        self.emit_quads(sg, node, nid, target_id, target_bounds, target_area_scale, pass_id, "shadow", "shadow", None, clip, rot, 1, clipped_area);
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
                self.emit_quads(sg, node, nid, target_id, target_bounds, target_area_scale, pass_id, shader, shader, None, clip, rot, 1, clipped_area);
                drawn_any = true;
                if !in_group && !corner {
                    if let Some(border) = node.get("border").and_then(Value::as_object) {
                        if border.get("size").and_then(Value::as_f64).unwrap_or(0.0) > 0.0 {
                            self.emit_quads(sg, node, nid, target_id, target_bounds, target_area_scale, pass_id, "border", shader, None, clip, rot, 1, 0.0);
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
                        target_bounds,
                        target_area_scale,
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
                let tclip = effective_text_clip(node, self.scale, in_group, target_id, clip);
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
                    let la = clipped_quad_area(Rect::new(lx, lt, lw, lh), xform, tclip, target_bounds);
                    if la > 0.0 {
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
                        scissor: effective_scissor_key(tclip, target_bounds),
                        rotated: rot,
                    };
                    self.record_batch(sg, node, nid, target_id, target_area_scale, state, line_area);
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
                        self.emit_quads(sg, node, nid, target_id, target_bounds, target_area_scale, pass_id, "shadow", "shadow", None, clip, rot, 1, clipped_area);
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
                        self.emit_quads(sg, node, nid, target_id, target_bounds, target_area_scale, pass_id, "stroke", "stroke", None, clip, rot, 1, 0.0);
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

    #[allow(clippy::too_many_arguments)]
    fn draw_post(
        &mut self,
        sg: &mut ScreenGuiAccum,
        node: &Value,
        nid: u64,
        target_id: u64,
        target_bounds: [f64; 4],
        target_area_scale: f64,
        pass_id: u8,
        group_ancestor_clip: Option<[f64; 4]>,
    ) {
        let Some(scroll) = node.get("scroll").and_then(Value::as_object) else { return };
        let thickness = (scroll.get("thickness").and_then(Value::as_f64).unwrap_or(0.0) - 1e-6).ceil() as i64;
        if thickness <= 0 || alpha8(1.0 - scroll.get("transparency").and_then(Value::as_f64).unwrap_or(0.0)) == 0 {
            return;
        }
        let clip = self.effective_node_clip(node, nid, target_id, target_bounds, false, group_ancestor_clip);
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
                        target_bounds,
                        target_area_scale,
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
        target_area_scale: f64,
        target_unturn: Option<Transform>,
        pass_id: u8,
        depth: u64,
        skip_group: Option<u64>,
        group_ancestor_clip: Option<[f64; 4]>,
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
                    let group_area_scale = (f64::from(nw) / flat.w) * (f64::from(nh) / flat.h);
                    let new_depth = depth + 1;
                    self.totals.canvas_group_targets += 1;
                    self.totals.canvas_rt_bytes += rt_bytes;
                    self.totals.canvas_rt_switches += 2;
                    self.totals.max_canvas_group_depth = self.totals.max_canvas_group_depth.max(new_depth);
                    sg.canvas_group_targets += 1;
                    sg.canvas_rt_bytes += rt_bytes;
                    sg.canvas_rt_switches += 2;
                    self.object_entry(gnode, gid, &sg.name).canvas_rt_bytes += rt_bytes;

                    // Recurse into offscreen CanvasGroup render target: background and descendants
                    // draw into the group's flat texture clipped only by the group's bounds and
                    // interior clipping ancestors, measured in actual offscreen RT texels.
                    let rt_bounds = flat.bounds();
                    let raw_group_clip = clip_rect(gnode, [0.0, 0.0], self.scale);
                    let child_unturn = if gnode.get("matrix").and_then(Value::as_array).is_some_and(|m| m.len() >= 6) {
                        let turn: Transform = std::array::from_fn(|i| component(&gnode["matrix"], i, IDENTITY[i]));
                        let det = turn[0] * turn[3] - turn[1] * turn[2];
                        if det.abs() > 1e-12 { Some(invert(turn)) } else { None }
                    } else {
                        None
                    };
                    self.last_state_by_target.remove(&gid);
                    if let Some(sub_draw) = obj.get("draw").and_then(Value::as_array) {
                        self.walk_entries(sg, sub_draw, gid, rt_bounds, group_area_scale, child_unturn, pass_id, new_depth, Some(gid), raw_group_clip);
                    }

                    // Composite offscreen RT onto parent target (parent's interior clip and unturn apply here)
                    let phys = physical_rect(gnode, self.scale).rect;
                    let xform = effective_node_transform(gnode, self.scale, target_unturn);
                    let clip = self.effective_node_clip(gnode, gid, target_id, target_bounds, false, group_ancestor_clip);
                    let area = clipped_quad_area(phys, xform, clip, target_bounds);
                    if area > 0.0 {
                        self.emit_quads(
                            sg,
                            gnode,
                            gid,
                            target_id,
                            target_bounds,
                            target_area_scale,
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
                self.draw_post(sg, node, nid, target_id, target_bounds, target_area_scale, pass_id, group_ancestor_clip);
            } else {
                self.draw_node(sg, node, nid, target_id, target_bounds, target_area_scale, target_unturn, pass_id, skip_group == Some(nid), group_ancestor_clip);
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
        "scrollPasses": stats.scroll_passes,
        "scrollMoved": stats.scroll_moved,
    })
}

fn layout_stats_json_scoped(stats: LayoutStats, scope: &'static str) -> Value {
    json!({
        "scope": scope,
        "passes": stats.passes,
        "computed": stats.computed,
        "memo": stats.memo,
        "measures": stats.measures,
        "stored": stats.stored,
        "skipped": stats.skipped,
        "moved": stats.moved,
        "texts": stats.texts,
        "scrollPasses": stats.scroll_passes,
        "scrollMoved": stats.scroll_moved,
    })
}

pub fn desktop_layout_cpu_ms(stats: LayoutStats) -> f64 {
    let scroll_passes = if stats.scroll_passes > 0 {
        stats.scroll_passes
    } else if stats.scroll_moved > 0 {
        1
    } else {
        0
    };
    stats.passes as f64 * STUDIO_LAYOUT_PASS_MS
        + (stats.stored + stats.moved + stats.scroll_moved) as f64 * STUDIO_LAYOUT_OBJECT_MS
        + scroll_passes as f64 * STUDIO_SCROLL_PASS_MS
}

fn draw_and_gpu_refusals() -> (Value, Value) {
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
    (draw_refusal, gpu_refusal)
}

fn desktop_layout_extrapolation(desk_ms: f64, scope: &'static str, extrapolated_from: &'static str) -> Value {
    json!({
        "status": "extrapolated",
        "basis": "extrapolated",
        "scope": scope,
        "ms": round9(desk_ms),
        "extrapolatedFrom": extrapolated_from,
        "source": "Studio 0.744 client script-triggered lazy-layout slope (native/src/engine/layout/install.rs: 2.3 us/pass + 0.312 us/(stored+moved) + 62 us/scroll + 0.312 us/scrollMoved)"
    })
}

fn phone_layout_extrapolation(desk_ms: f64, scope: &'static str, extrapolated_from: &'static str) -> Value {
    let central_low = round4(desk_ms * PHONE_UI_MAIN_RATIO_LOW);
    let central_high = round4(desk_ms * PHONE_UI_MAIN_RATIO_HIGH);
    let band_low = round4(desk_ms * PHONE_UI_MAIN_LOW);
    let band_high = round4(desk_ms * PHONE_UI_MAIN_HIGH);
    json!({
        "status": "extrapolated",
        "basis": "derived_extrapolation",
        "scope": scope,
        "rangeMs": [central_low, central_high],
        "bandMs": [band_low, band_high],
        "ratioBasis": "derived",
        "ratioRange": [round4(PHONE_UI_MAIN_RATIO_LOW), round4(PHONE_UI_MAIN_RATIO_HIGH)],
        "bandRatio": [round4(PHONE_UI_MAIN_LOW), round4(PHONE_UI_MAIN_HIGH)],
        "factors": {
            "capturedWholeUiPhoneOverDesktop": {
                "central": PHONE_UI_CAPTURED_RATIO_CENTRAL,
                "range": [PHONE_UI_CAPTURED_RATIO_LOW, PHONE_UI_CAPTURED_RATIO_HIGH],
                "basis": "recorded_whole_ui_pass"
            },
            "captureOverheadFactor": {
                "range": [PHONE_CAPTURE_FACTOR_LOW, PHONE_CAPTURE_FACTOR_HIGH],
                "basis": "assumed_unmeasured_for_layout"
            },
            "liveFrameWeight": {
                "value": PHONE_UI_LIVE_WEIGHT,
                "basis": "assumed_from_match_gui_weight"
            },
            "formula": "capturedWholeUiPhoneOverDesktop * captureOverheadFactor * liveFrameWeight"
        },
        "extrapolatedFrom": extrapolated_from,
        "source": "Derived from desktop-ref: 4.09 (range 2.65..8.0) whole-UI captured phone/desktop ratio * assumed 1.25..2.0 capture overhead factor (unmeasured for layout) * assumed 0.77 live frame weight (from mobile live-gameplay GUI category) -> central ratio 3.94x..6.30x (3.9x..6.3x), full band 2.55x..12.32x",
        "settledBy": "Record paired captured and uncaptured UpdateUILayouts timings on desktop to measure the layout capture overhead factor, and compare against recorded mobile mid-tier UI layout timings."
    })
}

pub fn predicted_sequence_json(desk_ms: f64, scope: &'static str) -> Value {
    let (draw_refusal, gpu_refusal) = draw_and_gpu_refusals();
    let desk_from = "Studio 0.744 script-triggered lazy-layout probe (100-1,000 card scroll and single-card edit slopes) extrapolated to frame layout passes";
    let phone_from = "desktop-ref extrapolated layoutCpuMs scaled by derived ratio 3.9x..6.3x (4.09 * assumed 1.25..2.0 capture factor * 0.77; full band 2.55x..12.32x)";
    json!({
        "desktop-ref": {
            "layoutCpuMs": desktop_layout_extrapolation(desk_ms, scope, desk_from),
            "drawPassCpuMs": draw_refusal,
            "gpuUiMs": gpu_refusal
        },
        "phone-mid": {
            "layoutCpuMs": phone_layout_extrapolation(desk_ms, scope, phone_from),
            "drawPassCpuMs": draw_refusal,
            "gpuUiMs": gpu_refusal
        }
    })
}

pub fn predicted_json(stats: LayoutStats) -> Value {
    predicted_sequence_json(desktop_layout_cpu_ms(stats), "per_frame")
}

fn predicted_single_live_json(stats: LayoutStats) -> Value {
    let desk_ms = desktop_layout_cpu_ms(stats);
    let (draw_refusal, gpu_refusal) = draw_and_gpu_refusals();
    let frame_refusal = json!({
        "status": "refused",
        "basis": "unmeasured",
        "scope": "per_frame_unavailable_in_single_moment",
        "ms": Value::Null,
        "reason": "Single-moment mode runs an export full-tree layout pass, not a frame's incremental layout churn; see fullTreeLayoutCpuMs for the one-shot full-tree cost.",
        "settledBy": "Run `rur cost` with --from and --frames (and --fps) to measure per-frame layout churn."
    });
    let desk_from = "Studio 0.744 script-triggered lazy-layout probe (2.3 us/pass + 0.312 us/record) extrapolated to laying the whole tree out once during export";
    let phone_from = "desktop-ref fullTreeLayoutCpuMs scaled by derived ratio 3.9x..6.3x (4.09 * assumed 1.25..2.0 capture factor * 0.77; full band 2.55x..12.32x)";
    json!({
        "desktop-ref": {
            "layoutCpuMs": frame_refusal,
            "fullTreeLayoutCpuMs": desktop_layout_extrapolation(desk_ms, "full_tree_export_pass", desk_from),
            "drawPassCpuMs": draw_refusal,
            "gpuUiMs": gpu_refusal
        },
        "phone-mid": {
            "layoutCpuMs": frame_refusal,
            "fullTreeLayoutCpuMs": phone_layout_extrapolation(desk_ms, "full_tree_export_pass", phone_from),
            "drawPassCpuMs": draw_refusal,
            "gpuUiMs": gpu_refusal
        }
    })
}

fn predicted_scene_only_json() -> Value {
    let (draw_refusal, gpu_refusal) = draw_and_gpu_refusals();
    let layout_refusal = json!({
        "status": "refused",
        "basis": "unmeasured",
        "scope": "unavailable_in_scene_mode",
        "ms": Value::Null,
        "reason": "Saved scene.json carries no layout pass counters.",
        "settledBy": "Run live with --story/--fixture and --from/--frames/--fps to measure per-frame layout churn."
    });
    json!({
        "desktop-ref": {
            "layoutCpuMs": layout_refusal,
            "drawPassCpuMs": draw_refusal,
            "gpuUiMs": gpu_refusal
        },
        "phone-mid": {
            "layoutCpuMs": layout_refusal,
            "drawPassCpuMs": draw_refusal,
            "gpuUiMs": gpu_refusal
        }
    })
}

fn format_hz(hz: f64) -> String {
    if !hz.is_finite() {
        return "?".into();
    }
    if (hz - hz.round()).abs() < 1e-6 {
        format!("{}", hz.round() as u64)
    } else {
        format!("{hz:.2}").trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

pub fn run_mode_info(run_mode: &str) -> Value {
    run_mode_info_with_cadence(run_mode, 60.0, "client", None)
}

pub fn run_mode_info_with_cadence(run_mode: &str, client_hz: f64, cadence_kind: &str, sample_fps: Option<f64>) -> Value {
    let hz_str = format_hz(client_hz);
    match run_mode {
        "play" => {
            let meaning = match sample_fps {
                Some(fps) => format!(
                    "Playing client cadence ({hz_str} Hz {cadence_kind}, 1 Heartbeat per client frame; sampled @ {} fps); steady-state layout counters and extrapolated layoutCpuMs are normalized per 1 client frame at {hz_str} Hz.",
                    format_hz(fps),
                ),
                None => format!(
                    "Playing client cadence ({hz_str} Hz {cadence_kind}, 1 Heartbeat per client frame); per-frame layout counters and extrapolated layoutCpuMs reflect one client {hz_str} Hz frame."
                ),
            };
            let mut obj = Map::new();
            obj.insert("mode".into(), json!("play"));
            obj.insert("clientHz".into(), json!(round4(client_hz)));
            obj.insert("cadenceKind".into(), json!(cadence_kind));
            if let Some(fps) = sample_fps {
                obj.insert("sampleFps".into(), json!(round4(fps)));
            }
            obj.insert("heartbeatsPerFrame".into(), json!(1));
            obj.insert("heartbeatsPerClientFrame".into(), json!(1));
            obj.insert("meaning".into(), json!(meaning));
            Value::Object(obj)
        }
        "edit" => {
            let mut obj = Map::new();
            obj.insert("mode".into(), json!("edit"));
            obj.insert("clientHz".into(), json!(round4(client_hz)));
            obj.insert("cadenceKind".into(), json!(cadence_kind));
            if let Some(fps) = sample_fps {
                obj.insert("sampleFps".into(), json!(round4(fps)));
            }
            obj.insert("heartbeatsPerFrame".into(), json!(4));
            obj.insert("meaning".into(), json!("Studio edit mode (240 Hz Heartbeat, 4 beats per 60 Hz render frame); per-frame layoutCpuMs is refused because Heartbeat-driven UI can run up to 4x a playing client's rate. Pass --mode play for client per-frame layout figures."));
            Value::Object(obj)
        }
        "scene" => json!({
            "mode": "scene",
            "heartbeatsPerFrame": Value::Null,
            "meaning": "Offline analysis of a saved scene.json (no engine run; layout counters unavailable)."
        }),
        other => json!({
            "mode": other,
            "heartbeatsPerFrame": Value::Null,
            "meaning": "Non-standard engine run mode; per-frame layoutCpuMs is refused unless run with --mode play."
        }),
    }
}

fn predicted_non_single_client_frame_json(client_frames: u32, client_hz: f64, sample_fps: f64, is_mount: bool) -> Value {
    let (draw_refusal, gpu_refusal) = draw_and_gpu_refusals();
    let hz_str = format_hz(client_hz);
    let fps_str = format_hz(sample_fps);
    let (basis, scope, reason) = if is_mount {
        (
            "mount_and_runup",
            "mount_and_runup_not_one_client_frame",
            format!("Frame 0 includes initial mount and run-up ({client_frames} client frames at {hz_str} Hz), not one steady client frame."),
        )
    } else {
        (
            "not_one_client_frame",
            "sample_not_one_client_frame",
            format!("Sample spans {client_frames} client frames (sampled @ {fps_str} fps on a {hz_str} Hz client); not printed as one client frame."),
        )
    };
    let layout_refusal = json!({
        "status": "refused",
        "basis": basis,
        "scope": scope,
        "ms": Value::Null,
        "clientFrames": client_frames,
        "reason": reason,
        "settledBy": "Read top-level predicted / sequence.summary.steadyStateLayoutCpuMs (normalized per 1 client frame) or sample at --fps matching --client-hz."
    });
    json!({
        "desktop-ref": {
            "layoutCpuMs": layout_refusal,
            "drawPassCpuMs": draw_refusal,
            "gpuUiMs": gpu_refusal
        },
        "phone-mid": {
            "layoutCpuMs": layout_refusal,
            "drawPassCpuMs": draw_refusal,
            "gpuUiMs": gpu_refusal
        }
    })
}

fn predicted_no_client_frames_json(sample_count: usize, client_hz: f64, sample_fps: f64) -> Value {
    let (draw_refusal, gpu_refusal) = draw_and_gpu_refusals();
    let hz_str = format_hz(client_hz);
    let fps_str = format_hz(sample_fps);
    let layout_refusal = json!({
        "status": "refused",
        "basis": "no_client_frame_stepped",
        "scope": "steady_state_no_client_frames",
        "ms": Value::Null,
        "reason": format!("0 client frames completed after frame 0 ({sample_count} samples @ {fps_str} fps on a {hz_str} Hz client); cannot price one client frame."),
        "settledBy": format!("Increase --frames or --duration so at least 1 {hz_str} Hz client frame completes after frame 0, or lower --fps.")
    });
    json!({
        "desktop-ref": {
            "layoutCpuMs": layout_refusal,
            "drawPassCpuMs": draw_refusal,
            "gpuUiMs": gpu_refusal
        },
        "phone-mid": {
            "layoutCpuMs": layout_refusal,
            "drawPassCpuMs": draw_refusal,
            "gpuUiMs": gpu_refusal
        }
    })
}

/// Result of probing play mode from an explicit `--mode edit` sequence run so edit-only stories
/// report the real end state instead of sending the caller back to `--mode play` in a circle.
#[derive(Clone, Debug)]
pub struct PlayModeProbe {
    pub mounts_in_play_mode: bool,
    pub status: String,
    pub failure: Option<String>,
    pub waiting_at: Option<String>,
}

/// Failure, hint, waiting location, and mode-probe metadata carried from an engine run into `rur-cost/1`.
#[derive(Clone, Debug, Default)]
pub struct RunOutcome {
    pub status: Option<String>,
    pub failure: Option<String>,
    pub waiting_at: Option<String>,
    pub hint: Option<String>,
    pub edit_mode_mounts: Option<bool>,
    pub play_mode_probe: Option<PlayModeProbe>,
}

const EMPTY_MOUNT_MESSAGE: &str =
    "story mounted nothing (0 instances under StoryTarget and 0 drawn nodes; zero counts are not a valid UI cost measurement)";

fn extract_waiting_at(value: &Value) -> Option<String> {
    if let Some(at) = value.get("waiting").and_then(Value::as_str).filter(|s| !s.trim().is_empty()) {
        return Some(at.trim().to_string());
    }
    let output = value.pointer("/diagnostics/output").and_then(Value::as_array)?;
    let waiting = output.iter().find(|entry| {
        entry["kind"] == "warn"
            && entry["message"]
                .as_str()
                .is_some_and(|message| message.starts_with("story mount is still yielding"))
    })?;
    let frame = waiting
        .get("trace")
        .and_then(Value::as_str)
        .and_then(|trace| trace.lines().map(str::trim).find(|frame| !frame.is_empty() && !frame.starts_with("rur.")));
    Some(match frame {
        Some(f) => crate::hints::story_frame(f, None),
        None => "the story's require".to_string(),
    })
}

fn is_error_wrapper_line(line: &str) -> bool {
    let trimmed = line.trim();
    let is_banner = trimmed.len() >= 12
        && trimmed.bytes().take(6).all(|b| b == b'-')
        && trimmed.bytes().rev().take(6).all(|b| b == b'-');
    trimmed.ends_with(':') || is_banner || trimmed == "Stack Begin" || trimmed == "Stack End"
}

fn is_internal_trace_frame(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.ends_with("function traceback")
        || trimmed.starts_with("rur.")
        || trimmed.contains(".vide.")
        || trimmed.starts_with("vide.")
}

/// Extracts `(cause_line, optional_context_frame)` from a raw Luau/Vide/React error string,
/// skipping wrapper-only prefix lines (e.g. `error while running root():`, `effect error:`,
/// `error while running branch():`) so the actual error cause and first user frame appear on screen.
pub fn summarize_failure_lines(failure: &str) -> (String, Option<String>) {
    let lines: Vec<&str> = failure.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    if lines.is_empty() {
        return (String::new(), None);
    }
    let cause_idx = lines
        .iter()
        .position(|line| !is_error_wrapper_line(line))
        .unwrap_or(0);
    let cause = lines[cause_idx].to_string();
    let rest = &lines[cause_idx + 1..];
    let context = rest
        .iter()
        .find(|line| !is_error_wrapper_line(line) && !is_internal_trace_frame(line) && !line.starts_with("... traceback"))
        .or_else(|| {
            rest.iter()
                .find(|line| !is_error_wrapper_line(line) && !line.ends_with("function traceback"))
        })
        .map(|s| (*s).to_string());
    (cause, context)
}

fn failure_headline(failure: &str, max_chars: usize) -> String {
    let (cause, context) = summarize_failure_lines(failure);
    let combined = match context {
        Some(ctx) if !cause.contains(&ctx) && cause.len() < 80 => format!("{cause} [{ctx}]"),
        _ => cause,
    };
    first_line(&combined, max_chars)
}

/// Extracts the story's status, root failure cause (unwrapping Luau's generic require wrapper via
/// `crate::hints::failure_cause`), waiting location (for yielded mounts), and optional hint from
/// a scene or sequence end report.
pub fn extract_run_outcome(value: &Value) -> RunOutcome {
    let status = value.get("status").and_then(Value::as_str).map(str::to_string);
    let diagnostics = &value["diagnostics"];
    let output = &diagnostics["output"];
    let waiting_at = extract_waiting_at(value);
    let failure = value
        .get("failure")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(|raw| crate::hints::failure_cause(raw, output).to_string())
        .or_else(|| {
            if status.as_deref() == Some("error") {
                output
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|e| e["kind"] == "error")
                    .filter_map(|e| e["message"].as_str())
                    .find(|s| !s.trim().is_empty())
                    .map(str::to_string)
            } else {
                None
            }
        });
    let hint = diagnostics
        .get("hint")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string);
    RunOutcome {
        status,
        failure,
        waiting_at,
        hint,
        edit_mode_mounts: None,
        play_mode_probe: None,
    }
}

/// Checks whether a source that failed or mounted nothing in play mode actually mounts UI under
/// Studio `--mode edit` at `probe_time` (matching `--from` in sequence mode or `--time` in single mode).
fn probe_mounts_in_edit_mode(engine_args: &engine::cli::Args, probe_time: f64) -> bool {
    let mut edit_args = engine_args.clone();
    edit_args.mode = Some("edit".into());
    edit_args.time = Some(probe_time.max(0.05));
    match engine::cli::run_once_with_cost(&edit_args) {
        Ok(run) => {
            run.scene.status == "ok"
                && (!run.dom_hidden.mounted_empty
                    || run.dom_hidden.gui_objects > 0
                    || run.dom_hidden.disabled_screen_guis > 0)
        }
        Err(_) => false,
    }
}

/// Checks whether a source run in explicit `--mode edit` also mounts cleanly under `--mode play`.
fn probe_mounts_in_play_mode(engine_args: &engine::cli::Args, probe_time: f64) -> PlayModeProbe {
    let mut play_args = engine_args.clone();
    play_args.mode = Some("play".into());
    play_args.time = Some(probe_time.max(0.05));
    match engine::cli::run_once_with_cost(&play_args) {
        Ok(run) => {
            let outcome = extract_run_outcome(&run.scene.scene);
            let status = outcome.status.unwrap_or_else(|| run.scene.status.clone());
            let mounted = !run.dom_hidden.mounted_empty
                || run.dom_hidden.gui_objects > 0
                || run.dom_hidden.disabled_screen_guis > 0;
            if status == "ok" && mounted {
                PlayModeProbe {
                    mounts_in_play_mode: true,
                    status: "ok".into(),
                    failure: None,
                    waiting_at: None,
                }
            } else {
                let effective_status = if status == "ok" { "empty".into() } else { status };
                let failure = if effective_status == "empty" {
                    Some(EMPTY_MOUNT_MESSAGE.to_string())
                } else if effective_status == "yielded" {
                    outcome
                        .waiting_at
                        .as_ref()
                        .map(|at| format!("the story's mount is still waiting, at {at}"))
                        .or(outcome.failure)
                } else {
                    outcome.failure
                };
                PlayModeProbe {
                    mounts_in_play_mode: false,
                    status: effective_status,
                    failure,
                    waiting_at: outcome.waiting_at,
                }
            }
        }
        Err(err) => PlayModeProbe {
            mounts_in_play_mode: false,
            status: "error".into(),
            failure: Some(err),
            waiting_at: None,
        },
    }
}

fn non_ok_reason(status: &str, failure: Option<&str>, drawn_nodes: u64) -> String {
    match status {
        "empty" => EMPTY_MOUNT_MESSAGE.to_string(),
        "yielded" if drawn_nodes > 0 => format!(
            "Story mount drew UI ({drawn_nodes} nodes) and then yielded without returning (for example, an inline loop in mount); per-frame layoutCpuMs is refused until mount returns."
        ),
        "yielded" => match failure {
            Some(msg) => format!("{} (0 drawn nodes; zero counts are not a valid UI cost measurement).", first_line(msg, 160)),
            None => "Story mount yielded before mounting UI; zero counts are not a valid UI cost measurement.".into(),
        },
        _ => match (failure, drawn_nodes > 0) {
            (Some(msg), true) => format!("Story failed ({}); counts from a failed run are not a valid UI cost measurement.", failure_headline(msg, 160)),
            (Some(msg), false) => format!("Story failed ({}); zero counts are not a valid UI cost measurement.", failure_headline(msg, 160)),
            (None, true) => "Story failed to run; counts from a failed run are not a valid UI cost measurement.".into(),
            (None, false) => "Story failed to run; zero counts are not a valid UI cost measurement.".into(),
        },
    }
}

fn non_ok_settled_by(
    status: &str,
    drawn_nodes: u64,
    edit_mode_mounts: Option<bool>,
    play_mode_mounts: Option<bool>,
) -> &'static str {
    if status == "yielded" && drawn_nodes > 0 {
        "Wrap any frame or animation loop inside task.spawn(...) or RunService.Heartbeat:Connect(...) so the mount function returns its cleanup handle."
    } else if edit_mode_mounts == Some(true) {
        "Story mounts in edit mode, not play, so it cannot get per-frame layoutCpuMs as-is; re-run with --mode edit for static counts, CanvasGroup RT, overdraw, and hidden DOM, or fix the play-mode wait/failure above."
    } else if play_mode_mounts == Some(true) {
        "Story mounts in play mode, not edit; re-run with --mode play (or use --from and --frames for a play-mode sequence)."
    } else {
        "Fix the story failure or mount path so the UI mounts cleanly before reading cost figures."
    }
}

fn predicted_non_ok_json(
    status: &str,
    failure: Option<&str>,
    drawn_nodes: u64,
    single_live: bool,
    edit_mode_mounts: Option<bool>,
    play_mode_mounts: Option<bool>,
) -> Value {
    let (draw_refusal, gpu_refusal) = draw_and_gpu_refusals();
    let basis = if status == "empty" {
        "nothing_mounted"
    } else if status == "yielded" && drawn_nodes > 0 {
        "mount_did_not_return"
    } else {
        "story_failed"
    };
    let scope = if status == "empty" {
        "refused_nothing_mounted"
    } else if status == "yielded" && drawn_nodes > 0 {
        "refused_mount_yielded_after_draw"
    } else {
        "refused_story_failed"
    };
    let layout_refusal = json!({
        "status": "refused",
        "basis": basis,
        "scope": scope,
        "ms": Value::Null,
        "reason": non_ok_reason(status, failure, drawn_nodes),
        "settledBy": non_ok_settled_by(status, drawn_nodes, edit_mode_mounts, play_mode_mounts),
    });
    if single_live {
        json!({
            "desktop-ref": {
                "layoutCpuMs": layout_refusal,
                "fullTreeLayoutCpuMs": layout_refusal,
                "drawPassCpuMs": draw_refusal,
                "gpuUiMs": gpu_refusal
            },
            "phone-mid": {
                "layoutCpuMs": layout_refusal,
                "fullTreeLayoutCpuMs": layout_refusal,
                "drawPassCpuMs": draw_refusal,
                "gpuUiMs": gpu_refusal
            }
        })
    } else {
        json!({
            "desktop-ref": {
                "layoutCpuMs": layout_refusal,
                "drawPassCpuMs": draw_refusal,
                "gpuUiMs": gpu_refusal
            },
            "phone-mid": {
                "layoutCpuMs": layout_refusal,
                "drawPassCpuMs": draw_refusal,
                "gpuUiMs": gpu_refusal
            }
        })
    }
}

fn edit_mode_fallback_json() -> Value {
    json!({
        "mountsInEditMode": true,
        "flag": "--mode edit",
        "provides": ["quads", "glyphQuads", "canvasRtBytes", "overdraw", "hiddenInShown"],
        "refuses": ["per-frame layoutCpuMs (240 Hz edit-mode Heartbeat)"],
        "note": "Story mounts in edit mode, not play; this story cannot get per-frame layoutCpuMs as-is. Pass --mode edit for static counts (quads, glyphs, CanvasGroup RT, overdraw, hidden DOM), or fix the play-mode wait/failure above."
    })
}

fn play_mode_probe_json(probe: &PlayModeProbe, edit_ok: bool) -> Value {
    let mut obj = Map::new();
    obj.insert("mountsInPlayMode".into(), json!(probe.mounts_in_play_mode));
    obj.insert("status".into(), json!(probe.status));
    if let Some(at) = &probe.waiting_at {
        obj.insert("waitingAt".into(), json!(at));
    }
    if let Some(f) = &probe.failure {
        obj.insert("failure".into(), json!(failure_headline(f, 200)));
    }
    if probe.mounts_in_play_mode && !edit_ok {
        obj.insert("flag".into(), json!("--mode play"));
        obj.insert(
            "note".into(),
            json!("Story mounts in play mode, not edit; pass --mode play (or --from and --frames for a play-mode sequence)."),
        );
    } else if !probe.mounts_in_play_mode && edit_ok {
        obj.insert(
            "note".into(),
            json!("This story does not mount in play mode, so per-frame layoutCpuMs is unavailable as-is. Fix the play-mode wait or failure to price per-frame layout, or use these edit-mode counts/RT/overdraw/hidden DOM only."),
        );
    }
    Value::Object(obj)
}

fn apply_run_outcome(report: &mut Value, outcome: &RunOutcome, single_live: bool) {
    let Some(obj) = report.as_object_mut() else { return };
    if let Some(s) = outcome.status.as_deref() {
        let cur = obj.get("status").and_then(Value::as_str).unwrap_or("ok");
        if s != "ok" || cur == "ok" {
            obj.insert("status".into(), json!(s));
        }
    }
    let status = obj.get("status").and_then(Value::as_str).unwrap_or("ok").to_string();
    let drawn_nodes = obj
        .get("totals")
        .and_then(|t| t.get("drawnNodes"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if let Some(hint) = &outcome.hint {
        obj.insert("hint".into(), json!(hint));
    }
    let play_mode_mounts = outcome.play_mode_probe.as_ref().map(|p| p.mounts_in_play_mode);
    if let Some(probe) = &outcome.play_mode_probe {
        obj.insert("playModeProbe".into(), play_mode_probe_json(probe, status == "ok"));
    }
    if status == "ok" {
        return;
    }
    if status == "yielded" {
        if let Some(at) = &outcome.waiting_at {
            obj.insert("waitingAt".into(), json!(at));
        }
    }
    let failure_text = if status == "empty" {
        EMPTY_MOUNT_MESSAGE.to_string()
    } else if let Some(msg) = &outcome.failure {
        msg.clone()
    } else if status == "yielded" {
        match &outcome.waiting_at {
            Some(at) => format!("the story's mount is still waiting, at {at}"),
            None => "story mount yielded and did not finish before capture".to_string(),
        }
    } else {
        format!("story run ended with status \"{status}\"")
    };
    obj.insert("failure".into(), json!(failure_text));
    if outcome.edit_mode_mounts == Some(true) {
        obj.insert("editModeFallback".into(), edit_mode_fallback_json());
    }
    obj.insert(
        "predicted".into(),
        predicted_non_ok_json(
            &status,
            Some(&failure_text),
            drawn_nodes,
            single_live,
            outcome.edit_mode_mounts,
            play_mode_mounts,
        ),
    );
    if let Some(fid) = obj.get_mut("fidelity").and_then(Value::as_object_mut) {
        fid.insert(
            "layoutCpuMs".into(),
            json!(format!("refused ({status}: {})", non_ok_reason(&status, Some(&failure_text), drawn_nodes))),
        );
    }
    if let Some(summary) = obj
        .get_mut("sequence")
        .and_then(|s| s.get_mut("summary"))
        .and_then(Value::as_object_mut)
    {
        let basis = if status == "empty" {
            "nothing_mounted"
        } else if status == "yielded" && drawn_nodes > 0 {
            "mount_did_not_return"
        } else {
            "story_failed"
        };
        summary.insert(
            "steadyStateLayoutCpuMs".into(),
            json!({
                "status": "refused",
                "basis": basis,
                "desktop-ref": Value::Null,
                "phone-mid-range": Value::Null,
                "phone-mid-band": Value::Null,
                "reason": non_ok_reason(&status, Some(&failure_text), drawn_nodes),
                "settledBy": non_ok_settled_by(&status, drawn_nodes, outcome.edit_mode_mounts, play_mode_mounts),
            }),
        );
    }
}

fn first_line(s: &str, max_chars: usize) -> String {
    let line = s.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("").trim();
    if line.chars().count() <= max_chars || max_chars <= 3 {
        line.to_string()
    } else {
        let head: String = line.chars().take(max_chars - 3).collect();
        format!("{head}...")
    }
}

fn edit_mode_settled_by(play_probe: Option<&PlayModeProbe>) -> String {
    match play_probe {
        Some(probe) if !probe.mounts_in_play_mode => {
            let detail = probe
                .failure
                .as_deref()
                .map(|f| failure_headline(f, 140))
                .or_else(|| probe.waiting_at.as_ref().map(|at| format!("mount still waiting at {at}")))
                .unwrap_or_else(|| format!("play mode ended with status \"{}\"", probe.status));
            format!(
                "This story does not mount in play mode ({detail}), so per-frame layoutCpuMs is unavailable as-is; fix the play-mode wait/failure to price per-frame layout, or use these edit-mode static counts only."
            )
        }
        _ => "Re-run with --mode play for a playing client's 60 Hz Heartbeat cadence (1 beat per frame).".into(),
    }
}

fn predicted_edit_mode_sequence_json(run_mode: &str, scope: &'static str, play_probe: Option<&PlayModeProbe>) -> Value {
    let (draw_refusal, gpu_refusal) = draw_and_gpu_refusals();
    let reason = if run_mode == "edit" {
        "Run used --mode edit (Studio 240 Hz Heartbeat, 4 beats per 60 Hz frame), which can price Heartbeat-driven UI at up to 4x a playing client's per-frame layout cost."
    } else {
        "Per-frame layoutCpuMs requires a playing client's 60 Hz Heartbeat cadence (--mode play)."
    };
    let layout_refusal = json!({
        "status": "refused",
        "basis": "edit_mode_cadence",
        "scope": scope,
        "ms": Value::Null,
        "reason": reason,
        "settledBy": edit_mode_settled_by(play_probe),
    });
    json!({
        "desktop-ref": {
            "layoutCpuMs": layout_refusal,
            "drawPassCpuMs": draw_refusal,
            "gpuUiMs": gpu_refusal
        },
        "phone-mid": {
            "layoutCpuMs": layout_refusal,
            "drawPassCpuMs": draw_refusal,
            "gpuUiMs": gpu_refusal
        }
    })
}

pub fn unmodeled_list() -> Value {
    json!([
        "draw_calls_and_batch_breaks: drawCalls and batchBreaks come from an unmeasured state-transition model (distinct image URIs break batches; client UI image atlasing and real draw counts are uncalibrated)",
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
    let default_mode = if dom_hidden.is_some() { "edit" } else { "scene" };
    analyze_scene_with_mode(scene, layout, dom_hidden, default_mode)
}

/// [`analyze_scene`] with an explicit engine `run_mode` (`"play"`, `"edit"`, or `"scene"`).
pub fn analyze_scene_with_mode(
    scene: &SceneStore,
    layout: LayoutStats,
    dom_hidden: Option<&DomHiddenSummary>,
    run_mode: &str,
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

    let mut analyzer = Analyzer::new(scene, vp.scale, Some(hidden));
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
            analyzer.walk_entries(&mut sg, draw_list, 0, screen_bounds, 1.0, None, pass_id, 0, None, None);
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
                "siblingIndex": o.sibling_index,
                "pathCount": o.path_count,
                "position": [round4(o.pos[0]), round4(o.pos[1])],
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

    let raw_status = scene
        .header()
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("ok");
    let mounted_empty = raw_status == "ok"
        && analyzer.totals.drawn_nodes == 0
        && hidden.gui_objects == 0
        && hidden.disabled_screen_guis == 0
        && dom_hidden.is_some_and(|h| h.mounted_empty);
    let status = if mounted_empty { "empty" } else { raw_status };

    let mut report = Map::new();
    report.insert("schema".into(), json!("rur-cost/1"));
    report.insert("status".into(), json!(status));
    report.insert("mode".into(), json!("single"));
    report.insert("runMode".into(), json!(run_mode));
    report.insert("runModeInfo".into(), run_mode_info(run_mode));
    report.insert("input".into(), json!(if dom_hidden.is_some() { "live" } else { "scene" }));
    report.insert(
        "viewport".into(),
        json!({
            "width": vp.width,
            "height": vp.height,
            "scale": vp.scale,
            "physicalWidth": phys_w,
            "physicalHeight": phys_h,
        }),
    );
    report.insert("totals".into(), totals_json);
    if dom_hidden.is_some() {
        report.insert("fullTreeLayout".into(), layout_stats_json_scoped(layout, "full_tree_export_pass"));
        if status == "ok" {
            report.insert("predicted".into(), predicted_single_live_json(layout));
        } else {
            report.insert("predicted".into(), predicted_non_ok_json(status, None, analyzer.totals.drawn_nodes, true, None, None));
        }
    } else {
        report.insert("predicted".into(), predicted_scene_only_json());
    }
    if mounted_empty {
        report.insert("failure".into(), json!(EMPTY_MOUNT_MESSAGE));
    }
    report.insert("screenGuis".into(), json!(screen_guis_json));
    report.insert("objects".into(), json!(objects_json));
    report.insert("hiddenSubtrees".into(), json!(hidden_subtrees_json));
    report.insert(
        "fidelity".into(),
        json!({
            "canvasGroupRt": "exact_studio_dimensions (rectangular float32 logical size, 2048 side cap, 2048x1024 area cap, culled/empty/transparent skipped)",
            "drawCalls": "unmeasured_state_transition_model (estimates batches from pass/shader/texture/clip/rotation transitions; no client capture backs these counts and UI image atlasing is unmodeled)",
            "batchBreaks": "unmeasured_state_transition_model (layerPass, renderTarget, clip, shader, texture, rotation; not calibrated against a client)",
            "layoutCpuMs": if dom_hidden.is_some() {
                "single_moment_export_pass_only (fullTreeLayoutCpuMs extrapolates lazy-layout slope; run --from and --frames for per-frame cost)"
            } else {
                "unavailable_in_scene_mode (run live with --from and --frames for per-frame layout cost)"
            },
            "hiddenInShown": if hidden.complete {
                "live_dom_traversal"
            } else {
                "scene_json_only_incomplete (run without --scene for full hidden subtree counts)"
            },
            "determinism": "zero_wall_clock_fields"
        }),
    );
    report.insert("unmodeled".into(), unmodeled_list());
    sort_json(Value::Object(report))
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

struct UncheckedField {
    field: &'static str,
    missing_before: bool,
    missing_after: bool,
}

struct EquivalenceResult {
    mismatches: Vec<EquivalenceMismatch>,
    unchecked: Vec<UncheckedField>,
}

fn format_unchecked_note(unchecked: &[UncheckedField]) -> Option<String> {
    if unchecked.is_empty() {
        return None;
    }
    let before_only: Vec<&str> = unchecked
        .iter()
        .filter(|u| u.missing_before && !u.missing_after)
        .map(|u| u.field)
        .collect();
    let after_only: Vec<&str> = unchecked
        .iter()
        .filter(|u| !u.missing_before && u.missing_after)
        .map(|u| u.field)
        .collect();
    let both: Vec<&str> = unchecked
        .iter()
        .filter(|u| u.missing_before && u.missing_after)
        .map(|u| u.field)
        .collect();
    let pronoun = |items: &[&str]| if items.len() == 1 { "it" } else { "them" };
    let mut parts: Vec<String> = Vec::new();
    if !before_only.is_empty() {
        parts.push(format!(
            "{} not checked (before report predates {})",
            before_only.join(", "),
            pronoun(&before_only)
        ));
    }
    if !after_only.is_empty() {
        parts.push(format!(
            "{} not checked (after report predates {})",
            after_only.join(", "),
            pronoun(&after_only)
        ));
    }
    if !both.is_empty() {
        parts.push(format!(
            "{} not checked (both reports predate {})",
            both.join(", "),
            pronoun(&both)
        ));
    }
    Some(parts.join("; "))
}

fn check_compare_equivalence(before: &Value, after: &Value) -> EquivalenceResult {
    let mut mismatches = Vec::new();
    let mut unchecked = Vec::new();

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
        if let (Some(bfrom), Some(afrom)) = (
            before.pointer("/sequence/summary/from").and_then(Value::as_f64),
            after.pointer("/sequence/summary/from").and_then(Value::as_f64),
        ) {
            if (bfrom - afrom).abs() > 1e-6 {
                mismatches.push(EquivalenceMismatch {
                    field: "sequence.from",
                    before: json!(bfrom),
                    after: json!(afrom),
                    reason: format!("sequence start time differs (before --from={bfrom}, after --from={afrom})"),
                    remedy: "Re-run both sequence arms with the same --from start time.".into(),
                });
            }
        }
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
        if let (Some(bhz), Some(ahz)) = (
            before.pointer("/sequence/summary/clientHz").and_then(Value::as_f64),
            after.pointer("/sequence/summary/clientHz").and_then(Value::as_f64),
        ) {
            if (bhz - ahz).abs() > 1e-6 {
                mismatches.push(EquivalenceMismatch {
                    field: "sequence.clientHz",
                    before: json!(bhz),
                    after: json!(ahz),
                    reason: format!("sequence clientHz differs (before={bhz}, after={ahz})"),
                    remedy: "Re-run both sequence arms with the same --client-hz.".into(),
                });
            }
        }
    }

    let b_run_mode = before.get("runMode").and_then(Value::as_str);
    let a_run_mode = after.get("runMode").and_then(Value::as_str);
    if b_run_mode != a_run_mode {
        let b_label = b_run_mode.unwrap_or("unspecified");
        let a_label = a_run_mode.unwrap_or("unspecified");
        mismatches.push(EquivalenceMismatch {
            field: "runMode",
            before: json!(b_label),
            after: json!(a_label),
            reason: format!(
                "runMode differs (before=\"{b_label}\", after=\"{a_label}\"): edit mode fires Heartbeat 4x per 60 Hz frame (240 Hz) whereas play mode fires 1x per frame (60 Hz)"
            ),
            remedy: "Re-run both arms with the same --mode (use --mode play for client 60 Hz per-frame layout pricing, or --mode edit for Studio edit mode).".into(),
        });
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

    let both_scene = b_input == "scene" && a_input == "scene";

    let b_src = before.get("source").filter(|v| !v.is_null());
    let a_src = after.get("source").filter(|v| !v.is_null());
    match (b_src, a_src) {
        (Some(b_src), Some(a_src)) => {
            if sort_json(b_src.clone()) != sort_json(a_src.clone()) {
                let fmt_src = |v: &Value| {
                    if let (Some(k), Some(t)) = (v.get("kind").and_then(Value::as_str), v.get("target").and_then(Value::as_str)) {
                        format!("{k}:{t}")
                    } else {
                        "unspecified".to_string()
                    }
                };
                mismatches.push(EquivalenceMismatch {
                    field: "source",
                    before: b_src.clone(),
                    after: a_src.clone(),
                    reason: format!("source differs (before={}, after={})", fmt_src(b_src), fmt_src(a_src)),
                    remedy: "Compare two runs of the same story/fixture/module/model (use --git-ref to compare revisions of one source).".into(),
                });
            }
        }
        (b, a) if !both_scene => {
            unchecked.push(UncheckedField {
                field: "source",
                missing_before: b.is_none(),
                missing_after: a.is_none(),
            });
        }
        _ => {}
    }

    let b_dev = before.get("device").and_then(Value::as_str);
    let a_dev = after.get("device").and_then(Value::as_str);
    match (b_dev, a_dev) {
        (Some(b_dev), Some(a_dev)) => {
            if b_dev != a_dev {
                mismatches.push(EquivalenceMismatch {
                    field: "device",
                    before: json!(b_dev),
                    after: json!(a_dev),
                    reason: format!("device differs (before=\"{b_dev}\", after=\"{a_dev}\")"),
                    remedy: "Re-run both arms with the same --device (desktop, touch, or gamepad).".into(),
                });
            }
        }
        (b, a) if !both_scene => {
            unchecked.push(UncheckedField {
                field: "device",
                missing_before: b.is_none(),
                missing_after: a.is_none(),
            });
        }
        _ => {}
    }

    let b_ctrl = before.get("controls").filter(|v| !v.is_null());
    let a_ctrl = after.get("controls").filter(|v| !v.is_null());
    match (b_ctrl, a_ctrl) {
        (Some(b_ctrl), Some(a_ctrl)) => {
            let b_ctrl = sort_json(b_ctrl.clone());
            let a_ctrl = sort_json(a_ctrl.clone());
            if b_ctrl != a_ctrl {
                mismatches.push(EquivalenceMismatch {
                    field: "controls",
                    before: b_ctrl.clone(),
                    after: a_ctrl.clone(),
                    reason: format!("controls differ (before={b_ctrl}, after={a_ctrl})"),
                    remedy: "Re-run both arms with the same --controls values.".into(),
                });
            }
        }
        (b, a) if !both_scene => {
            unchecked.push(UncheckedField {
                field: "controls",
                missing_before: b.is_none(),
                missing_after: a.is_none(),
            });
        }
        _ => {}
    }

    if b_mode == "single" && a_mode == "single" && !both_scene {
        let b_time = before.get("time").and_then(Value::as_f64);
        let a_time = after.get("time").and_then(Value::as_f64);
        match (b_time, a_time) {
            (Some(bt), Some(at)) => {
                if (bt - at).abs() > 1e-6 {
                    mismatches.push(EquivalenceMismatch {
                        field: "time",
                        before: json!(bt),
                        after: json!(at),
                        reason: format!("single-moment capture time differs (before --time={bt}, after --time={at})"),
                        remedy: "Re-run both single-moment arms with the same --time.".into(),
                    });
                }
            }
            (b, a) => {
                unchecked.push(UncheckedField {
                    field: "time",
                    missing_before: b.is_none(),
                    missing_after: a.is_none(),
                });
            }
        }
    }

    EquivalenceResult { mismatches, unchecked }
}

/// Deterministic A/B comparison between two `rur-cost/1` reports (`rrr-ab/1`).
/// Rejects non-cost-report inputs with `Err` and refuses incomparable arms with `verdict: "refused"`.
pub fn compare_reports(before: &Value, after: &Value) -> Result<Value, String> {
    validate_cost_report(before, "before", None)?;
    validate_cost_report(after, "after", None)?;

    let eq = check_compare_equivalence(before, after);
    let unchecked_fields: Vec<&str> = eq.unchecked.iter().map(|u| u.field).collect();
    let unchecked_note = format_unchecked_note(&eq.unchecked);
    let is_single_live = before["mode"].as_str() == Some("single")
        && after["mode"].as_str() == Some("single")
        && !(before["input"].as_str() == Some("scene") && after["input"].as_str() == Some("scene"));
    let mut all_checks = vec![
        "schema",
        "status",
        "source",
        "mode",
        "sequence.from",
        "runMode",
        "input",
        "hiddenInShown.complete",
        "viewport",
        "scale",
        "device",
        "controls",
    ];
    if is_single_live {
        all_checks.push("time");
    }
    let checks: Vec<&str> = all_checks
        .into_iter()
        .filter(|c| !unchecked_fields.contains(c))
        .collect();
    let checks = json!(checks);
    let mismatches = eq.mismatches;
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
    // Unmeasured state-transition model counts (`drawCalls`, `batchBreaks`) remain informational in
    // `counts` and do not drive `verdict`.
    const VERDICT_KEYS: [&str; 5] = [
        "quads",
        "glyphQuads",
        "canvasRtBytes",
        "screenOverdrawViewports",
        "offscreenOverdrawViewports",
    ];
    const LAYOUT_KEYS: [&str; 6] = ["passes", "computed", "stored", "moved", "scrollPasses", "scrollMoved"];

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

    let is_sequence = before["mode"].as_str() == Some("sequence");
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
        if is_sequence && matches!(key, "stored" | "moved" | "scrollPasses" | "scrollMoved") {
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
        let range_of = |rep: &Value| -> Option<[f64; 2]> {
            let arr = rep.pointer(&format!("/predicted/{tier}/layoutCpuMs/rangeMs"))?.as_array()?;
            if arr.len() < 2 {
                return None;
            }
            Some([arr[0].as_f64()?, arr[1].as_f64()?])
        };
        if let (Some(b), Some(a)) = (range_of(before), range_of(after)) {
            return json!({
                "ratioBasis": "derived",
                "beforeRangeMs": [round4(b[0]), round4(b[1])],
                "afterRangeMs": [round4(a[0]), round4(a[1])],
                "deltaRangeMs": [round4(a[0] - b[0]), round4(a[1] - b[1])],
            });
        }
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

    let mut equivalence = Map::new();
    equivalence.insert("passed".into(), json!(true));
    equivalence.insert("checks".into(), checks);
    if !unchecked_fields.is_empty() {
        equivalence.insert("unchecked".into(), json!(unchecked_fields));
    }
    if let Some(note) = unchecked_note {
        equivalence.insert("uncheckedNote".into(), json!(note));
    }

    Ok(sort_json(json!({
        "schema": "rrr-ab/1",
        "verdict": verdict,
        "verdictBasis": "measured_counts_only (quads, glyphQuads, canvasRtBytes, screenOverdrawViewports, offscreenOverdrawViewports, hiddenGuiObjects, and sequence layout churn; unmeasured model drawCalls and batchBreaks are informational only)",
        "equivalence": Value::Object(equivalence),
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
    client_frames: u32,
}

/// Accumulates per-frame deterministic UI cost and incremental `Layout.Stats` churn across a
/// multi-frame sequence (`rur cost --from/--to/--frames/--duration/--fps` and `rur sequence --cost`).
pub struct SequenceAccumulator {
    fps: f64,
    client_hz: f64,
    cadence_kind: &'static str,
    run_mode: String,
    frames: Vec<Value>,
    metrics: Vec<FrameMetrics>,
    times: Vec<f64>,
    steady_churn: BTreeMap<u64, engine::layout::LayoutChurnEntry>,
    last_dom_hidden: Option<DomHiddenSummary>,
    last_report: Option<Value>,
    unchanged_frames: u32,
    active_frames: u32,
    saw_live_dom: bool,
    any_non_empty: bool,
}

impl SequenceAccumulator {
    pub fn new(fps: f64, run_mode: &str) -> Self {
        Self::with_cadence(fps, 60.0, "client", run_mode)
    }

    pub fn with_cadence(fps: f64, client_hz: f64, cadence_kind: &'static str, run_mode: &str) -> Self {
        Self {
            fps,
            client_hz,
            cadence_kind,
            run_mode: run_mode.to_string(),
            frames: Vec::new(),
            metrics: Vec::new(),
            times: Vec::new(),
            steady_churn: BTreeMap::new(),
            last_dom_hidden: None,
            last_report: None,
            unchanged_frames: 0,
            active_frames: 0,
            saw_live_dom: false,
            any_non_empty: false,
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
        client_frames: u32,
        scene: &SceneStore,
        layout: LayoutStats,
        layout_churn: Vec<engine::layout::LayoutChurnEntry>,
        dom_hidden: Option<DomHiddenSummary>,
    ) {
        if index >= 1 && !layout_churn.is_empty() {
            // Align per-path ordinals with `top objs` (`scene.ordered_nodes()`) for scene nodes;
            // non-scene nodes (e.g. UIComponents) keep `take_churn`'s DOM-wide per-path ordinal.
            let mut scene_path_counts: BTreeMap<&str, u32> = BTreeMap::new();
            let mut scene_path_indices: BTreeMap<u64, u32> = BTreeMap::new();
            for node in scene.ordered_nodes() {
                if let (Some(id), Some(path)) = (node.get("id").and_then(node_id), node["path"].as_str()) {
                    let count = scene_path_counts.entry(path).or_insert(0);
                    *count += 1;
                    scene_path_indices.insert(id, *count);
                }
            }
            for mut entry in layout_churn {
                if let Some(&idx) = scene_path_indices.get(&entry.id) {
                    entry.sibling_index = idx;
                    entry.path_count = scene_path_counts.get(entry.path.as_str()).copied().unwrap_or(entry.path_count);
                }
                self.steady_churn
                    .entry(entry.id)
                    .and_modify(|acc| {
                        acc.stored += entry.stored;
                        acc.moved += entry.moved;
                        acc.scroll_passes += entry.scroll_passes;
                        acc.scroll_moved += entry.scroll_moved;
                        acc.pos = entry.pos;
                        acc.sibling_index = entry.sibling_index;
                        acc.path_count = entry.path_count;
                    })
                    .or_insert(entry);
            }
        }
        if dom_hidden.is_some() {
            self.saw_live_dom = true;
            self.last_dom_hidden = dom_hidden;
        }
        let report = if !unchanged || self.last_report.is_none() {
            let computed = analyze_scene_with_mode(scene, layout, self.last_dom_hidden.as_ref(), &self.run_mode);
            self.last_report = Some(computed.clone());
            computed
        } else {
            self.last_report.as_ref().expect("checked above").clone()
        };
        if self.last_dom_hidden.as_ref().is_some_and(|h| !h.mounted_empty || h.gui_objects > 0 || h.disabled_screen_guis > 0)
            || report["totals"]["drawnNodes"].as_u64().unwrap_or(0) > 0
        {
            self.any_non_empty = true;
        }
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
            client_frames,
        });
        self.times.push(time);
        let is_play = self.run_mode == "play";
        let frame_scope = if index == 0 {
            if is_play { "mount_and_runup" } else { "mount_and_runup_edit_mode" }
        } else if is_play {
            if client_frames == 1 { "per_client_frame" } else { "sample_multi_or_zero_client_frames" }
        } else {
            "per_frame_edit_mode_240hz"
        };
        let frame_predicted = if !is_play {
            predicted_edit_mode_sequence_json(&self.run_mode, "per_frame_refused_in_edit_mode", None)
        } else if index == 0 {
            predicted_non_single_client_frame_json(client_frames, self.client_hz, self.fps, true)
        } else if client_frames == 1 {
            predicted_sequence_json(desktop_layout_cpu_ms(layout), "per_client_frame")
        } else {
            predicted_non_single_client_frame_json(client_frames, self.client_hz, self.fps, false)
        };
        self.frames.push(json!({
            "index": index,
            "time": round9(time),
            "clientFrames": client_frames,
            "unchanged": unchanged,
            "changedNodes": changed_nodes,
            "removedNodes": removed_nodes,
            "totals": totals,
            "layout": layout_stats_json_scoped(layout, frame_scope),
            "predicted": frame_predicted,
        }));
    }

    pub fn finish(self, status_override: Option<&str>) -> Value {
        let outcome = RunOutcome {
            status: status_override.map(str::to_string),
            ..Default::default()
        };
        self.finish_with_outcome(&outcome)
    }

    pub fn finish_with_outcome(self, outcome: &RunOutcome) -> Value {
        let saw_live_dom = self.saw_live_dom;
        let any_non_empty = self.any_non_empty;
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

        let fields: [(&str, fn(&FrameMetrics) -> f64); 13] = [
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
            ("layoutScrollPasses", |m| m.layout.scroll_passes as f64),
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

        let steady_slice = if count > 1 { &self.metrics[1..] } else { &[][..] };
        let steady_client_frames: u64 = steady_slice.iter().map(|m| u64::from(m.client_frames)).sum();
        let has_steady_client_frames = steady_client_frames > 0;
        let denom = if has_steady_client_frames { steady_client_frames as f64 } else { 1.0 };
        let steady_passes = if has_steady_client_frames {
            steady_slice.iter().map(|m| m.layout.passes as f64).sum::<f64>() / denom
        } else {
            0.0
        };
        let steady_computed = if has_steady_client_frames {
            steady_slice.iter().map(|m| m.layout.computed as f64).sum::<f64>() / denom
        } else {
            0.0
        };
        let steady_stored = if has_steady_client_frames {
            steady_slice.iter().map(|m| m.layout.stored as f64).sum::<f64>() / denom
        } else {
            0.0
        };
        let steady_moved = if has_steady_client_frames {
            steady_slice.iter().map(|m| m.layout.moved as f64).sum::<f64>() / denom
        } else {
            0.0
        };
        let steady_scroll_passes = if has_steady_client_frames {
            steady_slice.iter().map(|m| m.layout.scroll_passes as f64).sum::<f64>() / denom
        } else {
            0.0
        };
        let steady_scroll = if has_steady_client_frames {
            steady_slice.iter().map(|m| m.layout.scroll_moved as f64).sum::<f64>() / denom
        } else {
            0.0
        };
        let steady_desk_ms = if has_steady_client_frames {
            steady_slice.iter().map(|m| desktop_layout_cpu_ms(m.layout)).sum::<f64>() / denom
        } else {
            0.0
        };

        let is_play = self.run_mode == "play";
        let hz_str = format_hz(self.client_hz);
        let fps_str = format_hz(self.fps);
        let mut base = self.last_report.unwrap_or_else(|| {
            analyze_scene_with_mode(&SceneStore::default(), LayoutStats::default(), None, &self.run_mode)
        });
        let steady_layout_json = json!({
            "scope": if !is_play {
                "steady_state_per_frame_mean_edit_mode_240hz"
            } else if has_steady_client_frames {
                "steady_state_per_client_frame_mean"
            } else {
                "steady_state_no_client_frames"
            },
            "clientHz": round4(self.client_hz),
            "cadenceKind": self.cadence_kind,
            "passes": round6(steady_passes),
            "computed": round6(steady_computed),
            "stored": round6(steady_stored),
            "moved": round6(steady_moved),
            "scrollPasses": round6(steady_scroll_passes),
            "scrollMoved": round6(steady_scroll),
        });
        let play_probe = outcome.play_mode_probe.as_ref();
        let steady_cpu_json = if !is_play {
            json!({
                "status": "refused",
                "basis": "edit_mode_cadence",
                "desktop-ref": Value::Null,
                "phone-mid-range": Value::Null,
                "phone-mid-band": Value::Null,
                "reason": "Run used --mode edit (Studio 240 Hz Heartbeat, 4 beats per 60 Hz frame), which can price Heartbeat-driven UI at up to 4x a playing client's per-frame layout cost.",
                "settledBy": edit_mode_settled_by(play_probe),
            })
        } else if !has_steady_client_frames {
            json!({
                "status": "refused",
                "basis": "no_client_frame_stepped",
                "desktop-ref": Value::Null,
                "phone-mid-range": Value::Null,
                "phone-mid-band": Value::Null,
                "reason": format!("0 client frames completed after frame 0 ({count} samples @ {fps_str} fps on a {hz_str} Hz client); cannot price one client frame."),
                "settledBy": format!("Increase --frames or --duration so at least 1 {hz_str} Hz client frame completes after frame 0, or lower --fps."),
            })
        } else {
            json!({
                "status": "extrapolated",
                "basis": "extrapolated",
                "clientHz": round4(self.client_hz),
                "cadenceKind": self.cadence_kind,
                "desktop-ref": round9(steady_desk_ms),
                "phone-mid-basis": "derived",
                "phone-mid-range": [
                    round4(steady_desk_ms * PHONE_UI_MAIN_RATIO_LOW),
                    round4(steady_desk_ms * PHONE_UI_MAIN_RATIO_HIGH),
                ],
                "phone-mid-ratio-range": [
                    round4(PHONE_UI_MAIN_RATIO_LOW),
                    round4(PHONE_UI_MAIN_RATIO_HIGH),
                ],
                "phone-mid-band": [
                    round4(steady_desk_ms * PHONE_UI_MAIN_LOW),
                    round4(steady_desk_ms * PHONE_UI_MAIN_HIGH),
                ],
                "phone-mid-band-ratio": [
                    round4(PHONE_UI_MAIN_LOW),
                    round4(PHONE_UI_MAIN_HIGH),
                ],
            })
        };
        let raw_status = outcome
            .status
            .as_deref()
            .or_else(|| base.get("status").and_then(Value::as_str))
            .unwrap_or("ok")
            .to_string();
        let final_status = if raw_status == "ok" && saw_live_dom && !any_non_empty {
            "empty".to_string()
        } else {
            raw_status
        };
        if let Some(obj) = base.as_object_mut() {
            obj.insert("mode".into(), json!("sequence"));
            obj.insert("runMode".into(), json!(self.run_mode));
            obj.insert("runModeInfo".into(), run_mode_info_with_cadence(&self.run_mode, self.client_hz, self.cadence_kind, Some(self.fps)));
            obj.insert("status".into(), json!(final_status));
            obj.remove("fullTreeLayout");
            obj.insert("layout".into(), steady_layout_json.clone());
            let mut raw_drivers: Vec<(f64, f64, engine::layout::LayoutChurnEntry, f64, f64, f64, f64)> = if has_steady_client_frames {
                self.steady_churn
                    .values()
                    .filter(|c| c.stored > 1e-9 || c.moved > 1e-9 || c.scroll_passes > 0 || c.scroll_moved > 0)
                    .map(|c| {
                        let s = c.stored / denom;
                        let m = c.moved / denom;
                        let sp = c.scroll_passes as f64 / denom;
                        let sm = c.scroll_moved as f64 / denom;
                        let records = s + m + sm;
                        let est_ms = sp * STUDIO_SCROLL_PASS_MS + records * STUDIO_LAYOUT_OBJECT_MS;
                        (est_ms, records, c.clone(), s, m, sp, sm)
                    })
                    .collect()
            } else {
                Vec::new()
            };
            let total_drivers = raw_drivers.len();
            let total_driver_records: f64 = raw_drivers.iter().map(|(_, rec, ..)| *rec).sum();
            let total_driver_ms: f64 = raw_drivers.iter().map(|(ms, ..)| *ms).sum();
            raw_drivers.sort_by(|a, b| {
                b.0.total_cmp(&a.0)
                    .then_with(|| b.1.total_cmp(&a.1))
                    .then_with(|| a.2.path.cmp(&b.2.path))
                    .then_with(|| a.2.sibling_index.cmp(&b.2.sibling_index))
                    .then_with(|| a.2.id.cmp(&b.2.id))
            });
            raw_drivers.truncate(MAX_LAYOUT_DRIVERS_REPORTED);
            let reported_driver_records: f64 = raw_drivers.iter().map(|(_, rec, ..)| *rec).sum();
            let reported_driver_ms: f64 = raw_drivers.iter().map(|(ms, ..)| *ms).sum();
            let drivers_json: Vec<Value> = raw_drivers
                .into_iter()
                .map(|(est_ms, _, c, s, m, sp, sm)| {
                    let share = if total_driver_ms > 1e-12 { est_ms / total_driver_ms } else { 0.0 };
                    json!({
                        "id": c.id,
                        "name": c.name,
                        "path": c.path,
                        "class": c.class,
                        "siblingIndex": c.sibling_index,
                        "pathCount": c.path_count,
                        "position": [round4(c.pos[0]), round4(c.pos[1])],
                        "stored": round6(s),
                        "moved": round6(m),
                        "scrollPasses": round6(sp),
                        "scrollMoved": round6(sm),
                        "desktopMs": round9(est_ms),
                        "share": round6(share),
                    })
                })
                .collect();
            let reported_drivers = drivers_json.len();
            obj.insert("layoutDrivers".into(), Value::Array(drivers_json));
            obj.insert(
                "layoutDriverSummary".into(),
                json!({
                    "totalDrivers": total_drivers,
                    "reportedDrivers": reported_drivers,
                    "totalRecords": round6(total_driver_records),
                    "reportedRecords": round6(reported_driver_records),
                    "totalDesktopMs": round9(total_driver_ms),
                    "reportedDesktopMs": round9(reported_driver_ms),
                }),
            );
            obj.insert(
                "predicted".into(),
                if !is_play {
                    predicted_edit_mode_sequence_json(&self.run_mode, "per_frame_refused_in_edit_mode", play_probe)
                } else if !has_steady_client_frames {
                    predicted_no_client_frames_json(count, self.client_hz, self.fps)
                } else {
                    predicted_sequence_json(steady_desk_ms, "steady_state_per_client_frame_mean")
                },
            );
            if let Some(fid) = obj.get_mut("fidelity").and_then(Value::as_object_mut) {
                fid.insert(
                    "layoutCpuMs".into(),
                    if !is_play {
                        if play_probe.is_some_and(|p| !p.mounts_in_play_mode) {
                            json!("refused_in_edit_mode (mode=edit fires Heartbeat at 240 Hz; play mode does not mount this story, so per-frame layoutCpuMs is unavailable as-is)")
                        } else {
                            json!("refused_in_edit_mode (mode=edit fires Heartbeat at 240 Hz, 4 beats per 60 Hz frame; pass --mode play for client per-frame layoutCpuMs)")
                        }
                    } else if !has_steady_client_frames {
                        json!(format!("refused_no_client_frames (0 client frames completed after frame 0 @ {fps_str} fps on {hz_str} Hz client)"))
                    } else {
                        json!(format!("extrapolated_from_lazy_layout_probe (mode=play, {hz_str} Hz {} cadence, 1 Heartbeat/client frame, sampled @ {fps_str} fps; desktop lazy-layout slope extrapolated to per-client-frame layout passes; phone-mid uses derived 3.9x..6.3x ratio, band 2.55x..12.32x, capture factor 1.25..2.0 unmeasured for layout)", self.cadence_kind))
                    },
                );
            }
            obj.insert(
                "sequence".into(),
                json!({
                    "summary": {
                        "frameCount": count,
                        "fps": self.fps,
                        "clientHz": round4(self.client_hz),
                        "cadenceKind": self.cadence_kind,
                        "runMode": self.run_mode,
                        "from": round9(self.times.first().copied().unwrap_or(0.0)),
                        "to": round9(self.times.last().copied().unwrap_or(0.0)),
                        "activeFrames": self.active_frames,
                        "unchangedFrames": self.unchanged_frames,
                        "steadyStateFrames": steady_slice.len(),
                        "steadyStateClientFrames": steady_client_frames,
                        "mean": mean_map,
                        "peak": peak_map,
                        "p95": p95_map,
                        "steadyStateLayoutMean": {
                            "passes": round6(steady_passes),
                            "computed": round6(steady_computed),
                            "stored": round6(steady_stored),
                            "moved": round6(steady_moved),
                            "scrollPasses": round6(steady_scroll_passes),
                            "scrollMoved": round6(steady_scroll),
                        },
                        "steadyStateLayoutCpuMs": steady_cpu_json,
                    },
                    "frames": self.frames,
                }),
            );
        }
        let mut effective = outcome.clone();
        effective.status = Some(final_status.to_string());
        apply_run_outcome(&mut base, &effective, false);
        sort_json(base)
    }
}

/// Runs a CPU-only multi-frame cost sequence (no GPU device, PNG encoding, or video encoding).
/// Defaults `--mode` to `play` (1 Heartbeat per client frame, matching a playing client) and
/// `spec.client_hz` to `Some(60.0)` unless the caller explicitly passed `--mode` / `--client-hz`.
pub fn run_sequence_cost(
    engine_args: &engine::cli::Args,
    spec: &engine::cli::SequenceSpec,
    repo: &Path,
) -> Result<Value, String> {
    let mut engine_args = engine_args.clone();
    if engine_args.mode.is_none() {
        engine_args.mode = Some("play".into());
    }
    let run_mode = engine_args.mode.as_deref().unwrap_or("play").to_string();
    let mut spec = spec.clone();
    if spec.client_hz.is_none() {
        spec.client_hz = Some(60.0);
    }
    spec.collect_cost = true;
    let mut resolver = crate::resolve::Resolver::new(repo.to_path_buf(), engine_args.offline);
    let mut frames_rx = crate::sequence::start_fresh(&engine_args, &spec)?;
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
        frames_rx = crate::sequence::start_fresh(&engine_args, &spec)?;
        first = recv_first(&frames_rx)?;
    }

    let mut scene = SceneStore::default();
    let mut acc = SequenceAccumulator::with_cadence(spec.fps, first.client_hz, first.cadence_kind, &run_mode);
    let actual_device = first.device.clone();
    let mut next = Some(first);
    let mut ready_status: Option<String> = None;
    let mut ready_waiting: Option<String> = None;
    let mut ready_diagnostics: Option<Value> = None;
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
        let engine::cli::SequenceFrame { index, time, mut reply, layout, client_frames, layout_churn, dom_hidden, .. } = frame;
        if reply["type"] == "error" {
            return Err(format!("frame {index}: {}", reply["message"].as_str().unwrap_or("the engine failed")));
        }
        if index == 0 {
            ready_status = reply.pointer("/delta/header/status").and_then(Value::as_str).map(str::to_string);
            ready_waiting = reply.pointer("/delta/header/waiting").and_then(Value::as_str).map(str::to_string);
            if !reply["diagnostics"].is_null() {
                ready_diagnostics = Some(reply["diagnostics"].clone());
            }
        }
        let unchanged = index > 0 && reply["unchanged"] == true;
        let outcome = scene.apply_owned(&mut reply).map_err(|e| format!("frame {index}: {e}"))?;
        acc.record_frame(
            index,
            time,
            unchanged,
            outcome.changed_nodes.len(),
            outcome.removed_nodes.len(),
            client_frames,
            &scene,
            layout,
            layout_churn,
            dom_hidden,
        );
    };
    let mut merged_end = end;
    if merged_end["diagnostics"].is_null() {
        if let Some(diag) = ready_diagnostics {
            merged_end["diagnostics"] = diag;
        }
    }
    if merged_end.get("waiting").is_none() {
        if let Some(at) = ready_waiting {
            merged_end["waiting"] = json!(at);
        }
    }
    let mut run_outcome = extract_run_outcome(&merged_end);
    let status = run_outcome
        .status
        .clone()
        .or(ready_status)
        .unwrap_or_else(|| "ok".into());
    run_outcome.status = Some(status.clone());
    let probe_time = spec.from.max(0.05);
    let is_empty_mount = status == "ok" && acc.saw_live_dom && !acc.any_non_empty;
    if run_mode == "play" && (status != "ok" || is_empty_mount) {
        run_outcome.edit_mode_mounts = Some(probe_mounts_in_edit_mode(&engine_args, probe_time));
    } else if run_mode == "edit" {
        run_outcome.play_mode_probe = Some(probe_mounts_in_play_mode(&engine_args, probe_time));
    }
    let mut report = acc.finish_with_outcome(&run_outcome);
    attach_engine_run_metadata(&mut report, &engine_args, repo, Some(&actual_device));
    Ok(sort_json(report))
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

fn format_ms_approx(ms: f64) -> String {
    if !ms.is_finite() || ms <= 0.0 {
        "0".into()
    } else if ms < 0.1 {
        format!("{ms:.3}")
    } else {
        format!("{ms:.2}")
    }
}

fn format_phone_range(range: &[f64; 2], band: &[f64; 2]) -> String {
    format!(
        "phone ~{}..{} ms (derived 3.9x..6.3x, 2/3 factors assumed; band {}..{} ms [2.55x..12.32x])",
        format_ms_approx(range[0]),
        format_ms_approx(range[1]),
        format_ms_approx(band[0]),
        format_ms_approx(band[1]),
    )
}

fn pair_at(value: &Value, ptr: &str) -> [f64; 2] {
    let arr = value.pointer(ptr).and_then(Value::as_array);
    let a = arr.and_then(|v| v.first()).and_then(Value::as_f64).unwrap_or(0.0);
    let b = arr.and_then(|v| v.get(1)).and_then(Value::as_f64).unwrap_or(0.0);
    [a, b]
}

fn trim_left(s: &str, max_chars: usize) -> String {
    let count = s.chars().count();
    if count <= max_chars || max_chars <= 3 {
        return s.to_string();
    }
    let skip = count - (max_chars - 3);
    let tail: String = s.chars().skip(skip).collect();
    format!("...{tail}")
}

fn format_middle_elided(
    segs: &[&str],
    prefix_len: usize,
    suffix_len: usize,
    max_chars: usize,
) -> String {
    let diff_end = segs.len().saturating_sub(suffix_len);
    let diff = segs[prefix_len..diff_end].join(".");
    let prefix_dots = if prefix_len > 0 { "..." } else { "" };
    if suffix_len == 0 {
        let full = format!("{prefix_dots}{diff}");
        return trim_left(&full, max_chars);
    }
    let suffix_segs = &segs[diff_end..];
    let leaf = suffix_segs.last().copied().unwrap_or("");
    let min_tail_chars = (leaf.chars().count() + 3).min(max_chars / 2);
    let head_budget = max_chars.saturating_sub(min_tail_chars);
    let mut head = format!("{prefix_dots}{diff}");
    if head.chars().count() > head_budget {
        head = head.chars().take(head_budget).collect();
    }
    let tail_budget = max_chars.saturating_sub(head.chars().count() + 3);
    let mut chosen: Vec<&str> = Vec::new();
    let mut used = 0usize;
    for &seg in suffix_segs.iter().rev() {
        let add = seg.chars().count() + usize::from(!chosen.is_empty());
        if used + add <= tail_budget {
            chosen.push(seg);
            used += add;
        } else {
            break;
        }
    }
    let tail = if !chosen.is_empty() {
        chosen.reverse();
        chosen.join(".")
    } else if tail_budget > 0 {
        let leaf_len = leaf.chars().count();
        leaf.chars().skip(leaf_len.saturating_sub(tail_budget)).collect()
    } else {
        String::new()
    };
    if tail.is_empty() {
        head
    } else {
        format!("{head}...{tail}")
    }
}

/// Formats a displayed slice of `objects` or `layoutDrivers` labels so two different items never
/// print the same line: when `trim_left` collapses distinct paths to the same tail, the middle is
/// elided around the distinguishing segments, and any label whose printed path+class still repeats
/// (or whose full path has `pathCount > 1`) appends `#index @x,y`.
fn format_object_labels(items: &[Value], max_path: usize) -> Vec<String> {
    let mut display_paths: Vec<String> = items
        .iter()
        .map(|o| trim_left(o["path"].as_str().unwrap_or("?"), max_path))
        .collect();

    let mut groups: std::collections::BTreeMap<String, Vec<usize>> = std::collections::BTreeMap::new();
    for (idx, p) in display_paths.iter().enumerate() {
        groups.entry(p.clone()).or_default().push(idx);
    }
    for indices in groups.values() {
        if indices.len() < 2 {
            continue;
        }
        let split: Vec<Vec<&str>> = indices
            .iter()
            .map(|&i| items[i]["path"].as_str().unwrap_or("?").split('.').collect())
            .collect();
        let all_identical = split.windows(2).all(|w| w[0] == w[1]);
        if all_identical {
            continue;
        }
        let min_len = split.iter().map(Vec::len).min().unwrap_or(0);
        let mut prefix_len = 0usize;
        while prefix_len < min_len && split.iter().all(|s| s[prefix_len] == split[0][prefix_len]) {
            prefix_len += 1;
        }
        let mut suffix_len = 0usize;
        while prefix_len + suffix_len + 1 < min_len
            && split
                .iter()
                .all(|s| s[s.len() - 1 - suffix_len] == split[0][split[0].len() - 1 - suffix_len])
        {
            suffix_len += 1;
        }
        for (&i, segs) in indices.iter().zip(split.iter()) {
            display_paths[i] = format_middle_elided(segs, prefix_len, suffix_len, max_path);
        }
    }

    let mut key_counts: std::collections::BTreeMap<(String, String), usize> = std::collections::BTreeMap::new();
    for (i, o) in items.iter().enumerate() {
        let class = o["class"].as_str().unwrap_or("?").to_string();
        *key_counts.entry((display_paths[i].clone(), class)).or_insert(0) += 1;
    }

    items
        .iter()
        .enumerate()
        .map(|(i, o)| {
            let path = &display_paths[i];
            let class = o["class"].as_str().unwrap_or("?");
            let printed_repeats = key_counts
                .get(&(path.clone(), class.to_string()))
                .copied()
                .unwrap_or(1)
                > 1;
            if o["pathCount"].as_u64().unwrap_or(1) > 1 || printed_repeats {
                let idx = o["siblingIndex"].as_u64().unwrap_or((i + 1) as u64);
                let x = o.pointer("/position/0").and_then(Value::as_f64).unwrap_or(0.0).round() as i64;
                let y = o.pointer("/position/1").and_then(Value::as_f64).unwrap_or(0.0).round() as i64;
                format!("{path} #{idx} ({class}) @{x},{y}")
            } else {
                format!("{path} ({class})")
            }
        })
        .collect()
}

fn format_signed_num(v: f64) -> String {
    if !v.is_finite() || v.abs() < 1e-9 {
        "0".into()
    } else if (v - v.round()).abs() < 1e-6 {
        let i = v.round() as i64;
        if i > 0 { format!("+{i}") } else { format!("{i}") }
    } else if v > 0.0 {
        format!("+{v:.3}")
    } else {
        format!("{v:.3}")
    }
}

/// Formats a `rur-cost/1` report as a compact, agent-readable one-screen stdout summary
/// (always <= 32 non-empty lines and well under 4 KiB; full JSON lives in `dest` or `--json`).
pub fn format_one_screen_report(report: &Value, dest: &Path) -> String {
    use std::fmt::Write as _;

    let mut out = String::new();
    let status = report["status"].as_str().unwrap_or("?");
    let mode = report["mode"].as_str().unwrap_or("single");
    let run_mode = report["runMode"].as_str().unwrap_or("unknown");
    let seq_summary = report.pointer("/sequence/summary");
    let client_hz = seq_summary
        .and_then(|s| s["clientHz"].as_f64())
        .or_else(|| report.pointer("/runModeInfo/clientHz").and_then(Value::as_f64))
        .unwrap_or(60.0);
    let cadence_kind = seq_summary
        .and_then(|s| s["cadenceKind"].as_str())
        .or_else(|| report.pointer("/runModeInfo/cadenceKind").and_then(Value::as_str))
        .unwrap_or("client");
    let hz_str = format_hz(client_hz);
    let mode_cadence = match run_mode {
        "play" => format!("play {hz_str} Hz {cadence_kind}"),
        "edit" => "edit 240 Hz".to_string(),
        "scene" => "scene".to_string(),
        other => other.to_string(),
    };
    let vp = &report["viewport"];
    let t = &report["totals"];
    let rt_bytes = t["canvasRtBytes"].as_u64().unwrap_or(0);

    if let Some(seq) = seq_summary {
        let _ = writeln!(
            out,
            "rur cost: {status} (sequence, {} samples @ {} fps, mode={mode_cadence}; headline=last sample; defaults: single=edit, seq=play) | {}x{} @ {}x ({}x{} px)",
            seq["frameCount"].as_u64().unwrap_or(0),
            format_hz(seq["fps"].as_f64().unwrap_or(60.0)),
            vp["width"].as_f64().unwrap_or(0.0),
            vp["height"].as_f64().unwrap_or(0.0),
            vp["scale"].as_f64().unwrap_or(1.0),
            vp["physicalWidth"].as_u64().unwrap_or(0),
            vp["physicalHeight"].as_u64().unwrap_or(0),
        );
    } else {
        let _ = writeln!(
            out,
            "rur cost: {status} ({mode}, mode={mode_cadence}; defaults: single=edit, seq=play) | {}x{} @ {}x ({}x{} px)",
            vp["width"].as_f64().unwrap_or(0.0),
            vp["height"].as_f64().unwrap_or(0.0),
            vp["scale"].as_f64().unwrap_or(1.0),
            vp["physicalWidth"].as_u64().unwrap_or(0),
            vp["physicalHeight"].as_u64().unwrap_or(0),
        );
    }

    if status != "ok" {
        if let Some(failure) = report.get("failure").and_then(Value::as_str).filter(|s| !s.trim().is_empty()) {
            let label = if status == "empty" { "empty" } else { "failure" };
            let (cause, context) = summarize_failure_lines(failure);
            if !cause.is_empty() {
                let _ = writeln!(out, "  {label}:   {}", first_line(&cause, 180));
            }
            if let Some(ctx) = context {
                let _ = writeln!(out, "             {}", first_line(&ctx, 180));
            }
        }
        if let Some(hint) = report.get("hint").and_then(Value::as_str).filter(|s| !s.trim().is_empty()) {
            let _ = writeln!(out, "  hint:      {}", first_line(hint, 180));
        }
        if status == "empty"
            && seq_summary.is_none()
            && report.get("time").and_then(Value::as_f64).unwrap_or(0.0) <= 1e-9
        {
            let _ = writeln!(
                out,
                "  time note: captured at --time 0; if this UI mounts after t=0, retry with a later --time (e.g. --time 0.5)",
            );
        }
        if report.pointer("/editModeFallback/mountsInEditMode").and_then(Value::as_bool) == Some(true) {
            let _ = writeln!(
                out,
                "  mode note: mounts in edit mode, not play; per-frame layoutCpuMs is unavailable as-is (pass --mode edit for counts/RT/overdraw/hidden DOM, or fix the wait/error above)",
            );
        } else if report.pointer("/playModeProbe/mountsInPlayMode").and_then(Value::as_bool) == Some(true) {
            let _ = writeln!(
                out,
                "  mode note: mounts in play mode, not edit (pass --mode play)",
            );
        }
    }

    let _ = writeln!(
        out,
        "  guis:      {} enabled, {} disabled ({} insts) | nodes: {}/{} drawn",
        t["screenGuis"].as_u64().unwrap_or(0),
        t["disabledScreenGuis"].as_u64().unwrap_or(0),
        t["disabledScreenGuiInstances"].as_u64().unwrap_or(0),
        t["drawnNodes"].as_u64().unwrap_or(0),
        t["totalNodes"].as_u64().unwrap_or(0),
    );

    let mut kind_parts: Vec<String> = Vec::new();
    if let Some(kinds) = t["quadsByKind"].as_object() {
        let mut pairs: Vec<(&str, u64)> = kinds
            .iter()
            .filter_map(|(k, v)| v.as_u64().filter(|&n| n > 0).map(|n| (k.as_str(), n)))
            .collect();
        pairs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        for (k, n) in pairs.into_iter().take(5) {
            kind_parts.push(format!("{k}={n}"));
        }
    }
    let kind_str = if kind_parts.is_empty() {
        String::new()
    } else {
        format!(" ({})", kind_parts.join(", "))
    };
    if let Some(peak) = seq_summary.and_then(|s| s.get("peak")) {
        let _ = writeln!(
            out,
            "  quads:     {}{kind_str} (peak {}) | glyphs: {} ({} fill + {} stroke, peak {})",
            t["quads"].as_u64().unwrap_or(0),
            peak["quads"].as_f64().unwrap_or(0.0).round() as u64,
            t["glyphQuads"].as_u64().unwrap_or(0),
            t["fillGlyphQuads"].as_u64().unwrap_or(0),
            t["strokeGlyphQuads"].as_u64().unwrap_or(0),
            peak["glyphQuads"].as_f64().unwrap_or(0.0).round() as u64,
        );
        let _ = writeln!(
            out,
            "  canvasRT:  {} (peak {} B) | {} targets, {} switches, max depth {}",
            format_bytes(rt_bytes),
            peak["canvasRtBytes"].as_f64().unwrap_or(0.0).round() as u64,
            t["canvasGroupTargets"].as_u64().unwrap_or(0),
            t["canvasRtSwitches"].as_u64().unwrap_or(0),
            t["maxCanvasGroupDepth"].as_u64().unwrap_or(0),
        );
        let _ = writeln!(
            out,
            "  overdraw:  {:.3}x screen (peak {:.3}x), {:.3}x offscreen (peak {:.3}x), {:.3}x total",
            t["screenOverdrawViewports"].as_f64().unwrap_or(0.0),
            peak["screenOverdrawViewports"].as_f64().unwrap_or(0.0),
            t["offscreenOverdrawViewports"].as_f64().unwrap_or(0.0),
            peak["offscreenOverdrawViewports"].as_f64().unwrap_or(0.0),
            t["totalOverdrawViewports"].as_f64().unwrap_or(0.0),
        );
    } else {
        let _ = writeln!(
            out,
            "  quads:     {}{kind_str} | glyphs: {} ({} fill + {} stroke)",
            t["quads"].as_u64().unwrap_or(0),
            t["glyphQuads"].as_u64().unwrap_or(0),
            t["fillGlyphQuads"].as_u64().unwrap_or(0),
            t["strokeGlyphQuads"].as_u64().unwrap_or(0),
        );
        let _ = writeln!(
            out,
            "  canvasRT:  {} | {} targets, {} switches, max depth {}",
            format_bytes(rt_bytes),
            t["canvasGroupTargets"].as_u64().unwrap_or(0),
            t["canvasRtSwitches"].as_u64().unwrap_or(0),
            t["maxCanvasGroupDepth"].as_u64().unwrap_or(0),
        );
        let _ = writeln!(
            out,
            "  overdraw:  {:.3}x screen, {:.3}x offscreen, {:.3}x total",
            t["screenOverdrawViewports"].as_f64().unwrap_or(0.0),
            t["offscreenOverdrawViewports"].as_f64().unwrap_or(0.0),
            t["totalOverdrawViewports"].as_f64().unwrap_or(0.0),
        );
    }
    let hid = &t["hiddenInShown"];
    let hid_scope = if hid["complete"].as_bool() == Some(true) { "live DOM" } else { "scene.json incomplete" };
    let _ = writeln!(
        out,
        "  hidden:    {} roots, {} guiObjects, {} instances ({hid_scope})",
        hid["roots"].as_u64().unwrap_or(0),
        hid["guiObjects"].as_u64().unwrap_or(0),
        hid["instances"].as_u64().unwrap_or(0),
    );

    if let Some(seq) = report.pointer("/sequence/summary") {
        let steady = &seq["steadyStateLayoutMean"];
        let steady_samples = seq["steadyStateFrames"].as_u64().unwrap_or(0);
        let steady_client = seq["steadyStateClientFrames"].as_u64().unwrap_or(steady_samples);
        let cadence_note = if run_mode == "play" {
            if steady_client != steady_samples {
                format!(" per {hz_str} Hz {cadence_kind} frame ({steady_client} client frames in {steady_samples} steady samples)")
            } else {
                format!(" per {hz_str} Hz {cadence_kind} frame")
            }
        } else {
            String::new()
        };
        let _ = writeln!(
            out,
            "  layout:    steady {:.2}p / {:.1}s / {:.1}m / {:.1}sp / {:.1}sm{cadence_note} ({} active, {} unchanged)",
            steady["passes"].as_f64().unwrap_or(0.0),
            steady["stored"].as_f64().unwrap_or(0.0),
            steady["moved"].as_f64().unwrap_or(0.0),
            steady["scrollPasses"].as_f64().unwrap_or(0.0),
            steady["scrollMoved"].as_f64().unwrap_or(0.0),
            seq["activeFrames"].as_u64().unwrap_or(0),
            seq["unchangedFrames"].as_u64().unwrap_or(0),
        );
        let cpu_status = seq.pointer("/steadyStateLayoutCpuMs/status").and_then(Value::as_str);
        let drawn_nodes = t["drawnNodes"].as_u64().unwrap_or(0);
        if status != "ok" {
            let detail = if status == "yielded" && drawn_nodes > 0 {
                format!("yielded after drawing {drawn_nodes} nodes; wrap the loop in task.spawn or Heartbeat:Connect so mount returns")
            } else if drawn_nodes > 0 {
                format!("{status} after drawing {drawn_nodes} nodes; counts from a failed run are not valid")
            } else {
                format!("{status}; zero counts are not a valid UI cost measurement")
            };
            let _ = writeln!(out, "  layoutMs:  refused ({detail})");
        } else if run_mode != "play" {
            if report.pointer("/playModeProbe/mountsInPlayMode").and_then(Value::as_bool) == Some(false) {
                let _ = writeln!(
                    out,
                    "  layoutMs:  refused in edit mode (240 Hz; play mode does not mount this story, so per-frame ms is unavailable as-is)",
                );
                let p_status = report.pointer("/playModeProbe/status").and_then(Value::as_str).unwrap_or("failed");
                let p_detail = report
                    .pointer("/playModeProbe/failure")
                    .and_then(Value::as_str)
                    .map(|f| failure_headline(f, 150))
                    .or_else(|| report.pointer("/playModeProbe/waitingAt").and_then(Value::as_str).map(|at| format!("waiting at {at}")))
                    .unwrap_or_else(|| format!("status \"{p_status}\""));
                let _ = writeln!(out, "  play note: play mode {p_status}: {p_detail} (fix this to price per-frame layout)");
            } else {
                let _ = writeln!(
                    out,
                    "  layoutMs:  refused in edit mode (240 Hz, 4 beats/frame; pass --mode play for 60 Hz client per-frame ms)",
                );
            }
        } else if cpu_status == Some("refused") {
            let reason = seq
                .pointer("/steadyStateLayoutCpuMs/reason")
                .and_then(Value::as_str)
                .unwrap_or("0 client frames completed after frame 0");
            let _ = writeln!(out, "  layoutMs:  refused ({})", first_line(reason, 160));
        } else {
            let desk_ms = seq.pointer("/steadyStateLayoutCpuMs/desktop-ref").and_then(Value::as_f64).unwrap_or(0.0);
            let phone_range = pair_at(seq, "/steadyStateLayoutCpuMs/phone-mid-range");
            let phone_band = pair_at(seq, "/steadyStateLayoutCpuMs/phone-mid-band");
            let _ = writeln!(
                out,
                "  layoutMs:  desktop-ref {desk_ms:.4} ms (extrapolated) | {}",
                format_phone_range(&phone_range, &phone_band),
            );
            let _ = writeln!(
                out,
                "  caveat:    absolute ms on deep trees is unchecked against a client (ancestor bookkeeping); before-vs-after on one story is valid",
            );
            if desk_ms <= 1e-12 && steady["passes"].as_f64().unwrap_or(0.0) <= 1e-12 {
                let _ = writeln!(
                    out,
                    "  note:      0 steady churn; opening/full-tree layout cost is on the single-moment screen (omit --from/--frames)",
                );
            }
        }
    } else if let Some(ft) = report.get("fullTreeLayout") {
        let drawn_nodes = t["drawnNodes"].as_u64().unwrap_or(0);
        let _ = writeln!(
            out,
            "  layout:    full-tree {}p / {}s / {}m (per-frame layoutCpuMs refused; use --from and --frames)",
            ft["passes"].as_u64().unwrap_or(0),
            ft["stored"].as_u64().unwrap_or(0),
            ft["moved"].as_u64().unwrap_or(0),
        );
        if status != "ok" {
            let detail = if status == "yielded" && drawn_nodes > 0 {
                format!("yielded after drawing {drawn_nodes} nodes; wrap the loop in task.spawn or Heartbeat:Connect so mount returns")
            } else if drawn_nodes > 0 {
                format!("{status} after drawing {drawn_nodes} nodes; counts from a failed run are not valid")
            } else {
                format!("{status}; zero counts are not a valid UI cost measurement")
            };
            let _ = writeln!(out, "  fullTree:  refused ({detail})");
        } else {
            let desk_ms = report.pointer("/predicted/desktop-ref/fullTreeLayoutCpuMs/ms").and_then(Value::as_f64).unwrap_or(0.0);
            let phone_range = pair_at(report, "/predicted/phone-mid/fullTreeLayoutCpuMs/rangeMs");
            let phone_band = pair_at(report, "/predicted/phone-mid/fullTreeLayoutCpuMs/bandMs");
            let _ = writeln!(
                out,
                "  fullTree:  desktop-ref {desk_ms:.4} ms (extrapolated) | {}",
                format_phone_range(&phone_range, &phone_band),
            );
            let _ = writeln!(
                out,
                "  caveat:    absolute ms on deep trees is unchecked against a client (ancestor bookkeeping); before-vs-after on one story is valid",
            );
        }
    } else {
        let _ = writeln!(
            out,
            "  layout:    refused (--scene has no layout counters; run live with --from and --frames)",
        );
    }

    let _ = writeln!(out, "  refused:   drawPassCpuMs=null, gpuUiMs=null (uncalibrated)");

    let mut cause_parts: Vec<String> = Vec::new();
    if let Some(causes) = t["batchBreaksByCause"].as_object() {
        for cause in ["layerPass", "renderTarget", "clip", "shader", "texture", "rotation"] {
            let n = causes.get(cause).and_then(Value::as_u64).unwrap_or(0);
            if n > 0 {
                cause_parts.push(format!("{cause}={n}"));
            }
        }
    }
    let cause_str = if cause_parts.is_empty() {
        String::new()
    } else {
        format!(" ({})", cause_parts.join(", "))
    };
    let _ = writeln!(
        out,
        "  model:     draws={}, breaks={}{cause_str} [unmeasured, informational]",
        t["drawCalls"].as_u64().unwrap_or(0),
        t["batchBreaks"].as_u64().unwrap_or(0),
    );

    if let Some(guis) = report["screenGuis"].as_array() {
        let mut enabled: Vec<&Value> = guis.iter().filter(|g| g["enabled"].as_bool() != Some(false)).collect();
        enabled.sort_by(|a, b| {
            b["canvasRtBytes"]
                .as_u64()
                .unwrap_or(0)
                .cmp(&a["canvasRtBytes"].as_u64().unwrap_or(0))
                .then_with(|| {
                    (b["quads"].as_u64().unwrap_or(0) + b["glyphQuads"].as_u64().unwrap_or(0))
                        .cmp(&(a["quads"].as_u64().unwrap_or(0) + a["glyphQuads"].as_u64().unwrap_or(0)))
                })
                .then_with(|| a["name"].as_str().unwrap_or("").cmp(b["name"].as_str().unwrap_or("")))
        });
        if !enabled.is_empty() {
            let shown = enabled.len().min(3);
            let _ = writeln!(out, "  top guis ({shown}/{}):", enabled.len());
            for g in enabled.into_iter().take(3) {
                let _ = writeln!(
                    out,
                    "    {} | quads={} glyphs={} rt={} B ov={:.3}x/{:.3}x hid={}",
                    trim_left(g["name"].as_str().unwrap_or("?"), 36),
                    g["quads"].as_u64().unwrap_or(0),
                    g["glyphQuads"].as_u64().unwrap_or(0),
                    g["canvasRtBytes"].as_u64().unwrap_or(0),
                    g["screenOverdrawViewports"].as_f64().unwrap_or(0.0),
                    g["offscreenOverdrawViewports"].as_f64().unwrap_or(0.0),
                    g["hiddenGuiObjects"].as_u64().unwrap_or(0),
                );
            }
        }
    }

    if let Some(objs) = report["objects"].as_array().filter(|a| !a.is_empty()) {
        let shown = objs.len().min(3);
        let labels = format_object_labels(&objs[..shown], 44);
        let _ = writeln!(out, "  top objs ({shown}/{}):", objs.len());
        for (o, label) in objs.iter().take(shown).zip(labels) {
            let _ = writeln!(
                out,
                "    {} | quads={} glyphs={} rt={} B ov={:.3}x/{:.3}x",
                label,
                o["quads"].as_u64().unwrap_or(0),
                o["glyphQuads"].as_u64().unwrap_or(0),
                o["canvasRtBytes"].as_u64().unwrap_or(0),
                o["screenOverdrawViewports"].as_f64().unwrap_or(0.0),
                o["offscreenOverdrawViewports"].as_f64().unwrap_or(0.0),
            );
        }
    }

    if let Some(drivers) = report["layoutDrivers"].as_array().filter(|a| !a.is_empty()) {
        let shown = drivers.len().min(5);
        let labels = format_object_labels(&drivers[..shown], 44);
        let total_drivers = report
            .pointer("/layoutDriverSummary/totalDrivers")
            .and_then(Value::as_u64)
            .unwrap_or(drivers.len() as u64);
        let total_driver_ms = report
            .pointer("/layoutDriverSummary/totalDesktopMs")
            .and_then(Value::as_f64)
            .unwrap_or_else(|| drivers.iter().map(|d| d["desktopMs"].as_f64().unwrap_or(0.0)).sum());
        let shown_ms: f64 = drivers
            .iter()
            .take(shown)
            .map(|d| d["desktopMs"].as_f64().unwrap_or(0.0))
            .sum();
        let share_pct = if total_driver_ms > 1e-12 {
            (shown_ms / total_driver_ms * 100.0).clamp(0.0, 100.0)
        } else {
            0.0
        };
        let _ = writeln!(out, "  top layout ({shown}/{total_drivers}, {share_pct:.0}% of churn):");
        for (d, label) in drivers.iter().take(shown).zip(labels) {
            let _ = writeln!(
                out,
                "    {} | {:.1}s/{:.1}m/{:.1}sp/{:.1}sm ({:.4} ms desk)",
                label,
                d["stored"].as_f64().unwrap_or(0.0),
                d["moved"].as_f64().unwrap_or(0.0),
                d["scrollPasses"].as_f64().unwrap_or(0.0),
                d["scrollMoved"].as_f64().unwrap_or(0.0),
                d["desktopMs"].as_f64().unwrap_or(0.0),
            );
        }
    }

    if let Some(hids) = report["hiddenSubtrees"].as_array().filter(|a| !a.is_empty()) {
        let shown = hids.len().min(2);
        let _ = writeln!(out, "  top hidden ({shown}/{}):", hids.len());
        for h in hids.iter().take(2) {
            let _ = writeln!(
                out,
                "    {} ({}) | {} guiObjects ({} insts)",
                trim_left(h["path"].as_str().unwrap_or("?"), 48),
                h["class"].as_str().unwrap_or("?"),
                h["guiObjects"].as_u64().unwrap_or(0),
                h["instances"].as_u64().unwrap_or(0),
            );
        }
    }

    let _ = writeln!(out, "  report:    {} (full JSON; pass --json for stdout JSON)", dest.display());
    out
}

/// Formats an `rrr-ab/1` comparison report as a compact one-screen stdout summary.
pub fn format_one_screen_compare(diff: &Value, dest: Option<&Path>) -> String {
    use std::fmt::Write as _;

    let mut out = String::new();
    let verdict = diff["verdict"].as_str().unwrap_or("?");
    if verdict == "refused" {
        let _ = writeln!(out, "rur cost compare: refused");
        let _ = writeln!(out, "  reason: {}", diff["reason"].as_str().unwrap_or("incomparable reports"));
        let _ = writeln!(out, "  remedy: {}", diff["remedy"].as_str().unwrap_or(""));
        if let Some(p) = dest {
            let _ = writeln!(out, "  report: {} (full JSON; pass --json for stdout JSON)", p.display());
        }
        return out;
    }

    let _ = writeln!(out, "rur cost compare: {verdict} (measured counts only; noise=0.0)");
    if let Some(note) = diff.pointer("/equivalence/uncheckedNote").and_then(Value::as_str) {
        let _ = writeln!(out, "  note:      {note}");
    }
    let num = |ptr: &str| diff.pointer(ptr).and_then(Value::as_f64).unwrap_or(0.0);
    let _ = writeln!(
        out,
        "  quads:     {} -> {} ({}) | glyphs: {} -> {} ({})",
        num("/counts/before/quads") as u64,
        num("/counts/after/quads") as u64,
        format_signed_num(num("/counts/delta/quads")),
        num("/counts/before/glyphQuads") as u64,
        num("/counts/after/glyphQuads") as u64,
        format_signed_num(num("/counts/delta/glyphQuads")),
    );
    let _ = writeln!(
        out,
        "  canvasRT:  {} B -> {} B ({} B) | targets: {} -> {} | switches: {} -> {}",
        num("/counts/before/canvasRtBytes") as u64,
        num("/counts/after/canvasRtBytes") as u64,
        format_signed_num(num("/counts/delta/canvasRtBytes")),
        num("/counts/before/canvasGroupTargets") as u64,
        num("/counts/after/canvasGroupTargets") as u64,
        num("/counts/before/canvasRtSwitches") as u64,
        num("/counts/after/canvasRtSwitches") as u64,
    );
    let _ = writeln!(
        out,
        "  overdraw:  screen {:.3}x -> {:.3}x ({}) | offscreen {:.3}x -> {:.3}x ({})",
        num("/counts/before/screenOverdrawViewports"),
        num("/counts/after/screenOverdrawViewports"),
        format_signed_num(num("/counts/delta/screenOverdrawViewports")),
        num("/counts/before/offscreenOverdrawViewports"),
        num("/counts/after/offscreenOverdrawViewports"),
        format_signed_num(num("/counts/delta/offscreenOverdrawViewports")),
    );
    let _ = writeln!(
        out,
        "  hidden:    {} -> {} ({} guiObjects)",
        num("/counts/before/hiddenGuiObjects") as u64,
        num("/counts/after/hiddenGuiObjects") as u64,
        format_signed_num(num("/counts/delta/hiddenGuiObjects")),
    );
    if let (Some(b_ms), Some(a_ms), Some(d_ms)) = (
        diff.pointer("/predicted/desktop-ref/layoutCpuMs/beforeMs").and_then(Value::as_f64),
        diff.pointer("/predicted/desktop-ref/layoutCpuMs/afterMs").and_then(Value::as_f64),
        diff.pointer("/predicted/desktop-ref/layoutCpuMs/deltaMs").and_then(Value::as_f64),
    ) {
        let _ = writeln!(
            out,
            "  layoutMs:  desktop-ref {b_ms:.4} -> {a_ms:.4} ms ({d_ms:+.4} ms, extrapolated)",
        );
    }
    let _ = writeln!(
        out,
        "  model:     draws {} -> {} ({}), breaks {} -> {} ({}) [unmeasured, informational]",
        num("/counts/before/drawCalls") as u64,
        num("/counts/after/drawCalls") as u64,
        format_signed_num(num("/counts/delta/drawCalls")),
        num("/counts/before/batchBreaks") as u64,
        num("/counts/after/batchBreaks") as u64,
        format_signed_num(num("/counts/delta/batchBreaks")),
    );
    if let Some(movers) = diff["screenGuiMovers"].as_array().filter(|m| !m.is_empty()) {
        let shown = movers.len().min(5);
        let _ = writeln!(out, "  movers ({shown}/{}):", movers.len());
        for m in movers.iter().take(5) {
            let _ = writeln!(
                out,
                "    {} | quads {} glyphs {} rt {} B ov {} hid {}",
                trim_left(m["name"].as_str().unwrap_or("?"), 32),
                format_signed_num(m["deltaQuads"].as_f64().unwrap_or(0.0)),
                format_signed_num(m["deltaGlyphQuads"].as_f64().unwrap_or(0.0)),
                format_signed_num(m["deltaCanvasRtBytes"].as_f64().unwrap_or(0.0)),
                format_signed_num(m["deltaScreenOverdrawViewports"].as_f64().unwrap_or(0.0)),
                format_signed_num(m["deltaHiddenGuiObjects"].as_f64().unwrap_or(0.0)),
            );
        }
    }
    if let Some(p) = dest {
        let _ = writeln!(out, "  report:    {} (full JSON; pass --json for stdout JSON)", p.display());
    }
    out
}

fn print_summary(report: &Value, dest: &Path) {
    let t = &report["totals"];
    let rt_bytes = t["canvasRtBytes"].as_u64().unwrap_or(0);
    let status = report["status"].as_str().unwrap_or("?");
    let run_mode = report["runMode"].as_str().unwrap_or("unknown");
    if status != "ok" {
        if let Some(failure) = report.get("failure").and_then(Value::as_str).filter(|s| !s.trim().is_empty()) {
            eprintln!("rur cost: {status}: {}", failure_headline(failure, 200));
        }
        if let Some(hint) = report.get("hint").and_then(Value::as_str).filter(|s| !s.trim().is_empty()) {
            eprintln!("rur cost: hint: {hint}");
        }
        if report.pointer("/editModeFallback/mountsInEditMode").and_then(Value::as_bool) == Some(true) {
            eprintln!(
                "rur cost: story mounts in edit mode, not play; per-frame layoutCpuMs is unavailable as-is (pass --mode edit for counts/RT/overdraw/hidden DOM, or fix the wait/error above)"
            );
        }
    }
    if let Some(seq) = report.pointer("/sequence/summary") {
        let frames = seq["frameCount"].as_u64().unwrap_or(0);
        let fps = seq["fps"].as_f64().unwrap_or(60.0);
        let client_hz = seq["clientHz"].as_f64().unwrap_or(60.0);
        let cadence_kind = seq["cadenceKind"].as_str().unwrap_or("client");
        let hz_str = format_hz(client_hz);
        let steady = &seq["steadyStateLayoutMean"];
        let mode_label = match run_mode {
            "play" => format!("mode=play {hz_str} Hz {cadence_kind}"),
            "edit" => "mode=edit 240 Hz".to_string(),
            other => other.to_string(),
        };
        let cpu_status = seq.pointer("/steadyStateLayoutCpuMs/status").and_then(Value::as_str);
        let layout_timing = if status != "ok" {
            format!("layout ms refused ({status}); draw/gpu refused")
        } else if run_mode != "play" {
            if report.pointer("/playModeProbe/mountsInPlayMode").and_then(Value::as_bool) == Some(false) {
                "per-frame layout ms unavailable as-is (edit mode 240 Hz; play mode does not mount this story); draw/gpu refused".to_string()
            } else {
                "per-frame layout ms refused in edit mode (pass --mode play for 60 Hz client cadence); draw/gpu refused".to_string()
            }
        } else if cpu_status == Some("refused") {
            let reason = seq
                .pointer("/steadyStateLayoutCpuMs/reason")
                .and_then(Value::as_str)
                .unwrap_or("0 client frames completed");
            format!("layout ms refused ({}); draw/gpu refused", first_line(reason, 120))
        } else {
            let desk_ms = seq.pointer("/steadyStateLayoutCpuMs/desktop-ref").and_then(Value::as_f64).unwrap_or(0.0);
            let phone_range = pair_at(seq, "/steadyStateLayoutCpuMs/phone-mid-range");
            let phone_band = pair_at(seq, "/steadyStateLayoutCpuMs/phone-mid-band");
            format!(
                "extrapolated desk {desk_ms:.4} ms, {}; draw/gpu refused",
                format_phone_range(&phone_range, &phone_band)
            )
        };
        eprintln!(
            "rur cost: {} ({} samples @ {} fps, {}) | quads={} glyphs={} | rt={} ({} targets) | overdraw={:.3}x screen / {:.3}x rt | steady layout={:.2}p/{:.1}s/{:.1}m/{:.1}sp/{:.1}sm ({}) | model draws={} breaks={} (unmeasured) -> {}",
            status,
            frames,
            format_hz(fps),
            mode_label,
            t["quads"].as_u64().unwrap_or(0),
            t["glyphQuads"].as_u64().unwrap_or(0),
            format_bytes(rt_bytes),
            t["canvasGroupTargets"].as_u64().unwrap_or(0),
            t["screenOverdrawViewports"].as_f64().unwrap_or(0.0),
            t["offscreenOverdrawViewports"].as_f64().unwrap_or(0.0),
            steady["passes"].as_f64().unwrap_or(0.0),
            steady["stored"].as_f64().unwrap_or(0.0),
            steady["moved"].as_f64().unwrap_or(0.0),
            steady["scrollPasses"].as_f64().unwrap_or(0.0),
            steady["scrollMoved"].as_f64().unwrap_or(0.0),
            layout_timing,
            t["drawCalls"].as_u64().unwrap_or(0),
            t["batchBreaks"].as_u64().unwrap_or(0),
            dest.display(),
        );
        return;
    }
    if let Some(ft) = report.get("fullTreeLayout") {
        let timing_str = if status != "ok" {
            format!("layout ms refused ({status}); draw/gpu refused")
        } else {
            let desk_ms = report.pointer("/predicted/desktop-ref/fullTreeLayoutCpuMs/ms").and_then(Value::as_f64).unwrap_or(0.0);
            let phone_range = pair_at(report, "/predicted/phone-mid/fullTreeLayoutCpuMs/rangeMs");
            let phone_band = pair_at(report, "/predicted/phone-mid/fullTreeLayoutCpuMs/bandMs");
            format!(
                "extrapolated desk {desk_ms:.4} ms, {}; per-frame cost needs --from and --frames; draw/gpu refused",
                format_phone_range(&phone_range, &phone_band)
            )
        };
        eprintln!(
            "rur cost: {} (mode={}) | guis={} (disabled={}) | quads={} glyphs={} | rt={} ({} targets, {} switches) | overdraw={:.3}x screen / {:.3}x rt | hidden={} roots / {} guiObjects | full-tree layout={}p/{}s ({}) | model draws={} breaks={} (unmeasured) -> {}",
            status,
            run_mode,
            t["screenGuis"].as_u64().unwrap_or(0),
            t["disabledScreenGuis"].as_u64().unwrap_or(0),
            t["quads"].as_u64().unwrap_or(0),
            t["glyphQuads"].as_u64().unwrap_or(0),
            format_bytes(rt_bytes),
            t["canvasGroupTargets"].as_u64().unwrap_or(0),
            t["canvasRtSwitches"].as_u64().unwrap_or(0),
            t["screenOverdrawViewports"].as_f64().unwrap_or(0.0),
            t["offscreenOverdrawViewports"].as_f64().unwrap_or(0.0),
            t["hiddenInShown"]["roots"].as_u64().unwrap_or(0),
            t["hiddenInShown"]["guiObjects"].as_u64().unwrap_or(0),
            ft["passes"].as_u64().unwrap_or(0),
            ft["stored"].as_u64().unwrap_or(0),
            timing_str,
            t["drawCalls"].as_u64().unwrap_or(0),
            t["batchBreaks"].as_u64().unwrap_or(0),
            dest.display(),
        );
    } else {
        eprintln!(
            "rur cost: {} (scene, mode={}) | guis={} (disabled={}) | quads={} glyphs={} | rt={} ({} targets, {} switches) | overdraw={:.3}x screen / {:.3}x rt | hidden={} roots / {} guiObjects (incomplete) | layout=unavailable (--scene; run live with --from and --frames; draw/gpu refused) | model draws={} breaks={} (unmeasured) -> {}",
            report["status"].as_str().unwrap_or("?"),
            run_mode,
            t["screenGuis"].as_u64().unwrap_or(0),
            t["disabledScreenGuis"].as_u64().unwrap_or(0),
            t["quads"].as_u64().unwrap_or(0),
            t["glyphQuads"].as_u64().unwrap_or(0),
            format_bytes(rt_bytes),
            t["canvasGroupTargets"].as_u64().unwrap_or(0),
            t["canvasRtSwitches"].as_u64().unwrap_or(0),
            t["screenOverdrawViewports"].as_f64().unwrap_or(0.0),
            t["offscreenOverdrawViewports"].as_f64().unwrap_or(0.0),
            t["hiddenInShown"]["roots"].as_u64().unwrap_or(0),
            t["hiddenInShown"]["guiObjects"].as_u64().unwrap_or(0),
            t["drawCalls"].as_u64().unwrap_or(0),
            t["batchBreaks"].as_u64().unwrap_or(0),
            dest.display(),
        );
    }
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
           [--from S] [--to S | --frames N | --duration S] [--fps N] [--client-hz N]
           [--mode edit|play] [--device desktop|touch|gamepad] [--topbar-inset px]
           [--controls <json|file>] [--actions <json|file>] [--compare <before>] [--json]
  rur cost --scene <scene.json> [--out <dir|file.json>] [--json]
  rur cost compare <before> <after> [--out <dir|file.json>] [--json]

Cadence:
  Sequence mode (--from/--frames/--to/--duration/--fps) defaults to --mode play at 60 Hz client
  cadence (--client-hz defaults to 60); --fps controls only how often samples are captured, and
  per-frame figures are normalized to one client frame (or refused when 0 client frames step).
  Explicit --mode edit (or `rur sequence --cost` without --mode play) refuses per-frame layoutCpuMs.

Outputs:
  stdout             compact one-screen summary (or full JSON with --json).
  <out>/cost.json    deterministic JSON (zero wall-clock fields): quads & glyph quads, clipped
                     overdraw, Studio CanvasGroup RT dimensions & switches, live-DOM hidden-in-shown
                     subtrees, layout pass counters (and per-frame churn in sequence mode),
                     extrapolated desktop / derived phone-mid layout CPU ms, an unmeasured
                     state-transition draw/batch model, and explicit refusals for uncalibrated
                     draw-pass CPU and mobile GPU UI ms.
";

struct CliOutput {
    value: Value,
    summary: String,
    json: bool,
    failed: bool,
}

const COST_OPTIONS: &[&str] = &[
    "--story",
    "--module",
    "--model",
    "--fixture",
    "--scene",
    "--repo",
    "--project",
    "--place",
    "--roots",
    "--rojo",
    "--rojo-at",
    "--mount",
    "--load",
    "--snapshot-dir",
    "--no-snapshots",
    "--studio",
    "--git-ref",
    "--plugin",
    "--offline",
    "--viewport",
    "--viewport-from",
    "--scale",
    "--time",
    "--mode",
    "--player",
    "--background",
    "--place-name",
    "--actions",
    "--controls",
    "--camera",
    "--place-id",
    "--rotation-clipping",
    "--topbar-inset",
    "--device",
    "--all-layers",
    "--verbose",
    "--from",
    "--to",
    "--frames",
    "--duration",
    "--fps",
    "--client-hz",
    "--out",
    "--tag",
    "--keep",
    "--compare",
    "--json",
    "--help",
];

const COMPARE_OPTIONS: &[&str] = &["--out", "--tag", "--keep", "--json", "--repo", "--help"];

fn validate_cost_cli_args(args: &Args, is_compare: bool) -> Result<(), Failure> {
    let allowed = if is_compare { COMPARE_OPTIONS } else { COST_OPTIONS };
    for opt in args.option_names() {
        if !allowed.contains(&opt.as_str()) {
            let cmd = if is_compare { "rur cost compare" } else { "rur cost" };
            return Err(Failure::Usage(format!(
                "unknown option for `{cmd}`: {opt} (run `rur cost --help` for supported options)"
            )));
        }
    }
    if is_compare {
        if args.positional.len() > 3 {
            return Err(Failure::Usage(format!(
                "cost compare takes exactly two reports (<before> <after>), got unexpected argument \"{}\"",
                args.positional[3]
            )));
        }
    } else if let Some(extra) = args.positional.first() {
        return Err(Failure::Usage(format!(
            "unexpected positional argument for `rur cost`: \"{extra}\" (use --story/--fixture/--module/--model/--scene, or `rur cost compare <before> <after>`)"
        )));
    }
    if !is_compare && args.present("--time") {
        let raw = args.given("--time").ok_or_else(|| {
            Failure::Usage("--time in `rur cost` takes a single moment in seconds (e.g. --time 0.5)".into())
        })?;
        let trimmed = raw.trim();
        let valid = !trimmed.contains(',')
            && !trimmed.contains("..")
            && trimmed.parse::<f64>().is_ok_and(|v| v.is_finite() && v >= 0.0);
        if !valid {
            return Err(Failure::Usage(format!(
                "--time in `rur cost` takes a single moment in seconds, got \"{raw}\" (use --from and --frames/--to/--duration with --fps for multiple moments)"
            )));
        }
    }
    Ok(())
}

fn normalize_source_path(path: &Path, bases: &[&Path]) -> String {
    for base in bases {
        if let Ok(rel) = path.strip_prefix(base) {
            return rel.to_string_lossy().replace('\\', "/");
        }
    }
    path.to_string_lossy().replace('\\', "/")
}

fn parse_controls_json(raw: Option<&str>, resolve_base: Option<&Context>) -> Value {
    let Some(raw) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return json!({});
    };
    if raw.starts_with('{') {
        if let Ok(v) = serde_json::from_str::<Value>(raw) {
            return sort_json(v);
        }
        return json!(raw);
    }
    let path = match resolve_base {
        Some(ctx) => ctx.resolve(raw),
        None => PathBuf::from(raw),
    };
    if let Ok(text) = std::fs::read_to_string(&path) {
        if let Ok(v) = serde_json::from_str::<Value>(&text) {
            return sort_json(v);
        }
    }
    json!(raw.replace('\\', "/"))
}

fn source_from_engine_args(engine_args: &engine::cli::Args, repo: &Path) -> Option<Value> {
    let (kind, raw) = if let Some(s) = engine_args.story.as_deref() {
        ("story", s)
    } else if let Some(s) = engine_args.module.as_deref() {
        ("module", s)
    } else if let Some(s) = engine_args.model.as_deref() {
        ("model", s)
    } else if let Some(s) = engine_args.fixture.as_deref() {
        ("fixture", s)
    } else {
        return None;
    };
    let mut bases: Vec<&Path> = Vec::new();
    if let Some(proj) = engine_args.project.as_deref() {
        bases.push(Path::new(proj));
    }
    bases.push(repo);
    let target = normalize_source_path(Path::new(raw), &bases);
    Some(json!({ "kind": kind, "target": target }))
}

fn source_from_cli_args(context: &Context, args: &Args) -> Option<Value> {
    for (flag, kind) in [
        ("--story", "story"),
        ("--module", "module"),
        ("--model", "model"),
        ("--fixture", "fixture"),
    ] {
        if let Some(raw) = args.value(flag) {
            let abs = context.resolve(&raw);
            let proj = context.project(args);
            let target = normalize_source_path(&abs, &[proj.as_path(), context.repo.as_path()]);
            return Some(json!({ "kind": kind, "target": target }));
        }
    }
    None
}

pub(crate) fn set_report_device(report: &mut Value, device: &str) {
    if let Some(obj) = report.as_object_mut() {
        obj.insert("device".into(), json!(device));
    }
    *report = sort_json(report.clone());
}

fn attach_engine_run_metadata(
    report: &mut Value,
    engine_args: &engine::cli::Args,
    repo: &Path,
    actual_device: Option<&str>,
) {
    let Some(obj) = report.as_object_mut() else { return };
    if let Some(src) = source_from_engine_args(engine_args, repo) {
        obj.insert("source".into(), src);
    }
    let device = actual_device
        .or(engine_args.device.as_deref())
        .unwrap_or("desktop");
    obj.insert("device".into(), json!(device));
    obj.insert("controls".into(), parse_controls_json(engine_args.controls.as_deref(), None));
}

fn attach_cli_run_metadata(
    report: &mut Value,
    context: &Context,
    args: &Args,
    engine_args: &engine::cli::Args,
    actual_device: Option<&str>,
    single_time: Option<f64>,
) {
    let Some(obj) = report.as_object_mut() else { return };
    if let Some(src) = source_from_cli_args(context, args) {
        obj.insert("source".into(), src);
    }
    let device = actual_device
        .or(engine_args.device.as_deref())
        .unwrap_or("desktop");
    obj.insert("device".into(), json!(device));
    obj.insert("controls".into(), parse_controls_json(args.given("--controls").as_deref(), Some(context)));
    if let Some(t) = single_time {
        obj.insert("time".into(), json!(round9(t)));
    }
}

fn run_cli(argv: &[String]) -> Result<CliOutput, Failure> {
    let (context, args) = Context::new(argv)?;
    let is_compare = args.positional.first().map(String::as_str) == Some("compare");
    validate_cost_cli_args(&args, is_compare)?;

    if is_compare {
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
        let out_path = if args.value("--out").is_some() {
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
            Some(out)
        } else {
            if refused {
                eprintln!(
                    "rur cost compare: refused — {} | Remedy: {}",
                    diff["reason"].as_str().unwrap_or("incomparable reports"),
                    diff["remedy"].as_str().unwrap_or("")
                );
            } else {
                eprintln!("rur cost compare: {}", diff["verdict"].as_str().unwrap_or("?"));
            }
            None
        };
        let summary = format_one_screen_compare(&diff, out_path.as_deref());
        return Ok(CliOutput {
            value: diff,
            summary,
            json: args.truthy("--json"),
            failed: refused,
        });
    }

    let is_sequence = ["--from", "--to", "--frames", "--duration", "--fps", "--client-hz"]
        .iter()
        .any(|k| args.present(k));
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
        let (mut spec, _) = crate::sequence::parse_options(&Value::Object(seq_opts), args.truthy("--offline"))
            .map_err(Failure::Usage)?;
        let client_hz = match args.value("--client-hz") {
            Some(raw) => {
                let parsed: f64 = raw
                    .trim()
                    .parse()
                    .ok()
                    .filter(|v: &f64| v.is_finite() && *v > 0.0 && *v <= 1000.0)
                    .ok_or_else(|| Failure::Usage(format!("--client-hz must be above 0 and at most 1000 (got {raw})")))?;
                parsed
            }
            None => 60.0,
        };
        spec.client_hz = Some(client_hz);
        let engine_args = build_engine_args(&context, &args)?;
        let mut report = run_sequence_cost(&engine_args, &spec, &context.repo).map_err(Failure::Error)?;
        let actual_device = report.get("device").and_then(Value::as_str).map(str::to_string);
        attach_cli_run_metadata(&mut report, &context, &args, &engine_args, actual_device.as_deref(), None);
        let report = sort_json(report);
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
        let run_mode = engine_args.mode.as_deref().unwrap_or("edit");
        let mut report = analyze_scene_with_mode(&store, run.layout, Some(&run.dom_hidden), run_mode);
        let mut outcome = extract_run_outcome(&run.scene.scene);
        let run_failed = report["status"] != "ok" || outcome.status.as_deref().is_some_and(|s| s != "ok");
        if run_mode == "play" && run_failed {
            let probe_time = engine_args.time.unwrap_or(0.05).max(0.05);
            outcome.edit_mode_mounts = Some(probe_mounts_in_edit_mode(&engine_args, probe_time));
        } else if run_mode == "edit" && run_failed {
            let probe_time = engine_args.time.unwrap_or(0.05).max(0.05);
            outcome.play_mode_probe = Some(probe_mounts_in_play_mode(&engine_args, probe_time));
        }
        apply_run_outcome(&mut report, &outcome, true);
        attach_cli_run_metadata(
            &mut report,
            &context,
            &args,
            &engine_args,
            Some(&run.device),
            Some(engine_args.time.unwrap_or(0.0)),
        );
        let report = sort_json(report);
        let name = format!("{}-cost", crate::story_args::source_name(&source, false));
        (report, name)
    };

    let dest = resolve_output_file(&context, &args, &default_name, "cost.json");
    write_json_file(&dest, &report)?;
    print_summary(&report, &dest);

    let mut summary = format_one_screen_report(&report, &dest);
    let failed = report["status"].as_str() != Some("ok");
    if let Some(before_arg) = args.value("--compare") {
        let before_path = resolve_cost_file(&context.resolve(&before_arg));
        let before = read_json_file(&before_path)?;
        validate_cost_report(&before, "before", Some(&before_path)).map_err(Failure::Error)?;
        let diff = compare_reports(&before, &report).map_err(Failure::Error)?;
        let refused = diff["verdict"].as_str() == Some("refused");
        let cmp_path = if let Some(parent) = dest.parent() {
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
            Some(cmp_path)
        } else {
            None
        };
        summary.push_str(&format_one_screen_compare(&diff, cmp_path.as_deref()));
        return Ok(CliOutput {
            value: sort_json(json!({ "cost": report, "comparison": diff })),
            summary,
            json: args.truthy("--json"),
            failed: failed || refused,
        });
    }

    Ok(CliOutput {
        value: report,
        summary,
        json: args.truthy("--json"),
        failed,
    })
}

/// The reports `rur-native cost compare <before> <after>` reads, named without an option
const COMPARE_INPUTS: [&str; 2] = ["before", "after"];

/// Refuses an unusable `cost compare` input in a release binary, as every other path is refused:
/// empty, blank, flag-like or missing. Startup calls it before the state directory is made
/// (`paths::initialize`); without it, an empty input fails only once it is read, and a blank or
/// flag-like one is read as a file of that name in the current folder. A checkout build keeps its
/// own handling. A help request reads no report, and a command line its parser refuses reports that
/// itself.
pub(crate) fn validate_arguments(argv: &[String]) -> Result<(), String> {
    if !crate::paths::is_standalone() || argv.iter().any(|arg| arg == "--help" || arg == "-h") {
        return Ok(());
    }
    let Ok(args) = crate::story_args::arguments(argv) else {
        return Ok(());
    };
    if args.positional.first().map(String::as_str) != Some("compare") {
        return Ok(());
    }
    for (position, name) in COMPARE_INPUTS.iter().enumerate() {
        let label = format!("cost compare {name}");
        match args.positional.get(position + 1) {
            Some(input) => crate::paths::validate_path_value(&label, input)?,
            None => return Err(format!("{label} needs a nonempty value")),
        }
    }
    Ok(())
}

/// Entrypoint for `rur-native cost ...`.
pub fn cli(argv: &[String]) -> i32 {
    if argv.iter().any(|a| a == "--help" || a == "-h") {
        print!("{HELP}");
        return 0;
    }
    match run_cli(argv) {
        Ok(out) => {
            if out.json {
                if let Ok(text) = serde_json::to_string_pretty(&out.value) {
                    println!("{text}");
                }
            } else {
                print!("{}", out.summary);
            }
            if out.failed { 2 } else { 0 }
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
    fn one_screen_summary_fits_one_screen_for_single_sequence_and_compare() {
        // Large single-moment report with many objects and hidden subtrees must stay compact (<= 40 lines, <= 4096 bytes)
        let mut nodes = vec![json!({"id": 1, "name": "Hud", "path": "PlayerGui.Hud", "class": "ScreenGui", "rect": [0, 0, 1920, 1080], "enabled": true})];
        let mut draw = Vec::new();
        for i in 2..=60u64 {
            nodes.push(json!({
                "id": i, "name": format!("Item{i}"), "path": format!("PlayerGui.Hud.List.Item{i}"),
                "class": "Frame", "rect": [(i % 10) * 80, (i / 10) * 80, 60, 60],
                "bg": {"color": [1, 1, 1], "transparency": 0}
            }));
            draw.push(json!(i));
        }
        let scene = store_of(json!({
            "viewport": {"width": 1920, "height": 1080, "scale": 1.0},
            "status": "ok",
            "nodes": nodes,
            "layers": [{"root": 1, "playerGui": true, "displayOrder": 0, "draw": draw}],
            "fonts": [],
            "images": []
        }));
        let mut hidden = DomHiddenSummary { complete: true, roots: 5, gui_objects: 50, instances: 75, ..Default::default() };
        for i in 0..5u64 {
            hidden.subtrees.push(HiddenSubtree {
                id: 100 + i, name: format!("Modal{i}"), path: format!("PlayerGui.Hud.Modal{i}"),
                class: "Frame".into(), screen_gui: "Hud".into(), gui_objects: 10, instances: 15,
            });
        }
        let single = analyze_scene_with_mode(&scene, LayoutStats { passes: 1, stored: 60, ..Default::default() }, Some(&hidden), "edit");
        let dest = Path::new("out/hud/cost.json");
        let text = format_one_screen_report(&single, dest);
        let non_empty = text.lines().filter(|l| !l.trim().is_empty()).count();
        assert!(non_empty <= 32, "single one-screen summary too tall ({non_empty} lines):\n{text}");
        assert!(text.len() <= 4096, "single one-screen summary too large ({} bytes)", text.len());
        assert!(!text.trim_start().starts_with('{'), "plain summary must not dump raw JSON");
        assert!(text.contains("0 B"), "summary must include exact RT bytes: {text}");
        assert!(text.contains("cost.json"), "summary must name report path: {text}");
        assert!(text.contains("derived 3.9x..6.3x") && text.contains("2.55x..12.32x"), "{text}");
        assert!(text.contains("assumed"), "phone range line must state assumed factors:\n{text}");

        // Sequence summary (both play and edit modes) must stay <= 32 lines even for many frames
        let mut acc_play = SequenceAccumulator::new(60.0, "play");
        let mut acc_edit = SequenceAccumulator::new(60.0, "edit");
        for i in 0..10u32 {
            let st = LayoutStats { scroll_passes: 1, scroll_moved: 120, ..Default::default() };
            acc_play.record_frame(i, f64::from(i) / 60.0, false, 1, 0, 1, &scene, st, Vec::new(), if i == 0 { Some(hidden.clone()) } else { None });
            acc_edit.record_frame(i, f64::from(i) / 60.0, false, 1, 0, 1, &scene, st, Vec::new(), if i == 0 { Some(hidden.clone()) } else { None });
        }
        let play_text = format_one_screen_report(&acc_play.finish(Some("ok")), dest);
        let play_lines = play_text.lines().filter(|l| !l.trim().is_empty()).count();
        assert!(play_lines <= 32 && play_text.len() <= 4096, "play seq summary too large ({play_lines} lines, {} B):\n{play_text}", play_text.len());
        assert!(play_text.contains("mode=play 60 Hz") && play_text.contains("0.0994 ms"), "{play_text}");
        assert!(play_text.contains("assumed"), "sequence phone range line must state assumed factors:\n{play_text}");

        let edit_text = format_one_screen_report(&acc_edit.finish(Some("ok")), dest);
        let edit_lines = edit_text.lines().filter(|l| !l.trim().is_empty()).count();
        assert!(edit_lines <= 32 && edit_text.len() <= 4096, "edit seq summary too large ({edit_lines} lines, {} B)", edit_text.len());
        assert!(edit_text.contains("mode=edit 240 Hz") && edit_text.contains("refused in edit mode"), "{edit_text}");

        // Compare summary (comparable + refused)
        let before = sample_cost_report("single", "live", "ok", 1920.0, 1080.0, 1.5, true, 10, 400_000, 10);
        let after = sample_cost_report("single", "live", "ok", 1920.0, 1080.0, 1.5, true, 8, 200_000, 0);
        let diff = compare_reports(&before, &after).unwrap();
        let cmp_text = format_one_screen_compare(&diff, Some(Path::new("out/compare.json")));
        assert!(cmp_text.lines().filter(|l| !l.trim().is_empty()).count() <= 20 && cmp_text.len() <= 2048, "{cmp_text}");
        assert!(cmp_text.contains("rur cost compare: improved") && cmp_text.contains("compare.json"), "{cmp_text}");

        let mut play_before = before.clone();
        play_before["runMode"] = json!("play");
        let ref_diff = compare_reports(&before, &play_before).unwrap();
        let ref_text = format_one_screen_compare(&ref_diff, None);
        assert!(ref_text.contains("rur cost compare: refused") && ref_text.contains("runMode"), "{ref_text}");
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
    fn clipped_canvas_group_draws_whole_texture_without_ancestor_clip_or_spurious_break() {
        // 800x600 at scale 1: 100x100 ClipsDescendants frame -> 200x100 CanvasGroup ->
        // 50x50 InView (inside ancestor clip) + 50x50 OutOfClip (in the half the ancestor clip hides).
        // Studio renders both the CanvasGroup's background and its descendants into the 200x100
        // offscreen texture unclipped by the group's ancestors (notes/images.md round 7,
        // render::groups::texture_clip_node); the ancestor clip applies only on composite.
        let scene = store_of(json!({
            "viewport": {"width": 800, "height": 600, "scale": 1.0},
            "status": "ok",
            "nodes": [
                {"id": 1, "name": "G", "path": "G", "class": "ScreenGui", "rect": [0, 0, 800, 600], "enabled": true},
                {"id": 2, "name": "Clip", "path": "G.Clip", "class": "Frame", "rect": [0, 0, 100, 100], "bg": {"color": [0, 0, 0], "transparency": 1}},
                {"id": 3, "name": "Group", "path": "G.Clip.Group", "class": "CanvasGroup", "rect": [0, 0, 200, 100], "clip": [0, 0, 100, 100], "bg": {"color": [1, 1, 1], "transparency": 0}, "group": {"transparency": 0}},
                {"id": 4, "name": "InView", "path": "G.Clip.Group.InView", "class": "Frame", "rect": [10, 10, 50, 50], "clip": [0, 0, 100, 100], "bg": {"color": [1, 0, 0], "transparency": 0}},
                {"id": 5, "name": "OutOfClip", "path": "G.Clip.Group.OutOfClip", "class": "Frame", "rect": [120, 10, 50, 50], "clip": [0, 0, 100, 100], "bg": {"color": [0, 0, 1], "transparency": 0}}
            ],
            "layers": [{"root": 1, "playerGui": true, "displayOrder": 0, "draw": [
                2,
                {"group": 3, "draw": [3, 4, 5]}
            ]}],
            "fonts": [],
            "images": []
        }));
        let report = analyze_scene(&scene, LayoutStats::default(), None);
        let t = &report["totals"];
        // Quads: Group bg (1) + InView (1) + OutOfClip (1) + Group composite (1) = 4
        assert_eq!(t["quads"], 4);
        assert_eq!(t["canvasRtBytes"], 80_000);
        // Offscreen overdraw: (20,000 + 2,500 + 2,500) / 480,000 = 0.052083
        assert_eq!(t["offscreenOverdrawViewports"], 0.052083);
        // Screen overdraw: composite clipped to 100x100 = 10,000 / 480,000 = 0.020833
        assert_eq!(t["screenOverdrawViewports"], 0.020833);
        assert_eq!(t["batchBreaksByCause"]["clip"], 0);
        assert_eq!(t["batchBreaks"], 0);
        let objects = report["objects"].as_array().unwrap();
        let in_view = objects.iter().find(|o| o["name"] == "InView").unwrap();
        assert_eq!(in_view["batchBreaks"], 0);
        assert_eq!(in_view["quads"], 1);
        let out_of_clip = objects.iter().find(|o| o["name"] == "OutOfClip").unwrap();
        assert_eq!(out_of_clip["quads"], 1);
        assert_eq!(out_of_clip["offscreenOverdrawViewports"], 0.005208);

        // An interior ClipsDescendants frame inside an ancestor-clipped CanvasGroup MUST still clip
        // its children and count its interior clip break.
        let interior = store_of(json!({
            "viewport": {"width": 800, "height": 600, "scale": 1.0},
            "status": "ok",
            "nodes": [
                {"id": 1, "name": "G", "path": "G", "class": "ScreenGui", "rect": [0, 0, 800, 600], "enabled": true},
                {"id": 3, "name": "Group", "path": "G.Clip.Group", "class": "CanvasGroup", "rect": [0, 0, 200, 100], "clip": [0, 0, 100, 100], "bg": {"color": [1, 1, 1], "transparency": 0}, "group": {"transparency": 0}},
                {"id": 4, "name": "InnerClipChild", "path": "G.Clip.Group.InnerClip.Child", "class": "Frame", "rect": [10, 10, 60, 60], "clip": [10, 10, 40, 40], "bg": {"color": [0, 1, 0], "transparency": 0}}
            ],
            "layers": [{"root": 1, "playerGui": true, "displayOrder": 0, "draw": [
                {"group": 3, "draw": [3, 4]}
            ]}],
            "fonts": [],
            "images": []
        }));
        let interior_rep = analyze_scene(&interior, LayoutStats::default(), None);
        // Group bg (20,000) + InnerClipChild clipped to 40x40 (1,600) = 21,600 / 480,000 = 0.045
        assert_eq!(interior_rep["totals"]["offscreenOverdrawViewports"], 0.045);
        assert_eq!(interior_rep["totals"]["batchBreaksByCause"]["clip"], 1);
    }

    #[test]
    fn canvas_group_first_child_has_no_spurious_clip_break() {
        // CanvasGroup [20, 20, 200, 100] with plain child whose exported clip equals the group's
        // rect: a scissor equal to the offscreen target's bounds changes no state, so clip breaks = 0.
        let scene = store_of(json!({
            "viewport": {"width": 800, "height": 600, "scale": 1.0},
            "status": "ok",
            "nodes": [
                {"id": 1, "name": "G", "path": "G", "class": "ScreenGui", "rect": [0, 0, 800, 600], "enabled": true},
                {"id": 10, "name": "BoxGroup", "path": "G.BoxGroup", "class": "CanvasGroup", "rect": [20, 20, 200, 100], "bg": {"color": [1, 1, 1], "transparency": 0}, "group": {"transparency": 0}},
                {"id": 11, "name": "PlainChild", "path": "G.BoxGroup.PlainChild", "class": "Frame", "rect": [30, 30, 50, 50], "clip": [20, 20, 200, 100], "bg": {"color": [0, 1, 0], "transparency": 0}}
            ],
            "layers": [{"root": 1, "playerGui": true, "displayOrder": 0, "draw": [
                {"group": 10, "draw": [10, 11]}
            ]}],
            "fonts": [],
            "images": []
        }));
        let report = analyze_scene(&scene, LayoutStats::default(), None);
        assert_eq!(report["totals"]["batchBreaksByCause"]["clip"], 0);
        assert_eq!(report["totals"]["batchBreaks"], 0);
        assert_eq!(report["totals"]["drawCalls"], 2);
        let child = report["objects"].as_array().unwrap().iter().find(|o| o["name"] == "PlainChild").unwrap();
        assert_eq!(child["batchBreaks"], 0);
        assert!(report["fidelity"]["drawCalls"].as_str().unwrap().contains("unmeasured"));
        assert!(report["fidelity"]["batchBreaks"].as_str().unwrap().contains("unmeasured"));
        assert!(!report["fidelity"]["batchBreaks"].as_str().unwrap().to_ascii_lowercase().contains("roblox"));
    }

    #[test]
    fn compare_reports_ignores_unmeasured_draw_and_batch_counts() {
        let mut before = sample_cost_report("single", "live", "ok", 1920.0, 1080.0, 1.5, true, 10, 0, 0);
        let mut after = sample_cost_report("single", "live", "ok", 1920.0, 1080.0, 1.5, true, 10, 0, 0);
        before["totals"]["drawCalls"] = json!(2);
        before["totals"]["batchBreaks"] = json!(1);
        after["totals"]["drawCalls"] = json!(8);
        after["totals"]["batchBreaks"] = json!(7);
        let diff = compare_reports(&before, &after).unwrap();
        assert_eq!(diff["verdict"], "unchanged");
        assert_eq!(diff["counts"]["delta"]["drawCalls"], 6.0);
        assert_eq!(diff["counts"]["delta"]["batchBreaks"], 6.0);
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
        // Non-scroll: 2 passes * 2.3 us + 15 records * 0.312 us = 4.6 + 4.68 = 9.28 us = 0.00928 ms
        // phone-mid derived ratio: 4.09 * (1.25..2.0) * 0.77 = 3.936625..6.2986 (~3.9x..6.3x),
        // full band: 2.65 * 1.25 * 0.77 = 2.550625 (~2.55x) .. 8.0 * 2.0 * 0.77 = 12.32 (~12.32x).
        let stats = LayoutStats {
            passes: 2,
            stored: 10,
            moved: 5,
            scroll_passes: 0,
            scroll_moved: 0,
            ..Default::default()
        };
        let pred = predicted_json(stats);
        assert_eq!(pred["desktop-ref"]["layoutCpuMs"]["basis"], "extrapolated");
        assert!((pred["desktop-ref"]["layoutCpuMs"]["ms"].as_f64().unwrap() - 0.00928).abs() < 1e-9);
        let phone = &pred["phone-mid"]["layoutCpuMs"];
        assert_eq!(phone["basis"], "derived_extrapolation");
        assert_eq!(phone["ratioBasis"], "derived");
        assert!(phone["ms"].is_null(), "derived phone layout must report a range, not a false-precision single point");
        assert!((phone["ratioRange"][0].as_f64().unwrap() - 3.9366).abs() < 1e-4);
        assert!((phone["ratioRange"][1].as_f64().unwrap() - 6.2986).abs() < 1e-4);
        assert!((phone["bandRatio"][0].as_f64().unwrap() - 2.5506).abs() < 1e-4);
        assert!((phone["bandRatio"][1].as_f64().unwrap() - 12.32).abs() < 1e-4);
        assert!((phone["rangeMs"][0].as_f64().unwrap() - 0.0365).abs() < 1e-4);
        assert!((phone["rangeMs"][1].as_f64().unwrap() - 0.0585).abs() < 1e-4);
        assert!((phone["bandMs"][0].as_f64().unwrap() - 0.0237).abs() < 1e-4);
        assert!((phone["bandMs"][1].as_f64().unwrap() - 0.1143).abs() < 1e-4);
        assert_eq!(phone["factors"]["capturedWholeUiPhoneOverDesktop"]["central"], 4.09);
        assert_eq!(phone["factors"]["captureOverheadFactor"]["basis"], "assumed_unmeasured_for_layout");
        assert_eq!(phone["factors"]["liveFrameWeight"]["value"], 0.77);
        assert_eq!(phone["factors"]["liveFrameWeight"]["basis"], "assumed_from_match_gui_weight");
        assert!(phone["settledBy"].as_str().unwrap().contains("UpdateUILayouts"));
        let phone_src = phone["source"].as_str().unwrap();
        assert!(phone_src.starts_with("Derived"), "phone source must state derived: {phone_src}");
        assert!(phone_src.contains("2.55x..12.32x"), "phone source must state full 2.55x..12.32x band: {phone_src}");
        assert!(phone_src.contains("unmeasured for layout"), "phone source must disclose unmeasured capture factor: {phone_src}");
        // Formatted summary prints derived range + 2.55x..12.32x band without 4-decimal false precision
        let summary = format_phone_range(&[0.3915, 0.6263], &[0.2536, 1.2251]);
        assert!(summary.contains("derived 3.9x..6.3x") && summary.contains("2.55x..12.32x"), "{summary}");
        assert!(summary.contains("assumed"), "formatted phone range must disclose assumed factors: {summary}");
        assert!(summary.contains("~0.39..0.63 ms") && summary.contains("0.25..1.23 ms"), "{summary}");

        // Scroll 100 cards with 2 children each (+ container = 301 records):
        // 62 us + 301 * 0.312 us = 155.912 us = 0.155912 ms (~156 us as recorded in install.rs)
        let scroll_100 = LayoutStats {
            scroll_passes: 1,
            scroll_moved: 301,
            ..Default::default()
        };
        assert!((desktop_layout_cpu_ms(scroll_100) - 0.155912).abs() < 1e-9);

        // Scroll 1,000 cards with 2 children each (+ container = 3,001 records):
        // 62 us + 3001 * 0.312 us = 998.312 us = 0.998312 ms (~998 us as recorded in install.rs)
        let scroll_1000 = LayoutStats {
            scroll_passes: 1,
            scroll_moved: 3001,
            ..Default::default()
        };
        assert!((desktop_layout_cpu_ms(scroll_1000) - 0.998312).abs() < 1e-9);

        // Two scrolls in one frame charge the 62 us base per scroll plus every moved record:
        // 2 * 62 us + 602 * 0.312 us = 311.824 us = 0.311824 ms
        let two_scrolls = LayoutStats {
            scroll_passes: 2,
            scroll_moved: 602,
            ..Default::default()
        };
        assert!((desktop_layout_cpu_ms(two_scrolls) - 0.311824).abs() < 1e-9);

        for tier in ["desktop-ref", "phone-mid"] {
            for metric in ["drawPassCpuMs", "gpuUiMs"] {
                assert_eq!(pred[tier][metric]["status"], "refused");
                assert!(pred[tier][metric]["ms"].is_null());
                assert!(pred[tier][metric]["reason"].as_str().unwrap().len() >= 12);
                assert!(pred[tier][metric]["settledBy"].as_str().unwrap().len() >= 12);
            }
        }
    }

    #[test]
    fn scroll_sequence_coalesces_per_frame_and_prices_100_cards_as_recorded() {
        let repo = crate::engine::default_repo();
        let root = repo.join("tools/tests/oracle-root");
        let tmp = std::env::temp_dir().join(format!("rur-scroll-cost-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&tmp);
        let fixture = tmp.join("scroll100.luau");
        let source = "\
return function(Target)
	local RunService = game:GetService(\"RunService\")
	local Scroller = Instance.new(\"ScrollingFrame\")
	Scroller.Size = UDim2.fromOffset(620, 500)
	Scroller.CanvasSize = UDim2.fromOffset(0, 5000)
	Scroller.Parent = Target
	local Tab = Instance.new(\"Frame\")
	Tab.BackgroundTransparency = 1
	Tab.Size = UDim2.new(1, 0, 0, 5000)
	Tab.Parent = Scroller
	for i = 1, 100 do
		local Card = Instance.new(\"Frame\")
		Card.Position = UDim2.fromOffset(0, (i - 1) * 40)
		Card.Size = UDim2.fromOffset(200, 36)
		Card.Parent = Tab
		local Main = Instance.new(\"Frame\")
		Main.Size = UDim2.fromScale(1, 1)
		Main.Parent = Card
		local Title = Instance.new(\"TextLabel\")
		Title.Size = UDim2.fromOffset(80, 20)
		Title.Text = \"\"
		Title.Parent = Card
	end
	local y = 0
	RunService.Heartbeat:Connect(function()
		y = (y + 10) % 2000
		Scroller.CanvasPosition = Vector2.new(0, y)
	end)
end
";
        std::fs::write(&fixture, source).unwrap();
        let argv: Vec<String> = vec![
            "--repo".into(), repo.to_string_lossy().into_owned(),
            "--roots".into(), root.to_string_lossy().into_owned(),
            "--fixture".into(), fixture.to_string_lossy().into_owned(),
            "--viewport".into(), "800x600".into(),
            "--scale".into(), "1".into(),
            "--no-snapshots".into(),
        ];
        let args = engine::cli::parse(&argv).unwrap();

        // Single-moment mode refuses per-frame layoutCpuMs and reports fullTreeLayoutCpuMs
        let single_run = engine::cli::run_once_with_cost(&args).unwrap();
        let mut store = SceneStore::default();
        store.apply(&single_run.scene.scene).unwrap();
        let single_rep = analyze_scene(&store, single_run.layout, Some(&single_run.dom_hidden));
        assert_eq!(single_rep["predicted"]["desktop-ref"]["layoutCpuMs"]["status"], "refused");
        assert!(single_rep["predicted"]["desktop-ref"]["layoutCpuMs"]["ms"].is_null());
        assert_eq!(single_rep["predicted"]["desktop-ref"]["fullTreeLayoutCpuMs"]["basis"], "extrapolated");
        assert!(single_rep["fullTreeLayout"]["stored"].as_u64().unwrap() >= 301);

        // Sequence mode: each steady-state 60 Hz frame applies 1 scroll moving 301 records (not 1204)
        // and prices it at 62 us + 301 * 0.312 us = 0.155912 ms
        let spec = engine::cli::SequenceSpec { from: 0.5, frames: 4, fps: 60.0, client_hz: Some(60.0), collect_cost: true };
        let seq_rep = run_sequence_cost(&args, &spec, &repo).unwrap();
        let frames = seq_rep["sequence"]["frames"].as_array().unwrap();
        for f in &frames[1..] {
            assert_eq!(f["layout"]["scrollPasses"], 1, "expected 1 scroll pass per 60 Hz frame");
            assert_eq!(f["layout"]["scrollMoved"], 301, "expected 301 records moved per scroll");
            let ms = f["predicted"]["desktop-ref"]["layoutCpuMs"]["ms"].as_f64().unwrap();
            assert!((ms - 0.155912).abs() < 1e-6, "expected 0.155912 ms, got {ms}");
            assert_eq!(f["predicted"]["desktop-ref"]["layoutCpuMs"]["basis"], "extrapolated");
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn counter_heartbeat_scroll_defaults_to_play_refuses_edit_and_compare_refuses_mismatch() {
        let repo = crate::engine::default_repo();
        let root = repo.join("tools/tests/oracle-root");
        let tmp = std::env::temp_dir().join(format!("rur-mode-cost-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&tmp);

        // 60 cards x 2 records (Card + Inner) = 120 moved records per scroll.
        // Counter-driven Heartbeat + AbsolutePosition read (prevents coalescing across beats).
        // Derived 1 client frame: 62 us + 120 * 0.312 us = 99.44 us = 0.09944 ms.
        let counter_fixture = tmp.join("counter_scroll.luau");
        let counter_src = "\
return function(Target)
	local RunService = game:GetService(\"RunService\")
	local Scroller = Instance.new(\"ScrollingFrame\")
	Scroller.Size = UDim2.fromOffset(600, 400)
	Scroller.CanvasSize = UDim2.fromOffset(0, 4000)
	Scroller.Parent = Target
	local Cards = {}
	for i = 1, 60 do
		local Card = Instance.new(\"Frame\")
		Card.Position = UDim2.fromOffset(0, (i - 1) * 50)
		Card.Size = UDim2.fromOffset(200, 40)
		Card.Parent = Scroller
		local Inner = Instance.new(\"Frame\")
		Inner.Size = UDim2.fromScale(1, 1)
		Inner.Parent = Card
		Cards[i] = Card
	end
	local step = 0
	RunService.Heartbeat:Connect(function()
		step += 1
		Scroller.CanvasPosition = Vector2.new(0, (step * 10) % 2000)
		local _ = Cards[1].AbsolutePosition
	end)
end
";
        std::fs::write(&counter_fixture, counter_src).unwrap();

        // Two scrolls per Heartbeat (write, read, write): 2 * 99.44 us = 0.19888 ms in play mode.
        let double_fixture = tmp.join("double_scroll.luau");
        let double_src = "\
return function(Target)
	local RunService = game:GetService(\"RunService\")
	local Scroller = Instance.new(\"ScrollingFrame\")
	Scroller.Size = UDim2.fromOffset(600, 400)
	Scroller.CanvasSize = UDim2.fromOffset(0, 4000)
	Scroller.Parent = Target
	local Cards = {}
	for i = 1, 60 do
		local Card = Instance.new(\"Frame\")
		Card.Position = UDim2.fromOffset(0, (i - 1) * 50)
		Card.Size = UDim2.fromOffset(200, 40)
		Card.Parent = Scroller
		local Inner = Instance.new(\"Frame\")
		Inner.Size = UDim2.fromScale(1, 1)
		Inner.Parent = Card
		Cards[i] = Card
	end
	local step = 0
	RunService.Heartbeat:Connect(function()
		step += 1
		Scroller.CanvasPosition = Vector2.new(0, (step * 20) % 2000)
		local _ = Cards[1].AbsolutePosition
		Scroller.CanvasPosition = Vector2.new(0, (step * 20 + 10) % 2000)
	end)
end
";
        std::fs::write(&double_fixture, double_src).unwrap();

        // Two writes with no read between them in one Heartbeat: must coalesce to 1 scroll pass (0.09944 ms).
        let coalesce_fixture = tmp.join("coalesce_scroll.luau");
        let coalesce_src = "\
return function(Target)
	local RunService = game:GetService(\"RunService\")
	local Scroller = Instance.new(\"ScrollingFrame\")
	Scroller.Size = UDim2.fromOffset(600, 400)
	Scroller.CanvasSize = UDim2.fromOffset(0, 4000)
	Scroller.Parent = Target
	for i = 1, 60 do
		local Card = Instance.new(\"Frame\")
		Card.Position = UDim2.fromOffset(0, (i - 1) * 50)
		Card.Size = UDim2.fromOffset(200, 40)
		Card.Parent = Scroller
		local Inner = Instance.new(\"Frame\")
		Inner.Size = UDim2.fromScale(1, 1)
		Inner.Parent = Card
	end
	local step = 0
	RunService.Heartbeat:Connect(function()
		step += 1
		Scroller.CanvasPosition = Vector2.new(0, (step * 20) % 2000)
		Scroller.CanvasPosition = Vector2.new(0, (step * 20 + 10) % 2000)
	end)
end
";
        std::fs::write(&coalesce_fixture, coalesce_src).unwrap();

        let make_args = |fixture: &Path, extra: &[&str]| {
            let mut argv: Vec<String> = vec![
                "--repo".into(), repo.to_string_lossy().into_owned(),
                "--roots".into(), root.to_string_lossy().into_owned(),
                "--fixture".into(), fixture.to_string_lossy().into_owned(),
                "--viewport".into(), "800x600".into(),
                "--scale".into(), "1".into(),
                "--no-snapshots".into(),
            ];
            argv.extend(extra.iter().map(|s| s.to_string()));
            engine::cli::parse(&argv).unwrap()
        };
        let spec = engine::cli::SequenceSpec { from: 0.5, frames: 4, fps: 60.0, client_hz: Some(60.0), collect_cost: true };

        // 1. Default run (no --mode): runs play mode (1 Heartbeat/frame) and matches client 0.09944 ms
        let default_rep = run_sequence_cost(&make_args(&counter_fixture, &[]), &spec, &repo).unwrap();
        assert_eq!(default_rep["runMode"], "play");
        assert_eq!(default_rep["runModeInfo"]["heartbeatsPerFrame"], 1);
        assert_eq!(default_rep["layout"]["scrollPasses"], 1.0);
        assert_eq!(default_rep["layout"]["scrollMoved"], 120.0);
        let desk_ms = default_rep["predicted"]["desktop-ref"]["layoutCpuMs"]["ms"].as_f64().unwrap();
        assert!((desk_ms - 0.09944).abs() < 1e-9, "expected 0.09944 ms in default play mode, got {desk_ms}");

        // 2. Explicit --mode edit: records runMode="edit" (240 Hz, 4 beats/frame) and refuses per-frame layoutCpuMs
        let edit_rep = run_sequence_cost(&make_args(&counter_fixture, &["--mode", "edit"]), &spec, &repo).unwrap();
        assert_eq!(edit_rep["runMode"], "edit");
        assert_eq!(edit_rep["runModeInfo"]["heartbeatsPerFrame"], 4);
        assert_eq!(edit_rep["layout"]["scrollPasses"], 4.0);
        assert_eq!(edit_rep["layout"]["scrollMoved"], 480.0);
        assert_eq!(edit_rep["predicted"]["desktop-ref"]["layoutCpuMs"]["status"], "refused");
        assert!(edit_rep["predicted"]["desktop-ref"]["layoutCpuMs"]["ms"].is_null());
        assert_eq!(edit_rep["predicted"]["phone-mid"]["layoutCpuMs"]["status"], "refused");
        assert!(edit_rep["predicted"]["phone-mid"]["layoutCpuMs"]["ms"].is_null());
        assert_eq!(edit_rep["sequence"]["summary"]["steadyStateLayoutCpuMs"]["status"], "refused");
        assert!(edit_rep["sequence"]["summary"]["steadyStateLayoutCpuMs"]["desktop-ref"].is_null());
        assert!(edit_rep["predicted"]["desktop-ref"]["layoutCpuMs"]["settledBy"].as_str().unwrap().contains("--mode play"));
        for f in &edit_rep["sequence"]["frames"].as_array().unwrap()[1..] {
            assert_eq!(f["predicted"]["desktop-ref"]["layoutCpuMs"]["status"], "refused");
            assert!(f["predicted"]["desktop-ref"]["layoutCpuMs"]["ms"].is_null());
        }

        // 3. Compare refuses default (play) vs --mode edit with reason and remedy
        let cmp = compare_reports(&default_rep, &edit_rep).unwrap();
        assert_eq!(cmp["verdict"], "refused");
        assert_eq!(cmp["equivalence"]["passed"], false);
        assert!(cmp["reason"].as_str().unwrap().contains("runMode"));
        assert!(cmp["remedy"].as_str().unwrap().contains("--mode"));

        // 4. Write-read-write (2 scrolls per frame): 0.19888 ms in default play mode
        let double_rep = run_sequence_cost(&make_args(&double_fixture, &[]), &spec, &repo).unwrap();
        assert_eq!(double_rep["runMode"], "play");
        assert_eq!(double_rep["layout"]["scrollPasses"], 2.0);
        assert_eq!(double_rep["layout"]["scrollMoved"], 240.0);
        let double_ms = double_rep["predicted"]["desktop-ref"]["layoutCpuMs"]["ms"].as_f64().unwrap();
        assert!((double_ms - 0.19888).abs() < 1e-9, "expected 0.19888 ms, got {double_ms}");

        // 5. Write-write without read in one Heartbeat: coalesces to 1 scroll pass (0.09944 ms)
        let coal_rep = run_sequence_cost(&make_args(&coalesce_fixture, &[]), &spec, &repo).unwrap();
        assert_eq!(coal_rep["layout"]["scrollPasses"], 1.0, "two writes without a read must coalesce");
        assert_eq!(coal_rep["layout"]["scrollMoved"], 120.0);
        let coal_ms = coal_rep["predicted"]["desktop-ref"]["layoutCpuMs"]["ms"].as_f64().unwrap();
        assert!((coal_ms - 0.09944).abs() < 1e-9, "expected 0.09944 ms, got {coal_ms}");

        let _ = std::fs::remove_dir_all(&tmp);
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
            "runMode": if input == "scene" { "scene" } else if mode == "sequence" { "play" } else { "edit" },
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

        // 8. runMode mismatch (edit vs play) -> refused on runMode
        let mut play_single = live_single.clone();
        play_single["runMode"] = json!("play");
        let ref_rm = compare_reports(&live_single, &play_single).unwrap();
        assert_eq!(ref_rm["verdict"], "refused");
        assert_eq!(ref_rm["equivalence"]["passed"], false);
        assert!(ref_rm["reason"].as_str().unwrap().contains("runMode"));
        assert!(ref_rm["remedy"].as_str().unwrap().contains("--mode"));
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
        let static_spec = engine::cli::SequenceSpec { from: 0.0, frames: 5, fps: 60.0, client_hz: Some(60.0), collect_cost: true };
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
        let anim_spec = engine::cli::SequenceSpec { from: 0.5, frames: 4, fps: 30.0, client_hz: Some(60.0), collect_cost: true };
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
            let single_run = engine::cli::run_once_with_cost(&make_args(&anim_path, &["--mode", "play", "--time", &format!("{t:.9}")])).unwrap();
            let mut store = SceneStore::default();
            store.apply(&single_run.scene.scene).unwrap();
            let single_rep = analyze_scene(&store, single_run.layout, Some(&single_run.dom_hidden));
            for key in ["drawnNodes", "quads", "glyphQuads", "drawCalls", "batchBreaks", "canvasGroupTargets", "canvasRtBytes", "screenOverdrawViewports"] {
                assert_eq!(af["totals"][key], single_rep["totals"][key], "frame {i} mismatch on {key}");
            }
        }
    }

    #[test]
    fn failing_and_empty_stories_surface_cause_refuse_zero_ms_and_note_edit_mode_fallback() {
        let repo = crate::engine::default_repo();
        let root = repo.join("tools/tests/oracle-root");
        let dir = std::env::temp_dir().join(format!("rur-cost-fail-empty-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let assert_edit = dir.join("assert_edit.luau");
        std::fs::write(
            &assert_edit,
            r#"return function(Target)
	local RunService = game:GetService("RunService")
	assert(not RunService:IsRunning(), "this story only mounts in edit mode")
	local F = Instance.new("Frame")
	F.Size = UDim2.fromOffset(100, 50)
	F.Parent = Target
end
"#,
        )
        .unwrap();

        let calls_is_edit = dir.join("calls_is_edit.luau");
        std::fs::write(
            &calls_is_edit,
            r#"return function(Target)
	local RunService = game:GetService("RunService")
	local _ = RunService:IsEdit()
	local F = Instance.new("Frame")
	F.Size = UDim2.fromOffset(100, 50)
	F.Parent = Target
end
"#,
        )
        .unwrap();

        let early_return = dir.join("early_return.luau");
        std::fs::write(
            &early_return,
            r#"return function(Target)
	local RunService = game:GetService("RunService")
	if RunService:IsRunning() then
		return
	end
	local F = Instance.new("Frame")
	F.Size = UDim2.fromOffset(100, 50)
	F.Parent = Target
end
"#,
        )
        .unwrap();

        let always_err = dir.join("always_err.luau");
        std::fs::write(
            &always_err,
            r#"return function(_Target)
	error("always broken story")
end
"#,
        )
        .unwrap();

        let make_args = |fixture: &Path, extra: &[&str]| {
            let mut argv: Vec<String> = vec![
                "--repo".into(), repo.to_string_lossy().into_owned(),
                "--roots".into(), root.to_string_lossy().into_owned(),
                "--fixture".into(), fixture.to_string_lossy().into_owned(),
                "--viewport".into(), "800x600".into(),
                "--scale".into(), "1".into(),
                "--no-snapshots".into(),
            ];
            argv.extend(extra.iter().map(|s| s.to_string()));
            engine::cli::parse(&argv).unwrap()
        };
        let seq_spec = engine::cli::SequenceSpec { from: 0.0, frames: 4, fps: 60.0, client_hz: Some(60.0), collect_cost: true };
        let dest = Path::new("out/cost.json");

        // 1. assert(not RunService:IsRunning()) -> error with cause, refused ms, and edit-mode fallback note
        let rep_assert = run_sequence_cost(&make_args(&assert_edit, &[]), &seq_spec, &repo).unwrap();
        assert_eq!(rep_assert["status"], "error");
        let assert_fail = rep_assert["failure"].as_str().expect("missing failure text");
        assert!(assert_fail.contains("this story only mounts in edit mode"), "got: {assert_fail}");
        assert_eq!(rep_assert["predicted"]["desktop-ref"]["layoutCpuMs"]["status"], "refused");
        assert!(rep_assert["predicted"]["desktop-ref"]["layoutCpuMs"]["ms"].is_null());
        assert_eq!(rep_assert["sequence"]["summary"]["steadyStateLayoutCpuMs"]["status"], "refused");
        assert!(rep_assert["sequence"]["summary"]["steadyStateLayoutCpuMs"]["desktop-ref"].is_null());
        assert_eq!(rep_assert["editModeFallback"]["mountsInEditMode"], true);
        let screen_assert = format_one_screen_report(&rep_assert, dest);
        assert!(screen_assert.contains("this story only mounts in edit mode"), "{screen_assert}");
        assert!(screen_assert.contains("--mode edit"), "{screen_assert}");
        assert!(screen_assert.contains("refused"), "{screen_assert}");
        assert!(!screen_assert.contains("0.0000 ms"), "must never print 0.0000 ms for errored story:\n{screen_assert}");

        // 2. RunService:IsEdit() -> error with capability cause and edit-mode fallback note
        let rep_isedit = run_sequence_cost(&make_args(&calls_is_edit, &[]), &seq_spec, &repo).unwrap();
        assert_eq!(rep_isedit["status"], "error");
        let isedit_fail = rep_isedit["failure"].as_str().expect("missing IsEdit failure text");
        assert!(isedit_fail.contains("IsEdit"), "got: {isedit_fail}");
        assert_eq!(rep_isedit["editModeFallback"]["mountsInEditMode"], true);

        // 3. Early return in play mode -> status "empty" (NOT "ok"), refused ms, edit-mode fallback note
        let rep_empty = run_sequence_cost(&make_args(&early_return, &[]), &seq_spec, &repo).unwrap();
        assert_eq!(rep_empty["status"], "empty", "early-return story must not be ok: {rep_empty}");
        let empty_fail = rep_empty["failure"].as_str().expect("missing empty reason");
        assert!(empty_fail.contains("mounted nothing"), "got: {empty_fail}");
        assert_eq!(rep_empty["predicted"]["desktop-ref"]["layoutCpuMs"]["status"], "refused");
        assert!(rep_empty["predicted"]["desktop-ref"]["layoutCpuMs"]["ms"].is_null());
        assert_eq!(rep_empty["editModeFallback"]["mountsInEditMode"], true);
        let screen_empty = format_one_screen_report(&rep_empty, dest);
        assert!(screen_empty.contains("rur cost: empty"), "{screen_empty}");
        assert!(screen_empty.contains("mounted nothing"), "{screen_empty}");
        assert!(screen_empty.contains("--mode edit"), "{screen_empty}");
        assert!(screen_empty.contains("per-frame layoutCpuMs is unavailable as-is"), "{screen_empty}");
        assert!(!screen_empty.contains("0.0000 ms"), "must never print 0.0000 ms for empty mount:\n{screen_empty}");

        // Compare refuses empty reports
        let cmp_empty = compare_reports(&rep_empty, &rep_empty).unwrap();
        assert_eq!(cmp_empty["verdict"], "refused");

        // 4. Story that errors in both play and edit -> surfaces cause, does NOT falsely claim edit-mode mount
        let rep_always = run_sequence_cost(&make_args(&always_err, &[]), &seq_spec, &repo).unwrap();
        assert_eq!(rep_always["status"], "error");
        assert!(rep_always["failure"].as_str().unwrap_or("").contains("always broken story"));
        assert!(rep_always.get("editModeFallback").is_none(), "must not claim edit-mode mount when edit also fails");

        // Single-moment error also surfaces cause and refuses fullTreeLayoutCpuMs
        let single_run = engine::cli::run_once_with_cost(&make_args(&always_err, &["--time", "0.05"])).unwrap();
        let mut store = SceneStore::default();
        store.apply(&single_run.scene.scene).unwrap();
        let mut single_rep = analyze_scene_with_mode(&store, single_run.layout, Some(&single_run.dom_hidden), "edit");
        let outcome = extract_run_outcome(&single_run.scene.scene);
        apply_run_outcome(&mut single_rep, &outcome, true);
        assert_eq!(single_rep["status"], "error");
        assert!(single_rep["failure"].as_str().unwrap_or("").contains("always broken story"));
        assert_eq!(single_rep["predicted"]["desktop-ref"]["fullTreeLayoutCpuMs"]["status"], "refused");
        let screen_single = format_one_screen_report(&single_rep, dest);
        assert!(screen_single.contains("always broken story"), "{screen_single}");
        assert!(!screen_single.contains("0.0000 ms"), "{screen_single}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn client_cadence_is_one_client_frame_at_30_60_120_fps_and_refuses_zero_client_frames() {
        // Review fixture: 37 cards x 3 records (Card + Inner + Title) = 111 moved records per scroll.
        // Hand figure for one client frame: 62 us + 0.312 us * 111 = 96.632 us = 0.096632 ms.
        let repo = crate::engine::default_repo();
        let root = repo.join("tools/tests/oracle-root");
        let dir = std::env::temp_dir().join(format!("rur-cost-cadence-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let fx = dir.join("scroll37_x3.luau");
        std::fs::write(
            &fx,
            r#"return function(Target)
	local RunService = game:GetService("RunService")
	local Scroller = Instance.new("ScrollingFrame")
	Scroller.Size = UDim2.fromOffset(600, 400)
	Scroller.CanvasSize = UDim2.fromOffset(0, 4000)
	Scroller.Parent = Target
	local Cards = {}
	for i = 1, 37 do
		local Card = Instance.new("Frame")
		Card.Position = UDim2.fromOffset(0, (i - 1) * 50)
		Card.Size = UDim2.fromOffset(200, 40)
		Card.Parent = Scroller
		local Inner = Instance.new("Frame")
		Inner.Size = UDim2.fromScale(1, 1)
		Inner.Parent = Card
		local Title = Instance.new("TextLabel")
		Title.Size = UDim2.fromOffset(80, 20)
		Title.Text = ""
		Title.Parent = Card
		Cards[i] = Card
	end
	local step = 0
	RunService.Heartbeat:Connect(function()
		step += 1
		Scroller.CanvasPosition = Vector2.new(0, (step * 10) % 2000)
		local _ = Cards[1].AbsolutePosition
	end)
end
"#,
        )
        .unwrap();
        let argv: Vec<String> = vec![
            "--repo".into(), repo.to_string_lossy().into_owned(),
            "--roots".into(), root.to_string_lossy().into_owned(),
            "--fixture".into(), fx.to_string_lossy().into_owned(),
            "--viewport".into(), "800x600".into(),
            "--scale".into(), "1".into(),
            "--no-snapshots".into(),
        ];
        let args = engine::cli::parse(&argv).unwrap();
        let expected_ms = 0.096632;
        let dest = Path::new("out/cost.json");

        // 1. --fps 60 (60 Hz client): 1 client frame per sample -> 1.0 sp / 111.0 sm / 0.096632 ms
        let rep60 = run_sequence_cost(
            &args,
            &engine::cli::SequenceSpec { from: 0.5, frames: 6, fps: 60.0, client_hz: Some(60.0), collect_cost: true },
            &repo,
        )
        .unwrap();
        assert_eq!(rep60["sequence"]["summary"]["clientHz"], 60.0);
        assert_eq!(rep60["sequence"]["summary"]["steadyStateClientFrames"], 5);
        assert_eq!(rep60["layout"]["scrollPasses"], 1.0);
        assert_eq!(rep60["layout"]["scrollMoved"], 111.0);
        let ms60 = rep60["predicted"]["desktop-ref"]["layoutCpuMs"]["ms"].as_f64().unwrap();
        assert!((ms60 - expected_ms).abs() < 1e-9, "fps=60 expected {expected_ms}, got {ms60}");
        let f60 = rep60["sequence"]["frames"].as_array().unwrap();
        assert_eq!(f60[0]["predicted"]["desktop-ref"]["layoutCpuMs"]["status"], "refused");
        assert_eq!(f60[1]["clientFrames"], 1);
        assert!((f60[1]["predicted"]["desktop-ref"]["layoutCpuMs"]["ms"].as_f64().unwrap() - expected_ms).abs() < 1e-9);

        // 2. --fps 30 (60 Hz client): 2 client frames per sample, normalized per 1 client frame -> 0.096632 ms
        //    Individual 2-client-frame samples refuse per-frame layoutCpuMs so they are never printed as 1 client frame.
        let rep30 = run_sequence_cost(
            &args,
            &engine::cli::SequenceSpec { from: 0.5, frames: 6, fps: 30.0, client_hz: Some(60.0), collect_cost: true },
            &repo,
        )
        .unwrap();
        assert_eq!(rep30["sequence"]["summary"]["fps"], 30.0);
        assert_eq!(rep30["sequence"]["summary"]["clientHz"], 60.0);
        assert_eq!(rep30["sequence"]["summary"]["steadyStateClientFrames"], 10);
        assert_eq!(rep30["layout"]["scrollPasses"], 1.0);
        assert_eq!(rep30["layout"]["scrollMoved"], 111.0);
        let ms30 = rep30["predicted"]["desktop-ref"]["layoutCpuMs"]["ms"].as_f64().unwrap();
        assert!((ms30 - expected_ms).abs() < 1e-9, "fps=30 expected {expected_ms}, got {ms30}");
        let f30 = rep30["sequence"]["frames"].as_array().unwrap();
        assert_eq!(f30[1]["clientFrames"], 2);
        assert_eq!(f30[1]["predicted"]["desktop-ref"]["layoutCpuMs"]["status"], "refused");
        assert!(f30[1]["predicted"]["desktop-ref"]["layoutCpuMs"]["ms"].is_null());
        let screen30 = format_one_screen_report(&rep30, dest);
        assert!(screen30.contains("30 fps") && screen30.contains("mode=play 60 Hz client"), "{screen30}");
        assert!(screen30.contains("0.0966 ms") && !screen30.contains("0.1933 ms"), "{screen30}");

        // 3. --fps 120 (60 Hz client): 0 or 1 client frames per sample, normalized per 1 client frame -> 0.096632 ms
        let rep120 = run_sequence_cost(
            &args,
            &engine::cli::SequenceSpec { from: 0.5, frames: 6, fps: 120.0, client_hz: Some(60.0), collect_cost: true },
            &repo,
        )
        .unwrap();
        assert_eq!(rep120["sequence"]["summary"]["fps"], 120.0);
        assert_eq!(rep120["sequence"]["summary"]["clientHz"], 60.0);
        assert_eq!(rep120["sequence"]["summary"]["steadyStateClientFrames"], 2);
        assert_eq!(rep120["layout"]["scrollPasses"], 1.0);
        assert_eq!(rep120["layout"]["scrollMoved"], 111.0);
        let ms120 = rep120["predicted"]["desktop-ref"]["layoutCpuMs"]["ms"].as_f64().unwrap();
        assert!((ms120 - expected_ms).abs() < 1e-9, "fps=120 expected {expected_ms}, got {ms120}");
        let f120 = rep120["sequence"]["frames"].as_array().unwrap();
        assert_eq!(f120[1]["clientFrames"], 0);
        assert_eq!(f120[1]["predicted"]["desktop-ref"]["layoutCpuMs"]["status"], "refused");
        assert_eq!(f120[2]["clientFrames"], 1);
        assert!((f120[2]["predicted"]["desktop-ref"]["layoutCpuMs"]["ms"].as_f64().unwrap() - expected_ms).abs() < 1e-9);

        // 4. --fps 120 --client-hz 120: explicitly runs 120 Hz client cadence and labels it 120 Hz
        let rep120_hz = run_sequence_cost(
            &args,
            &engine::cli::SequenceSpec { from: 0.5, frames: 6, fps: 120.0, client_hz: Some(120.0), collect_cost: true },
            &repo,
        )
        .unwrap();
        assert_eq!(rep120_hz["sequence"]["summary"]["clientHz"], 120.0);
        assert_eq!(rep120_hz["sequence"]["summary"]["steadyStateClientFrames"], 5);
        assert_eq!(rep120_hz["layout"]["scrollPasses"], 1.0);
        assert_eq!(rep120_hz["layout"]["scrollMoved"], 111.0);
        let ms120_hz = rep120_hz["predicted"]["desktop-ref"]["layoutCpuMs"]["ms"].as_f64().unwrap();
        assert!((ms120_hz - expected_ms).abs() < 1e-9);
        let screen120_hz = format_one_screen_report(&rep120_hz, dest);
        assert!(screen120_hz.contains("mode=play 120 Hz client"), "{screen120_hz}");
        assert!(!screen120_hz.contains("mode=play 60 Hz"), "{screen120_hz}");
        let cmp_hz = compare_reports(&rep120, &rep120_hz).unwrap();
        assert_eq!(cmp_hz["verdict"], "refused");
        assert!(cmp_hz["reason"].as_str().unwrap().contains("clientHz"));

        // 5. --fps 120 --frames 2 (60 Hz client -> 0 steady client frames): must refuse layoutCpuMs, never 0.0000 ms
        let rep_zero = run_sequence_cost(
            &args,
            &engine::cli::SequenceSpec { from: 0.5, frames: 2, fps: 120.0, client_hz: Some(60.0), collect_cost: true },
            &repo,
        )
        .unwrap();
        assert_eq!(rep_zero["sequence"]["summary"]["steadyStateClientFrames"], 0);
        assert_eq!(rep_zero["predicted"]["desktop-ref"]["layoutCpuMs"]["status"], "refused");
        assert!(rep_zero["predicted"]["desktop-ref"]["layoutCpuMs"]["ms"].is_null());
        assert_eq!(rep_zero["sequence"]["summary"]["steadyStateLayoutCpuMs"]["status"], "refused");
        let screen_zero = format_one_screen_report(&rep_zero, dest);
        assert!(screen_zero.contains("refused") && !screen_zero.contains("0.0000 ms"), "{screen_zero}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    fn run_cg_shape_fixture(name: &str, src: &str) -> Value {
        let repo = crate::engine::default_repo();
        let root = repo.join("tools/tests/oracle-root");
        let dir = std::env::temp_dir().join(format!("rur-cost-cg-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let fx = dir.join(format!("{name}.luau"));
        std::fs::write(&fx, src).unwrap();
        let argv: Vec<String> = vec![
            "--repo".into(), repo.to_string_lossy().into_owned(),
            "--roots".into(), root.to_string_lossy().into_owned(),
            "--fixture".into(), fx.to_string_lossy().into_owned(),
            "--viewport".into(), "800x600".into(),
            "--scale".into(), "1".into(),
            "--no-snapshots".into(),
        ];
        let args = engine::cli::parse(&argv).unwrap();
        let run = engine::cli::run_once_with_cost(&args).unwrap();
        let mut store = SceneStore::default();
        store.apply(&run.scene.scene).unwrap();
        let rep = analyze_scene_with_mode(&store, run.layout, Some(&run.dom_hidden), "edit");
        let _ = std::fs::remove_dir_all(&dir);
        rep
    }

    #[test]
    fn canvas_group_nested_interior_clip() {
        // Shape 1: ClipA(100x100) -> Outer CG(200x100) -> ClipB(150x50) -> Inner CG(200x100) -> Child(150,50,50,50).
        // Outer bg 20,000 + Inner bg 20,000 + Child unclipped 2,500 + Inner composite clipped to ClipB 7,500 = 50,000 px.
        let nested = run_cg_shape_fixture(
            "nested_clips",
            r#"return function(Target)
	local ClipA = Instance.new("Frame")
	ClipA.Size = UDim2.fromOffset(100, 100)
	ClipA.BackgroundTransparency = 1
	ClipA.BorderSizePixel = 0
	ClipA.ClipsDescendants = true
	ClipA.Parent = Target
	local Outer = Instance.new("CanvasGroup")
	Outer.Size = UDim2.fromOffset(200, 100)
	Outer.BorderSizePixel = 0
	Outer.Parent = ClipA
	local ClipB = Instance.new("Frame")
	ClipB.Size = UDim2.fromOffset(150, 50)
	ClipB.BackgroundTransparency = 1
	ClipB.BorderSizePixel = 0
	ClipB.ClipsDescendants = true
	ClipB.Parent = Outer
	local Inner = Instance.new("CanvasGroup")
	Inner.Size = UDim2.fromOffset(200, 100)
	Inner.BorderSizePixel = 0
	Inner.Parent = ClipB
	local Child = Instance.new("Frame")
	Child.Position = UDim2.fromOffset(150, 50)
	Child.Size = UDim2.fromOffset(50, 50)
	Child.BorderSizePixel = 0
	Child.Parent = Inner
end
"#,
        );
        assert_eq!(nested["totals"]["canvasRtBytes"], 160_000);
        assert_eq!(nested["totals"]["screenOverdrawViewports"], 0.020833);
        assert_eq!(
            nested["totals"]["offscreenOverdrawViewports"],
            0.104167,
            "nested ClipB composite must be 50,000 / 480,000 = 0.104167, got {:?}",
            nested["totals"]["offscreenOverdrawViewports"]
        );
    }

    #[test]
    fn canvas_group_shared_edge_interior_clip() {
        // Shape 2: ClipA(100x100) -> Group CG(200x100) -> InteriorClip(100x100) -> Child(200x50).
        // Group bg 20,000 + Child clipped by InteriorClip 5,000 = 25,000 px (not stripped to 30,000).
        let shared_edge = run_cg_shape_fixture(
            "shared_edge",
            r#"return function(Target)
	local ClipA = Instance.new("Frame")
	ClipA.Size = UDim2.fromOffset(100, 100)
	ClipA.BackgroundTransparency = 1
	ClipA.BorderSizePixel = 0
	ClipA.ClipsDescendants = true
	ClipA.Parent = Target
	local Group = Instance.new("CanvasGroup")
	Group.Size = UDim2.fromOffset(200, 100)
	Group.BorderSizePixel = 0
	Group.Parent = ClipA
	local InteriorClip = Instance.new("Frame")
	InteriorClip.Size = UDim2.fromOffset(100, 100)
	InteriorClip.BackgroundTransparency = 1
	InteriorClip.BorderSizePixel = 0
	InteriorClip.ClipsDescendants = true
	InteriorClip.Parent = Group
	local Child = Instance.new("Frame")
	Child.Size = UDim2.fromOffset(200, 50)
	Child.BorderSizePixel = 0
	Child.Parent = InteriorClip
end
"#,
        );
        assert_eq!(shared_edge["totals"]["canvasRtBytes"], 80_000);
        assert_eq!(shared_edge["totals"]["screenOverdrawViewports"], 0.020833);
        assert_eq!(
            shared_edge["totals"]["offscreenOverdrawViewports"],
            0.052083,
            "interior clip sharing edge x=100 with outer clip must keep Child clipped to 5,000 px (25,000 / 480,000 = 0.052083), got {:?}",
            shared_edge["totals"]["offscreenOverdrawViewports"]
        );
    }

    #[test]
    fn canvas_group_size_cap_texel_area() {
        // Shape 3: Size-capped CanvasGroup (4096x512 -> 2048x256 RT, texel scale 0.25) with 100x100 child:
        // RT bg 524,288 + Child 2,500 = 526,788 texels (1.097475x viewport, not 4.3899x).
        let size_cap = run_cg_shape_fixture(
            "size_cap",
            r#"return function(Target)
	local Group = Instance.new("CanvasGroup")
	Group.Size = UDim2.fromOffset(4096, 512)
	Group.BorderSizePixel = 0
	Group.Parent = Target
	local Child = Instance.new("Frame")
	Child.Size = UDim2.fromOffset(100, 100)
	Child.BorderSizePixel = 0
	Child.Parent = Group
end
"#,
        );
        assert_eq!(size_cap["totals"]["canvasRtBytes"], 2_097_152);
        assert_eq!(size_cap["totals"]["screenOverdrawViewports"], 0.853333);
        assert_eq!(
            size_cap["totals"]["offscreenOverdrawViewports"],
            1.097475,
            "size-capped 4096x512 -> 2048x256 RT must count 526,788 texels (1.097475x viewport), got {:?}",
            size_cap["totals"]["offscreenOverdrawViewports"]
        );
    }

    #[test]
    fn nested_canvas_group_composite_strips_outer_ancestor_clip() {
        // Guards the composite-onto-parent-CanvasGroup rule on both --scene and live DOM:
        // ClipA(100x100) -> Outer CG(200x100) -> Inner CG(200x100) with no interior clip.
        // Inner composites onto Outer's 200x100 RT unclipped by ClipA (20,000 px), giving
        // 20,000 (Outer bg) + 20,000 (Inner bg) + 20,000 (Inner composite) = 60,000 / 480,000 = 0.125.
        // Using Outer's raw ancestor clip on Inner's composite would drop Inner composite to 10,000 (0.104167).
        let scene = store_of(json!({
            "viewport": {"width": 800, "height": 600, "scale": 1.0},
            "status": "ok",
            "nodes": [
                {"id": 1, "name": "G", "path": "G", "class": "ScreenGui", "rect": [0, 0, 800, 600], "enabled": true},
                {"id": 2, "name": "ClipA", "path": "G.ClipA", "class": "Frame", "rect": [0, 0, 100, 100], "bg": {"color": [0, 0, 0], "transparency": 1}},
                {"id": 3, "name": "Outer", "path": "G.ClipA.Outer", "class": "CanvasGroup", "rect": [0, 0, 200, 100], "clip": [0, 0, 100, 100], "bg": {"color": [1, 1, 1], "transparency": 0}, "group": {"transparency": 0}},
                {"id": 4, "name": "Inner", "path": "G.ClipA.Outer.Inner", "class": "CanvasGroup", "rect": [0, 0, 200, 100], "clip": [0, 0, 100, 100], "bg": {"color": [0, 1, 0], "transparency": 0}, "group": {"transparency": 0}}
            ],
            "layers": [{"root": 1, "playerGui": true, "displayOrder": 0, "draw": [
                2,
                {"group": 3, "draw": [3, {"group": 4, "draw": [4]}]}
            ]}],
            "fonts": [],
            "images": []
        }));
        let scene_rep = analyze_scene(&scene, LayoutStats::default(), None);
        assert_eq!(scene_rep["totals"]["offscreenOverdrawViewports"], 0.125);
        assert_eq!(scene_rep["totals"]["screenOverdrawViewports"], 0.020833);

        // Live DOM path with complete clip_rects / parent_of agrees identically:
        let mut hidden = DomHiddenSummary {
            complete: true,
            ..Default::default()
        };
        hidden.parent_of.insert(2, 1);
        hidden.parent_of.insert(3, 2);
        hidden.parent_of.insert(4, 3);
        hidden.clip_rects.insert(2, [0.0, 0.0, 100.0, 100.0]);
        hidden.clip_rects.insert(3, [0.0, 0.0, 200.0, 100.0]);
        hidden.clip_rects.insert(4, [0.0, 0.0, 200.0, 100.0]);
        let live_rep = analyze_scene_with_mode(&scene, LayoutStats::default(), Some(&hidden), "edit");
        assert_eq!(live_rep["totals"]["offscreenOverdrawViewports"], 0.125);
        assert_eq!(live_rep["totals"]["screenOverdrawViewports"], 0.020833);
    }

    #[test]
    fn screen_disambiguates_repeating_paths_and_names_layout_drivers() {
        // Repeating sibling cards under a scrolling frame: top objs lines must each identify a
        // distinct object (#index + class + @x,y) and top layout must name the ScrollingFrame driver.
        let repo = crate::engine::default_repo();
        let root = repo.join("tools/tests/oracle-root");
        let dir = std::env::temp_dir().join(format!("rur-cost-screen-ids-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let fx = dir.join("cards_scroll.luau");
        std::fs::write(
            &fx,
            r#"return function(Target)
	local RunService = game:GetService("RunService")
	local Scroller = Instance.new("ScrollingFrame")
	Scroller.Name = "CardList"
	Scroller.Size = UDim2.fromOffset(400, 400)
	Scroller.CanvasSize = UDim2.fromOffset(0, 2000)
	Scroller.Parent = Target
	local Cards = {}
	for i = 1, 10 do
		local Card = Instance.new("Frame")
		Card.Name = "Card"
		Card.Position = UDim2.fromOffset(0, (i - 1) * 50)
		Card.Size = UDim2.fromOffset(200, 40)
		Card.Parent = Scroller
		Cards[i] = Card
	end
	local step = 0
	RunService.Heartbeat:Connect(function()
		step += 1
		Scroller.CanvasPosition = Vector2.new(0, (step * 5) % 500)
		local _ = Cards[1].AbsolutePosition
	end)
end
"#,
        )
        .unwrap();
        let argv: Vec<String> = vec![
            "--repo".into(), repo.to_string_lossy().into_owned(),
            "--roots".into(), root.to_string_lossy().into_owned(),
            "--fixture".into(), fx.to_string_lossy().into_owned(),
            "--viewport".into(), "800x600".into(),
            "--scale".into(), "1".into(),
            "--no-snapshots".into(),
        ];
        let args = engine::cli::parse(&argv).unwrap();
        let rep = run_sequence_cost(
            &args,
            &engine::cli::SequenceSpec { from: 0.0, frames: 5, fps: 60.0, client_hz: Some(60.0), collect_cost: true },
            &repo,
        )
        .unwrap();
        let drivers = rep["layoutDrivers"].as_array().expect("layoutDrivers array");
        assert!(!drivers.is_empty(), "scrolling sequence must attribute layout drivers: {rep}");
        assert_eq!(drivers[0]["name"], "CardList");
        assert_eq!(drivers[0]["class"], "ScrollingFrame");
        assert_eq!(drivers[0]["scrollPasses"], 1.0);
        assert_eq!(drivers[0]["scrollMoved"], 10.0);

        let screen = format_one_screen_report(&rep, Path::new("out/cost.json"));
        let obj_lines: Vec<&str> = screen
            .lines()
            .skip_while(|l| !l.trim_start().starts_with("top objs"))
            .skip(1)
            .take_while(|l| l.starts_with("    "))
            .collect();
        assert!(obj_lines.len() >= 3, "expected 3 top objs lines:\n{screen}");
        let card_lines: Vec<&&str> = obj_lines.iter().filter(|l| l.contains(".Card ")).collect();
        assert!(card_lines.len() >= 2, "expected repeating Card lines:\n{screen}");
        assert_ne!(card_lines[0], card_lines[1], "repeating sibling paths must be disambiguated:\n{screen}");
        assert!(card_lines.iter().all(|l| l.contains('#') && l.contains('@')), "expected #index and @x,y:\n{screen}");
        assert!(screen.contains("top layout (1/1, 100% of churn):"), "screen must show top layout section with share:\n{screen}");
        assert!(screen.contains("CardList (ScrollingFrame)") && screen.contains("1.0sp/10.0sm"), "screen must name ScrollingFrame driver:\n{screen}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn top_layout_names_changed_objects_with_ancestor_share_and_counts_all_drivers() {
        // Hand case: 5 Row frames, each with an Icon whose Size changes every frame.
        // Total stored per frame = 11 (StoryTarget + 5 Rows + 5 Icons).
        // `top layout` must list ONLY the 5 changed Icons (never StoryTarget or Rows), each with
        // its exact share of the stored records (1 own + 1 Row + 1/5 StoryTarget = 2.2 stored),
        // disambiguated by full-path ordinal (#1..#5) and position, with 100% share.
        let repo = crate::engine::default_repo();
        let root = repo.join("tools/tests/oracle-root");
        let dir = std::env::temp_dir().join(format!("rur-cost-layout-drivers-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let rows_fx = dir.join("five_rows_icons.luau");
        std::fs::write(
            &rows_fx,
            r#"return function(Target)
	local RunService = game:GetService("RunService")
	local icons = {}
	for i = 1, 5 do
		local Row = Instance.new("Frame")
		Row.Name = "Row"
		Row.Position = UDim2.fromOffset(0, (i - 1) * 60)
		Row.Size = UDim2.fromOffset(300, 50)
		Row.Parent = Target
		local Icon = Instance.new("Frame")
		Icon.Name = "Icon"
		Icon.Size = UDim2.fromOffset(30, 30)
		Icon.Parent = Row
		icons[i] = Icon
	end
	local step = 0
	RunService.Heartbeat:Connect(function()
		step += 1
		local sz = 20 + (step % 20)
		for i = 1, 5 do
			icons[i].Size = UDim2.fromOffset(sz, sz)
		end
	end)
end
"#,
        )
        .unwrap();

        // 12 changing bars (> 10) to verify totalDrivers is counted before truncation and share reflects shown/total
        let twelve_fx = dir.join("twelve_bars.luau");
        std::fs::write(
            &twelve_fx,
            r#"return function(Target)
	local RunService = game:GetService("RunService")
	local bars = {}
	for i = 1, 12 do
		local Bar = Instance.new("Frame")
		Bar.Name = "Bar"
		Bar.Position = UDim2.fromOffset((i - 1) * 20, 0)
		Bar.Size = UDim2.fromOffset(16, 40)
		Bar.Parent = Target
		bars[i] = Bar
	end
	local step = 0
	RunService.Heartbeat:Connect(function()
		step += 1
		local h = 20 + (step % 30)
		for i = 1, 12 do
			bars[i].Size = UDim2.fromOffset(16, h)
		end
	end)
end
"#,
        )
        .unwrap();

        // Placement-property change (Rotation, as spinning rings): must name the rotating children, not parent
        let rot_fx = dir.join("rotating_rings.luau");
        std::fs::write(
            &rot_fx,
            r#"return function(Target)
	local RunService = game:GetService("RunService")
	local Holder = Instance.new("Frame")
	Holder.Name = "Holder"
	Holder.Size = UDim2.fromOffset(200, 200)
	Holder.Parent = Target
	local rings = {}
	for i = 1, 3 do
		local Ring = Instance.new("Frame")
		Ring.Name = "Ring" .. tostring(i)
		Ring.Size = UDim2.fromOffset(40, 40)
		Ring.Parent = Holder
		rings[i] = Ring
	end
	local step = 0
	RunService.Heartbeat:Connect(function()
		step += 1
		for i = 1, 3 do
			rings[i].Rotation = (step * (3 + i)) % 360
		end
	end)
end
"#,
        )
        .unwrap();

        let make_args = |fixture: &Path| {
            let argv: Vec<String> = vec![
                "--repo".into(), repo.to_string_lossy().into_owned(),
                "--roots".into(), root.to_string_lossy().into_owned(),
                "--fixture".into(), fixture.to_string_lossy().into_owned(),
                "--viewport".into(), "800x600".into(),
                "--scale".into(), "1".into(),
                "--no-snapshots".into(),
            ];
            engine::cli::parse(&argv).unwrap()
        };
        let spec = engine::cli::SequenceSpec { from: 0.5, frames: 6, fps: 60.0, client_hz: Some(60.0), collect_cost: true };

        // 1. Hand case: 5 Rows x 1 changing Icon
        let rep = run_sequence_cost(&make_args(&rows_fx), &spec, &repo).unwrap();
        assert_eq!(rep["layout"]["stored"], 11.0);
        assert_eq!(rep["layoutDriverSummary"]["totalDrivers"], 5);
        assert_eq!(rep["layoutDriverSummary"]["reportedDrivers"], 5);
        let drivers = rep["layoutDrivers"].as_array().unwrap();
        assert_eq!(drivers.len(), 5, "expected only the 5 changed Icons, got: {drivers:?}");
        let sum_stored: f64 = drivers.iter().map(|d| d["stored"].as_f64().unwrap()).sum();
        assert!((sum_stored - 11.0).abs() < 1e-4, "driver stored shares must sum to total stored 11.0, got {sum_stored}");
        for (idx, d) in drivers.iter().enumerate() {
            assert_eq!(d["name"], "Icon", "driver {idx} must be Icon, got {d}");
            assert_eq!(d["pathCount"], 5, "all 5 Icons share path and must have pathCount=5: {d}");
            assert_eq!(d["siblingIndex"], (idx + 1) as u64, "distinct per-path ordinal expected: {d}");
            let s = d["stored"].as_f64().unwrap();
            assert!((s - 2.2).abs() < 1e-4, "each Icon causes 1 + 1 (Row) + 0.2 (StoryTarget) = 2.2 stored, got {s}");
        }
        let screen = format_one_screen_report(&rep, Path::new("out/cost.json"));
        assert!(screen.contains("top layout (5/5, 100% of churn):"), "{screen}");
        let layout_lines: Vec<&str> = screen
            .lines()
            .skip_while(|l| !l.trim_start().starts_with("top layout"))
            .skip(1)
            .take_while(|l| l.starts_with("    "))
            .collect();
        assert_eq!(layout_lines.len(), 5, "must print all 5 Icons:\n{screen}");
        let unique: std::collections::HashSet<&str> = layout_lines.iter().copied().collect();
        assert_eq!(unique.len(), 5, "all 5 Icon lines must be distinct:\n{screen}");
        for (i, line) in layout_lines.iter().enumerate() {
            assert!(line.contains(".Row.Icon ") && line.contains(&format!("#{}", i + 1)) && line.contains(&format!("@0,{}", i * 60)), "bad line {i}: {line}");
            assert!(line.contains("2.2s/"), "expected 2.2s share on line {i}: {line}");
        }

        // 2. 12 changing Bars: totalDrivers=12 (not truncated to 10), screen prints 5/12 (~42% of churn)
        let rep12 = run_sequence_cost(&make_args(&twelve_fx), &spec, &repo).unwrap();
        assert_eq!(rep12["layout"]["stored"], 13.0);
        assert_eq!(rep12["layoutDriverSummary"]["totalDrivers"], 12);
        let d12 = rep12["layoutDrivers"].as_array().unwrap();
        assert_eq!(d12.len(), 12);
        let sum12: f64 = d12.iter().map(|d| d["stored"].as_f64().unwrap()).sum();
        assert!((sum12 - 13.0).abs() < 1e-4, "12 bars stored must sum to 13.0, got {sum12}");
        let screen12 = format_one_screen_report(&rep12, Path::new("out/cost.json"));
        assert!(screen12.contains("top layout (5/12, 42% of churn):"), "{screen12}");

        // 3. Rotating rings (placement property): names Ring1, Ring2, Ring3, NOT Holder or StoryTarget
        let rep_rot = run_sequence_cost(&make_args(&rot_fx), &spec, &repo).unwrap();
        assert_eq!(rep_rot["layoutDriverSummary"]["totalDrivers"], 3);
        let d_rot = rep_rot["layoutDrivers"].as_array().unwrap();
        let names: Vec<&str> = d_rot.iter().map(|d| d["name"].as_str().unwrap()).collect();
        assert_eq!(names, vec!["Ring1", "Ring2", "Ring3"], "expected the 3 rotating rings: {d_rot:?}");
        let sum_rot: f64 = d_rot.iter().map(|d| d["stored"].as_f64().unwrap()).sum();
        let total_rot = rep_rot["layout"]["stored"].as_f64().unwrap();
        assert!((sum_rot - total_rot).abs() < 1e-4, "ring shares {sum_rot} must equal total stored {total_rot}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn edit_only_dead_end_yielded_location_inline_loop_and_nested_error() {
        let repo = crate::engine::default_repo();
        let root = repo.join("tools/tests/oracle-root");
        let dir = std::env::temp_dir().join(format!("rur-cost-finding2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // 1. Edit-only story that yields on WaitForChild in play mode and mounts cleanly in edit mode
        let edit_only_wait = dir.join("edit_only_wait.luau");
        std::fs::write(
            &edit_only_wait,
            r#"return function(Target)
	local RunService = game:GetService("RunService")
	if RunService:IsRunning() then
		game:GetService("ReplicatedStorage"):WaitForChild("NeverArrivesInPlay")
	end
	local F = Instance.new("Frame")
	F.Size = UDim2.fromOffset(100, 50)
	F.Parent = Target
end
"#,
        )
        .unwrap();

        // 2. Mount-then-loop story: draws UI then stays yielded in an inline loop
        let inline_loop = dir.join("inline_loop.luau");
        std::fs::write(
            &inline_loop,
            r#"return function(Target)
	local F = Instance.new("Frame")
	F.Size = UDim2.fromOffset(120, 40)
	F.Parent = Target
	local step = 0
	while true do
		step += 1
		F.Position = UDim2.fromOffset(step % 50, 0)
		task.wait()
	end
end
"#,
        )
        .unwrap();

        // 3. Playable story to verify --mode edit STILL recommends --mode play when play actually mounts
        let playable = dir.join("playable.luau");
        std::fs::write(
            &playable,
            r#"return function(Target)
	local F = Instance.new("Frame")
	F.Size = UDim2.fromOffset(80, 40)
	F.Parent = Target
end
"#,
        )
        .unwrap();

        // 4. Delayed edit-only mount (only mounts after t = 0.25) to verify edit probe uses --from, not 0.05
        let delayed_edit = dir.join("delayed_edit.luau");
        std::fs::write(
            &delayed_edit,
            r#"return function(Target)
	local RunService = game:GetService("RunService")
	if RunService:IsRunning() then return end
	task.delay(0.25, function()
		local F = Instance.new("Frame")
		F.Size = UDim2.fromOffset(60, 60)
		F.Parent = Target
	end)
end
"#,
        )
        .unwrap();

        let make_args = |fixture: &Path, extra: &[&str]| {
            let mut argv: Vec<String> = vec![
                "--repo".into(), repo.to_string_lossy().into_owned(),
                "--roots".into(), root.to_string_lossy().into_owned(),
                "--fixture".into(), fixture.to_string_lossy().into_owned(),
                "--viewport".into(), "800x600".into(),
                "--scale".into(), "1".into(),
                "--no-snapshots".into(),
            ];
            argv.extend(extra.iter().map(|s| s.to_string()));
            engine::cli::parse(&argv).unwrap()
        };
        let dest = Path::new("out/cost.json");
        let spec = engine::cli::SequenceSpec { from: 0.0, frames: 4, fps: 60.0, client_hz: Some(60.0), collect_cost: true };

        // Case 1a: Play sequence on edit-only wait story prints exact file:line waiting location + end-state note
        let rep_play = run_sequence_cost(&make_args(&edit_only_wait, &[]), &spec, &repo).unwrap();
        assert_eq!(rep_play["status"], "yielded");
        let waiting_at = rep_play["waitingAt"].as_str().expect("yielded report must carry waitingAt");
        assert!(waiting_at.contains("edit_only_wait") && waiting_at.contains(":4"), "expected file:line in waitingAt, got: {waiting_at}");
        assert!(rep_play["failure"].as_str().unwrap_or("").contains(":4"), "{rep_play}");
        assert_eq!(rep_play["editModeFallback"]["mountsInEditMode"], true);
        let screen_play = format_one_screen_report(&rep_play, dest);
        assert!(screen_play.contains("edit_only_wait") && screen_play.contains(":4"), "play screen must print wait location:\n{screen_play}");
        assert!(screen_play.contains("unavailable as-is"), "play screen must state end state:\n{screen_play}");

        // Case 1b: --mode edit sequence on edit-only story must NOT say "pass --mode play" (no circular remedy)
        let rep_edit = run_sequence_cost(&make_args(&edit_only_wait, &["--mode", "edit"]), &spec, &repo).unwrap();
        assert_eq!(rep_edit["status"], "ok");
        assert_eq!(rep_edit["playModeProbe"]["mountsInPlayMode"], false);
        let probe_wait = rep_edit["playModeProbe"]["waitingAt"].as_str().unwrap_or("");
        assert!(probe_wait.contains("edit_only_wait") && probe_wait.contains(":4"), "got: {probe_wait}");
        let screen_edit = format_one_screen_report(&rep_edit, dest);
        assert!(screen_edit.contains("unavailable as-is"), "edit screen must state end state:\n{screen_edit}");
        assert!(screen_edit.contains("edit_only_wait") && screen_edit.contains(":4"), "edit screen must name play-mode wait to fix:\n{screen_edit}");
        assert!(!screen_edit.contains("pass --mode play"), "edit-only screen must not loop back to --mode play:\n{screen_edit}");
        let settled_by = rep_edit["predicted"]["desktop-ref"]["layoutCpuMs"]["settledBy"].as_str().unwrap_or("");
        assert!(!settled_by.contains("Re-run with --mode play"), "settledBy must not loop back to --mode play: {settled_by}");

        // Case 1c: Playable story run in --mode edit still points to --mode play
        let rep_playable_edit = run_sequence_cost(&make_args(&playable, &["--mode", "edit"]), &spec, &repo).unwrap();
        assert_eq!(rep_playable_edit["playModeProbe"]["mountsInPlayMode"], true);
        let screen_playable_edit = format_one_screen_report(&rep_playable_edit, dest);
        assert!(screen_playable_edit.contains("pass --mode play"), "{screen_playable_edit}");

        // Case 2: Mount-then-loop draws UI then yields -> never claims "zero counts", names task.spawn remedy
        let rep_loop = run_sequence_cost(&make_args(&inline_loop, &[]), &spec, &repo).unwrap();
        assert_eq!(rep_loop["status"], "yielded");
        assert!(rep_loop["totals"]["drawnNodes"].as_u64().unwrap_or(0) > 0, "inline loop drew UI: {rep_loop}");
        let loop_wait = rep_loop["waitingAt"].as_str().unwrap_or("");
        assert!(loop_wait.contains("inline_loop") && loop_wait.contains(":9"), "got: {loop_wait}");
        let screen_loop = format_one_screen_report(&rep_loop, dest);
        assert!(!screen_loop.contains("zero counts"), "drawn-then-yielded screen must not claim zero counts:\n{screen_loop}");
        assert!(screen_loop.contains("task.spawn"), "drawn-then-yielded screen must name spawn remedy:\n{screen_loop}");
        let loop_reason = rep_loop["predicted"]["desktop-ref"]["layoutCpuMs"]["reason"].as_str().unwrap_or("");
        assert!(!loop_reason.contains("zero counts"), "reason must not claim zero counts: {loop_reason}");

        // Case 3: Nested wrapper error (Vide root -> effect -> branch) surfaces root cause and user frame
        let nested_failure = "error while running root():\n\neffect error:\nerror while running branch():\n\nattempt to call a nil value\nReplicatedStorage.Packages.vide.untrack:6 function traceback\nReplicatedStorage.Packages.vide.untrack:17 function untrack\nReplicatedStorage.Stories.Panel.story:604 function BuildPanelState\n";
        let (cause, ctx) = summarize_failure_lines(nested_failure);
        assert_eq!(cause, "attempt to call a nil value");
        assert_eq!(ctx.as_deref(), Some("ReplicatedStorage.Stories.Panel.story:604 function BuildPanelState"));
        let mut fake = analyze_scene(&SceneStore::default(), LayoutStats::default(), None);
        let outcome = RunOutcome {
            status: Some("error".into()),
            failure: Some(nested_failure.into()),
            ..Default::default()
        };
        apply_run_outcome(&mut fake, &outcome, false);
        let screen_nested = format_one_screen_report(&fake, dest);
        assert!(screen_nested.contains("attempt to call a nil value"), "{screen_nested}");
        assert!(screen_nested.contains("Panel.story:604"), "{screen_nested}");
        assert!(!screen_nested.contains("error while running root():"), "{screen_nested}");
        let nested_reason = fake["predicted"]["desktop-ref"]["layoutCpuMs"]["reason"].as_str().unwrap_or("");
        assert!(nested_reason.contains("attempt to call a nil value"), "{nested_reason}");

        // Case 4: Delayed edit-only mount probed at --from (0.5), not hardcoded 0.05
        let delayed_spec = engine::cli::SequenceSpec { from: 0.5, frames: 4, fps: 60.0, client_hz: Some(60.0), collect_cost: true };
        let rep_delayed = run_sequence_cost(&make_args(&delayed_edit, &[]), &delayed_spec, &repo).unwrap();
        assert_eq!(rep_delayed["status"], "empty");
        assert_eq!(rep_delayed["editModeFallback"]["mountsInEditMode"], true, "edit probe must run at --from (0.5)");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rotated_quad_area_is_clipped_polygon_not_bounding_box() {
        let vp = 800.0 * 600.0;

        // Case 1: 100x100 Frame at Rotation = 45 on 800x600 @ 1x -> 10,000 px (not 20,000 bbox)
        let rep1 = run_cg_shape_fixture(
            "rot45",
            r#"return function(Target)
	local f = Instance.new("Frame")
	f.Name = "Box45"
	f.Size = UDim2.fromOffset(100, 100)
	f.Position = UDim2.fromOffset(200, 200)
	f.Rotation = 45
	f.BorderSizePixel = 0
	f.Parent = Target
end
"#,
        );
        let ov1 = rep1["totals"]["screenOverdrawViewports"].as_f64().unwrap();
        assert!((ov1 * vp - 10_000.0).abs() < 1.0, "100x100 @ 45 deg must be 10,000 px, got {}", ov1 * vp);

        // Case 2: 200x200 Frame under 15-deg ancestor -> 40,000 px on screen AND inside a CanvasGroup
        let rep2_screen = run_cg_shape_fixture(
            "rot15_screen",
            r#"return function(Target)
	local anc = Instance.new("Frame")
	anc.Size = UDim2.fromOffset(400, 400)
	anc.Position = UDim2.fromOffset(200, 100)
	anc.Rotation = 15
	anc.BackgroundTransparency = 1
	anc.BorderSizePixel = 0
	anc.Parent = Target
	local f = Instance.new("Frame")
	f.Name = "Child200"
	f.Size = UDim2.fromOffset(200, 200)
	f.Position = UDim2.fromOffset(100, 100)
	f.BorderSizePixel = 0
	f.Parent = anc
end
"#,
        );
        let ov2_s = rep2_screen["totals"]["screenOverdrawViewports"].as_f64().unwrap();
        assert!((ov2_s * vp - 40_000.0).abs() < 1.0, "200x200 @ 15 deg on screen must be 40,000 px, got {}", ov2_s * vp);

        let rep2_group = run_cg_shape_fixture(
            "rot15_group",
            r#"return function(Target)
	local grp = Instance.new("CanvasGroup")
	grp.Name = "Grp"
	grp.Size = UDim2.fromOffset(600, 500)
	grp.Position = UDim2.fromOffset(100, 50)
	grp.BackgroundTransparency = 1
	grp.BorderSizePixel = 0
	grp.Parent = Target
	local anc = Instance.new("Frame")
	anc.Size = UDim2.fromOffset(400, 400)
	anc.Position = UDim2.fromOffset(100, 50)
	anc.Rotation = 15
	anc.BackgroundTransparency = 1
	anc.BorderSizePixel = 0
	anc.Parent = grp
	local f = Instance.new("Frame")
	f.Name = "Child200"
	f.Size = UDim2.fromOffset(200, 200)
	f.Position = UDim2.fromOffset(100, 100)
	f.BorderSizePixel = 0
	f.Parent = anc
end
"#,
        );
        let child_off = rep2_group["objects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["name"] == "Child200")
            .unwrap()["offscreenOverdrawViewports"]
            .as_f64()
            .unwrap();
        assert!((child_off * vp - 40_000.0).abs() < 1.0, "200x200 @ 15 deg in CanvasGroup must be 40,000 px, got {}", child_off * vp);

        // Case 3: Holdout 2 — 4096x512 outer CanvasGroup (0.25x area scale) -> Folder ->
        // 15-deg clipping Frame -> 200x100 inner CanvasGroup with text.
        // Fully inside outer CG: inner composite is 200 * 100 * 0.25 = 5,000 texels (not 8,125 bbox).
        let rep3_inside = run_cg_shape_fixture(
            "holdout2_inside",
            r#"return function(Target)
	local outer = Instance.new("CanvasGroup")
	outer.Name = "Outer"
	outer.Size = UDim2.fromOffset(4096, 512)
	outer.Position = UDim2.fromOffset(0, 0)
	outer.BackgroundTransparency = 1
	outer.BorderSizePixel = 0
	outer.Parent = Target
	local folder = Instance.new("Folder")
	folder.Name = "Wrap"
	folder.Parent = outer
	local clip = Instance.new("Frame")
	clip.Name = "Clip15"
	clip.Size = UDim2.fromOffset(400, 300)
	clip.Position = UDim2.fromOffset(100, 100)
	clip.Rotation = 15
	clip.ClipsDescendants = true
	clip.BackgroundTransparency = 1
	clip.BorderSizePixel = 0
	clip.Parent = folder
	local inner = Instance.new("CanvasGroup")
	inner.Name = "Inner"
	inner.Size = UDim2.fromOffset(200, 100)
	inner.Position = UDim2.fromOffset(80, 80)
	inner.BorderSizePixel = 0
	inner.Parent = clip
	local lbl = Instance.new("TextLabel")
	lbl.Name = "Title"
	lbl.Size = UDim2.fromOffset(120, 30)
	lbl.Position = UDim2.fromOffset(10, 10)
	lbl.BackgroundTransparency = 1
	lbl.Text = "Hello"
	lbl.Parent = inner
end
"#,
        );
        let inner_obj = rep3_inside["objects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["name"] == "Inner")
            .unwrap();
        // Inner draws 200x100 = 20,000 texels into its own 1x RT + 200x100 * 0.25 = 5,000 texels
        // composited onto Outer's capped RT -> 25,000 texels total (was 28,125 with bbox).
        let inner_texels = inner_obj["offscreenOverdrawViewports"].as_f64().unwrap() * vp;
        assert!(
            (inner_texels - 25_000.0).abs() < 2.0,
            "Inner CG own RT (20,000) + rotated composite into 0.25x Outer (5,000) must be 25,000 texels, got {inner_texels}"
        );

        // Overhanging edge: 100x100 @ 45 deg centered at (0, 200) -> exactly half (5,000 px) inside viewport [0..800]
        let rep_clip = run_cg_shape_fixture(
            "rot45_half_clipped",
            r#"return function(Target)
	local f = Instance.new("Frame")
	f.Name = "HalfBox"
	f.AnchorPoint = Vector2.new(0.5, 0.5)
	f.Size = UDim2.fromOffset(100, 100)
	f.Position = UDim2.fromOffset(0, 200)
	f.Rotation = 45
	f.BorderSizePixel = 0
	f.Parent = Target
end
"#,
        );
        let half_px = rep_clip["totals"]["screenOverdrawViewports"].as_f64().unwrap() * vp;
        assert!((half_px - 5_000.0).abs() < 2.0, "half-clipped 100x100 @ 45 deg must be 5,000 px, got {half_px}");

        // Case 4: Constant overdraw across rotation angles (0, 15, 37, 45, 72 deg)
        let mut angle_overdraws = Vec::new();
        for deg in [0, 15, 37, 45, 72] {
            let rep = run_cg_shape_fixture(
                &format!("angle_{deg}"),
                &format!(
                    r#"return function(Target)
	local f = Instance.new("Frame")
	f.Size = UDim2.fromOffset(120, 80)
	f.Position = UDim2.fromOffset(300, 250)
	f.Rotation = {deg}
	f.BorderSizePixel = 0
	f.Parent = Target
end
"#
                ),
            );
            angle_overdraws.push(rep["totals"]["screenOverdrawViewports"].as_f64().unwrap());
        }
        for (i, &ov) in angle_overdraws.iter().enumerate() {
            assert!(
                (ov - angle_overdraws[0]).abs() < 1e-6,
                "angle index {i} overdraw {ov} diverged from 0 deg ({})",
                angle_overdraws[0]
            );
        }
    }

    #[test]
    fn compare_equivalence_source_controls_from_device_and_cli_option_validation() {
        let mut base = sample_cost_report("sequence", "live", "ok", 1200.0, 900.0, 1.0, true, 10, 0, 0);
        base["runMode"] = json!("play");
        base["source"] = json!({"kind": "story", "target": "Stories/A.story.luau"});
        base["device"] = json!("desktop");
        base["controls"] = json!({"Theme": "Dark", "Count": 3});
        base["sequence"] = json!({"summary": {"from": 0.5, "frameCount": 10, "fps": 60.0, "clientHz": 60.0}});

        // Identical arms with reordered controls keys pass equivalence
        let mut same = base.clone();
        same["controls"] = json!({"Count": 3, "Theme": "Dark"});
        let ok_diff = compare_reports(&base, &same).unwrap();
        assert_eq!(ok_diff["verdict"], "unchanged");
        assert_eq!(ok_diff["equivalence"]["passed"], true);

        // 1. Different source -> refused
        let mut diff_src = base.clone();
        diff_src["source"] = json!({"kind": "story", "target": "Stories/B.story.luau"});
        let r_src = compare_reports(&base, &diff_src).unwrap();
        assert_eq!(r_src["verdict"], "refused");
        assert!(r_src["reason"].as_str().unwrap().contains("source"), "{r_src}");

        // 2. Different controls -> refused
        let mut diff_ctrl = base.clone();
        diff_ctrl["controls"] = json!({"Theme": "Light", "Count": 3});
        let r_ctrl = compare_reports(&base, &diff_ctrl).unwrap();
        assert_eq!(r_ctrl["verdict"], "refused");
        assert!(r_ctrl["reason"].as_str().unwrap().contains("controls"), "{r_ctrl}");

        // 3. Different --from -> refused
        let mut diff_from = base.clone();
        diff_from["sequence"]["summary"]["from"] = json!(0.0);
        let r_from = compare_reports(&base, &diff_from).unwrap();
        assert_eq!(r_from["verdict"], "refused");
        assert!(r_from["reason"].as_str().unwrap().contains("--from"), "{r_from}");

        // 4. Different --device -> refused
        let mut diff_dev = base.clone();
        diff_dev["device"] = json!("touch");
        let r_dev = compare_reports(&base, &diff_dev).unwrap();
        assert_eq!(r_dev["verdict"], "refused");
        assert!(r_dev["reason"].as_str().unwrap().contains("device"), "{r_dev}");

        // 5. Unknown option (e.g. typo in --frames) -> Usage error (exit 64)
        let bad_frames = format!("{}frams", "--");
        let typo = run_cli(&["--fixture".into(), "x.luau".into(), bad_frames.clone(), "10".into()]);
        match typo {
            Err(Failure::Usage(msg)) => {
                assert!(msg.contains(&bad_frames), "expected unknown option name, got: {msg}");
            }
            Err(other) => panic!("expected Usage error for unknown option, got: {other:?}"),
            Ok(_) => panic!("expected Usage error for unknown option, got Ok"),
        }

        // 6. Unknown option on `cost compare` -> Usage error (exit 64)
        let bad_opt = format!("{}bogus", "--");
        let cmp_typo = run_cli(&["compare".into(), "a.json".into(), "b.json".into(), bad_opt.clone(), "1".into()]);
        match cmp_typo {
            Err(Failure::Usage(msg)) => {
                assert!(msg.contains(&bad_opt), "expected unknown compare option, got: {msg}");
            }
            Err(other) => panic!("expected Usage error for unknown compare option, got: {other:?}"),
            Ok(_) => panic!("expected Usage error for unknown compare option, got Ok"),
        }

        // 7. --time list -> Usage error explaining cost takes a single moment
        let multi_time = run_cli(&["--fixture".into(), "x.luau".into(), "--time".into(), "0.2,0.5".into()]);
        match multi_time {
            Err(Failure::Usage(msg)) => {
                assert!(msg.contains("single moment"), "expected single-moment explanation, got: {msg}");
                assert!(msg.contains("--from"), "expected sequence alternative, got: {msg}");
            }
            Err(other) => panic!("expected Usage error for multi-value --time, got: {other:?}"),
            Ok(_) => panic!("expected Usage error for multi-value --time, got Ok"),
        }
    }

    #[test]
    fn non_uniform_size_cap_peaks_static_note_time0_empty_and_defaults_on_screen() {
        // 1. Non-uniform CanvasGroup size cap (3000x15 @ 1x -> 2048x11 RT):
        //    nw/w = 2048/3000 = 0.682667 vs nh/h = 11/15 = 0.733333 (7.4% apart).
        //    True area scale = (2048/3000)*(11/15) = 0.5006222.
        //    Group bg (3000x15 = 45,000) + Child (1500x15 = 22,500) = 67,500 -> 33,792.0 texels (0.070400x).
        //    (nw/w)^2 would give 31,457.28 (0.065536x); (nh/h)^2 would give 36,300.0 (0.075625x).
        let vp = 800.0 * 600.0;
        let rep = run_cg_shape_fixture(
            "non_uniform_cap",
            r#"return function(Target)
	local Group = Instance.new("CanvasGroup")
	Group.Name = "Strip"
	Group.Size = UDim2.fromOffset(3000, 15)
	Group.BorderSizePixel = 0
	Group.Parent = Target
	local Child = Instance.new("Frame")
	Child.Name = "Half"
	Child.Size = UDim2.fromOffset(1500, 15)
	Child.BorderSizePixel = 0
	Child.Parent = Group
end
"#,
        );
        assert_eq!(rep["totals"]["canvasRtBytes"], 2048 * 11 * 4);
        let texels = rep["totals"]["offscreenOverdrawViewports"].as_f64().unwrap() * vp;
        assert!(
            (texels - 33_792.0).abs() < 2.0,
            "non-uniform 3000x15 -> 2048x11 cap must count 33,792 texels (not (nw/w)^2=31,457 or (nh/h)^2=36,300), got {texels}"
        );

        // 2. Sequence screen labels headline=last sample, prints peaks beside counts, shows defaults,
        //    and on a static window (0 steady churn) notes that opening cost is on the single-moment screen.
        let dest = Path::new("out/test/cost.json");
        let full_scene = store_of(json!({
            "viewport": {"width": 800, "height": 600, "scale": 1.0},
            "status": "ok",
            "nodes": [
                {"id": 1, "name": "G", "path": "G", "class": "ScreenGui", "rect": [0, 0, 800, 600], "enabled": true},
                {"id": 2, "name": "Card", "path": "G.Card", "class": "Frame", "rect": [0, 0, 200, 100], "bg": {"color": [1, 1, 1], "transparency": 0}}
            ],
            "layers": [{"root": 1, "playerGui": true, "displayOrder": 0, "draw": [2]}],
            "fonts": [],
            "images": []
        }));
        let empty_scene = store_of(json!({
            "viewport": {"width": 800, "height": 600, "scale": 1.0},
            "status": "ok",
            "nodes": [
                {"id": 1, "name": "G", "path": "G", "class": "ScreenGui", "rect": [0, 0, 800, 600], "enabled": true}
            ],
            "layers": [{"root": 1, "playerGui": true, "displayOrder": 0, "draw": []}],
            "fonts": [],
            "images": []
        }));
        let mut acc = SequenceAccumulator::new(60.0, "play");
        acc.record_frame(0, 0.5, false, 1, 0, 1, &full_scene, LayoutStats { passes: 1, stored: 2, ..Default::default() }, Vec::new(), None);
        acc.record_frame(1, 0.516667, false, 0, 1, 1, &empty_scene, LayoutStats::default(), Vec::new(), None);
        let seq_rep = acc.finish(Some("ok"));
        let seq_screen = format_one_screen_report(&seq_rep, dest);
        assert!(seq_screen.contains("headline=last sample"), "sequence header must say last sample:\n{seq_screen}");
        assert!(seq_screen.contains("defaults: single=edit, seq=play"), "header must state two defaults:\n{seq_screen}");
        assert!(seq_screen.contains("quads:     0 (peak 1)"), "destroyed panel must still show peak quads:\n{seq_screen}");
        assert!(seq_screen.contains("0.000x screen (peak 0.042x)"), "destroyed panel must still show peak overdraw:\n{seq_screen}");
        assert!(seq_screen.contains("single-moment screen"), "static sequence must point to single-moment opening cost:\n{seq_screen}");

        // 3. Single-moment --time 0 empty mount suggests retrying with a later --time; --time 0.5 does not.
        let empty_hidden = DomHiddenSummary { complete: true, mounted_empty: true, ..Default::default() };
        let mut empty_t0 = analyze_scene_with_mode(&empty_scene, LayoutStats::default(), Some(&empty_hidden), "edit");
        empty_t0["time"] = json!(0.0);
        let screen_t0 = format_one_screen_report(&empty_t0, dest);
        assert!(screen_t0.contains("defaults: single=edit, seq=play"), "{screen_t0}");
        assert!(screen_t0.contains("later --time"), "--time 0 empty screen must suggest a later --time:\n{screen_t0}");
        let mut empty_t05 = empty_t0.clone();
        empty_t05["time"] = json!(0.5);
        let screen_t05 = format_one_screen_report(&empty_t05, dest);
        assert!(!screen_t05.contains("captured at --time 0"), "--time 0.5 must not say captured at --time 0:\n{screen_t05}");
    }

    #[test]
    fn deep_tree_ancestor_caveat_prints_when_layout_ms_extrapolated_and_not_when_refused() {
        let dest = Path::new("out/caveat/cost.json");
        let scene = store_of(json!({
            "viewport": {"width": 800, "height": 600, "scale": 1.0},
            "status": "ok",
            "nodes": [
                {"id": 1, "name": "G", "path": "G", "class": "ScreenGui", "rect": [0, 0, 800, 600], "enabled": true},
                {"id": 2, "name": "Card", "path": "G.Card", "class": "Frame", "rect": [0, 0, 100, 50], "bg": {"color": [1, 1, 1], "transparency": 0}}
            ],
            "layers": [{"root": 1, "playerGui": true, "displayOrder": 0, "draw": [2]}],
            "fonts": [],
            "images": []
        }));
        let hidden = DomHiddenSummary { complete: true, ..Default::default() };

        // 1. Play sequence with extrapolated layoutMs prints the deep-tree caveat
        let mut acc_play = SequenceAccumulator::new(60.0, "play");
        for i in 0..3u32 {
            acc_play.record_frame(i, f64::from(i) / 60.0, false, 1, 0, 1, &scene, LayoutStats { passes: 1, stored: 4, ..Default::default() }, Vec::new(), if i == 0 { Some(hidden.clone()) } else { None });
        }
        let play_screen = format_one_screen_report(&acc_play.finish(Some("ok")), dest);
        assert!(play_screen.contains("caveat:") && play_screen.contains("unchecked against a client") && play_screen.contains("before-vs-after"), "{play_screen}");

        // 2. Single-moment live report with extrapolated fullTree prints the caveat
        let single_live = analyze_scene_with_mode(&scene, LayoutStats { passes: 1, stored: 4, ..Default::default() }, Some(&hidden), "edit");
        let single_screen = format_one_screen_report(&single_live, dest);
        assert!(single_screen.contains("caveat:") && single_screen.contains("unchecked against a client"), "{single_screen}");

        // 3. Refused reports (edit-mode sequence, --scene) do not print the caveat
        let mut acc_edit = SequenceAccumulator::new(60.0, "edit");
        for i in 0..3u32 {
            acc_edit.record_frame(i, f64::from(i) / 60.0, false, 1, 0, 1, &scene, LayoutStats { passes: 1, stored: 4, ..Default::default() }, Vec::new(), if i == 0 { Some(hidden.clone()) } else { None });
        }
        let edit_screen = format_one_screen_report(&acc_edit.finish(Some("ok")), dest);
        assert!(!edit_screen.contains("caveat:"), "refused edit sequence must not print layout ms caveat:\n{edit_screen}");

        let scene_only = analyze_scene(&scene, LayoutStats::default(), None);
        let scene_screen = format_one_screen_report(&scene_only, dest);
        assert!(!scene_screen.contains("caveat:"), "--scene report must not print layout ms caveat:\n{scene_screen}");
    }

    #[test]
    fn live_device_recorded_in_cost_report_and_refused_by_compare() {
        let dir = std::env::temp_dir().join(format!("rur-cost-live-device-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let fixture = dir.join("touch_probe.luau");
        std::fs::write(
            &fixture,
            r#"local UserInputService = game:GetService("UserInputService")
return function(Target)
	local lbl = Instance.new("TextLabel")
	lbl.Name = if UserInputService.TouchEnabled then "TouchYes" else "TouchNo"
	lbl.Size = UDim2.fromOffset(120, 40)
	lbl.Text = lbl.Name
	lbl.Parent = Target
end
"#,
        )
        .unwrap();
        let repo = crate::engine::default_repo().to_string_lossy().into_owned();
        let roots = crate::engine::default_repo()
            .join("tools/tests/oracle-root")
            .to_string_lossy()
            .into_owned();
        let fixture_s = fixture.to_string_lossy().into_owned();

        let run_arm = |out_name: &str, extra: &[&str]| -> (CliOutput, PathBuf) {
            let out_dir = dir.join(out_name);
            let mut argv = vec![
                "--repo".into(),
                repo.clone(),
                "--roots".into(),
                roots.clone(),
                "--no-snapshots".into(),
                "--viewport".into(),
                "800x600".into(),
                "--scale".into(),
                "1".into(),
                "--fixture".into(),
                fixture_s.clone(),
                "--out".into(),
                out_dir.to_string_lossy().into_owned(),
            ];
            argv.extend(extra.iter().map(|s| (*s).to_string()));
            let out = run_cli(&argv).unwrap();
            assert!(!out.failed, "arm {out_name} failed:\n{}", out.summary);
            (out, out_dir.join("cost.json"))
        };

        let has_obj = |rep: &Value, name: &str| {
            rep["objects"]
                .as_array()
                .is_some_and(|arr| arr.iter().any(|o| o["name"] == name))
        };

        // Single-moment live arms: desktop, touch, gamepad
        let (desk_single, desk_single_path) = run_arm("single_desk", &["--time", "0.1"]);
        let (touch_single, touch_single_path) = run_arm("single_touch", &["--time", "0.1", "--device", "touch"]);
        let (pad_single, _) = run_arm("single_pad", &["--time", "0.1", "--device", "gamepad"]);
        assert!(has_obj(&desk_single.value, "TouchNo"), "desktop arm must see TouchEnabled=false");
        assert!(has_obj(&touch_single.value, "TouchYes"), "touch arm must see TouchEnabled=true");
        assert_eq!(desk_single.value["device"], "desktop");
        assert_eq!(touch_single.value["device"], "touch");
        assert_eq!(pad_single.value["device"], "gamepad");

        let cmp_single = run_cli(&[
            "compare".into(),
            desk_single_path.to_string_lossy().into_owned(),
            touch_single_path.to_string_lossy().into_owned(),
        ])
        .unwrap();
        assert!(cmp_single.failed, "compare must refuse desktop vs touch single arms");
        assert_eq!(cmp_single.value["verdict"], "refused");
        assert!(
            cmp_single.value["reason"].as_str().unwrap_or("").contains("device"),
            "refusal must name device: {}",
            cmp_single.value
        );

        // Sequence live arms: desktop vs touch
        let (desk_seq, desk_seq_path) = run_arm("seq_desk", &["--from", "0", "--frames", "3", "--fps", "60"]);
        let (touch_seq, touch_seq_path) = run_arm("seq_touch", &["--from", "0", "--frames", "3", "--fps", "60", "--device", "touch"]);
        assert!(has_obj(&desk_seq.value, "TouchNo"));
        assert!(has_obj(&touch_seq.value, "TouchYes"));
        assert_eq!(desk_seq.value["device"], "desktop");
        assert_eq!(touch_seq.value["device"], "touch");

        let cmp_seq = run_cli(&[
            "compare".into(),
            desk_seq_path.to_string_lossy().into_owned(),
            touch_seq_path.to_string_lossy().into_owned(),
        ])
        .unwrap();
        assert!(cmp_seq.failed, "compare must refuse desktop vs touch sequence arms");
        assert_eq!(cmp_seq.value["verdict"], "refused");
        assert!(
            cmp_seq.value["reason"].as_str().unwrap_or("").contains("device"),
            "refusal must name device: {}",
            cmp_seq.value
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn compare_accepts_legacy_report_missing_metadata_fields_with_note() {
        let dir = std::env::temp_dir().join(format!("rur-cost-legacy-cmp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let fixture = dir.join("legacy_card.luau");
        std::fs::write(
            &fixture,
            r#"return function(Target)
	local f = Instance.new("Frame")
	f.Size = UDim2.fromOffset(100, 50)
	f.BorderSizePixel = 0
	f.Parent = Target
end
"#,
        )
        .unwrap();
        let repo = crate::engine::default_repo().to_string_lossy().into_owned();
        let roots = crate::engine::default_repo()
            .join("tools/tests/oracle-root")
            .to_string_lossy()
            .into_owned();
        let fixture_s = fixture.to_string_lossy().into_owned();

        // 1. Real single-moment report with non-default device & controls, compared against a copy
        //    with `source`, `device`, `controls`, and `time` removed (matching a pre-branch report).
        let after_single_dir = dir.join("after_single");
        let after_single = run_cli(&[
            "--repo".into(),
            repo.clone(),
            "--roots".into(),
            roots.clone(),
            "--no-snapshots".into(),
            "--viewport".into(),
            "800x600".into(),
            "--scale".into(),
            "1".into(),
            "--fixture".into(),
            fixture_s.clone(),
            "--time".into(),
            "0.2".into(),
            "--device".into(),
            "touch".into(),
            "--controls".into(),
            r#"{"Scale":2}"#.into(),
            "--out".into(),
            after_single_dir.to_string_lossy().into_owned(),
        ])
        .unwrap();
        assert!(!after_single.failed);

        let mut legacy_single = after_single.value.clone();
        if let Some(obj) = legacy_single.as_object_mut() {
            obj.remove("source");
            obj.remove("device");
            obj.remove("controls");
            obj.remove("time");
        }
        let legacy_single_path = dir.join("legacy_single.json");
        write_report(&legacy_single_path, &legacy_single).unwrap();

        let cmp_single = run_cli(&[
            "compare".into(),
            legacy_single_path.to_string_lossy().into_owned(),
            after_single_dir.join("cost.json").to_string_lossy().into_owned(),
        ])
        .unwrap();
        assert!(
            !cmp_single.failed,
            "pre-branch single report must compare, not refuse:\n{}",
            cmp_single.summary
        );
        assert_eq!(cmp_single.value["verdict"], "unchanged");
        assert_eq!(
            cmp_single.value["equivalence"]["unchecked"],
            json!(["source", "device", "controls", "time"])
        );
        let note_lines: Vec<&str> = cmp_single
            .summary
            .lines()
            .filter(|l| l.trim_start().starts_with("note:"))
            .collect();
        assert_eq!(note_lines.len(), 1, "expected exactly one unchecked note line:\n{}", cmp_single.summary);
        assert!(
            note_lines[0].contains("source, device, controls, time")
                && note_lines[0].contains("before report predates"),
            "{}",
            note_lines[0]
        );

        // 2. Real sequence report vs pre-branch copy lacking source, device, controls
        let after_seq_dir = dir.join("after_seq");
        let after_seq = run_cli(&[
            "--repo".into(),
            repo,
            "--roots".into(),
            roots,
            "--no-snapshots".into(),
            "--viewport".into(),
            "800x600".into(),
            "--scale".into(),
            "1".into(),
            "--fixture".into(),
            fixture_s,
            "--from".into(),
            "0".into(),
            "--frames".into(),
            "3".into(),
            "--fps".into(),
            "60".into(),
            "--device".into(),
            "touch".into(),
            "--out".into(),
            after_seq_dir.to_string_lossy().into_owned(),
        ])
        .unwrap();
        assert!(!after_seq.failed);
        let mut legacy_seq = after_seq.value.clone();
        if let Some(obj) = legacy_seq.as_object_mut() {
            obj.remove("source");
            obj.remove("device");
            obj.remove("controls");
        }
        let legacy_seq_path = dir.join("legacy_seq.json");
        write_report(&legacy_seq_path, &legacy_seq).unwrap();

        let cmp_seq = run_cli(&[
            "compare".into(),
            legacy_seq_path.to_string_lossy().into_owned(),
            after_seq_dir.join("cost.json").to_string_lossy().into_owned(),
        ])
        .unwrap();
        assert!(!cmp_seq.failed, "pre-branch sequence report must compare:\n{}", cmp_seq.summary);
        assert_eq!(cmp_seq.value["verdict"], "unchanged");
        assert_eq!(
            cmp_seq.value["equivalence"]["unchecked"],
            json!(["source", "device", "controls"])
        );
        assert!(
            cmp_seq.summary.contains("source, device, controls not checked (before report predates them)"),
            "{}",
            cmp_seq.summary
        );

        // 3. Partial legacy report (only `device` missing) still checks `source` when both have it
        let mut partial = after_seq.value.clone();
        partial.as_object_mut().unwrap().remove("device");
        partial["source"] = json!({"kind": "fixture", "target": "other.luau"});
        let diff_src = compare_reports(&partial, &after_seq.value).unwrap();
        assert_eq!(diff_src["verdict"], "refused");
        assert!(diff_src["reason"].as_str().unwrap_or("").contains("source"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rojo_hint_suppressed_for_remote_invoke_and_partial_rojo_project() {
        let dir = std::env::temp_dir().join(format!("rur-cost-rojo-hint-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let repo = crate::engine::default_repo();

        // 1. Reviewer's fixture: mounts a Frame and then calls InvokeServer() on a RemoteFunction it made itself,
        //    beside a valid default.project.json with an unmounted Packages folder -> must print NO --rojo hint.
        let inv_dir = dir.join("invoke_proj");
        std::fs::create_dir_all(inv_dir.join("ReplicatedStorage")).unwrap();
        std::fs::create_dir_all(inv_dir.join("Packages")).unwrap();
        std::fs::write(inv_dir.join("Packages/Kit.luau"), "return {}").unwrap();
        std::fs::write(
            inv_dir.join("default.project.json"),
            r#"{"name":"Inv","tree":{"$className":"DataModel","ReplicatedStorage":{"$path":"ReplicatedStorage","Packages":{"$path":"Packages"}}}}"#,
        )
        .unwrap();
        let inv_fixture = inv_dir.join("ReplicatedStorage/wait_remote.luau");
        std::fs::write(
            &inv_fixture,
            r#"return function(Target)
	local f = Instance.new("Frame")
	f.Size = UDim2.fromOffset(100, 40)
	f.Parent = Target
	local rf = Instance.new("RemoteFunction")
	rf.Name = "OwnRemote"
	rf.Parent = game:GetService("ReplicatedStorage")
	rf:InvokeServer()
end
"#,
        )
        .unwrap();
        let make_args = |fixture: &Path, roots: &str| -> engine::cli::Args {
            let argv: Vec<String> = vec![
                "--repo".into(),
                repo.to_string_lossy().into_owned(),
                "--roots".into(),
                roots.to_string(),
                "--fixture".into(),
                fixture.to_string_lossy().into_owned(),
                "--viewport".into(),
                "800x600".into(),
                "--scale".into(),
                "1".into(),
                "--no-snapshots".into(),
            ];
            engine::cli::parse(&argv).unwrap()
        };
        let spec = engine::cli::SequenceSpec { from: 0.0, frames: 3, fps: 60.0, client_hz: Some(60.0), collect_cost: true };
        let rep_inv = run_sequence_cost(&make_args(&inv_fixture, &inv_dir.to_string_lossy()), &spec, &repo).unwrap();
        assert_eq!(rep_inv["status"], "yielded");
        assert!(rep_inv["totals"]["drawnNodes"].as_u64().unwrap_or(0) >= 1);
        assert!(
            rep_inv["waitingAt"].as_str().unwrap_or("").contains("wait_remote:8"),
            "expected wait_remote:8, got {:?}",
            rep_inv["waitingAt"]
        );
        assert!(
            rep_inv.get("hint").is_none(),
            "InvokeServer() on own RemoteFunction must not print a --rojo hint: {:?}",
            rep_inv.get("hint")
        );

        // 2. Partial Rojo project covering fewer roots than the run mounted -> must print NO --rojo hint.
        let part_dir = dir.join("partial_proj");
        let shared_root = part_dir.join("common_root");
        let place_root = part_dir.join("place_root");
        std::fs::create_dir_all(shared_root.join("ReplicatedStorage/Core")).unwrap();
        std::fs::create_dir_all(place_root.join("ReplicatedStorage")).unwrap();
        std::fs::create_dir_all(place_root.join("Extra")).unwrap();
        std::fs::write(shared_root.join("ReplicatedStorage/Core/Lib.luau"), "return {}").unwrap();
        std::fs::write(place_root.join("Extra/Net.luau"), "return {}").unwrap();
        std::fs::write(
            place_root.join("default.project.json"),
            r#"{"name":"PlaceOnly","tree":{"$className":"DataModel","ReplicatedStorage":{"$path":"ReplicatedStorage","Extra":{"$path":"Extra"}}}}"#,
        )
        .unwrap();
        let wait_story = place_root.join("ReplicatedStorage/WaitExtra.story.luau");
        std::fs::write(
            &wait_story,
            r#"local ReplicatedStorage = game:GetService("ReplicatedStorage")
return function(Target)
	local _ = ReplicatedStorage:WaitForChild("Extra")
end
"#,
        )
        .unwrap();
        let make_story_args = |story: &Path, roots: &str| -> engine::cli::Args {
            let argv: Vec<String> = vec![
                "--repo".into(),
                repo.to_string_lossy().into_owned(),
                "--roots".into(),
                roots.to_string(),
                "--story".into(),
                story.to_string_lossy().into_owned(),
                "--viewport".into(),
                "800x600".into(),
                "--scale".into(),
                "1".into(),
                "--no-snapshots".into(),
            ];
            engine::cli::parse(&argv).unwrap()
        };
        let multi = format!("{};{}", shared_root.display(), place_root.display());
        let rep_partial = run_sequence_cost(&make_story_args(&wait_story, &multi), &spec, &repo).unwrap();
        assert_eq!(rep_partial["status"], "yielded");
        assert!(
            rep_partial.get("hint").is_none(),
            "partial Rojo project that drops the other mounted root must not print a --rojo hint: {:?}",
            rep_partial.get("hint")
        );

        // 3. Same story run with ONLY place_root (which default.project.json DOES cover) -> DOES print --rojo hint.
        let rep_covered = run_sequence_cost(&make_story_args(&wait_story, &place_root.to_string_lossy()), &spec, &repo).unwrap();
        assert_eq!(rep_covered["status"], "yielded");
        let covered_hint = rep_covered["hint"].as_str().unwrap_or("");
        assert!(
            covered_hint.contains("pass --rojo") && covered_hint.contains("ReplicatedStorage.Extra"),
            "covering Rojo project matching WaitForChild should print --rojo hint, got: {covered_hint:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn single_moment_play_only_story_probes_play_mode_and_suggests_flag() {
        let dir = std::env::temp_dir().join(format!("rur-cost-play-only-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let repo = crate::engine::default_repo().to_string_lossy().into_owned();
        let roots = crate::engine::default_repo()
            .join("tools/tests/oracle-root")
            .to_string_lossy()
            .into_owned();

        let err_in_edit = dir.join("play_only_err.luau");
        std::fs::write(
            &err_in_edit,
            r#"local RunService = game:GetService("RunService")
return function(Target)
	if not RunService:IsRunning() then
		error("requires play mode runtime")
	end
	local f = Instance.new("Frame")
	f.Size = UDim2.fromOffset(120, 40)
	f.Parent = Target
end
"#,
        )
        .unwrap();

        let empty_in_edit = dir.join("play_only_empty.luau");
        std::fs::write(
            &empty_in_edit,
            r#"local RunService = game:GetService("RunService")
return function(Target)
	if not RunService:IsRunning() then
		return function() end
	end
	local f = Instance.new("Frame")
	f.Size = UDim2.fromOffset(120, 40)
	f.Parent = Target
end
"#,
        )
        .unwrap();

        let broken_both = dir.join("broken_both.luau");
        std::fs::write(
            &broken_both,
            r#"return function(_Target)
	error("broken in edit and play")
end
"#,
        )
        .unwrap();

        let run_single = |fixture: &Path, name: &str, extra: &[&str]| {
            let out_dir = dir.join(name);
            let mut argv: Vec<String> = vec![
                "--repo".into(),
                repo.clone(),
                "--roots".into(),
                roots.clone(),
                "--no-snapshots".into(),
                "--viewport".into(),
                "800x600".into(),
                "--scale".into(),
                "1".into(),
                "--fixture".into(),
                fixture.to_string_lossy().into_owned(),
                "--out".into(),
                out_dir.to_string_lossy().into_owned(),
            ];
            argv.extend(extra.iter().map(|s| (*s).to_string()));
            run_cli(&argv).unwrap()
        };

        // 1. Fails in default single-moment edit mode -> probes play mode and tells user `--mode play` mounts it
        let out_err = run_single(&err_in_edit, "err_default", &[]);
        assert!(out_err.failed);
        assert_eq!(out_err.value["status"], "error");
        assert_eq!(out_err.value.pointer("/playModeProbe/mountsInPlayMode"), Some(&json!(true)));
        assert!(
            out_err.summary.contains("mounts in play mode, not edit (pass --mode play)"),
            "failing single-moment screen must say --mode play mounts it:\n{}",
            out_err.summary
        );
        assert!(
            out_err
                .value
                .pointer("/predicted/desktop-ref/layoutCpuMs/settledBy")
                .and_then(Value::as_str)
                .unwrap_or("")
                .contains("--mode play"),
            "{}",
            out_err.value["predicted"]
        );

        // Passing --mode play mounts cleanly
        let out_play = run_single(&err_in_edit, "err_play", &["--mode", "play"]);
        assert!(!out_play.failed, "play mode should mount:\n{}", out_play.summary);
        assert_eq!(out_play.value["status"], "ok");

        // 2. Mounts empty in default single-moment edit mode -> also probes play mode
        let out_empty = run_single(&empty_in_edit, "empty_default", &[]);
        assert!(out_empty.failed);
        assert_eq!(out_empty.value["status"], "empty");
        assert_eq!(out_empty.value.pointer("/playModeProbe/mountsInPlayMode"), Some(&json!(true)));
        assert!(
            out_empty.summary.contains("mounts in play mode, not edit (pass --mode play)"),
            "empty single-moment screen must say --mode play mounts it:\n{}",
            out_empty.summary
        );

        // 3. Broken in both edit and play -> playModeProbe is false and screen does NOT suggest --mode play
        let out_both = run_single(&broken_both, "broken_both", &[]);
        assert!(out_both.failed);
        assert_eq!(out_both.value.pointer("/playModeProbe/mountsInPlayMode"), Some(&json!(false)));
        assert!(
            !out_both.summary.contains("pass --mode play"),
            "story broken in play mode too must not claim --mode play mounts it:\n{}",
            out_both.summary
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn top_layout_disambiguates_paths_that_print_alike() {
        let make_driver = |id: u64, path: &str, y: f64| {
            json!({
                "id": id,
                "name": "Glow",
                "path": path,
                "class": "UIShadow",
                "siblingIndex": 1,
                "pathCount": 1,
                "position": [100.0, y],
                "stored": 10.0,
                "moved": 0.0,
                "scrollPasses": 0.0,
                "scrollMoved": 0.0,
                "desktopMs": 0.00312,
                "share": 0.333333,
            })
        };

        // 1. Three objects whose paths differ ONLY near the root, sharing a >44-char tail
        let root_drivers = vec![
            make_driver(1, "PanelAlpha.Subtree.Card.Control.Segmented.All.Face.Fill.Glow", 10.0),
            make_driver(2, "PanelBeta.Subtree.Card.Control.Segmented.All.Face.Fill.Glow", 60.0),
            make_driver(3, "PanelGamma.Subtree.Card.Control.Segmented.All.Face.Fill.Glow", 110.0),
        ];
        let mut report = analyze_scene(&SceneStore::default(), LayoutStats::default(), None);
        report["status"] = json!("ok");
        report["layoutDrivers"] = Value::Array(root_drivers);
        report["layoutDriverSummary"] = json!({"totalDrivers": 3, "totalDesktopMs": 0.00936});
        let text = format_one_screen_report(&report, Path::new("out/cost.json"));
        let layout_lines: Vec<&str> = text
            .lines()
            .skip_while(|l| !l.contains("top layout"))
            .skip(1)
            .take(3)
            .collect();
        assert_eq!(layout_lines.len(), 3, "{text}");
        let unique: std::collections::BTreeSet<&str> = layout_lines.iter().copied().collect();
        assert_eq!(unique.len(), 3, "top layout lines must not print alike:\n{text}");
        assert!(layout_lines[0].contains("PanelAlpha"), "{}", layout_lines[0]);
        assert!(layout_lines[1].contains("PanelBeta"), "{}", layout_lines[1]);
        assert!(layout_lines[2].contains("PanelGamma"), "{}", layout_lines[2]);

        // 2. Three objects differing in the middle (common root + common >44-char tail, like settings rows)
        let mid_drivers = vec![
            make_driver(11, "CoreGui.Window.Holder.Main.Rows.Units.Visibility.Card.Control.Segmented.All.Face.Fill.Glow", 20.0),
            make_driver(12, "CoreGui.Window.Holder.Main.Rows.Social.ShowUsers.Card.Control.Segmented.All.Face.Fill.Glow", 70.0),
            make_driver(13, "CoreGui.Window.Holder.Main.Rows.Social.TradeMode.Card.Control.Segmented.All.Face.Fill.Glow", 120.0),
        ];
        report["layoutDrivers"] = Value::Array(mid_drivers);
        let mid_text = format_one_screen_report(&report, Path::new("out/cost.json"));
        let mid_lines: Vec<&str> = mid_text
            .lines()
            .skip_while(|l| !l.contains("top layout"))
            .skip(1)
            .take(3)
            .collect();
        let mid_unique: std::collections::BTreeSet<&str> = mid_lines.iter().copied().collect();
        assert_eq!(mid_unique.len(), 3, "middle-differing top layout lines must be distinct:\n{mid_text}");
        assert!(mid_lines[0].contains("Visibility"), "{}", mid_lines[0]);
        assert!(mid_lines[1].contains("ShowUsers"), "{}", mid_lines[1]);
        assert!(mid_lines[2].contains("TradeMode"), "{}", mid_lines[2]);

        // 3. Fallback when printed path+class still collides (even with pathCount == 1): appends #index @x,y
        let dup_drivers = vec![
            make_driver(21, "CoreGui.Window.Rows.Item.Card.Control.Segmented.All.Face.Fill.Glow", 30.0),
            make_driver(22, "CoreGui.Window.Rows.Item.Card.Control.Segmented.All.Face.Fill.Glow", 90.0),
        ];
        report["layoutDrivers"] = Value::Array(dup_drivers);
        let dup_text = format_one_screen_report(&report, Path::new("out/cost.json"));
        let dup_lines: Vec<&str> = dup_text
            .lines()
            .skip_while(|l| !l.contains("top layout"))
            .skip(1)
            .take(2)
            .collect();
        assert_ne!(dup_lines[0], dup_lines[1], "colliding labels must fall back to #index @x,y:\n{dup_text}");
        assert!(dup_lines[0].contains("@100,30") && dup_lines[1].contains("@100,90"), "{dup_text}");
    }

    #[test]
    fn compare_checks_single_moment_time() {
        let dir = std::env::temp_dir().join(format!("rur-cost-cmp-time-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let fixture = dir.join("timed_card.luau");
        std::fs::write(
            &fixture,
            r#"return function(Target)
	local f = Instance.new("Frame")
	f.Size = UDim2.fromOffset(100, 50)
	f.Parent = Target
end
"#,
        )
        .unwrap();
        let repo = crate::engine::default_repo().to_string_lossy().into_owned();
        let roots = crate::engine::default_repo()
            .join("tools/tests/oracle-root")
            .to_string_lossy()
            .into_owned();
        let fixture_s = fixture.to_string_lossy().into_owned();

        let run_at = |name: &str, t: &str| {
            let out_dir = dir.join(name);
            let out = run_cli(&[
                "--repo".into(),
                repo.clone(),
                "--roots".into(),
                roots.clone(),
                "--no-snapshots".into(),
                "--viewport".into(),
                "800x600".into(),
                "--scale".into(),
                "1".into(),
                "--fixture".into(),
                fixture_s.clone(),
                "--time".into(),
                t.into(),
                "--out".into(),
                out_dir.to_string_lossy().into_owned(),
            ])
            .unwrap();
            assert!(!out.failed, "arm {name} failed:\n{}", out.summary);
            out_dir.join("cost.json")
        };

        let t05_a = run_at("t05_a", "0.5");
        let t05_b = run_at("t05_b", "0.5");
        let t15 = run_at("t15", "1.5");

        let cmp_diff = run_cli(&[
            "compare".into(),
            t05_a.to_string_lossy().into_owned(),
            t15.to_string_lossy().into_owned(),
        ])
        .unwrap();
        assert!(cmp_diff.failed, "compare must refuse --time 0.5 vs --time 1.5:\n{}", cmp_diff.summary);
        assert_eq!(cmp_diff.value["verdict"], "refused");
        assert!(
            cmp_diff.value["reason"].as_str().unwrap_or("").contains("--time"),
            "{}",
            cmp_diff.value
        );
        assert!(
            cmp_diff.value["equivalence"]["mismatches"]
                .as_array()
                .is_some_and(|arr| arr.iter().any(|m| m["field"] == "time")),
            "{}",
            cmp_diff.value
        );

        let cmp_same = run_cli(&[
            "compare".into(),
            t05_a.to_string_lossy().into_owned(),
            t05_b.to_string_lossy().into_owned(),
        ])
        .unwrap();
        assert!(!cmp_same.failed, "matching --time arms must compare:\n{}", cmp_same.summary);
        assert_eq!(cmp_same.value["verdict"], "unchanged");
        assert!(
            cmp_same.value["equivalence"]["checks"]
                .as_array()
                .is_some_and(|arr| arr.iter().any(|c| c == "time")),
            "{}",
            cmp_same.value
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
