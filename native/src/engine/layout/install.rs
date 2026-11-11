//! Port of runtime/layout/install.luau: wires the layout into the engine: computed-property getters
//! with lazy layout (a read lays out the instance's tree first, like the real engine), invalidation
//! on GUI changes (the PropertyChanged / HierarchyChanged hooks), the per-frame layout pass (Render
//! phase), CanvasPosition clamping, UIGridStyleLayout.ApplyLayout and UIPageLayout navigation.

use std::rc::Rc;

use mlua::prelude::*;

use crate::engine::GuiEngine;
use super::{Layout, pass::Pass};
use crate::engine::{
    Engine, InstanceId, NameId,
    lua::{convert, instance::set_property, none},
    scheduler::Phase,
    services::{GetterFn, MethodFn, SetterFn},
    text::{FontPaths, TextEngine},
    value::Value,
};

/// Runs `f` with a pass over the engine's layout, state, hooks and text engine. None while a pass
/// is already running (the Luau `Computing` flag) or before the layout is installed.
fn with_pass<R>(engine: &Engine, f: impl FnOnce(&mut Pass) -> R) -> Option<R> {
    let Ok(mut guard) = engine.layout().try_borrow_mut() else { return None };
    let layout = guard.as_mut()?;
    let hooks = engine.hooks.borrow();
    let mut state = engine.state.borrow_mut();
    let mut text = engine.text().borrow_mut();
    let text = text.get_or_insert_with(|| TextEngine::new(FontPaths::for_repo(&engine.options.repo.to_string_lossy())));
    let mut pass = Pass { l: layout, state: &mut state, hooks: &hooks, reflection: &engine.reflection, text };
    Some(f(&mut pass))
}

/// `Layout.Ensure(Proxy)`: lays out `id`'s tree when it changed (lazy computed-property reads),
/// then applies pending scrolls. A tree changed since the frame's layout point (`Layout::mark_frame_layout`) is one a client lays
/// out for this read: the script pays the pass ([`lazy_layout_price`]); one changed before it the client had laid out already.
pub fn ensure(engine: &Engine, id: InstanceId) {
    let _memory = crate::memtrace::scope(crate::memtrace::Tag::Layout);
    let price = with_pass(engine, |pass| {
        if pass.state.dom.get(id).is_none() {
            return 0.0;
        }
        let root = pass.find_root(id);
        // (what the per-frame pass laid out of this tree's changes since the frame's layout point: a client lays them out now)
        let mut price = pass.l.owed.remove(&root).unwrap_or(0.0);
        if !pass.is_laid_out(root) {
            let charged = pass.l.changed_since_frame_layout(root);
            let before = pass.l.stats;
            pass.run(root);
            if charged {
                price += lazy_layout_price(&before, &pass.l.stats);
                if pass.l.print_stats {
                    let (after, before) = (pass.l.stats, before);
                    eprintln!(
                        "layout-lazy {:.2} us computed={} stored={} moved={} skipped={} memo={}",
                        price * 1e6,
                        after.computed - before.computed,
                        after.stored - before.stored,
                        after.moved - before.moved,
                        after.skipped - before.skipped,
                        after.memo - before.memo
                    );
                }
            }
        }
        // (a scroll made since the frame's layout point: a client lays the scrolled tree out at its next read, this one for this
        // tree; every pending scroll is applied here, another tree's is owed to its own next read)
        for (scrolled, moved, after) in pass.process_scrolls() {
            if !after || moved == 0 {
                continue;
            }
            let scroll = LAZY_SCROLL_PASS + LAZY_LAYOUT_OBJECT * moved as f64;
            if pass.l.print_stats {
                eprintln!("layout-lazy-scroll {:.2} us moved={moved}", scroll * 1e6);
            }
            let scrolled_root = pass.find_root(scrolled);
            if scrolled_root == root {
                price += scroll;
            } else {
                *pass.l.owed.entry(scrolled_root).or_default() += scroll;
            }
        }
        price
    });
    if let Some(price) = price.filter(|price| *price > 0.0) {
        engine.state.borrow().sched.charge(price);
    }
}

/// What a client spends laying a changed GUI out for a script's read (`ensure`), by the records the pass stored or moved (one it
/// skipped, unchanged, costs about nothing).
fn lazy_layout_price(before: &super::Stats, after: &super::Stats) -> f64 {
    let objects = (after.stored - before.stored) + (after.moved - before.moved);
    LAZY_LAYOUT_PASS + LAZY_LAYOUT_OBJECT * objects as f64
}

