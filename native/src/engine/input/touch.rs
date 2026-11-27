//! Touch input (`--device touch`): any number of fingers, and dragging a ScrollingFrame with a finger.
//!
//! Measured in a 0.747 client (touches injected with VirtualInputManager:SendTouchEvent, driven frame by frame
//! from RenderStepped; notes/interaction.md "Touch", "Two fingers" and "Touch drag-scrolling"):
//!   - Every touch has an InputObject of its own (a new one each time a finger comes down) and its own hover: an
//!     object two fingers are over gets MouseEnter / InputBegan / MouseLeave / InputEnded from each of them.
//!   - Events are processed in the order the fingers send them; a finger's moves within one frame are one move.
//!   - The first finger down on a button takes it: a second finger on the same button gets MouseButton1Down and
//!     MouseButton1Up but never Activated or MouseButton1Click.
//!   - GuiState: Press where a finger comes down, Idle where any finger leaves or lifts (even with another finger
//!     still on the object), Press again where a finger moves onto an object a finger that is still down came down
//!     on.
//!   - The legacy Mouse and GetMouseLocation follow the latest finger event; Mouse.Button1Down / Button1Up fire for
//!     every finger that does not land on a button.
//!   - Dragging: of the topmost ScrollingFrame under a finger that comes down and its ScrollingFrame ancestors, the
//!     outermost that can scroll is that finger's while no other finger holds it (touch_frame). It starts to scroll
//!     once the finger is more than 11.5 px from where it came down and the travel's longer axis is one the frame can
//!     scroll (else never; start_axis decides at once or waits), that move scrolling nothing and being the last
//!     InputChanged the frame gets from it, then follows the finger 1:1. Past an edge the overscroll is
//!     min(previous + step, band(max(u, S))), band(x) = (1 - 1 / (0.55 x / d + 1)) d (d the window
//!     along the axis, u the finger's travel since the canvas was last inside, S its distance from where it came
//!     down when the drag started).
//!   - Releasing: past an edge the canvas springs back (a critically damped spring, omega = 6 pi, as the wheel's
//!     with a lower omega); inside, it coasts at 1.305 x the mean of the finger's last 5 per-frame velocities: each
//!     frame x -= v h and v *= 1 - 2.35 h (h = 0.99021 dt), stopping once |v| < 17.477 px/s. Coasting past an edge
//!     starts a spring from rest at the last frame inside, f(0.1714 |v| - 0.89 p) out (p the distance left to the
//!     edge), and the canvas shows whichever of the two is nearer the edge.
//!   - A finger coming down on the ScrollingFrame stops its coasting (after that frame's step), a touch elsewhere
//!     does not, and a script write of CanvasPosition does not either. When the finger dragging a frame lifts,
//!     another finger on it takes the frame over at a move that follows a move of its own in the frame before; its
//!     release velocity counts from where it was before that move.
//!   - ScrollingEnabled false ends a coast, holds a spring back and stops a drag from following (Drag::paused / held).
//! Gestures (TouchTap, TouchPinch, TouchRotate, TouchPan, TouchSwipe, TouchLongPress) never fired from injected
//! touches, with one finger or two, so none is emulated.

use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
};

use mlua::prelude::*;
use rustc_hash::FxHashSet;
use serde_json::Value as Json;

use super::{
    Cx, Press, activation_count, button_key, deliver_to_actions, detector_under, drag, json_number, position_delta,
    scrolling::{self, Axis},
    textbox, vector2, vector3,
};
use crate::engine::{GuiEngine, InstanceId, NameId, dom::flags, instance as core, lua::instance_value, value::{Str, Value}};

/// The highest finger number an event may name
pub const MAX_FINGER: u32 = 9;

/// Measured: a drag starts once the finger is more than this far from where it came down (11 px does not start it, 12
/// does, in 1 to 12 px steps; P6c: 11.40 px (3, 11) and (7, 9) do not, 11.70 px (4, 11) does)
const SLOP: f64 = 11.5;
/// Past the slop (P6c, measured on Y, X and XY frames in 24 directions): the travel's longer axis decides at once when
/// it is at least DECIDED times the shorter one ((7, 12), (4, 11), (11, 4) do; (6, 10), (7, 10), (8, 9) wait: the ratio
/// lies in (1.667, 1.714]); an exact diagonal decides at once too; a drag that waits starts at a move that leaves the
/// longer axis more than WAIT px from where the finger came down and itself goes more than WAIT_STEP px along it
/// (whatever the ratio by then: (7, -10) then (7, -15), (7, -20) wait, (7, -25) starts; (8, -21) reached by 2 px waits)
const DECIDED: f64 = 1.7;
const WAIT: f64 = 20.0;
const WAIT_STEP: f64 = 2.0;
/// The release velocity: GAIN x the mean of the finger's last SAMPLES per-frame velocities
const GAIN: f64 = 1.305;
const SAMPLES: usize = 5;
/// Coasting: each frame x -= v h, v *= 1 - FRICTION h, h = STEP_SCALE dt; stops once |v| < STOP px/s
const FRICTION: f64 = 2.35;
const STEP_SCALE: f64 = 0.99021;
const STOP: f64 = 17.477;
/// The rubber band past an edge: band(x) = (1 - 1 / (BAND x / d + 1)) d
const BAND: f64 = 0.55;
/// A coast past an edge bounces from band(BOUNCE_V |v| - BOUNCE_P p) out
const BOUNCE_V: f64 = 0.1714;
const BOUNCE_P: f64 = 0.89;

#[inline]
fn f32r(value: f64) -> f64 {
    value as f32 as f64
}

/// The spring that brings an overscrolled canvas back (the wheel's spring with omega = 6 pi)
fn spring_omega() -> f64 {
    f32r(6.0 * std::f64::consts::PI)
}

/// The rubber band: how far past an edge `travel` px of finger past it shows, for a window `size` px long
fn band(travel: f64, size: f64) -> f64 {
    if travel <= 0.0 || size <= 0.0 {
        return 0.0;
    }
    (1.0 - 1.0 / (travel * BAND / size + 1.0)) * size
}

/// The travel that shows `over` px past an edge (band's inverse)
fn band_inverse(over: f64, size: f64) -> f64 {
    if over <= 0.0 || size <= 0.0 {
        return 0.0;
    }
    let ratio = (over / size).min(0.999_999);
    (1.0 / (1.0 - ratio) - 1.0) * size / BAND
}

