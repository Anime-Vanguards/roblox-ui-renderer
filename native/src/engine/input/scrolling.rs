//! ScrollingFrame input (port of `runtime/input/scrolling.luau`): mouse-wheel routing and smooth
//! scrolling, measured in Studio (tools/interaction/experiments/wheel-virtual.mjs, wheel2.mjs;
//! notes/interaction.md):
//!   - one notch moves the target CanvasPosition by 100 logical px, whatever the frame size;
//!   - CanvasPosition follows a critically damped 4.5 Hz spring with float32 position/velocity
//!     state and snaps within 1 px; measured frame traces live in tools/interaction/reference;
//!   - the wheel goes to the topmost ScrollingFrame under the cursor that can still move that way
//!     (ScrollingEnabled = false frames are skipped); a frame at the end of its range fires a
//!     MouseWheel event and passes the wheel on.
//! Plus the presentation protocol (predicted scrolling in the client: EnablePresentation,
//! ApplyPresentation, PresentationSnapshot) and scroll bar thumbs.

use std::cell::{Cell, RefCell};

use mlua::prelude::*;
use rustc_hash::{FxHashMap, FxHashSet};
use serde_json::{Map, Value as Json, json};

use crate::engine::GuiEngine;
use super::{Cx, Input, clamp, vector2};
use crate::engine::{
    Engine, InstanceId, NameId, State,
    dom::flags,
    export::number,
    layout::Layout,
    value::Value,
};

const NOTCH_PIXELS: f64 = 100.0;
const SNAP_DISTANCE: f64 = 1.0;

#[inline]
fn f32r(value: f64) -> f64 {
    value as f32 as f64
}

fn omega() -> f64 {
    f32r(9.0 * std::f64::consts::PI)
}

#[derive(Clone, Copy, Debug)]
pub struct Axis {
    pub position: f64,
    pub velocity: f64,
    pub target: f64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Motion {
    pub x: Option<Axis>,
    pub y: Option<Axis>,
}

/// A float32 Vector2 (the Luau Vector2 values of the presentation state)
type V2 = (f32, f32);

#[derive(Clone, Copy, Debug)]
struct Presented {
    epoch: f64,
    sequence: f64,
    owned: bool,
    position: V2,
    velocity: V2,
    target: V2,
    max_x: f64,
    max_y: f64,
    /// The ScrollingDirection enum item (None: not an enum item, the name "")
    direction: Option<u32>,
    eligible: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Grab {
    pub target: InstanceId,
    vertical: bool,
    start_mouse: f64,
    start_canvas: f64,
    ratio: f64,
}

#[derive(Default)]
pub struct Scroll {
    /// Active smooth scrolls per ScrollingFrame (insertion order)
    motions: RefCell<Vec<(InstanceId, Motion)>>,
    enabled: Cell<bool>,
    sequence: Cell<f64>,
    pending: RefCell<Option<Json>>,
    presentation_writing: Cell<Option<InstanceId>>,
    motion_writing: Cell<Option<InstanceId>>,
    /// The presentation state of every ScrollingFrame a snapshot or packet named, in that order
    /// (entries of freed frames are dropped, [`Scroll::forget`])
    presented: RefCell<Vec<(InstanceId, Presented)>>,
    /// Where a frame's entry is in `presented`
    slots: RefCell<FxHashMap<InstanceId, usize>>,
    /// Presented frames by creation id
    ids: RefCell<FxHashMap<u32, InstanceId>>,
    /// The ScrollingFrames in the DataModel, in slot order, at a structure version of the dom
    /// (`snapshot`)
    listed: RefCell<Option<(u64, Vec<InstanceId>)>>,
    pub(crate) grabbed: RefCell<Option<Grab>>,
    initialized: Cell<bool>,
}

impl Scroll {
    fn init(&self) {
        if !self.initialized.get() {
            self.initialized.set(true);
            self.sequence.set(-1.0);
        }
    }

    fn motion(&self, id: InstanceId) -> Option<Motion> {
        self.motions.borrow().iter().find(|(m, _)| *m == id).map(|(_, m)| *m)
    }

    fn set_motion(&self, id: InstanceId, motion: Motion) {
        let mut motions = self.motions.borrow_mut();
        match motions.iter_mut().find(|(m, _)| *m == id) {
            Some(entry) => entry.1 = motion,
            None => motions.push((id, motion)),
        }
    }

    fn remove_motion(&self, id: InstanceId) {
        self.motions.borrow_mut().retain(|(m, _)| *m != id);
    }

    fn presented(&self, id: InstanceId) -> Option<Presented> {
        let slot = *self.slots.borrow().get(&id)?;
        Some(self.presented.borrow()[slot].1)
    }

