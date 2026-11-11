//! Path2D against the layout: where a path's parent is (the emulator's `path2d` asks through its frame source), and
//! the frame's point at which a path on screen has its spline brought up to date.
//!
//! Measured in a rendered client (Studio 0.744; the emulator's tools/parity/probes/path2d_layout.luau):
//!
//! - A path resolves against its parent's rectangle on the screen (AbsolutePosition and AbsoluteSize, the top bar's
//!   inset included; a ScrollingFrame's window, not its canvas), turned as the parent and its ancestors are.
//! - The curve queries see the parent as the last frame laid it out: a resize reaches them with the next frame. So
//!   the frames are kept here ([`Frames`]), taken at every frame's layout and, for a parent first asked about
//!   between frames, when it is asked.
//! - The frame's layout brings the spline of a path up to date when its parent is shown (in an enabled
//!   LayerCollector that is displayed, under visible ancestors), whatever the path's own Visible says.

use std::rc::Rc;

use rustc_hash::FxHashMap;

use super::{
    Engine, GuiEngine, InstanceId,
    layout::{self, Record},
    path2d::{self as core, Frame},
    scheduler::Phase,
};

/// Where the parents of paths were at the last frame's layout (or when first asked since).
#[derive(Default)]
pub struct Frames {
    parents: FxHashMap<InstanceId, Frame>,
}

/// The rectangle of a layout record as a path's parent: the record's own (unturned) box, and the turn its
/// descendants get.
pub fn frame_of(record: &Record) -> Frame {
    let frame = Frame::new([record.x as f32, record.y as f32], [record.w as f32, record.h as f32]);
    match record.transform {
        Some(transform) => frame.turned(transform.map(|term| term as f32)),
        None => frame,
    }
}

/// The parent's frame as the layout has it now (laid out first), None for a parent the layout does not place:
/// an instance without a record, and a LayerCollector that is not displayed (it has no screen to measure; the
/// emulator answers what Roblox does for one, 800 by 600 for a ScreenGui or SurfaceGui and nothing for a
/// BillboardGui).
fn current(engine: &Engine, parent: InstanceId) -> Option<Frame> {
    let collector = engine.reflection.is_a(engine.state.borrow().dom.at(parent).class, "LayerCollector");
    if collector && !layout::displayed_roots(engine).contains(&parent) {
        return None;
    }
    layout::ensure(engine, parent);
    let guard = engine.layout().try_borrow().ok()?;
    guard.as_ref()?.record(parent).map(frame_of)
}

/// The frame's layout: every path's parent as it is now, and the splines of the paths on screen.
fn frame_layout(engine: &Rc<Engine>) {
    let paths = core::parented(engine);
    if paths.is_empty() {
        let mut frames = engine.gui().path_frames.borrow_mut();
        if !frames.parents.is_empty() {
            frames.parents.clear();
        }
        return;
    }
    let displayed = layout::displayed_roots(engine);
    let mut parents = FxHashMap::default();
    for path in paths {
        let Some(parent) = engine.state.borrow().dom.parent_of(path) else { continue };
        if !core::valid_parent(engine, Some(parent)) {
            continue;
        }
        let shown = {
            let Ok(guard) = engine.layout().try_borrow() else { continue };
            let Some(layout) = guard.as_ref() else { continue };
            let root = layout.find_root(&engine.state.borrow(), &engine.reflection, parent);
            displayed.contains(&root) && layout.record(parent).is_some_and(|record| record.visible)
        };
        if let Some(frame) = current(engine, parent) {
            parents.insert(parent, frame);
        }
        if shown {
            core::laid_out(engine, path);
        }
    }
    engine.gui().path_frames.borrow_mut().parents = parents;
}

pub fn install(engine: &Rc<Engine>) {
    core::set_frame_source(
        engine,
        Rc::new(|engine, parent| {
            if let Some(frame) = engine.gui().path_frames.borrow().parents.get(&parent) {
                return Some(*frame);
            }
            let frame = current(engine, parent)?;
            engine.gui().path_frames.borrow_mut().parents.insert(parent, frame);
            Some(frame)
        }),
    );
    // (after the layout's own Render hook: layout::install runs first)
    engine.state.borrow_mut().sched.add_hook(
        Phase::Render,
        Rc::new(|_, engine, _| {
            frame_layout(engine);
            Ok(())
        }),
    );
}