/// A finger dragging (or about to drag) a ScrollingFrame
#[derive(Clone, Copy, Debug)]
struct Drag {
    frame: InstanceId,
    /// Where the finger came down (None: it took the frame over and starts counting at its next move)
    origin: Option<(f64, f64)>,
    dragging: bool,
    /// Past the slop at an angle that does not decide the axis yet (DECIDED, WAIT)
    waiting: bool,
    /// ScrollingEnabled false during the drag (measured, P6c): once the drag has scrolled, moves while the frame is
    /// disabled are lost and the first move after does not scroll (`paused`); a drag that started but has not scrolled
    /// yet catches up the finger's whole travel from where it started at the first move after (`held`); before the drag
    /// starts, the slop counts again from the first move after (`reanchor`)
    paused: bool,
    held: bool,
    reanchor: bool,
    scrolled: bool,
    /// Where the finger was when the drag started
    started_at: (f64, f64),
    /// Its distance from `origin` when the drag started
    start: f64,
    /// Per axis, where the finger was the last time the canvas was inside the range
    inside: (f64, f64),
}

impl Drag {
    fn new(frame: InstanceId, origin: Option<(f64, f64)>, at: (f64, f64)) -> Self {
        Drag {
            frame,
            origin,
            dragging: false,
            waiting: false,
            paused: false,
            held: false,
            reanchor: false,
            scrolled: false,
            started_at: at,
            start: 0.0,
            inside: at,
        }
    }
}

/// A finger that is down
struct Finger {
    id: u32,
    /// Its InputObject (a new one per touch)
    object: InstanceId,
    value: Option<LuaValue>,
    x: f64,
    y: f64,
    /// A move waiting for the frame
    pending: Option<(f64, f64)>,
    /// The objects it is over (MouseEnter fired), topmost first
    hover: Vec<InstanceId>,
    /// The objects it came down on
    pressed: Vec<InstanceId>,
    press: Press,
    /// It took `press.button` (no other finger held it)
    takes: bool,
    drag: Option<Drag>,
    /// (frame, x, y) where it was, one per frame, for the release velocity
    samples: Vec<(u64, f64, f64)>,
}

/// A ScrollingFrame's coasting / spring on one axis
#[derive(Clone, Copy, Debug, Default)]
struct Coast {
    /// px/s in the finger's direction (the canvas moves against it); 0 = not coasting
    velocity: f32,
    spring: Option<Axis>,
    /// The spring overtook the coast this frame: the coast ends at the next
    handed: bool,
}

impl Coast {
    fn active(&self) -> bool {
        self.velocity != 0.0 || self.spring.is_some()
    }
}

#[derive(Clone, Copy, Debug)]
struct Motion {
    x: Coast,
    y: Coast,
    /// The frame it began in (it does not move in that frame) and the last frame it moved in
    began: u64,
    stepped: u64,
}

#[derive(Default)]
pub(crate) struct Touch {
    fingers: RefCell<Vec<Finger>>,
    /// Fingers with a pending move, in the order the moves came
    order: RefCell<Vec<u32>>,
    /// Objects showing GuiState Press
    pub shown: RefCell<FxHashSet<InstanceId>>,
    /// Frames counted by the input's step; events between two steps belong to the next frame
    frame: Cell<u64>,
    times: RefCell<VecDeque<(u64, f64)>>,
    last_dt: Cell<f64>,
    motions: RefCell<Vec<(InstanceId, Motion)>>,
    /// The finger driving the UIDragDetector / Draggable drag or the scroll bar thumb
    holder: Cell<Option<u32>>,
    /// The ScrollingFrames the layout lets past their range
    overscroll: RefCell<FxHashSet<InstanceId>>,
}

impl Touch {
    pub fn any_down(&self) -> bool {
        !self.fingers.borrow().is_empty()
    }

    /// Starts frame `frame` at virtual time `now`
    pub fn begin_frame(&self, now: f64, dt: f64) {
        let frame = self.frame.get() + 1;
        self.frame.set(frame);
        let mut times = self.times.borrow_mut();
        times.push_back((frame, now));
        while times.len() > 64 {
            times.pop_front();
        }
        if dt > 0.0 {
            self.last_dt.set(dt);
        }
    }

    fn time_of(&self, frame: u64) -> Option<f64> {
        self.times.borrow().iter().find(|(f, _)| *f == frame).map(|(_, t)| *t)
    }

    fn dt(&self) -> f64 {
        let dt = self.last_dt.get();
        if dt > 0.0 { dt } else { 1.0 / 60.0 }
    }

    /// GetScrollVelocity: a coasting frame's velocity, (0, 0) otherwise
    pub fn velocity(&self, frame: InstanceId) -> (f64, f64) {
        self.motions
            .borrow()
            .iter()
            .find(|(id, _)| *id == frame)
            .map_or((0.0, 0.0), |(_, m)| (m.x.velocity as f64, m.y.velocity as f64))
    }

    /// Drops a ScrollingFrame's coasting (ScrollToTop, ResetScrollVelocity, a freed frame)
    pub fn forget(&self, frame: InstanceId) {
        self.motions.borrow_mut().retain(|(id, _)| *id != frame);
    }

    /// Instances held while touches are down or frames coast (pinned like the press state)
    pub fn held(&self, out: &mut FxHashSet<InstanceId>) {
        for finger in self.fingers.borrow().iter() {
            out.insert(finger.object);
            out.extend(finger.hover.iter().copied());
            out.extend(finger.pressed.iter().copied());
            out.extend(finger.press.button);
            out.extend(finger.press.sink);
            out.extend(finger.press.receivers.iter().copied());
            out.extend(finger.drag.map(|d| d.frame));
        }
        out.extend(self.motions.borrow().iter().map(|(id, _)| *id));
    }
}

/// The finger an event names: `finger` (a whole number from 0 to MAX_FINGER), 0 when left out
pub fn finger_of(event: &Json) -> u32 {
    json_number(event.get("finger"))
        .filter(|f| *f >= 0.0 && f.fract() == 0.0 && *f <= MAX_FINGER as f64)
        .map_or(0, |f| f as u32)
}

/// An event's point for finger `id`: its x, y, else where that finger is, else the pointer (whole logical px, as
/// for the mouse)
pub(crate) fn point(cx: &Cx, id: u32, event: &Json) -> (f64, f64) {
    let (dx, dy) = cx.i.touches.fingers.borrow().iter().find(|f| f.id == id).map_or_else(|| cx.i.mouse_location(), |f| (f.x, f.y));
    let x = json_number(event.get("x")).unwrap_or(dx);
    let y = json_number(event.get("y")).unwrap_or(dy);
    ((x + 0.5).floor(), (y + 0.5).floor())
}

