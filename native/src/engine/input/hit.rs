//! Hit-testing (port of `runtime/input/hit.luau`): the GuiObjects under a screen point, topmost
//! first, following the scene exporter's draw order: layer collectors by DisplayOrder,
//! ZIndexBehavior Sibling / Global, rotation matrices, and clipping by ClipsDescendants,
//! ScrollingFrame windows and CanvasGroups.
//!
//! Measured in Studio (notes/interaction.md): every GuiObject under the cursor takes part (occluded
//! ones too), whatever its Active / transparency; Interactable = false objects get no input.
//!
//! The draw order is cached until a GUI property (other than the visual-only ones) or the
//! hierarchy changes (the input's PropertyChanged / HierarchyChanged hooks), like the Luau
//! `Hit.Invalidate`. Entries copy their record's rect at build time: every layout change comes
//! with such an invalidating change, so the copy equals the Luau entry's record table.

use std::rc::Rc;

use crate::engine::GuiEngine;
use crate::engine::{
    Engine, InstanceId, State,
    layout::{Layout, Record, Transform, absolute_position, record::{compose, rotation_about}},
    value::Value,
};

use super::Input;

/// A CanvasGroup the entry is drawn in: its rect, matrix and whether it is culled.
#[derive(Clone, Debug)]
pub struct Group {
    pub rect: [f64; 4],
    pub matrix: Option<Transform>,
    pub culled: bool,
}

/// One GuiObject of the draw order.
#[derive(Clone, Debug)]
pub struct Entry {
    pub id: InstanceId,
    /// The record's X, Y, W, H
    pub rect: [f64; 4],
    /// None = identity
    pub matrix: Option<Transform>,
    pub clip: Option<[f64; 4]>,
    pub culled: bool,
    pub groups: Option<Rc<Vec<Group>>>,
}

const IDENTITY: Transform = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// Maps a screen point into the matrix' local (unrotated) space.
pub fn to_local(m: &Option<Transform>, x: f64, y: f64) -> (f64, f64) {
    let Some(m) = m else { return (x, y) };
    let determinant = m[0] * m[3] - m[2] * m[1];
    if determinant == 0.0 {
        return (f64::INFINITY, f64::INFINITY);
    }
    let (dx, dy) = (x - m[4], y - m[5]);
    ((m[3] * dx - m[2] * dy) / determinant, (-m[1] * dx + m[0] * dy) / determinant)
}

/// Clipping.Rect(Record, true)
fn clip_rect(record: &Record) -> [f64; 4] {
    let (x, y) = absolute_position(record);
    match record.window {
        Some(window) => [x + window.x - record.x, y + window.y - record.y, window.w, window.h],
        None => [x, y, record.w, record.h],
    }
}

/// Clipping.Outside
fn outside(record: &Record, clip: Option<[f64; 4]>) -> bool {
    let Some(clip) = clip else { return false };
    if clip[2] <= 0.0 || clip[3] <= 0.0 {
        return true;
    }
    let (x, y) = absolute_position(record);
    x + record.w <= clip[0] || x >= clip[0] + clip[2] || y + record.h <= clip[1] || y >= clip[1] + clip[3]
}

struct Item {
    entry: Option<Entry>,
    z: f64,
    sequence: u32,
    group: bool,
    children: Vec<Item>,
}

struct Collect<'a> {
    input: &'a Input,
    state: &'a State,
    layout: &'a Layout,
    global: bool,
    rotation_clipping: bool,
    sequence: u32,
}

