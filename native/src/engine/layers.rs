//! `--all-layers`: every on-screen LayerCollector a run leaves under the local player's PlayerGui or
//! CoreGui, exported in the order Roblox draws them, so a fixture can lay out a whole PlayerGui (a
//! HUD split into several ScreenGuis, a SerializationService blob loaded with `--load`) instead of
//! only what it parents into the story gui.
//!
//! Roblox draws the PlayerGui pass under CoreGui's, each by DisplayOrder; ScreenGuis of equal
//! DisplayOrder draw in the order they were last parented or turned Enabled, the latest on top
//! (measured in Studio Edit and Play Solo, 45 of 45 steps). Property edits,
//! new descendants, renames and a DisplayOrder change and back keep that order, and it is not the
//! child order. [`LayerOrder`] stamps each LayerCollector when it gets a parent or its Enabled turns
//! true, and the renderer's sort by DisplayOrder is stable, so the export lists the layers by stamp.

use std::rc::Rc;

use rustc_hash::FxHashMap;

use super::{Engine, GuiEngine, InstanceId, K, NameId, services, value::Value};

/// When each LayerCollector last got a parent or turned Enabled on (a counter, not a time).
#[derive(Default)]
pub struct LayerOrder {
    next: u64,
    stamps: FxHashMap<InstanceId, u64>,
}

impl LayerOrder {
    fn stamp(&mut self, id: InstanceId) {
        self.next += 1;
        self.stamps.insert(id, self.next);
    }

    /// 0 for a LayerCollector never parented or enabled through the engine (a snapshot's)
    fn of(&self, id: InstanceId) -> u64 {
        self.stamps.get(&id).copied().unwrap_or(0)
    }
}

/// The hooks that keep [`LayerOrder`] (every engine, so a standby VM made before its run's options
/// are known has them).
pub fn install(engine: &Rc<Engine>) {
    let reflection = engine.reflection.clone();
    let Some(layer_collector) = reflection.class_id("LayerCollector") else { return };
    let mut hooks = engine.hooks.borrow_mut();
    let weak = Rc::downgrade(engine);
    let classes = reflection.clone();
    hooks.hierarchy_changed.push(Box::new(move |state, id, _, new_parent| {
        if new_parent.is_none() || !classes.class(state.dom.at(id).class).is_a(layer_collector) {
            return;
        }
        if let Some(engine) = weak.upgrade() {
            engine.gui().layer_order.borrow_mut().stamp(id);
        }
    }));
    let weak = Rc::downgrade(engine);
    let classes = reflection.clone();
    hooks.property_changed.push(Box::new(move |state, id, name| {
        // (hooks run only when the value changed: Enabled false -> true)
        if name != K::Enabled as NameId || !matches!(state.dom.raw_get(id, name), Some(Value::Bool(true))) {
            return;
        }
        if !classes.class(state.dom.at(id).class).is_a(layer_collector) {
            return;
        }
        if let Some(engine) = weak.upgrade() {
            engine.gui().layer_order.borrow_mut().stamp(id);
        }
    }));
    let weak = Rc::downgrade(engine);
    hooks.instance_freed.push(Box::new(move |_, id| {
        if let Some(engine) = weak.upgrade() {
            if let Ok(mut order) = engine.gui().layer_order.try_borrow_mut() {
                order.stamps.remove(&id);
            }
        }
    }));
}

/// The export roots of an `--all-layers` run: the enabled ScreenGuis (and plugin widgets) under the
/// local player's PlayerGui, then those under CoreGui, each list by [`LayerOrder`] (ties in child
/// order), looking through Folders and other non-GUI containers as Roblox does. SurfaceGuis and
/// BillboardGuis draw in the 3D world and are left out. `roots` (the story gui and what the run
/// already exports) are kept: one that is not found there follows at the end.
pub fn screen_layers(engine: &Rc<Engine>, roots: &[InstanceId]) -> Vec<InstanceId> {
    let reflection = &engine.reflection;
    let class = |name: &str| reflection.class_id(name);
    let (Some(layer_collector), Some(gui_base)) = (class("LayerCollector"), class("GuiBase2d")) else { return roots.to_vec() };
    let world_bound = [class("SurfaceGuiBase"), class("BillboardGui")];
    let game = engine.world().game;
    let core_gui = services::get_service(engine, game, "CoreGui").ok();
    let state = engine.state.borrow();
    let player_gui = engine
        .world()
        .local_player
        .zip(class("PlayerGui"))
        .and_then(|(player, player_gui)| state.dom.find_child_of_class(player, player_gui));
    let order = engine.gui().layer_order.borrow();
    let mut result = Vec::new();
    for container in [player_gui, core_gui].into_iter().flatten() {
        let mut found = Vec::new();
        let mut stack: Vec<InstanceId> = state.dom.at(container).children.iter().rev().copied().collect();
        while let Some(id) = stack.pop() {
            let info = reflection.class(state.dom.at(id).class);
            if info.is_a(layer_collector) {
                let in_world = world_bound.iter().flatten().any(|&c| info.is_a(c));
                let enabled = !matches!(state.dom.raw_get(id, K::Enabled as NameId), Some(Value::Bool(false)));
                if !in_world && (enabled || roots.contains(&id)) {
                    found.push(id);
                }
            } else if !info.is_a(gui_base) {
                stack.extend(state.dom.at(id).children.iter().rev().copied());
            }
        }
        // (stable: equal stamps keep child order)
        found.sort_by_key(|&id| order.of(id));
        result.extend(found);
    }
    for &root in roots {
        if !result.contains(&root) {
            result.push(root);
        }
    }
    result
}