impl Cx<'_> {
    /// A new InputObject for a touch (a client makes one per touch)
    fn touch_object(&self) -> (InstanceId, Option<LuaValue>) {
        let n = &self.i.n;
        let created = {
            let mut state = self.engine.state.borrow_mut();
            let class = self.i.c.InputObject.expect("InputObject class");
            core::create(&mut state, &self.engine.reflection, class)
        };
        self.set_silent(created, n.Name, Value::String(Str::from("InputObject")));
        self.set_silent(created, n.UserInputType, self.item("UserInputType", "Touch"));
        self.set_silent(created, n.UserInputState, self.item("UserInputState", "None"));
        self.set_silent(created, n.KeyCode, self.item("KeyCode", "Unknown"));
        self.set_silent(created, n.Position, vector3(0.0, 0.0, 0.0));
        self.set_silent(created, n.Delta, vector3(0.0, 0.0, 0.0));
        (created, instance_value(self.lua, self.engine, created).ok())
    }

    /// The pointer follows the latest finger event (GetMouseLocation, the legacy Mouse)
    fn follow(&self, x: f64, y: f64) -> LuaResult<()> {
        let mut mouse = self.i.mouse.get();
        mouse.x = x;
        mouse.y = y;
        mouse.processed_x = x;
        mouse.processed_y = y;
        mouse.pending = false;
        mouse.positioned = true;
        self.i.mouse.set(mouse);
        self.update_legacy_mouse()
    }

    /// The objects any finger is over (the input's hover list: pins, the per-frame state refresh)
    fn refresh_touch_hover(&self) {
        let mut list = Vec::new();
        let mut set = FxHashSet::default();
        for finger in self.i.touches.fingers.borrow().iter() {
            for &id in &finger.hover {
                if set.insert(id) {
                    list.push(id);
                }
            }
        }
        *self.i.hovered.borrow_mut() = list;
        *self.i.hovered_set.borrow_mut() = set;
    }

    fn is_scroller(&self, id: InstanceId) -> bool {
        self.class_name_is(id, self.i.c.ScrollingFrame) && !self.is_false(id, self.i.n.ScrollingEnabled)
    }

    /// The finger holding ScrollingFrame `frame`, if any
    fn frame_holder(&self, frame: InstanceId) -> Option<u32> {
        self.i.touches.fingers.borrow().iter().find(|f| f.drag.is_some_and(|d| d.frame == frame)).map(|f| f.id)
    }

    /// A ScrollingFrame under the finger that no other finger holds: the finger's input counts as processed
    fn free_scroller_under(&self, id: u32, list: &[InstanceId]) -> bool {
        list.iter().any(|&o| self.is_scroller(o) && self.frame_holder(o).is_none_or(|holder| holder == id))
    }

    /// Processed for a finger going down or moving: an Active object, a GuiButton / TextBox, a UIDragDetector or a
    /// ScrollingFrame no other finger holds under it (measured: a second finger on a frame another finger drags is
    /// not processed)
    fn finger_processed(&self, id: u32, list: &[InstanceId]) -> bool {
        self.any_active(list) || self.sink_of(list).0.is_some() || detector_under(self, list) || self.free_scroller_under(id, list)
    }

    fn touch_signals(&self, event: NameId, object: InstanceId, processed: bool) {
        self.set_last_input_type("Touch");
        self.fire_uis(event, vec![Value::Instance(object), Value::Bool(processed)]);
    }
}

/// A move of finger `id`: the pointer follows at once, the finger's own update waits for the frame (one per frame, at
/// its latest point). A finger that is not down only moves the pointer.
pub(crate) fn moved(cx: &Cx, id: u32, x: f64, y: f64) -> LuaResult<()> {
    let queued = {
        let mut fingers = cx.i.touches.fingers.borrow_mut();
        match fingers.iter_mut().find(|f| f.id == id) {
            Some(finger) => {
                finger.pending = Some((x, y));
                true
            }
            None => false,
        }
    };
    if queued {
        let mut order = cx.i.touches.order.borrow_mut();
        if !order.contains(&id) {
            order.push(id);
        }
    }
    let mut mouse = cx.i.mouse.get();
    mouse.x = x;
    mouse.y = y;
    mouse.positioned = true;
    cx.i.mouse.set(mouse);
    cx.update_legacy_mouse()
}

/// Per frame: each finger's waiting move, in the order the moves came
pub(crate) fn process_moves(cx: &Cx) -> LuaResult<()> {
    process_moves_in(cx, cx.i.touches.frame.get())
}

/// The waiting moves as frame `frame`'s (before a finger goes down or lifts, a client handles a frame's touch events
/// in the order they came)
fn process_moves_in(cx: &Cx, frame: u64) -> LuaResult<()> {
    let order = std::mem::take(&mut *cx.i.touches.order.borrow_mut());
    for id in order {
        let pending = cx.i.touches.fingers.borrow_mut().iter_mut().find(|f| f.id == id).and_then(|f| f.pending.take());
        if let Some((x, y)) = pending {
            finger_moved(cx, id, x, y, frame)?;
            // (each touch event's handlers run before the next one's, as for dispatched events)
            super::scheduler::flush_deferred(cx.lua, cx.engine)?;
        }
    }
    Ok(())
}

