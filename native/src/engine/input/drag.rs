//! Dragging GuiObjects with the mouse (port of `runtime/input/drag.luau`): legacy
//! GuiObject.Draggable and UIDragDetector.
//!
//! Measured (tools/interaction/experiments/drag.mjs, drag2.mjs, drag3.mjs):
//!   - Draggable (Active = true, Draggable = true): pressing it fires DragBegin(Position) before
//!     InputBegan; every move sets Position = start Position + (mouse - press point) in offset;
//!     releasing fires DragStopped(x, y) before InputEnded. Works in edit mode.
//!   - UIDragDetector does nothing in Studio edit mode, which is where UI Labs stories run. In play
//!     mode the documented behaviour is emulated (not measured): TranslatePlane / TranslateLine,
//!     ResponseStyle Offset / Scale / Custom*, DragUDim2, BoundingUI, Min/MaxDragTranslation,
//!     DragStart / DragContinue / DragEnd(inputPosition).

use mlua::prelude::*;
use rbx_dom_weak::types as dom;

use super::{Cx, clamp, vector2};
use crate::engine::{InstanceId, value::Value};

#[derive(Clone, Copy, Debug)]
pub struct Active {
    pub target: InstanceId,
    pub detector: Option<InstanceId>,
    start_x: f64,
    start_y: f64,
    start_position: dom::UDim2,
}

fn udim2_of(value: Option<Value>) -> dom::UDim2 {
    value.and_then(|v| v.as_udim2()).unwrap_or(dom::UDim2::new(dom::UDim::new(0.0, 0), dom::UDim::new(0.0, 0)))
}

fn first_detector(cx: &Cx, id: InstanceId) -> Option<InstanceId> {
    let children = cx.engine.state.borrow().dom.get(id)?.children.clone();
    children
        .into_iter()
        .find(|&child| cx.class_name_is(child, cx.i.c.UIDragDetector) && !cx.is_false(child, cx.i.n.Enabled))
}

/// A left press over `receivers` (topmost first). True when a drag started. `edit_mode`:
/// UIDragDetectors are inert (measured).
pub(crate) fn press(cx: &Cx, receivers: &[InstanceId], x: f64, y: f64, edit_mode: bool) -> bool {
    let n = &cx.i.n;
    for &id in receivers {
        if cx.is_true(id, n.Draggable) && cx.is_true(id, n.Active) {
            let start = cx.get(id, n.Position);
            let start_position = udim2_of(start.clone());
            *cx.i.drag.borrow_mut() = Some(Active { target: id, detector: None, start_x: x, start_y: y, start_position });
            cx.fire(id, n.DragBegin, vec![start.unwrap_or(Value::Nil)]);
            return true;
        }
        if !edit_mode {
            if let Some(detector) = first_detector(cx, id) {
                let start_position = udim2_of(cx.get(id, n.Position));
                *cx.i.drag.borrow_mut() = Some(Active { target: id, detector: Some(detector), start_x: x, start_y: y, start_position });
                cx.fire(detector, n.DragStart, vec![vector2(x, y)]);
                return true;
            }
        }
    }
    false
}

/// Clamps an offset translation so the dragged object stays inside `bounding` (logical px).
fn bound(cx: &Cx, active: &Active, bounding: InstanceId, dx: f64, dy: f64) -> (f64, f64) {
    let record = cx.record(active.target, |r| (r.x, r.y, r.w, r.h));
    let bounds = cx.record(bounding, |r| (r.x, r.y, r.w, r.h));
    let (Some(record), Some(bounds)) = (record, bounds) else { return (dx, dy) };
    // The record reflects the current (already moved) position: undo the applied translation first
    let applied = udim2_of(cx.get(active.target, cx.i.n.Position));
    let base = active.start_position;
    let start_x = record.0 - (applied.x.offset as f64 - base.x.offset as f64);
    let start_y = record.1 - (applied.y.offset as f64 - base.y.offset as f64);
    let (min_x, max_x) = (bounds.0 - start_x, bounds.0 + bounds.2 - (start_x + record.2));
    let (min_y, max_y) = (bounds.1 - start_y, bounds.1 + bounds.3 - (start_y + record.3));
    let mut result = (dx, dy);
    if min_x <= max_x {
        result.0 = clamp(dx, min_x, max_x);
    }
    if min_y <= max_y {
        result.1 = clamp(dy, min_y, max_y);
    }
    result
}

fn set_position(cx: &Cx, id: InstanceId, scale_x: f64, offset_x: f64, scale_y: f64, offset_y: f64) -> LuaResult<()> {
    let value = cx.datatype("UDim2", "new", (scale_x, offset_x, scale_y, offset_y))?;
    cx.assign(id, "Position", value)
}