// A client's lazy layout, measured in a rendered client of a game's lobby (roblox-sim, Studio 0.744; notes/macro layoutprobe, two
// sessions of 15 rounds): a grid of cards in a ScrollingFrame, its canvas moved and every card's AbsolutePosition read in the same
// handler, the first read 156 us for 100 cards (301 records the scroll moves here), 334-379 for 300 (901), 998 for 1,000 (3,001):
// 62 us and 0.312 a record; one card's inner frame resized, or its label's text changed, among 300 (4 records stored, 300 skipped)
// 3.6-4.0 us. The client lays every changed GUI out once a frame for nothing a script pays, after the render step's bindings and
// before PreRender: a read there, or later in the frame, of a change made before it costs 2-7 us, as a clean read does.
const LAZY_LAYOUT_PASS: f64 = 2.3e-6;
const LAZY_LAYOUT_OBJECT: f64 = 0.312e-6;
const LAZY_SCROLL_PASS: f64 = 62e-6;

/// `Layout.Ensure(Proxy)` for a caller that already holds the engine state: a PropertyChanged
/// hook (runtime/input scrolling.luau refreshes a presented ScrollingFrame when ScrollingEnabled
/// changes). Added by the input port (phase 5); a no-op while a pass runs, like `ensure`.
pub fn ensure_with_state(engine: &Engine, state: &mut crate::engine::State, id: InstanceId) {
    let _memory = crate::memtrace::scope(crate::memtrace::Tag::Layout);
    let Ok(mut guard) = engine.layout().try_borrow_mut() else { return };
    let Some(layout) = guard.as_mut() else { return };
    let hooks = engine.hooks.borrow();
    let mut text = engine.text().borrow_mut();
    let text = text.get_or_insert_with(|| TextEngine::new(FontPaths::for_repo(&engine.options.repo.to_string_lossy())));
    let mut pass = Pass { l: layout, state, hooks: &hooks, reflection: &engine.reflection, text };
    if pass.state.dom.get(id).is_none() {
        return;
    }
    let root = pass.find_root(id);
    if !pass.is_laid_out(root) {
        pass.run(root);
    }
    let _ = pass.process_scrolls();
}

/// `Layout.UpdateAll(Game)`: lays out every displayed root whose tree changed (the per-frame pass).
pub fn update_all(engine: &Engine) {
    let _memory = crate::memtrace::scope(crate::memtrace::Tag::Layout);
    let game = engine.world().game;
    with_pass(engine, |pass| {
        let roots = pass.displayed_roots(game);
        let mut ran = false;
        // (a change since the frame's layout point that this pass lays out is still owed to the tree's next read: ensure)
        for root in roots {
            if !pass.is_laid_out(root) {
                let owed = pass.l.changed_since_frame_layout(root);
                let before = pass.l.stats;
                pass.run(root);
                if owed {
                    *pass.l.owed.entry(root).or_default() += lazy_layout_price(&before, &pass.l.stats);
                }
                ran = true;
            }
        }
        if !pass.l.scrolled_is_empty() {
            let started = std::time::Instant::now();
            for (scrolled, moved, after) in pass.process_scrolls() {
                if after && moved > 0 {
                    let root = pass.find_root(scrolled);
                    *pass.l.owed.entry(root).or_default() += LAZY_SCROLL_PASS + LAZY_LAYOUT_OBJECT * moved as f64;
                }
            }
            pass.l.stats.time += super::elapsed(started);
            ran = true;
        }
        if ran && pass.l.print_stats {
            let stats = pass.l.stats;
            eprintln!(
                "layout-stats {} passes={} computed={} memo={} measures={} stored={} skipped={} moved={} texts={}",
                stats.time * 1000.0,
                stats.passes,
                stats.computed,
                stats.memo,
                stats.measures,
                stats.stored,
                stats.skipped,
                stats.moved,
                stats.texts
            );
            pass.l.stats = Default::default();
        }
    });
}

/// The LayerCollectors that are displayed (`Pass::displayed_roots`), in draw order.
pub fn displayed_roots(engine: &Engine) -> Vec<InstanceId> {
    let game = engine.world().game;
    with_pass(engine, |pass| pass.displayed_roots(game)).unwrap_or_default()
}

/// `Layout.Invalidate()`: the next Ensure / UpdateAll lays everything out again.
pub fn invalidate(engine: &Engine) {
    if let Ok(mut guard) = engine.layout().try_borrow_mut() {
        if let Some(layout) = guard.as_mut() {
            layout.invalidate();
        }
    }
}

/// `Layout.MarkChanged(Proxy)` from outside a pass.
pub fn mark_changed(engine: &Engine, id: InstanceId) {
    if let Ok(mut guard) = engine.layout().try_borrow_mut() {
        if let Some(layout) = guard.as_mut() {
            layout.mark_changed(&engine.state.borrow(), id);
        }
    }
}