/// Finger `id` moved to (x, y) in frame `frame`: MouseMoved on the objects under it, the legacy Mouse.Move,
/// InputChanged on those it stays on (not on a ScrollingFrame it drags, nor inside it), MouseLeave + InputEnded on those
/// it left, MouseEnter + InputBegan on those it entered, then the drag and UIS InputChanged and TouchMoved.
fn finger_moved(cx: &Cx, id: u32, x: f64, y: f64, frame: u64) -> LuaResult<()> {
    let n = &cx.i.n;
    let Some((object, from, hover, drag_before, moved_before)) = cx
        .i
        .touches
        .fingers
        .borrow()
        .iter()
        .find(|f| f.id == id)
        .map(|f| (f.object, (f.x, f.y), f.hover.clone(), f.drag, f.samples.last().is_some_and(|s| s.0 + 1 == frame)))
    else {
        return Ok(());
    };
    cx.follow(x, y)?;
    let (position, delta) = position_delta(cx, object, x, y);
    cx.set_input(object, "Change", position, Some(delta));
    let list = cx.hit_list(x, y);
    let now: FxHashSet<InstanceId> = list.iter().copied().collect();
    let point = || vec![Value::Number(x), Value::Number(y)];

    // The drag: a finger that took a frame over starts counting here; past the slop the drag starts (this move still
    // reaches the frame as InputChanged); a dragging finger's moves scroll the frame
    let mut drag = drag_before;
    let mut scroll_to = None;
    let mut took_over = false;
    let mut refused = false;
    if let Some(d) = drag.as_mut() {
        match d.origin {
            // Measured (P6c): a finger takes the frame over at a move that follows a move of its own in the frame
            // before (a finger that rested since it came down takes it at its second move)
            None if moved_before => {
                d.origin = Some((x, y));
                stop_motion(cx, d.frame, frame);
                took_over = true;
            }
            None => {}
            Some(_) if !d.dragging && cx.is_false(d.frame, cx.i.n.ScrollingEnabled) => d.reanchor = true,
            Some(_) if !d.dragging && d.reanchor => {
                d.origin = Some((x, y));
                d.reanchor = false;
            }
            Some(origin) if !d.dragging => {
                let (tx, ty) = (x - origin.0, y - origin.1);
                let distance = tx.hypot(ty);
                if d.waiting || distance > SLOP {
                    match start_axis(cx, d.frame, tx, ty, (x - from.0, y - from.1), d.waiting) {
                        Some(true) => {
                            d.dragging = true;
                            d.waiting = false;
                            d.start = distance;
                            d.started_at = (x, y);
                            d.inside = inside_point(cx, d.frame, (x, y));
                        }
                        Some(false) => d.waiting = true,
                        None => refused = true,
                    }
                }
            }
            Some(_) => scroll_to = Some(d.frame),
        }
    }
    if refused {
        // (the finger lets the frame go: this touch never scrolls it)
        drag = None;
    }
    let dragged_frame = drag_before.filter(|d| d.dragging).map(|d| d.frame);
    let inside_dragged = |o: InstanceId| {
        dragged_frame.is_some_and(|frame| o == frame || cx.engine.state.borrow().dom.is_descendant_of(o, frame))
    };

    for &o in &list {
        cx.fire(o, n.MouseMoved, point());
    }
    cx.fire_legacy("Move", Vec::new())?;
    for &o in &list {
        if hover.contains(&o) && !inside_dragged(o) {
            cx.fire(o, n.InputChanged, vec![Value::Instance(object)]);
        }
    }
    for &o in &hover {
        if !now.contains(&o) {
            cx.i.touches.shown.borrow_mut().remove(&o);
            cx.i.touch(o);
            if cx.alive(o) {
                cx.fire(o, n.MouseLeave, point());
                cx.fire(o, n.InputEnded, vec![Value::Instance(object)]);
            }
        }
    }
    cx.flush_states();
    let pressed_by_any = |o: InstanceId| cx.i.touches.fingers.borrow().iter().any(|f| f.pressed.contains(&o));
    for &o in &list {
        if !hover.contains(&o) {
            if pressed_by_any(o) {
                cx.i.touches.shown.borrow_mut().insert(o);
                cx.i.touch(o);
            }
            cx.fire(o, n.MouseEnter, point());
            cx.fire(o, n.InputBegan, vec![Value::Instance(object)]);
        }
    }
    {
        let mut fingers = cx.i.touches.fingers.borrow_mut();
        if let Some(finger) = fingers.iter_mut().find(|f| f.id == id) {
            finger.x = x;
            finger.y = y;
            finger.hover = list.clone();
            finger.drag = drag;
            // (measured, IN4: a finger that leaves the button it came down on and comes back still clicks it at the
            // release; only a drag cancels the click)
            if drag.is_some_and(|d| d.dragging) {
                finger.press.clickable = false;
                finger.press.dragged = true;
            }
            if took_over {
                // Measured (P6c, IN12b): the release velocity of a finger that took the frame over counts from where
                // the finger was before that move, at that move's time (its first interval holds two moves: 6 moves
                // of 10 px from the takeover read 1.2 x a lone drag's velocity, 4 moves 1.33 x, 20 moves 1 x)
                finger.samples.clear();
                finger.samples.push((frame, from.0, from.1));
            } else {
                match finger.samples.last_mut() {
                    Some(last) if last.0 == frame => *last = (frame, x, y),
                    _ => finger.samples.push((frame, x, y)),
                }
            }
            if finger.samples.len() > 16 {
                finger.samples.remove(0);
            }
        }
    }
    cx.refresh_touch_hover();
    cx.flush_states();
    if cx.i.touches.holder.get() == Some(id) {
        scrolling::drag(cx, x, y)?;
        drag::move_to(cx, x, y)?;
    }
    if let Some(frame_id) = scroll_to {
        drag_frame(cx, id, frame_id, from, (x, y))?;
    }
    let processed = cx.finger_processed(id, &list);
    cx.touch_signals(n.InputChanged, object, processed);
    cx.touch_signals(n.TouchMoved, object, processed);
    sync_overscroll(cx);
    Ok(())
}

/// Finger `id` came down at (x, y): MouseButton1Down on the button it lands on (else the legacy Mouse.Button1Down),
/// MouseEnter + InputBegan(Touch Begin) on the objects under it, which show Press, then UIS InputBegan and
/// TouchStarted. It stops the coasting of the ScrollingFrames under it and holds the topmost one no other finger
/// holds.
pub(crate) fn down(cx: &Cx, id: u32, x: f64, y: f64) -> LuaResult<()> {
    process_moves_in(cx, cx.i.touches.frame.get() + 1)?;
    if cx.i.touches.fingers.borrow().iter().any(|f| f.id == id) {
        // A finger that is down again: its touch ends where it is first
        let (fx, fy) = cx.i.touches.fingers.borrow().iter().find(|f| f.id == id).map(|f| (f.x, f.y)).unwrap();
        up(cx, id, fx, fy)?;
    }
    let n = &cx.i.n;
    let frame = cx.i.touches.frame.get() + 1;
    let first = !cx.i.touches.any_down();
    cx.follow(x, y)?;
    cx.i.clicked_this_frame.set(true);
    let (object, value) = cx.touch_object();
    cx.set_input(object, "Begin", vector3(x, y - cx.inset(), 0.0), Some(vector3(0.0, 0.0, 0.0)));
    let list = cx.hit_list(x, y);
    let (sink, _) = cx.sink_of(&list);
    let button = sink.filter(|&sink| cx.is(sink, cx.i.c.GuiButton));
    let takes = button.is_some_and(|b| !cx.i.touches.fingers.borrow().iter().any(|f| f.takes && f.press.button == Some(b)));
    for &o in &list {
        if cx.class_name_is(o, cx.i.c.ScrollingFrame) {
            stop_motion(cx, o, frame);
        }
    }
    let processed = cx.finger_processed(id, &list);
    match button {
        Some(button) => cx.fire(button, n.MouseButton1Down, vec![Value::Number(x), Value::Number(y)]),
        None => cx.fire_legacy("Button1Down", Vec::new())?,
    }
    // Measured: a focused TextBox loses focus after the legacy Mouse.Button1Down
    if let Some(focused) = textbox::focused_box(cx) {
        if sink != Some(focused) {
            textbox::unfocus(cx, focused, false, Some(object), true);
        }
    }
    let mut press = Press { button, sink, clickable: button.is_some(), receivers: list.clone(), dragged: false };
    let mut grabbed = false;
    if cx.i.touches.holder.get().is_none() {
        let bar = scrolling::press(cx, &list, x, y);
        grabbed = bar;
        if bar {
            cx.i.touches.holder.set(Some(id));
        } else if drag::press(cx, &list, x, y, cx.engine.options.edit_mode()) {
            cx.i.touches.holder.set(Some(id));
            let detected = cx.i.drag.borrow().as_ref().is_some_and(|active| active.detector.is_some() && Some(active.target) == button);
            if detected {
                press.clickable = false;
                press.dragged = true;
            }
        }
    }
    let scroller = if grabbed { None } else { touch_frame(cx, &list) };
    for &o in &list {
        cx.i.touches.shown.borrow_mut().insert(o);
        cx.i.touch(o);
        cx.fire(o, n.MouseEnter, vec![Value::Number(x), Value::Number(y)]);
        cx.fire(o, n.InputBegan, vec![Value::Instance(object)]);
    }
    cx.i.touches.fingers.borrow_mut().push(Finger {
        id,
        object,
        value,
        x,
        y,
        pending: None,
        hover: list.clone(),
        pressed: list.clone(),
        press,
        takes,
        drag: scroller.map(|frame| Drag::new(frame, Some((x, y)), (x, y))),
        samples: vec![(frame, x, y)],
    });
    cx.refresh_touch_hover();
    cx.flush_states();
    cx.touch_signals(n.InputBegan, object, processed);
    cx.touch_signals(n.TouchStarted, object, processed);
    if first && !processed {
        deliver_to_actions(cx, button_key(cx, 1.0), true);
    }
    sync_overscroll(cx);
    Ok(())
}