pub(crate) fn move_to(cx: &Cx, x: f64, y: f64) -> LuaResult<()> {
    let Some(active) = *cx.i.drag.borrow() else { return Ok(()) };
    if !cx.alive(active.target) {
        *cx.i.drag.borrow_mut() = None;
        return Ok(());
    }
    let n = &cx.i.n;
    let (mut dx, mut dy) = (x - active.start_x, y - active.start_y);
    let start = active.start_position;
    let (sx_scale, sx_offset) = (start.x.scale as f64, start.x.offset as f64);
    let (sy_scale, sy_offset) = (start.y.scale as f64, start.y.offset as f64);
    let Some(detector) = active.detector else {
        return set_position(cx, active.target, sx_scale, sx_offset + dx, sy_scale, sy_offset + dy);
    };

    let style = cx.enum_name(detector, n.DragStyle);
    if style == "TranslateLine" {
        let (ax, ay) = match cx.get(detector, n.DragAxis) {
            Some(Value::Vector2(v)) => (v.x as f64, v.y as f64),
            _ => (1.0, 0.0),
        };
        let length = (ax * ax + ay * ay).sqrt();
        if length > 0.0 {
            let along = (dx * ax + dy * ay) / length;
            dx = ax / length * along;
            dy = ay / length * along;
        }
    } else if style != "TranslatePlane" {
        cx.fire(detector, n.DragContinue, vec![vector2(x, y)]);
        return Ok(());
    }

    // Scale for the parent's UIScale: offsets are in the parent's (unscaled) units
    let parent = cx.engine.state.borrow().dom.get(active.target).and_then(|i| i.parent);
    let parent_record = parent.and_then(|p| cx.record(p, |r| (r.scale, r.w, r.h)));
    let factor = match parent_record {
        Some((scale, _, _)) if scale != 0.0 => scale,
        _ => 1.0,
    };
    dx /= factor;
    dy /= factor;

    if let Some(Value::Instance(bounding)) = cx.get(detector, n.BoundingUI) {
        let (bx, by) = bound(cx, &active, bounding, dx * factor, dy * factor);
        dx = bx / factor;
        dy = by / factor;
    }

    let min = cx.get(detector, n.MinDragTranslation).and_then(|v| v.as_udim2());
    let max = cx.get(detector, n.MaxDragTranslation).and_then(|v| v.as_udim2());
    if let (Some(min), Some(max)) = (min, max) {
        if max.x.offset > min.x.offset || max.y.offset > min.y.offset {
            if max.x.offset > min.x.offset {
                dx = clamp(dx, min.x.offset as f64, max.x.offset as f64);
            }
            if max.y.offset > min.y.offset {
                dy = clamp(dy, min.y.offset as f64, max.y.offset as f64);
            }
        }
    }

    let response = cx.enum_name(detector, n.ResponseStyle);
    let mut translation = cx.datatype("UDim2", "fromOffset", (dx, dy))?;
    let mut scale = (0.0, 0.0);
    if response == "Scale" || response == "CustomScale" {
        let width = match parent_record {
            Some((_, w, _)) if w > 0.0 => w / factor,
            _ => 1.0,
        };
        let height = match parent_record {
            Some((_, _, h)) if h > 0.0 => h / factor,
            _ => 1.0,
        };
        translation = cx.datatype("UDim2", "fromScale", (dx / width, dy / height))?;
        if let Value::UDim2(t) = crate::engine::lua::convert::from_lua(&translation) {
            scale = (t.x.scale as f64, t.y.scale as f64);
        }
    }
    let stored = crate::engine::lua::convert::from_lua(&translation);
    cx.set(detector, n.DragUDim2, stored);
    if response == "Offset" {
        set_position(cx, active.target, sx_scale, sx_offset + dx, sy_scale, sy_offset + dy)?;
    } else if response == "Scale" {
        set_position(cx, active.target, sx_scale + scale.0, sx_offset, sy_scale + scale.1, sy_offset)?;
    }
    cx.fire(detector, n.DragContinue, vec![vector2(x, y)]);
    Ok(())
}

pub(crate) fn release(cx: &Cx, x: f64, y: f64) {
    let Some(active) = cx.i.drag.borrow_mut().take() else { return };
    match active.detector {
        None => cx.fire(active.target, cx.i.n.DragStopped, vec![Value::Number(x), Value::Number(y)]),
        Some(detector) => cx.fire(detector, cx.i.n.DragEnd, vec![vector2(x, y)]),
    }
}