    fn update(&self, id: InstanceId, f: impl FnOnce(&mut Presented)) {
        let Some(slot) = self.slots.borrow().get(&id).copied() else { return };
        f(&mut self.presented.borrow_mut()[slot].1);
    }

    fn insert(&self, id: InstanceId, serial: u32, state: Presented) {
        let mut presented = self.presented.borrow_mut();
        self.slots.borrow_mut().insert(id, presented.len());
        presented.push((id, state));
        self.ids.borrow_mut().insert(serial, id);
    }

    /// Drops the state of a freed instance (the instance GC, lua/raw.rs). A story that rebuilds its
    /// UI (a control toggling between two windows) would otherwise leave an entry per ScrollingFrame
    /// it ever made, each looked at by every later frame's packet and snapshot. Entries of freed
    /// frames have no effect: every reader skips ids the dom no longer has.
    pub(crate) fn forget(&self, id: InstanceId) {
        let Some(slot) = self.slots.borrow_mut().remove(&id) else { return };
        let mut presented = self.presented.borrow_mut();
        presented.remove(slot);
        let mut slots = self.slots.borrow_mut();
        for (index, (later, _)) in presented.iter().enumerate().skip(slot) {
            slots.insert(*later, index);
        }
        self.ids.borrow_mut().retain(|_, frame| *frame != id);
        self.motions.borrow_mut().retain(|(m, _)| *m != id);
    }

    #[cfg(test)]
    pub(crate) fn presented_count(&self) -> usize {
        self.presented.borrow().len()
    }

    /// Scrolling.Animating
    pub fn animating(&self, id: InstanceId) -> bool {
        self.motion(id).is_some()
    }
}

/// Reads for the scrolling rules (state + layout records), usable inside hooks.
struct View<'a> {
    engine: &'a Engine,
    i: &'a Input,
    state: &'a State,
    layout: Option<&'a Layout>,
}

impl View<'_> {
    fn get(&self, id: InstanceId, name: NameId) -> Option<&Value> {
        self.state.dom.get(id)?;
        self.state.dom.raw_get(id, name)
    }

    fn vector(&self, id: InstanceId, name: NameId) -> V2 {
        match self.get(id, name) {
            Some(Value::Vector2(v)) => (v.x, v.y),
            _ => (0.0, 0.0),
        }
    }

    fn canvas_position(&self, id: InstanceId) -> V2 {
        self.vector(id, self.i.n.CanvasPosition)
    }

    /// The ScrollingDirection enum item
    fn direction(&self, id: InstanceId) -> Option<u32> {
        match self.get(id, self.i.n.ScrollingDirection) {
            Some(Value::EnumItem(item)) => Some(*item),
            _ => None,
        }
    }

    /// The direction's name ("X", "Y", "XY"; "" when the property holds no enum item)
    fn direction_name(&self, direction: Option<u32>) -> &str {
        direction_name(self.engine, direction)
    }

    fn scrolling_disabled(&self, id: InstanceId) -> bool {
        matches!(self.get(id, self.i.n.ScrollingEnabled), Some(Value::Bool(false)))
    }

    fn is_scrolling_frame(&self, id: InstanceId) -> bool {
        self.state.dom.get(id).is_some_and(|i| Some(i.class) == self.i.c.ScrollingFrame)
    }

    /// Scrolling.Range: scroll range per axis, as the layout clamps CanvasPosition.
    fn range(&self, id: InstanceId) -> (f64, f64) {
        let Some(record) = self.layout.and_then(|l| l.record(id)) else { return (0.0, 0.0) };
        let Some(bars) = record.scroll_bars else { return (0.0, 0.0) };
        let direction = self.direction_name(self.direction(id));
        let canvas = self.vector(id, self.i.n.AbsoluteCanvasSize);
        let window = self.vector(id, self.i.n.AbsoluteWindowSize);
        let (canvas_x, canvas_y) = (canvas.0 as f64, canvas.1 as f64);
        let scrollable_x = if direction == "Y" { canvas_x > record.w + 1e-6 } else { bars.horizontal };
        let scrollable_y = if direction == "X" { canvas_y > record.h + 1e-6 } else { bars.vertical };
        let max_x = if scrollable_x { (canvas_x - window.0 as f64).max(0.0) } else { 0.0 };
        let max_y = if scrollable_y { (canvas_y - window.1 as f64).max(0.0) } else { 0.0 };
        (max_x, max_y)
    }

    fn eligible(&self, id: InstanceId, allow_disabled: bool) -> bool {
        let Some(instance) = self.state.dom.get(id) else { return false };
        let Some(record) = self.layout.and_then(|l| l.record(id)) else { return false };
        !instance.has(flags::DESTROYED)
            && Some(instance.class) == self.i.c.ScrollingFrame
            && instance.parent.is_some()
            && record.visible
            && record.rotation == 0.0
            && record.outer.is_none()
            && (allow_disabled || !self.scrolling_disabled(id))
            && self.i.is_displayed(self.engine, self.state, self.i.find_root(self.state, id))
            && self.i.interactable(self.state, id)
    }
}