/// Finger `id` lifted at (x, y) (the frame's waiting moves first): on the button it came down on and took MouseButton1Up,
/// Activated(touch, count) and (unless it left the button or dragged) MouseButton1Click; on another button only
/// MouseButton1Up; elsewhere the legacy Mouse.Button1Up, and a TextBox it came down on takes focus. Then MouseLeave +
/// InputEnded(Touch End) on the objects it is over (Idle), UIS InputEnded and TouchEnded (InputEnded before the
/// MouseLeaves when it dragged a ScrollingFrame, measured). The frame it dragged springs back or coasts.
pub(crate) fn up(cx: &Cx, id: u32, x: f64, y: f64) -> LuaResult<()> {
    process_moves_in(cx, cx.i.touches.frame.get() + 1)?;
    let finger = {
        let mut fingers = cx.i.touches.fingers.borrow_mut();
        let index = fingers.iter().position(|f| f.id == id);
        index.map(|index| fingers.remove(index))
    };
    let Some(finger) = finger else {
        return cx.follow(x, y);
    };
    let n = &cx.i.n;
    cx.follow(x, y)?;
    let object = finger.object;
    cx.set_input(object, "End", vector3(x, y - cx.inset(), 0.0), Some(vector3(0.0, 0.0, 0.0)));
    if cx.i.touches.holder.get() == Some(id) {
        cx.i.touches.holder.set(None);
        scrolling::release(cx);
        drag::release(cx, x, y);
    }
    let dragging = finger.drag.is_some_and(|d| d.dragging);
    let list = cx.hit_list(x, y);
    let (sink, _) = cx.sink_of(&list);
    match sink.filter(|&sink| cx.is(sink, cx.i.c.GuiButton)) {
        Some(button) => {
            cx.i.touch(button);
            cx.fire(button, n.MouseButton1Up, vec![Value::Number(x), Value::Number(y)]);
            let taken = finger.takes && finger.press.button == Some(button) && !finger.press.dragged;
            if taken && !cx.is_false(button, n.Active) {
                let count = activation_count(cx, button);
                cx.fire(button, n.Activated, vec![Value::Instance(object), Value::Number(count)]);
            }
            if taken && finger.press.clickable {
                cx.fire(button, n.MouseButton1Click, Vec::new());
            }
        }
        None => {
            cx.fire_legacy("Button1Up", Vec::new())?;
            if let Some(text_box) = sink.filter(|&sink| cx.is(sink, cx.i.c.TextBox)) {
                if finger.press.sink == Some(text_box) && !dragging {
                    textbox::press(cx, text_box, x, y, object)?;
                    textbox::release(cx);
                }
            }
        }
    }
    // Measured: the release is processed over an Active object, a button / TextBox or a detector, or when the finger
    // dragged a ScrollingFrame (a tap on one is not)
    let processed = cx.any_active(&list) || sink.is_some() || detector_under(cx, &list) || dragging;
    if dragging {
        cx.touch_signals(n.InputEnded, object, processed);
    }
    for &o in &finger.hover {
        cx.i.touches.shown.borrow_mut().remove(&o);
        cx.i.touch(o);
        if cx.alive(o) {
            cx.fire(o, n.MouseLeave, vec![Value::Number(x), Value::Number(y)]);
            cx.fire(o, n.InputEnded, vec![Value::Instance(object)]);
        }
    }
    cx.refresh_touch_hover();
    cx.flush_states();
    if !dragging {
        cx.touch_signals(n.InputEnded, object, processed);
    }
    cx.touch_signals(n.TouchEnded, object, processed);
    if !cx.i.touches.any_down() {
        deliver_to_actions(cx, button_key(cx, 1.0), false);
    }
    if let Some(drag) = finger.drag {
        if drag.dragging {
            release_frame(cx, drag.frame, &finger.samples);
        }
        // Another finger on the frame takes it over (it starts counting at its next move)
        let mut fingers = cx.i.touches.fingers.borrow_mut();
        if let Some(next) = fingers.iter_mut().find(|f| f.drag.is_none() && f.hover.contains(&drag.frame)) {
            next.drag = Some(Drag::new(drag.frame, None, (next.x, next.y)));
        }
    }
    drop(finger.value);
    sync_overscroll(cx);
    Ok(())
}

// Dragging, coasting and springing back

struct Geometry {
    max: (f64, f64),
    window: (f64, f64),
    allow: (bool, bool),
    elastic: bool,
}

fn geometry(cx: &Cx, frame: InstanceId) -> Geometry {
    cx.update_layout();
    let max = scrolling::range(cx, frame);
    let window = cx.vector2(frame, cx.i.n.AbsoluteWindowSize);
    let direction = cx.enum_name(frame, cx.i.n.ScrollingDirection);
    let elastic = cx.enum_name(frame, cx.i.n.ElasticBehavior) != "Never";
    Geometry { max, window, allow: (direction != "Y" && max.0 > 0.0, direction != "X" && max.1 > 0.0), elastic }
}

/// The ScrollingFrame a finger coming down drags (P6c, measured): the outermost ScrollingFrame that can scroll among
/// the topmost one under the finger and its ScrollingFrame ancestors. A frame inside a scrolling frame does not scroll by
/// touch (its ancestor does, or nothing when the ancestor cannot scroll that way); one inside frames that cannot scroll
/// does. None when another finger holds that frame or it is not ScrollingEnabled (a disabled ancestor still takes the
/// finger, so the frame inside it does not scroll either).
fn touch_frame(cx: &Cx, list: &[InstanceId]) -> Option<InstanceId> {
    let class = cx.i.c.ScrollingFrame;
    let topmost = list.iter().copied().find(|&o| cx.class_name_is(o, class))?;
    let mut chosen = None;
    let mut current = Some(topmost);
    while let Some(id) = current {
        if cx.class_name_is(id, class) {
            let g = geometry(cx, id);
            if g.allow.0 || g.allow.1 {
                chosen = Some(id);
            }
        }
        current = cx.engine.state.borrow().dom.get(id).and_then(|instance| instance.parent);
    }
    chosen.filter(|&frame| cx.is_scroller(frame) && cx.frame_holder(frame).is_none())
}