impl Collect<'_> {
    fn z(&self, id: InstanceId) -> f64 {
        self.state.dom.raw_get(id, self.input.n.ZIndex).and_then(Value::as_number).unwrap_or(1.0)
    }

    /// OrderedChildren: GuiChildren, sorted by ZIndex (stable) unless Global.
    fn ordered_children(&self, id: InstanceId) -> Vec<InstanceId> {
        let mut children = self.input.gui_children(self.state, id);
        if !self.global && children.len() > 1 {
            let mut keyed: Vec<(f64, usize, InstanceId)> = children.iter().enumerate().map(|(i, &c)| (self.z(c), i, c)).collect();
            keyed.sort_by(|a, b| {
                if a.0 == b.0 { a.1.cmp(&b.1) } else { a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal) }
            });
            children = keyed.into_iter().map(|k| k.2).collect();
        }
        children
    }

    fn visit(
        &mut self,
        id: InstanceId,
        parent: &mut Item,
        out: &mut Vec<Entry>,
        matrix: Option<Transform>,
        clip: Option<[f64; 4]>,
        groups: Option<Rc<Vec<Group>>>,
    ) {
        let n = &self.input.n;
        for child in self.ordered_children(id) {
            let Some(record) = self.layout.record(child) else { continue };
            if !record.visible {
                continue;
            }
            let rotation = record
                .own_rotation
                .unwrap_or_else(|| self.state.dom.raw_get(child, n.Rotation).and_then(Value::as_number).unwrap_or(0.0));
            let child_rotated = record.rotation != 0.0;
            let mut child_matrix = matrix;
            if rotation != 0.0 {
                let about = rotation_about(rotation, record.x + record.w / 2.0, record.y + record.h / 2.0);
                child_matrix = Some(compose(&matrix.unwrap_or(IDENTITY), &about));
            }
            self.sequence += 1;
            let escapes = child_rotated && !self.rotation_clipping;
            let entry = Entry {
                id: child,
                rect: [record.x, record.y, record.w, record.h],
                matrix: child_matrix,
                clip: if escapes { None } else { clip },
                culled: escapes && outside(record, clip),
                groups: groups.clone(),
            };
            let culled = entry.culled;
            let class = self.state.dom.at(child).class;
            let is_group = Some(class) == self.input.c.CanvasGroup;

            let mut child_clip = clip;
            let clips = matches!(self.state.dom.raw_get(child, n.ClipsDescendants), Some(Value::Bool(true))) || is_group;
            if clips && !child_rotated {
                let rect = clip_rect(record);
                let (mut x1, mut y1) = (rect[0], rect[1]);
                let (mut x2, mut y2) = (rect[0] + rect[2], rect[1] + rect[3]);
                if let Some(c) = clip {
                    x1 = x1.max(c[0]);
                    y1 = y1.max(c[1]);
                    x2 = x2.min(c[0] + c[2]);
                    y2 = y2.min(c[1] + c[3]);
                }
                child_clip = Some([x1, y1, (x2 - x1).max(0.0), (y2 - y1).max(0.0)]);
            }
            let child_groups = if is_group {
                let mut list = groups.as_ref().map(|g| (**g).clone()).unwrap_or_default();
                list.push(Group { rect: [record.x, record.y, record.w, record.h], matrix: child_matrix, culled });
                Some(Rc::new(list))
            } else {
                groups.clone()
            };

            if self.global {
                let mut item = Item { entry: Some(entry), z: self.z(child), sequence: self.sequence, group: is_group, children: Vec::new() };
                self.visit(child, &mut item, out, child_matrix, child_clip, child_groups);
                parent.children.push(item);
            } else {
                out.push(entry);
                self.visit(child, parent, out, child_matrix, child_clip, child_groups);
            }
        }
    }
}

fn flatten(item: Item, list: &mut Vec<Item>) {
    for mut child in item.children {
        if child.group {
            list.push(child);
        } else {
            let grandchildren = std::mem::take(&mut child.children);
            let holder = Item { entry: None, z: 0.0, sequence: 0, group: false, children: grandchildren };
            list.push(child);
            flatten(holder, list);
        }
    }
}

fn emit_global(mut item: Item, include_self: bool, out: &mut Vec<Entry>) {
    if include_self {
        if let Some(entry) = item.entry.take() {
            out.push(entry);
        }
    }
    let mut list = Vec::new();
    flatten(item, &mut list);
    // (Z, Sequence) is a total order: any sort gives table.sort's result
    list.sort_by(|a, b| {
        if a.z != b.z { a.z.partial_cmp(&b.z).unwrap_or(std::cmp::Ordering::Equal) } else { a.sequence.cmp(&b.sequence) }
    });
    for child in list {
        if child.group {
            emit_global(child, true, out);
        } else if let Some(entry) = child.entry {
            out.push(entry);
        }
    }
}

impl Input {
    /// Layout.GuiChildren: GuiObject children, looking through Folders.
    pub(crate) fn gui_children(&self, state: &State, id: InstanceId) -> Vec<InstanceId> {
        let mut out = Vec::new();
        self.gui_children_into(state, id, &mut out);
        out
    }