fn direction_name(engine: &Engine, direction: Option<u32>) -> &str {
    direction.map_or("", |item| &engine.reflection.enum_item_info(item).name)
}

fn with_view<R>(engine: &Engine, input: &Input, f: impl FnOnce(&View) -> R) -> R {
    let state = engine.state.borrow();
    let guard = engine.layout().try_borrow().ok();
    let layout = guard.as_ref().and_then(|g| g.as_ref());
    f(&View { engine, i: input, state: &state, layout })
}

fn with_view_in<R>(engine: &Engine, input: &Input, state: &State, f: impl FnOnce(&View) -> R) -> R {
    let guard = engine.layout().try_borrow().ok();
    let layout = guard.as_ref().and_then(|g| g.as_ref());
    f(&View { engine, i: input, state, layout })
}

fn range(cx: &Cx, id: InstanceId) -> (f64, f64) {
    with_view(cx.engine, cx.i, |v| v.range(id))
}

fn eligible(cx: &Cx, id: InstanceId) -> bool {
    with_view(cx.engine, cx.i, |v| v.eligible(id, false))
}

// Presentation state (read / reset / refresh work on a View so the hooks can use them)

fn presentation_state(engine: &Engine, input: &Input, state: &State, id: InstanceId) -> Presented {
    let scroll = &input.scroll;
    if let Some(existing) = scroll.presented(id) {
        return existing;
    }
    let created = with_view_in(engine, input, state, |v| {
        let position = v.canvas_position(id);
        let (max_x, max_y) = v.range(id);
        Presented {
            epoch: 0.0,
            sequence: -1.0,
            owned: false,
            position,
            velocity: (0.0, 0.0),
            target: position,
            max_x,
            max_y,
            direction: v.direction(id),
            eligible: v.eligible(id, false),
        }
    });
    scroll.insert(id, state.dom.at(id).serial, created);
    created
}

fn reset_presentation(engine: &Engine, input: &Input, state: &State, id: InstanceId) {
    let scroll = &input.scroll;
    let Some(current) = scroll.presented(id) else { return };
    if current.owned {
        scroll.remove_motion(id);
    }
    let position = with_view_in(engine, input, state, |v| v.canvas_position(id));
    scroll.update(id, |p| {
        p.owned = false;
        p.epoch += 1.0;
        p.position = position;
        p.target = position;
        p.velocity = (0.0, 0.0);
    });
}

fn resume_motion(input: &Input, id: InstanceId, position: V2, target: V2, velocity: V2) {
    input.scroll.set_motion(
        id,
        Motion {
            x: Some(Axis { position: position.0 as f64, velocity: velocity.0 as f64, target: target.0 as f64 }),
            y: Some(Axis { position: position.1 as f64, velocity: velocity.1 as f64, target: target.1 as f64 }),
        },
    );
}

fn refresh_presentation(engine: &Engine, input: &Input, state: &State, id: InstanceId) {
    let scroll = &input.scroll;
    let Some(current) = scroll.presented(id) else { return };
    let (max_x, max_y, direction, supported, position, paused_ok) = with_view_in(engine, input, state, |v| {
        let (max_x, max_y) = v.range(id);
        (max_x, max_y, v.direction(id), v.eligible(id, false), v.canvas_position(id), v.scrolling_disabled(id) && v.eligible(id, true))
    });
    if current.max_x != max_x
        || current.max_y != max_y
        || current.direction != direction
        || current.eligible != supported
        || current.position != position
    {
        // A disabled browser-owned scroller hands its last accepted spring back to the runtime,
        // where Step pauses it. Geometry or script-position changes still cancel.
        let paused = current.owned
            && current.eligible
            && !supported
            && paused_ok
            && current.max_x == max_x
            && current.max_y == max_y
            && current.direction == direction
            && current.position == position;
        reset_presentation(engine, input, state, id);
        if paused {
            resume_motion(input, id, position, current.target, current.velocity);
        }
    }
    scroll.update(id, |p| {
        p.max_x = max_x;
        p.max_y = max_y;
        p.direction = direction;
        p.eligible = supported;
    });
}