/// Whether a finger past the slop starts dragging `frame` (Some(true)), waits (Some(false)) or never scrolls it (None),
/// by its travel (tx, ty) from where it came down and this move's step (P6c, measured in 24 directions on Y, X and XY
/// frames): the longer axis must be one the frame can scroll (else the touch never scrolls it); past the slop it decides
/// at once when it is at least DECIDED times the shorter one (an exact diagonal too), else the drag waits until a move
/// of more than WAIT_STEP px along it leaves it more than WAIT px out.
fn start_axis(cx: &Cx, frame: InstanceId, tx: f64, ty: f64, step: (f64, f64), waiting: bool) -> Option<bool> {
    let g = geometry(cx, frame);
    let (ax, ay) = (tx.abs(), ty.abs());
    if ax == ay {
        // (an exact diagonal; one reached while waiting keeps waiting: the 46 degree ray's (14, 14))
        if !(g.allow.0 || g.allow.1) {
            return None;
        }
        let moved = if g.allow.1 { step.1.abs() } else { step.0.abs() };
        return Some(!waiting || (ay > WAIT && moved > WAIT_STEP));
    }
    let (long, short, moved, can) = if ay > ax { (ay, ax, step.1.abs(), g.allow.1) } else { (ax, ay, step.0.abs(), g.allow.0) };
    if !can {
        return None;
    }
    Some((long > WAIT && moved > WAIT_STEP) || (!waiting && long >= DECIDED * short))
}

/// Where the finger counts as last inside when a drag starts: here, or (on an axis already past its edge) as far
/// back as the band needs to show the overscroll it has
fn inside_point(cx: &Cx, frame: InstanceId, at: (f64, f64)) -> (f64, f64) {
    let g = geometry(cx, frame);
    let canvas = cx.vector2(frame, cx.i.n.CanvasPosition);
    let axis = |c: f64, max: f64, size: f64, at: f64| {
        if c < 0.0 {
            at - band_inverse(-c, size)
        } else if c > max {
            at + band_inverse(c - max, size)
        } else {
            at
        }
    };
    (axis(canvas.0, g.max.0, g.window.0, at.0), axis(canvas.1, g.max.1, g.window.1, at.1))
}

/// One axis of a dragging finger's move: 1:1 inside the range, the band past an edge (measured)
fn drag_axis(c: f64, step: f64, finger: f64, inside: &mut f64, start: f64, max: f64, size: f64, elastic: bool) -> f64 {
    let raw = c - step;
    if !elastic {
        *inside = finger;
        return raw.clamp(0.0, max);
    }
    if (0.0..=max).contains(&raw) {
        *inside = finger;
        return raw;
    }
    if raw < 0.0 {
        let travel = finger - *inside;
        -(-raw).min(band(travel.max(start), size))
    } else {
        let travel = *inside - finger;
        max + (raw - max).min(band(travel.max(start), size))
    }
}

/// A dragging finger moved from `from` to `to`: the frame's canvas follows
fn drag_frame(cx: &Cx, id: u32, frame: InstanceId, from: (f64, f64), to: (f64, f64)) -> LuaResult<()> {
    if !cx.alive(frame) {
        return Ok(());
    }
    let Some(mut drag) = cx.i.touches.fingers.borrow().iter().find(|f| f.id == id).and_then(|f| f.drag) else { return Ok(()) };
    // Measured (P6c): a frame that is not ScrollingEnabled does not follow the finger (Drag::paused / held)
    let disabled = cx.is_false(frame, cx.i.n.ScrollingEnabled);
    let mut from = from;
    if disabled || drag.paused {
        if disabled {
            if drag.scrolled {
                drag.paused = true;
            } else {
                drag.held = true;
            }
        } else {
            drag.paused = false;
        }
        if let Some(finger) = cx.i.touches.fingers.borrow_mut().iter_mut().find(|f| f.id == id) {
            finger.drag = Some(drag);
        }
        return Ok(());
    }
    if drag.held {
        from = drag.started_at;
        drag.held = false;
    }
    drag.scrolled = true;
    let g = geometry(cx, frame);
    let canvas = cx.vector2(frame, cx.i.n.CanvasPosition);
    let mut next = canvas;
    if g.allow.0 {
        next.0 = drag_axis(canvas.0, to.0 - from.0, to.0, &mut drag.inside.0, drag.start, g.max.0, g.window.0, g.elastic);
    }
    if g.allow.1 {
        next.1 = drag_axis(canvas.1, to.1 - from.1, to.1, &mut drag.inside.1, drag.start, g.max.1, g.window.1, g.elastic);
    }
    if let Some(finger) = cx.i.touches.fingers.borrow_mut().iter_mut().find(|f| f.id == id) {
        finger.drag = Some(drag);
    }
    write_canvas(cx, frame, canvas, next);
    Ok(())
}

fn write_canvas(cx: &Cx, frame: InstanceId, current: (f64, f64), next: (f64, f64)) {
    if (next.0 as f32, next.1 as f32) == (current.0 as f32, current.1 as f32) {
        return;
    }
    mark_overscroll(cx, frame);
    cx.set(frame, cx.i.n.CanvasPosition, vector2(next.0, next.1));
}

/// The release velocity per axis: GAIN x the mean of the last SAMPLES per-frame velocities of the finger (from where
/// it came down), px/s in the finger's direction
fn release_velocity(cx: &Cx, samples: &[(u64, f64, f64)]) -> (f64, f64) {
    let timed: Vec<(f64, f64, f64)> = samples.iter().filter_map(|&(frame, x, y)| cx.i.touches.time_of(frame).map(|t| (t, x, y))).collect();
    let mut velocities: Vec<(f64, f64)> = Vec::new();
    for pair in timed.windows(2) {
        let dt = pair[1].0 - pair[0].0;
        if dt > 0.0 {
            velocities.push(((pair[1].1 - pair[0].1) / dt, (pair[1].2 - pair[0].2) / dt));
        }
    }
    let last = &velocities[velocities.len().saturating_sub(SAMPLES)..];
    if last.is_empty() {
        return (0.0, 0.0);
    }
    let count = last.len() as f64;
    let (sx, sy) = last.iter().fold((0.0, 0.0), |(a, b), (vx, vy)| (a + vx, b + vy));
    (GAIN * sx / count, GAIN * sy / count)
}