    fn gui_children_into(&self, state: &State, id: InstanceId, out: &mut Vec<InstanceId>) {
        for &child in &state.dom.at(id).children {
            let class = state.dom.at(child).class;
            if self.class_is(class, self.c.GuiObject) {
                out.push(child);
            } else if Some(class) == self.c.Folder {
                self.gui_children_into(state, child, out);
            }
        }
    }

    /// Layout's Container: the PlayerGui / CoreGui / StarterGui / PluginGui holding `id`.
    pub(crate) fn container(&self, state: &State, id: InstanceId) -> Option<InstanceId> {
        let mut current = state.dom.at(id).parent;
        while let Some(node) = current {
            let class = Some(state.dom.at(node).class);
            if class == self.c.PlayerGui
                || class == self.c.CoreGui
                || class == self.c.StarterGui
                || class == self.c.PluginGui
                || class == self.c.DockWidgetPluginGui
            {
                return Some(node);
            }
            current = state.dom.at(node).parent;
        }
        None
    }

    /// Layout.IsDisplayed
    pub(crate) fn is_displayed(&self, engine: &Engine, state: &State, root: InstanceId) -> bool {
        match self.container(state, root) {
            None => false,
            Some(holder) => Some(state.dom.at(holder).class) != self.c.StarterGui || engine.options.edit_mode(),
        }
    }

    /// Layout.FindRoot: the topmost GuiBase2d ancestor-or-self (through Folders).
    pub(crate) fn find_root(&self, state: &State, id: InstanceId) -> InstanceId {
        let mut root = id;
        let mut current = state.dom.at(id).parent;
        while let Some(node) = current {
            let class = state.dom.at(node).class;
            if self.class_is(class, self.c.GuiBase2d) {
                root = node;
            } else if Some(class) != self.c.Folder {
                break;
            }
            current = state.dom.at(node).parent;
        }
        root
    }

    /// Layout.DisplayedRoots: every LayerCollector of CoreGui, StarterGui (edit mode) and the
    /// players' PlayerGuis, by DisplayOrder then creation id.
    pub(crate) fn displayed_roots(&self, engine: &Engine, state: &State) -> Vec<InstanceId> {
        fn collect(input: &Input, state: &State, holder: InstanceId, out: &mut Vec<InstanceId>) {
            for &child in &state.dom.at(holder).children {
                let class = state.dom.at(child).class;
                if input.class_is(class, input.c.LayerCollector) {
                    out.push(child);
                } else if Some(class) == input.c.Folder {
                    collect(input, state, child, out);
                }
            }
        }
        let game = engine.world().game;
        let mut result = Vec::new();
        for &child in &state.dom.at(game).children {
            let class = Some(state.dom.at(child).class);
            if class == self.c.CoreGui || (class == self.c.StarterGui && engine.options.edit_mode()) {
                collect(self, state, child, &mut result);
            } else if class == self.c.Players {
                for &player in &state.dom.at(child).children {
                    for &holder in &state.dom.at(player).children {
                        if Some(state.dom.at(holder).class) == self.c.PlayerGui {
                            collect(self, state, holder, &mut result);
                        }
                    }
                }
            }
        }
        let order = |id: InstanceId| state.dom.raw_get(id, self.n.DisplayOrder).and_then(Value::as_number).unwrap_or(0.0);
        result.sort_by(|&a, &b| {
            let (oa, ob) = (order(a), order(b));
            if oa != ob {
                oa.partial_cmp(&ob).unwrap_or(std::cmp::Ordering::Equal)
            } else {
                state.dom.at(a).serial.cmp(&state.dom.at(b).serial)
            }
        });
        result
    }

    /// Hit.Invalidate(OrderChanged)
    pub(crate) fn invalidate_hit(&self) {
        self.hit_dirty.set(true);
    }