/// Scrolling.PropertyChanged (from the input's PropertyChanged hook)
pub(crate) fn property_changed(engine: &Engine, input: &Input, state: &mut State, id: InstanceId, name: NameId) {
    let scroll = &input.scroll;
    let n = &input.n;
    if name != n.CanvasPosition && name != n.ScrollingEnabled {
        return;
    }
    // Changed-value script writes cancel the wheel destination. Same-value assignments do not
    // notify this hook and leave it running, as measured in Studio.
    if name == n.CanvasPosition && scroll.motion_writing.get() != Some(id) && scroll.presentation_writing.get() != Some(id) {
        scroll.remove_motion(id);
    }
    if !scroll.enabled.get() || scroll.presented(id).is_none() {
        return;
    }
    if scroll.presentation_writing.get() != Some(id) && name == n.CanvasPosition {
        reset_presentation(engine, input, state, id);
    } else if name == n.ScrollingEnabled {
        crate::engine::layout::ensure_with_state(engine, state, id);
        refresh_presentation(engine, input, state, id);
    }
}

pub(crate) fn enable_presentation(input: &Input, engine: &Engine, enabled: bool) {
    let scroll = &input.scroll;
    scroll.init();
    if scroll.enabled.get() == enabled {
        return;
    }
    scroll.enabled.set(enabled);
    *scroll.pending.borrow_mut() = None;
    scroll.sequence.set(-1.0);
    let ids: Vec<InstanceId> = scroll.presented.borrow().iter().map(|(id, _)| *id).collect();
    let state = engine.state.borrow();
    for id in ids {
        if state.dom.get(id).is_none() {
            continue;
        }
        reset_presentation(engine, input, &state, id);
        scroll.update(id, |p| p.sequence = -1.0);
    }
}

fn finite(value: Option<&Json>) -> Option<f64> {
    value.and_then(Json::as_f64).filter(|v| v.is_finite())
}

fn pair(value: Option<&Json>) -> Option<(f64, f64)> {
    let Some(Json::Array(items)) = value else { return None };
    if items.len() != 2 {
        return None;
    }
    let (a, b) = (finite(items.first())?, finite(items.get(1))?);
    (a.abs() <= 3.402823466e38 && b.abs() <= 3.402823466e38).then_some((a, b))
}

fn sequence(value: Option<&Json>) -> Option<f64> {
    finite(value).filter(|v| *v >= 0.0 && *v <= 9007199254740991.0 && *v == v.floor())
}

fn samples(packet: &Json) -> Vec<&Json> {
    match packet.get("scrolls") {
        Some(Json::Array(items)) => items.iter().collect(),
        Some(Json::Object(map)) => map.values().collect(),
        _ => Vec::new(),
    }
}

fn alive(engine: &Engine, id: InstanceId) -> bool {
    engine.state.borrow().dom.get(id).is_some()
}

fn ensure(engine: &Engine, id: InstanceId) {
    crate::engine::layout::ensure(engine, id);
}