/// The dragging finger lifted: per axis the canvas springs back from past an edge, or coasts
fn release_frame(cx: &Cx, frame: InstanceId, samples: &[(u64, f64, f64)]) {
    if !cx.alive(frame) {
        return;
    }
    let g = geometry(cx, frame);
    let canvas = cx.vector2(frame, cx.i.n.CanvasPosition);
    // Measured (P6c): a release while the frame is not ScrollingEnabled does not coast
    let (vx, vy) = if cx.is_false(frame, cx.i.n.ScrollingEnabled) { (0.0, 0.0) } else { release_velocity(cx, samples) };
    let coast = |allow: bool, c: f64, max: f64, v: f64| {
        let mut coast = Coast::default();
        if !allow {
            return coast;
        }
        if c < 0.0 || c > max {
            let target = if c < 0.0 { 0.0 } else { max };
            coast.spring = Some(Axis { position: c, velocity: 0.0, target });
        } else if v.abs() >= STOP {
            coast.velocity = v as f32;
        }
        coast
    };
    let motion = Motion {
        x: coast(g.allow.0, canvas.0, g.max.0, vx),
        y: coast(g.allow.1, canvas.1, g.max.1, vy),
        began: cx.i.touches.frame.get() + 1,
        stepped: 0,
    };
    let mut motions = cx.i.touches.motions.borrow_mut();
    motions.retain(|(id, _)| *id != frame);
    if motion.x.active() || motion.y.active() {
        motions.push((frame, motion));
    }
}

/// How far `c` is past the range [0, max] (0 inside)
fn over(c: f64, max: f64) -> f64 {
    if c < 0.0 {
        -c
    } else if c > max {
        c - max
    } else {
        0.0
    }
}

/// One frame of one axis: the coast moves the canvas (and bounces at an edge), the spring brings it back
fn advance_axis(c: f64, coast: &mut Coast, max: f64, size: f64, elastic: bool, dt: f64) -> f64 {
    let omega = spring_omega();
    if coast.handed {
        coast.velocity = 0.0;
        coast.handed = false;
    }
    let mut out = c;
    let mut coasting = None;
    let mut fresh = false;
    if coast.velocity != 0.0 {
        let h = f32r(STEP_SCALE * dt);
        let v = coast.velocity as f64;
        let moved = f32r(c - f32r(v * h));
        let decayed = f32r(v * f32r(1.0 - FRICTION * h));
        coast.velocity = if decayed.abs() < STOP { 0.0 } else { decayed as f32 };
        if !elastic && ((c <= 0.0 && v > 0.0) || (c >= max && v < 0.0)) {
            // ElasticBehavior Never, measured: a coast at an edge heading out stops there; one that reaches the edge
            // stops at it (its velocity reads on for that frame)
            coast.velocity = 0.0;
        } else if !elastic && (moved < 0.0 || moved > max) {
            out = moved.clamp(0.0, max);
        } else {
            if coast.spring.is_none() && over(c, max) == 0.0 && over(moved, max) > 0.0 {
                // Past the edge: a spring from rest that began at the last frame inside (this is its first step)
                let (edge, left) = if moved < 0.0 { (0.0, c) } else { (max, max - c) };
                let out_by = band(BOUNCE_V * v.abs() - BOUNCE_P * left, size);
                let start = if moved < 0.0 { edge - out_by } else { edge + out_by };
                let mut spring = Axis { position: f32r(start), velocity: 0.0, target: edge };
                scrolling::spring_step(&mut spring, dt, omega);
                coast.spring = Some(spring);
                fresh = true;
            }
            coasting = Some(moved);
            out = moved;
        }
    }
    if let Some(spring) = coast.spring.as_mut() {
        if !fresh {
            scrolling::spring_step(spring, dt, omega);
        }
        match coasting {
            Some(moved) if over(moved, max) <= over(spring.position, max) => out = moved,
            Some(_) => {
                out = spring.position;
                coast.handed = true;
            }
            None => out = spring.position,
        }
        if spring.position == spring.target && spring.velocity == 0.0 && coast.velocity == 0.0 {
            out = spring.target;
            coast.spring = None;
            coast.handed = false;
        }
    }
    out
}

/// One frame of a frame's coasting / spring (not in the frame it began in, nor twice in one frame)
fn advance(cx: &Cx, frame: InstanceId, motion: &mut Motion, dt: f64) {
    let g = geometry(cx, frame);
    let canvas = cx.vector2(frame, cx.i.n.CanvasPosition);
    let mut next = canvas;
    if motion.x.active() {
        next.0 = advance_axis(canvas.0, &mut motion.x, g.max.0, g.window.0, g.elastic, dt);
    }
    if motion.y.active() {
        next.1 = advance_axis(canvas.1, &mut motion.y, g.max.1, g.window.1, g.elastic, dt);
    }
    write_canvas(cx, frame, canvas, next);
}

/// Per frame, before the frame's touch moves (a client steps coasting before it handles the frame's input)
pub(crate) fn step(cx: &Cx, dt: f64) {
    let frame = cx.i.touches.frame.get();
    let frames: Vec<InstanceId> = cx.i.touches.motions.borrow().iter().map(|(id, _)| *id).collect();
    for id in frames {
        let Some(mut motion) = cx.i.touches.motions.borrow().iter().find(|(f, _)| *f == id).map(|(_, m)| *m) else { continue };
        if !cx.alive(id) {
            cx.i.touches.forget(id);
            continue;
        }
        if motion.began >= frame || motion.stepped >= frame {
            continue;
        }
        // Measured (P6c): ScrollingEnabled false ends a coast in that frame (GetScrollVelocity reads 0 and it stays
        // ended when the frame is enabled again) and holds a spring back, which goes on from where it was once the
        // frame is enabled again (as the wheel's spring pauses)
        if cx.is_false(id, cx.i.n.ScrollingEnabled) {
            let mut motions = cx.i.touches.motions.borrow_mut();
            for axis in [&mut motion.x, &mut motion.y] {
                axis.velocity = 0.0;
                axis.handed = false;
            }
            if motion.x.active() || motion.y.active() {
                if let Some(entry) = motions.iter_mut().find(|(f, _)| *f == id) {
                    entry.1 = motion;
                }
            } else {
                motions.retain(|(f, _)| *f != id);
            }
            continue;
        }
        advance(cx, id, &mut motion, dt);
        motion.stepped = frame;
        let mut motions = cx.i.touches.motions.borrow_mut();
        if motion.x.active() || motion.y.active() {
            if let Some(entry) = motions.iter_mut().find(|(f, _)| *f == id) {
                entry.1 = motion;
            }
        } else {
            motions.retain(|(f, _)| *f != id);
        }
    }
    sync_overscroll(cx);
}

/// A touch on `frame` (in frame `at`) stops its coasting: the frame's step happens first, as a client steps coasting
/// before the frame's input
fn stop_motion(cx: &Cx, frame: InstanceId, at: u64) {
    let Some(mut motion) = cx.i.touches.motions.borrow().iter().find(|(f, _)| *f == frame).map(|(_, m)| *m) else { return };
    if motion.began < at && motion.stepped < at && cx.alive(frame) {
        advance(cx, frame, &mut motion, cx.i.touches.dt());
    }
    cx.i.touches.forget(frame);
}

fn mark_overscroll(cx: &Cx, frame: InstanceId) {
    if cx.i.touches.overscroll.borrow_mut().insert(frame) {
        if let Some(layout) = cx.engine.layout().borrow_mut().as_mut() {
            layout.overscroll.insert(frame);
        }
    }
}