/// `Layout.Pages(LayoutProxy)`: the pages of a UIPageLayout in order.
pub fn pages(engine: &Engine, layout: InstanceId) -> Vec<InstanceId> {
    with_pass(engine, |pass| pass.pages(layout)).unwrap_or_default()
}

/// `Layout.ClampCanvasPosition`: the assigned position clamped against the frame's current layout
/// (laid out first, like reading AbsoluteSize).
fn clamp_canvas_position(engine: &Engine, id: InstanceId, value: Value) -> Value {
    ensure(engine, id);
    let Value::Vector2(position) = &value else { return value };
    let clamped = with_pass(engine, |pass| pass.clamp_canvas(id, position.x as f64, position.y as f64)).flatten();
    match clamped {
        Some((x, y)) if x != position.x as f64 || y != position.y as f64 => {
            Value::Vector2(rbx_dom_weak::types::Vector2::new(x as f32, y as f32))
        }
        _ => value,
    }
}

fn read(lua: &Lua, engine: &Engine, id: InstanceId, name: NameId) -> LuaResult<LuaValue> {
    let value = engine.state.borrow().dom.raw_get(id, name).cloned();
    match value {
        Some(value) => convert::to_lua(lua, engine, &value),
        None => Ok(LuaValue::Nil),
    }
}

/// A getter that lays out the instance's tree first.
fn lazy(name: NameId) -> GetterFn {
    Rc::new(move |lua, engine, id| {
        ensure(engine, id);
        Ok(read(lua, engine, id, name)?)
    })
}

/// A layout object's getter: lays out its parent's tree first.
fn lazy_parent(name: NameId) -> GetterFn {
    Rc::new(move |lua, engine, id| {
        let parent = engine.state.borrow().dom.at(id).parent;
        if let Some(parent) = parent {
            ensure(engine, parent);
        }
        Ok(read(lua, engine, id, name)?)
    })
}