fn apply_presentation(cx: &Cx, packet: &Json, wheel_only: bool, commit_queued: bool) -> LuaResult<bool> {
    let scroll = &cx.i.scroll;
    scroll.init();
    if !scroll.enabled.get() {
        return Ok(false);
    }
    let engine = cx.engine;
    let valid = packet.is_object()
        && packet.get("version").and_then(Json::as_f64) == Some(1.0)
        && sequence(packet.get("seq")).is_some()
        && matches!(packet.get("scrolls"), Some(Json::Array(_) | Json::Object(_)))
        && samples(packet).len() <= 256;
    if !valid || matches!(packet.get("scrolls"), Some(Json::Array(items)) if items.len() > 256) {
        let owned: Vec<InstanceId> = scroll.presented.borrow().iter().filter(|(_, p)| p.owned).map(|(id, _)| *id).collect();
        for id in owned {
            if alive(engine, id) {
                reset_presentation(engine, cx.i, &engine.state.borrow(), id);
            }
        }
        return Ok(false);
    }
    let seq = sequence(packet.get("seq")).unwrap();
    if seq < scroll.sequence.get() {
        return Ok(false);
    }
    let list = samples(packet);
    let mut applied = false;
    let included: FxHashSet<u64> = list
        .iter()
        .filter(|s| s.is_object())
        .filter_map(|s| sequence(s.get("id")))
        .map(|id| id as u64)
        .collect();
    // Each fresh packet is the complete ownership set; omitted scrollers return to runtime motion.
    if seq > scroll.sequence.get() {
        // (only owned frames are released below; nothing in this loop makes another frame owned)
        let ids: Vec<InstanceId> = scroll.presented.borrow().iter().filter(|(_, p)| p.owned).map(|(id, _)| *id).collect();
        for id in ids {
            let Some(current) = scroll.presented(id) else { continue };
            let serial = engine.state.borrow().dom.get(id).map(|i| i.serial as u64);
            if !(current.owned && serial.is_none_or(|s| !included.contains(&s))) {
                continue;
            }
            if serial.is_none() {
                continue;
            }
            ensure(engine, id);
            refresh_presentation(engine, cx.i, &engine.state.borrow(), id);
            let Some(current) = scroll.presented(id) else { continue };
            if !current.owned {
                continue;
            }
            let (target, velocity) = (current.target, current.velocity);
            reset_presentation(engine, cx.i, &engine.state.borrow(), id);
            // Voluntary handoff preserves accepted wheel targets while the runtime resumes from its
            // authoritative pose. Invalid packets and programmatic resets do not.
            if eligible(cx, id) {
                let position = scroll.presented(id).map(|p| p.position).unwrap_or_default();
                resume_motion(cx.i, id, position, target, velocity);
            }
        }
    }
    for sample in list {
        let proxy = if sample.is_object() { sequence(sample.get("id")) } else { None }
            .and_then(|id| scroll.ids.borrow().get(&(id as u32)).copied().filter(|_| id <= u32::MAX as f64))
            .filter(|&id| alive(engine, id));
        let Some(id) = proxy else { continue };
        presentation_state(engine, cx.i, &engine.state.borrow(), id);
        ensure(engine, id);
        refresh_presentation(engine, cx.i, &engine.state.borrow(), id);
        let (epoch, position, velocity, target) =
            match (sequence(sample.get("epoch")), pair(sample.get("position")), pair(sample.get("velocity")), pair(sample.get("target"))) {
                (Some(e), Some(p), Some(v), Some(t)) => (e, p, v, t),
                _ => {
                    reset_presentation(engine, cx.i, &engine.state.borrow(), id);
                    continue;
                }
            };
        let current = scroll.presented(id).unwrap();
        if epoch != current.epoch || seq < current.sequence || (seq == current.sequence && !commit_queued) {
            continue;
        }
        if !current.eligible {
            reset_presentation(engine, cx.i, &engine.state.borrow(), id);
            continue;
        }
        let x = clamp(position.0, 0.0, current.max_x);
        let y = clamp(position.1, 0.0, current.max_y);
        let target_x = clamp(target.0, 0.0, current.max_x);
        let target_y = clamp(target.1, 0.0, current.max_y);
        scroll.update(id, |p| {
            p.owned = true;
            p.sequence = seq;
        });
        scroll.remove_motion(id);
        if !wheel_only {
            scroll.presentation_writing.set(Some(id));
            let value = cx.to_lua(&vector2(x, y));
            let result = value.and_then(|value| cx.assign(id, "CanvasPosition", value));
            scroll.presentation_writing.set(None);
            result?;
            let now = with_view(engine, cx.i, |v| v.canvas_position(id));
            scroll.update(id, |p| p.position = now);
        }
        scroll.update(id, |p| {
            p.target = (target_x as f32, target_y as f32);
            p.velocity = (if x == position.0 { velocity.0 as f32 } else { 0.0 }, if y == position.1 { velocity.1 as f32 } else { 0.0 });
        });
        applied = true;
    }
    scroll.sequence.set(scroll.sequence.get().max(seq));
    Ok(applied || samples(packet).is_empty())
}

/// Scrolling.ApplyPresentation(packet, phase)
pub(crate) fn apply_presentation_phase(cx: &Cx, packet: &Json, phase: &str) -> LuaResult<bool> {
    let scroll = &cx.i.scroll;
    scroll.init();
    match phase {
        "frame" => {
            if !scroll.enabled.get() {
                return Ok(false);
            }
            let pending_seq = scroll.pending.borrow().as_ref().filter(|p| p.is_object()).and_then(|p| sequence(p.get("seq")));
            let packet_seq = if packet.is_object() { sequence(packet.get("seq")) } else { None };
            if let (Some(pending), Some(seq)) = (pending_seq, packet_seq) {
                if seq < pending {
                    return Ok(false);
                }
            }
            *scroll.pending.borrow_mut() = if packet.is_null() { None } else { Some(packet.clone()) };
            Ok(true)
        }
        "wheel" => {
            let applied = apply_presentation(cx, packet, true, false)?;
            if applied {
                *scroll.pending.borrow_mut() = if packet.is_null() { None } else { Some(packet.clone()) };
            }
            Ok(applied)
        }
        _ => apply_presentation(cx, packet, false, false),
    }
}

/// Scrolling.FlushPresentation
pub(crate) fn flush_presentation(cx: &Cx) -> LuaResult<bool> {
    let packet = cx.i.scroll.pending.borrow_mut().take();
    match packet {
        None => Ok(false),
        Some(packet) => apply_presentation(cx, &packet, false, true),
    }
}

/// Whether the root of `id`'s tree is the DataModel (always, in an engine without one)
fn in_data_model(state: &State, id: InstanceId, game: Option<InstanceId>) -> bool {
    if game.is_none() {
        return true;
    }
    let mut current = id;
    while let Some(parent) = state.dom.get(current).and_then(|instance| instance.parent) {
        current = parent;
    }
    Some(current) == game
}