/// The layout keeps a frame's CanvasPosition past its range while a finger drags it or it coasts / springs back
fn sync_overscroll(cx: &Cx) {
    let mut wanted: FxHashSet<InstanceId> = cx.i.touches.motions.borrow().iter().map(|(id, _)| *id).collect();
    for finger in cx.i.touches.fingers.borrow().iter() {
        if let Some(drag) = finger.drag.filter(|d| d.dragging) {
            wanted.insert(drag.frame);
        }
    }
    let mut current = cx.i.touches.overscroll.borrow_mut();
    if *current == wanted {
        return;
    }
    let state = cx.engine.state.borrow();
    if let Ok(mut guard) = cx.engine.layout().try_borrow_mut() {
        if let Some(layout) = guard.as_mut() {
            for id in current.iter() {
                if !wanted.contains(id) {
                    layout.overscroll.remove(id);
                    if state.dom.get(*id).is_some_and(|i| !i.has(flags::DESTROYED)) {
                        layout.mark_scrolled(*id);
                    }
                }
            }
            for &id in &wanted {
                layout.overscroll.insert(id);
            }
        }
    }
    *current = wanted;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The coast and its bounce past an edge, replayed from each fling capture's release (its canvas position and
    /// GetScrollVelocity) of a 0.747 touch client (P6b, fresh 200 x 300 frames, canvas 3000, range 0..2700): every
    /// later frame within 1.25 px of the client's.
    #[test]
    fn coasts_and_bounces_follow_the_clients_flings() {
        let captures: &[(&str, f64, f64, &[f64])] = &[
        ("clock3a fresh fling to top", 200.0000, 1594.5720, &[173.6839, 148.3885, 124.0740, 100.7026, 78.2376, 56.6439, 35.8876, 15.9364, -3.2411, -21.6748, -39.3936, -49.9967, -41.6016, -34.0991, -27.6183, -22.1534, -17.6278, -13.9322, -10.9481, -8.5603, -6.6643, -5.1684, -3.9947, -3.0782, -2.3655, -1.8134, -1.3870, -1.0588, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000]),
        ("clock3a fresh fling hard to top", 100.0000, 3095.6033, &[48.9116, -0.1954, -47.3979, -92.7698, -88.4927, -73.6336, -60.3545, -48.8836, -39.2109, -31.2008, -24.6597, -19.3778, -15.1515, -11.7956, -9.1479, -7.0705, -5.4483, -4.1869, -3.2097, -2.4550, -1.8740, -1.4278, -1.0860, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000]),
        ("clock3a fresh fling soft to top", 36.0000, 239.4759, &[32.0478, 28.2489, 24.5973, 21.0873, 17.7135, 14.4705, 11.3533, 8.3570, 5.4769, 2.7085, 0.0474, -2.5104, -4.9691, -7.3323, -9.0223, -7.5074, -6.1535, -4.9840, -3.9978, -3.1811, -2.5142, -1.9757, -1.5448, -1.2026, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000]),
        ("clock3a fresh fling to bottom", 2500.0000, -1539.4508, &[2525.4065, 2549.8276, 2573.3015, 2595.8650, 2617.5535, 2638.4006, 2658.4395, 2677.7009, 2696.2156, 2714.0120, 2731.1182, 2747.5610, 2748.2676, 2740.1628, 2732.9199, 2726.6631, 2721.3872, 2717.0183, 2713.4504, 2710.5693, 2708.2642, 2706.4338, 2704.9897, 2703.8564, 2702.9717, 2702.2837, 2701.7507, 2701.3391, 2701.0222, 2700.0000, 2700.0000, 2700.0000, 2700.0000, 2700.0000, 2700.0000, 2700.0000, 2700.0000, 2700.0000]),
        ("clock3b fresh fling to top", 200.0000, 1555.4418, &[174.3297, 149.6550, 125.9373, 103.1394, 81.2256, 60.1618, 39.9149, 20.4533, 1.7464, -16.2350, -33.5190, -50.1326, -48.7735, -40.5838, -33.2649, -26.9426, -21.6115, -17.1966, -13.5914, -10.6802, -8.3509, -6.5012, -5.0419, -3.8970, -3.0029, -2.3077, -1.7690, -1.3531, -1.0329, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000]),
        ("clock3b fresh fling hard to top", 100.0000, 3116.6411, &[48.5644, -0.8764, -48.3997, -94.0798, -88.8392, -73.9219, -60.5908, -49.0749, -39.3645, -31.3229, -24.7562, -19.4536, -15.2108, -11.8417, -9.1837, -7.0982, -5.4697, -4.2033, -3.2222, -2.4646, -1.8813, -1.4334, -1.0902, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000]),
        ("clock3b fresh fling soft to top", 36.0000, 572.1875, &[26.5569, 17.4800, 8.7551, 0.3687, -7.6925, -15.4411, -22.8892, -24.6529, -20.5134, -16.8140, -13.6183, -10.9237, -8.6921, -6.8699, -5.3984, -4.2210, -3.2861, -2.5485, -1.9697, -1.5178, -1.1664, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000]),
        ("clock3b fresh fling to bottom", 2500.0000, -1395.1143, &[2523.0244, 2545.1558, 2566.4287, 2586.8767, 2606.5317, 2625.4243, 2643.5842, 2661.0398, 2677.8184, 2693.9463, 2709.4487, 2724.3499, 2738.6731, 2743.5349, 2736.2249, 2729.6921, 2724.0488, 2719.2903, 2715.3496, 2712.1316, 2709.5332, 2707.4539, 2705.8030, 2704.5005, 2703.4785, 2702.6804, 2702.0598, 2701.5791, 2701.2078, 2700.0000, 2700.0000, 2700.0000, 2700.0000, 2700.0000, 2700.0000, 2700.0000, 2700.0000, 2700.0000]),
        ];
        for (label, start, velocity, rows) in captures {
            let mut coast = Coast { velocity: *velocity as f32, ..Default::default() };
            let mut c = *start;
            for (k, expected) in rows.iter().enumerate() {
                c = advance_axis(c, &mut coast, 2700.0, 300.0, true, 1.0 / 60.0);
                assert!((c - expected).abs() <= 1.25, "{label} frame {}: rur {c:.4}, client {expected:.4}", k + 1);
            }
        }
    }

    /// The rubber band and its inverse
    #[test]
    fn band_matches_the_measured_pulls() {
        assert!((band(20.0, 300.0) - 10.6109).abs() < 1e-4);
        assert!((band(80.0, 300.0) - 38.3721).abs() < 1e-4);
        assert!((band(12.0, 300.0) - 6.4579).abs() < 1e-4);
        for x in [1.0, 12.0, 80.0, 400.0] {
            assert!((band_inverse(band(x, 300.0), 300.0) - x).abs() < 1e-6);
        }
    }
}