/// Installs the layout: state, hooks, getters, setters, methods and the Render-phase pass.
pub fn install(engine: &Rc<Engine>) {
    let reflection = engine.reflection.clone();
    *engine.layout().borrow_mut() = Some(Layout::new(&reflection, &engine.options));
    let n = |name: &str| reflection.name_id(name);
    let class = |name: &str| reflection.class_id(name);

    // Changes mark the changed instance's subtree (and its ancestors) for the next incremental pass
    {
        let mut hooks = engine.hooks.borrow_mut();
        let weak = Rc::downgrade(engine);
        hooks.property_changed.push(Box::new(move |state, id, name| {
            let Some(engine) = weak.upgrade() else { return };
            // (a pass in progress holds the layout: its own writes are computed properties or
            // Layout.Writing writes, which the Luau hook ignores too)
            let Ok(mut guard) = engine.layout().try_borrow_mut() else { return };
            if let Some(layout) = guard.as_mut() {
                layout.property_changed(state, &engine.reflection, id, name);
            }
        }));
        let weak = Rc::downgrade(engine);
        hooks.hierarchy_changed.push(Box::new(move |state, id, old_parent, _| {
            let Some(engine) = weak.upgrade() else { return };
            let Ok(mut guard) = engine.layout().try_borrow_mut() else { return };
            if let Some(layout) = guard.as_mut() {
                layout.hierarchy_changed(state, id, old_parent);
            }
        }));
        // (instance GC, lua/raw.rs: a freed slot's layout state goes with it; the generation check
        // already hides it from a reused slot)
        let weak = Rc::downgrade(engine);
        hooks.instance_freed.push(Box::new(move |_, id| {
            let Some(engine) = weak.upgrade() else { return };
            let Ok(mut guard) = engine.layout().try_borrow_mut() else { return };
            if let Some(layout) = guard.as_mut() {
                layout.forget(id);
            }
        }));
    }

    let mut behaviors = engine.behaviors.borrow_mut();
    if let Some(base) = class("GuiBase2d") {
        for name in ["AbsolutePosition", "AbsoluteSize", "AbsoluteRotation"] {
            if let Some(name) = n(name) {
                behaviors.add_getter(base, name, lazy(name));
            }
        }
    }
    for class_name in ["TextLabel", "TextButton", "TextBox"] {
        let Some(text_class) = class(class_name) else { continue };
        for name in ["TextBounds", "TextFits"] {
            if let Some(name) = n(name) {
                behaviors.add_getter(text_class, name, lazy(name));
            }
        }
    }
    if let Some(grid_style) = class("UIGridStyleLayout") {
        if let Some(name) = n("AbsoluteContentSize") {
            behaviors.add_getter(grid_style, name, lazy_parent(name));
        }
        if let Some(name) = n("ApplyLayout") {
            let apply: MethodFn = Rc::new(|_, engine, id, _| {
                mark_changed(engine, id);
                let parent = engine.state.borrow().dom.at(id).parent;
                if let Some(parent) = parent {
                    ensure(engine, parent);
                }
                Ok(none())
            });
            behaviors.add_method(grid_style, name, apply);
        }
    }
    if let Some(grid) = class("UIGridLayout") {
        for name in ["AbsoluteCellSize", "AbsoluteCellCount"] {
            if let Some(name) = n(name) {
                behaviors.add_getter(grid, name, lazy_parent(name));
            }
        }
    }

    // UIPageLayout navigation (offline pages switch immediately, as with Animated = false)
    if let (Some(page), Some(current_page)) = (class("UIPageLayout"), n("CurrentPage")) {
        let set_page = move |engine: &Engine, layout: InstanceId, target: Option<InstanceId>| {
            if let Some(target) = target {
                set_property(engine, layout, current_page, Value::Instance(target));
                mark_changed(engine, layout);
            }
        };
        if let Some(name) = n("JumpTo") {
            let jump: MethodFn = Rc::new(move |_, engine, id, args| {
                set_page(engine, id, args.instance(1));
                Ok(none())
            });
            behaviors.add_method(page, name, jump);
        }
        if let Some(name) = n("JumpToIndex") {
            let jump: MethodFn = Rc::new(move |_, engine, id, args| {
                let index = args.number(1).unwrap_or(0.0);
                let list = pages(engine, id);
                let target = if index >= 0.0 && index.fract() == 0.0 { list.get(index as usize).copied() } else { None };
                set_page(engine, id, target);
                Ok(none())
            });
            behaviors.add_method(page, name, jump);
        }
        let circular = n("Circular");
        for (member, step) in [("Next", 1i64), ("Previous", -1i64)] {
            let Some(name) = n(member) else { continue };
            let navigate: MethodFn = Rc::new(move |_, engine, id, _| {
                let list = pages(engine, id);
                let current = engine.state.borrow().dom.raw_get(id, current_page).and_then(Value::as_instance);
                let index = list.iter().position(|p| Some(*p) == current).map_or(0, |i| i as i64);
                let is_circular = circular.is_some_and(|c| matches!(engine.state.borrow().dom.raw_get(id, c), Some(Value::Bool(true))));
                let wanted = index + step;
                let target = if wanted >= 0 && (wanted as usize) < list.len() {
                    Some(list[wanted as usize])
                } else if is_circular && !list.is_empty() {
                    Some(if step > 0 { list[0] } else { list[list.len() - 1] })
                } else {
                    None
                };
                set_page(engine, id, target);
                Ok(none())
            });
            behaviors.add_method(page, name, navigate);
        }
    }

    if let Some(scrolling) = class("ScrollingFrame") {
        for name in ["AbsoluteCanvasSize", "AbsoluteWindowSize"] {
            if let Some(name) = n(name) {
                behaviors.add_getter(scrolling, name, lazy(name));
            }
        }
        // Measured (layout-probe-canvaspos): the assigned position is clamped at once against the
        // frame's current layout (see ClampCanvasPosition).
        if let Some(canvas_position) = n("CanvasPosition") {
            let setter: SetterFn = Rc::new(move |lua, engine, id, value| {
                let class = engine.state.borrow().dom.at(id).class;
                let position = match engine.reflection.class_property(class, canvas_position).cloned() {
                    Some(info) => convert::coerce(lua, engine, &info, &value)?,
                    None => convert::from_lua(&value),
                };
                let clamped = clamp_canvas_position(engine, id, position);
                set_property(engine, id, canvas_position, clamped);
                Ok(())
            });
            behaviors.add_setter(scrolling, canvas_position, setter);
        }
    }
    drop(behaviors);
    engine.clear_member_cache();

    // The frame's layout point: the emulator's Tween phase runs after the render step's bindings and before RenderStepped and
    // PreRender, where a client lays its changed GUIs out (measured, notes in Layout::mark_frame_layout); TweenService steps
    // later, after the PreRender handlers
    engine.state.borrow_mut().sched.add_hook(
        Phase::Tween,
        Rc::new(|_, engine, _| {
            if let Ok(mut guard) = engine.layout().try_borrow_mut() {
                if let Some(layout) = guard.as_mut() {
                    layout.mark_frame_layout();
                }
            }
            Ok(())
        }),
    );
    engine.state.borrow_mut().sched.add_hook(
        Phase::Render,
        Rc::new(|_, engine, _| {
            update_all(engine);
            Ok(())
        }),
    );
}