/// Scrolling.PresentationSnapshot
pub(crate) fn snapshot(cx: &Cx) -> Option<Json> {
    let scroll = &cx.i.scroll;
    scroll.init();
    if !scroll.enabled.get() {
        return None;
    }
    let engine = cx.engine;
    // (the ScrollingFrames in slot order, from the dom's class index rather than a walk over
    // every slot)
    // Only frames in the DataModel: a frame in a tree a script removed and still holds (a story that
    // rebuilds its UI and keeps the old one alive through its connections) cannot be scrolled or
    // drawn, and a client has no node for it. Listing them made every reply cost in proportion to
    // every ScrollingFrame the story ever made. Which frames are in the DataModel only changes with
    // a parent change somewhere (the dom's structure version), so the list is kept until then.
    let frames: Vec<InstanceId> = {
        let mut state = engine.state.borrow_mut();
        let guard = engine.layout().borrow();
        let layout = guard.as_ref()?;
        let game = state.collection.game;
        let version = state.dom.structure_version;
        let mut listed = scroll.listed.borrow_mut();
        if game.is_none() || listed.as_ref().map(|(at, _)| *at) != Some(version) {
            let mut frames = cx.i.c.ScrollingFrame.map(|class| state.dom.instances_of_class(class)).unwrap_or_default();
            frames.retain(|&id| !state.dom.at(id).has(flags::DESTROYED) && in_data_model(&state, id, game));
            *listed = Some((version, frames));
        }
        let frames = &listed.as_ref()?.1;
        frames.iter().copied().filter(|&id| state.dom.get(id).is_some() && layout.record(id).is_some()).collect()
    };
    let mut scrolls = Vec::new();
    for id in frames {
        let state = engine.state.borrow();
        presentation_state(engine, cx.i, &state, id);
        refresh_presentation(engine, cx.i, &state, id);
        let current = scroll.presented(id).unwrap();
        let (position, mut velocity, mut target) = (current.position, current.velocity, current.target);
        if let Some(motion) = scroll.motion(id).filter(|_| !current.owned) {
            velocity = (
                motion.x.map_or(0.0, |a| a.velocity) as f32,
                motion.y.map_or(0.0, |a| a.velocity) as f32,
            );
            target = (
                motion.x.map_or(position.0 as f64, |a| a.target) as f32,
                motion.y.map_or(position.1 as f64, |a| a.target) as f32,
            );
        }
        let serial = state.dom.at(id).serial;
        let row = json!({
            "id": serial, "epoch": number(current.epoch), "seq": number(current.sequence),
            "position": [number(position.0 as f64), number(position.1 as f64)],
            "velocity": [number(velocity.0 as f64), number(velocity.1 as f64)],
            "target": [number(target.0 as f64), number(target.1 as f64)],
            "range": [number(current.max_x), number(current.max_y)],
            "direction": direction_name(engine, current.direction), "owned": current.owned, "enabled": current.eligible,
        });
        scrolls.push((serial, row));
    }
    scrolls.sort_by_key(|(serial, _)| *serial);
    let mut result = Map::new();
    result.insert("version".into(), json!(1));
    result.insert("seq".into(), number(scroll.sequence.get()));
    result.insert("scrolls".into(), Json::Array(scrolls.into_iter().map(|(_, row)| row).collect()));
    Some(Json::Object(result))
}

/// The axis target a new notch starts from: the running animation's target, else the position.
fn current_target(cx: &Cx, id: InstanceId, vertical: bool) -> f64 {
    let scroll = &cx.i.scroll;
    if scroll.enabled.get() {
        if let Some(p) = scroll.presented(id).filter(|p| p.owned) {
            return if vertical { p.target.1 as f64 } else { p.target.0 as f64 };
        }
    }
    if let Some(motion) = scroll.motion(id) {
        if let Some(axis) = if vertical { motion.y } else { motion.x } {
            return axis.target;
        }
    }
    let (x, y) = cx.vector2(id, cx.i.n.CanvasPosition);
    if vertical { y } else { x }
}