    /// Hit.DrawOrder: all GuiObjects of the displayed ScreenGuis in draw order (bottom first).
    pub fn draw_order(&self, engine: &Engine) -> Rc<Vec<Entry>> {
        if !self.hit_dirty.get() {
            if let Some(cached) = self.hit_order.borrow().as_ref() {
                return cached.clone();
            }
        }
        let state = engine.state.borrow();
        let layout_guard = engine.layout().borrow();
        let Some(layout) = layout_guard.as_ref() else { return Rc::new(Vec::new()) };
        let mut out = Vec::new();
        for root in self.displayed_roots(engine, &state) {
            let class = state.dom.at(root).class;
            if !self.class_is(class, self.c.ScreenGui) {
                continue;
            }
            if layout.record(root).is_none() || matches!(state.dom.raw_get(root, self.n.Enabled), Some(Value::Bool(false))) {
                continue;
            }
            let global = state
                .dom
                .raw_get(root, self.n.ZIndexBehavior)
                .and_then(Value::as_enum)
                .is_some_and(|item| &*engine.reflection.enum_item_info(item).name == "Global");
            let mut collect = Collect {
                input: self,
                state: &state,
                layout,
                global,
                rotation_clipping: engine.options.clips_descendants_supports_rotation,
                sequence: 0,
            };
            let mut tree = Item { entry: None, z: 0.0, sequence: 0, group: false, children: Vec::new() };
            collect.visit(root, &mut tree, &mut out, None, None, None);
            if global {
                emit_global(tree, false, &mut out);
            }
        }
        let order = Rc::new(out);
        *self.hit_order.borrow_mut() = Some(order.clone());
        self.hit_dirty.set(false);
        order
    }

    /// Hit.Interactable: an Interactable = false object and all its descendants get no input.
    pub fn interactable(&self, state: &State, id: InstanceId) -> bool {
        let mut current = Some(id);
        while let Some(node) = current {
            let Some(instance) = state.dom.get(node) else { return true };
            if !self.class_is(instance.class, self.c.GuiObject) {
                return true;
            }
            if matches!(state.dom.raw_get(node, self.n.Interactable), Some(Value::Bool(false))) {
                return false;
            }
            current = instance.parent;
        }
        true
    }

    /// Hit.At: the interactable GuiObjects under the screen point, topmost first.
    /// Profile: the "hitTesting" bucket (draw order included), hitQueries, hitEntries, and
    /// hitQueryHits for a repeated query (same draw order and point), which the Luau runtime answers
    /// from its same-point cache (this port recomputes it; the result is the same).
    pub fn hit_at(&self, engine: &Engine, x: f64, y: f64) -> Vec<InstanceId> {
        crate::engine::profile::enter("hitTesting");
        crate::engine::profile::count("hitQueries", 1);
        let order = self.draw_order(engine);
        {
            let mut last = self.hit_last.borrow_mut();
            let repeated = last.as_ref().is_some_and(|(o, lx, ly)| Rc::ptr_eq(o, &order) && *lx == x && *ly == y);
            if repeated {
                crate::engine::profile::count("hitQueryHits", 1);
            } else {
                crate::engine::profile::count("hitEntries", order.len() as u64);
                *last = Some((order.clone(), x, y));
            }
        }
        let state = engine.state.borrow();
        let result =
            order.iter().rev().filter(|entry| contains(entry, x, y) && self.interactable(&state, entry.id)).map(|entry| entry.id).collect();
        crate::engine::profile::leave();
        result
    }
}

/// Measured (probes/hit-geometry.luau, 5009 points): rects and clips are closed on all four edges.
const EPSILON: f64 = 1e-6;

/// Hit.Contains: inside the entry's (rotated) rect and its clip.
pub fn contains(entry: &Entry, x: f64, y: f64) -> bool {
    if entry.culled {
        return false;
    }
    if let Some(groups) = &entry.groups {
        for group in groups.iter() {
            if group.culled {
                return false;
            }
            let (gx, gy) = to_local(&group.matrix, x, y);
            let r = group.rect;
            if gx < r[0] - EPSILON || gy < r[1] - EPSILON || gx > r[0] + r[2] + EPSILON || gy > r[1] + r[3] + EPSILON {
                return false;
            }
        }
    }
    if let Some(clip) = entry.clip {
        if clip[2] <= 0.0 || clip[3] <= 0.0 {
            return false;
        }
        if x < clip[0] - EPSILON || y < clip[1] - EPSILON || x > clip[0] + clip[2] + EPSILON || y > clip[1] + clip[3] + EPSILON {
            return false;
        }
    }
    let (lx, ly) = to_local(&entry.matrix, x, y);
    let r = entry.rect;
    lx >= r[0] - EPSILON && ly >= r[1] - EPSILON && lx <= r[0] + r[2] + EPSILON && ly <= r[1] + r[3] + EPSILON
}