/// Scrolling.Route: offers a vertical wheel of `notches` (+ = away from the user = up) to the
/// ScrollingFrames of `list` (topmost first). Returns the frame that took it and the frames tried
/// at the end of their range (they fire a MouseWheel event of their own, measured).
pub(crate) fn route(cx: &Cx, list: &[InstanceId], notches: f64, taker: Option<&Json>) -> (Option<InstanceId>, Vec<InstanceId>) {
    let scroll = &cx.i.scroll;
    scroll.init();
    let mut preferred = None;
    if scroll.enabled.get() {
        if let Some(taker) = sequence(taker) {
            for &id in list {
                let serial = cx.engine.state.borrow().dom.get(id).map(|i| i.serial as f64);
                if serial == Some(taker) && scroll.presented(id).is_some_and(|p| p.owned) && eligible(cx, id) {
                    preferred = Some(id);
                    break;
                }
            }
        }
    }
    let mut failed = Vec::new();
    for &id in list {
        let is_frame = with_view(cx.engine, cx.i, |v| v.is_scrolling_frame(id));
        if !is_frame || cx.is_false(id, cx.i.n.ScrollingEnabled) {
            continue;
        }
        let (_, max_y) = range(cx, id);
        if max_y <= 0.0 {
            continue;
        }
        // The browser already accumulated this notch; its selected recipient remains valid when
        // that new target reaches the boundary.
        if Some(id) == preferred {
            return (Some(id), failed);
        }
        let target = current_target(cx, id, true);
        let wanted = target - notches * NOTCH_PIXELS;
        let clamped = clamp(wanted, 0.0, max_y);
        if (clamped - target).abs() > 1e-6 && preferred.is_none() {
            // ScrollingDirection X frames take a vertical wheel without moving (measured)
            let owned = scroll.enabled.get() && scroll.presented(id).is_some_and(|p| p.owned);
            if cx.enum_name(id, cx.i.n.ScrollingDirection) != "X" && !owned {
                let mut motion = scroll.motion(id).unwrap_or_default();
                let (_, position_y) = cx.vector2(id, cx.i.n.CanvasPosition);
                let mut axis = motion.y.unwrap_or(Axis { position: position_y, velocity: 0.0, target: position_y });
                axis.target = clamped;
                motion.y = Some(axis);
                scroll.set_motion(id, motion);
            }
            return (Some(id), failed);
        }
        if preferred.is_some() && (clamped - target).abs() > 1e-6 {
            continue;
        }
        // At the end of its range: its own MouseWheel event fires, the next frame gets the wheel
        failed.push(id);
    }
    (None, failed)
}

/// Studio rounds each operation to float32; regrouping this closed form changes the trajectory.
pub fn step_axis(axis: &mut Axis, dt: f64) {
    let omega = omega();
    let dt = f32r(dt);
    let offset = f32r(axis.position - axis.target);
    let omega_dt = f32r(omega * dt);
    let decay = f32r((-omega_dt).exp());
    let blend = f32r(axis.velocity + f32r(omega * offset));
    let position_term = f32r(f32r(offset * f32r(1.0 + omega_dt)) + f32r(axis.velocity * dt));
    axis.position = f32r(axis.target + f32r(position_term * decay));
    axis.velocity = f32r(f32r(axis.velocity - f32r(omega * f32r(blend * dt))) * decay);
    if (axis.target - axis.position).abs() < SNAP_DISTANCE {
        axis.position = axis.target;
        axis.velocity = 0.0;
    }
}

/// Scrolling.Step: advances every smooth scroll by one frame and writes CanvasPosition (a normal
/// property write: change signals fire every frame, like in Studio).
pub(crate) fn step(cx: &Cx, dt: f64) -> LuaResult<()> {
    let scroll = &cx.i.scroll;
    flush_presentation(cx)?;
    let ids: Vec<InstanceId> = scroll.motions.borrow().iter().map(|(id, _)| *id).collect();
    for id in ids {
        let Some(mut motion) = scroll.motion(id) else { continue };
        if !cx.alive(id) {
            scroll.remove_motion(id);
            continue;
        }
        let current = cx.vector2(id, cx.i.n.CanvasPosition);
        // Layout can silently clamp CanvasPosition when no property listener exists. A forced
        // clamp cancels the destination just like a notified changed-value write.
        if motion.x.is_some_and(|a| a.position != current.0) || motion.y.is_some_and(|a| a.position != current.1) {
            scroll.remove_motion(id);
            continue;
        }
        // Disabling scrolling pauses the spring; enabling it resumes the same state.
        if cx.is_false(id, cx.i.n.ScrollingEnabled) {
            continue;
        }
        let (mut x, mut y) = current;
        let mut done = true;
        if let Some(axis) = motion.x.as_mut() {
            step_axis(axis, dt);
            x = axis.position;
            done = done && axis.position == axis.target;
        }
        if let Some(axis) = motion.y.as_mut() {
            step_axis(axis, dt);
            y = axis.position;
            done = done && axis.position == axis.target;
        }
        scroll.set_motion(id, motion);
        let next = (x as f32, y as f32);
        if next != (current.0 as f32, current.1 as f32) {
            scroll.motion_writing.set(Some(id));
            let value = cx.to_lua(&vector2(x, y));
            let result = value.and_then(|value| cx.assign(id, "CanvasPosition", value));
            scroll.motion_writing.set(None);
            result?;
        }
        if done {
            scroll.remove_motion(id);
        }
    }
    Ok(())
}

/// Scrolling.Stop
pub(crate) fn stop(cx: &Cx, id: InstanceId) {
    let scroll = &cx.i.scroll;
    scroll.remove_motion(id);
    if scroll.enabled.get() && scroll.presented(id).is_some() && alive(cx.engine, id) {
        reset_presentation(cx.engine, cx.i, &cx.engine.state.borrow(), id);
    }
}

// Scroll bar thumbs, measured (tools/interaction/experiments/thumb.mjs): pressing the thumb and
// moving scrolls immediately by mouse delta * max / (track - thumb) (track = AbsoluteWindowSize
// along the bar, thumb = max(2 * thickness, track * track / canvas)); the drag continues outside
// the frame and clamps; a press on the track outside the thumb does nothing; dragging the content
// does nothing with a mouse.

enum Thumb {
    None,
    Track,
    Axis(bool, f64),
}

fn thumb_at(cx: &Cx, id: InstanceId, x: f64, y: f64) -> Thumb {
    let Some((rx, ry, rw, rh, bars)) = cx.record(id, |r| (r.x, r.y, r.w, r.h, r.scroll_bars)) else { return Thumb::None };
    let Some(bars) = bars else { return Thumb::None };
    let thickness = bars.thickness;
    let n = &cx.i.n;
    let canvas = cx.vector2(id, n.AbsoluteCanvasSize);
    let window = cx.vector2(id, n.AbsoluteWindowSize);
    let position = cx.vector2(id, n.CanvasPosition);
    let (max_x, max_y) = range(cx, id);

    if bars.vertical && canvas.1 > 0.0 {
        let left = if bars.left { rx } else { rx + rw - thickness };
        if x >= left && x < left + thickness && y >= ry && y < ry + window.1 {
            let track = window.1;
            let thumb = (2.0 * thickness).max(track * track / canvas.1);
            let travel = (track - thumb).max(0.0);
            let top = ry + if max_y > 0.0 { position.1 / max_y * travel } else { 0.0 };
            if y >= top && y < top + thumb && travel > 0.0 {
                return Thumb::Axis(true, max_y / travel);
            }
            return Thumb::Track;
        }
    }
    if bars.horizontal && canvas.0 > 0.0 {
        let top = ry + rh - thickness;
        let left0 = if bars.left && bars.vertical { rx + thickness + 1.0 } else { rx };
        if y >= top && y < top + thickness && x >= left0 && x < left0 + window.0 {
            let track = window.0;
            let thumb = (2.0 * thickness).max(track * track / canvas.0);
            let travel = (track - thumb).max(0.0);
            let start = left0 + if max_x > 0.0 { position.0 / max_x * travel } else { 0.0 };
            if x >= start && x < start + thumb && travel > 0.0 {
                return Thumb::Axis(false, max_x / travel);
            }
            return Thumb::Track;
        }
    }
    Thumb::None
}

/// Scrolling.Press: a left press over `list` grabs a thumb of the first ScrollingFrame whose bar
/// is under the point. True when the press landed on a bar (thumb or track).
pub(crate) fn press(cx: &Cx, list: &[InstanceId], x: f64, y: f64) -> bool {
    for &id in list {
        let is_frame = with_view(cx.engine, cx.i, |v| v.is_scrolling_frame(id));
        if !is_frame || cx.is_false(id, cx.i.n.ScrollingEnabled) {
            continue;
        }
        match thumb_at(cx, id, x, y) {
            Thumb::Axis(vertical, ratio) => {
                let position = cx.vector2(id, cx.i.n.CanvasPosition);
                stop(cx, id);
                *cx.i.scroll.grabbed.borrow_mut() = Some(Grab {
                    target: id,
                    vertical,
                    start_mouse: if vertical { y } else { x },
                    start_canvas: if vertical { position.1 } else { position.0 },
                    ratio,
                });
                return true;
            }
            Thumb::Track => return true,
            Thumb::None => {}
        }
    }
    false
}

/// Scrolling.Drag
pub(crate) fn drag(cx: &Cx, x: f64, y: f64) -> LuaResult<()> {
    let Some(grab) = *cx.i.scroll.grabbed.borrow() else { return Ok(()) };
    if !cx.alive(grab.target) {
        *cx.i.scroll.grabbed.borrow_mut() = None;
        return Ok(());
    }
    let (max_x, max_y) = range(cx, grab.target);
    let position = cx.vector2(grab.target, cx.i.n.CanvasPosition);
    let next = if grab.vertical {
        vector2(position.0, clamp(grab.start_canvas + (y - grab.start_mouse) * grab.ratio, 0.0, max_y))
    } else {
        vector2(clamp(grab.start_canvas + (x - grab.start_mouse) * grab.ratio, 0.0, max_x), position.1)
    };
    let value = cx.to_lua(&next)?;
    cx.assign(grab.target, "CanvasPosition", value)
}

/// Scrolling.Release
pub(crate) fn release(cx: &Cx) {
    *cx.i.scroll.grabbed.borrow_mut() = None;
}
