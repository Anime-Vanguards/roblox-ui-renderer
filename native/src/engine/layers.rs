//! The order of a run's LayerCollectors: `--all-layers` (every on-screen LayerCollector a run leaves under the local
//! player's PlayerGui or CoreGui, exported in the order Roblox draws them, so a fixture can lay out a whole PlayerGui: a
//! HUD split into several ScreenGuis, a SerializationService blob loaded with `--load`), ScreenGuis inside GuiObjects
//! (each a ScreenGui of its own), and the order input meets them in.
//!
//! Roblox draws the PlayerGui pass under CoreGui's, each by DisplayOrder; ScreenGuis of equal
//! DisplayOrder draw in the order they came into the PlayerGui (or CoreGui) or turned Enabled, the latest on top (measured
//! in Studio Edit and Play Solo, 45 of 45 steps; and in a 0.747 client, 41 scenes, notes/layout.md "ScreenGuis inside
//! GuiObjects"). Property edits, new descendants, renames, a DisplayOrder change and back, and a move inside the
//! PlayerGui (into a Folder of it, or a nested ScreenGui from one frame to another) keep that order, and it is not the
//! child order. A subtree that comes in stamps its LayerCollectors children first: a ScreenGui inside a frame of
//! another draws under the one that holds it when both came in together, above it when it came in later.
//!
//! Input meets ScreenGuis of equal DisplayOrder in another order (GetGuiObjectsAtPosition's, which the mouse's and a
//! finger's events follow, measured in the same client): the order they came in, parents first, and Enabled does not
//! change it. [`LayerOrder`] keeps both stamps; the renderer's sort by DisplayOrder is stable, so the export lists the
//! layers by their draw stamp.

use std::rc::Rc;

use rustc_hash::FxHashMap;

use super::{ClassId, Engine, GuiEngine, InstanceId, K, NameId, Reflection, State, services, value::Value};

/// When each LayerCollector came into a PlayerGui / CoreGui / StarterGui (or, for drawing, turned Enabled on): counters,
/// not times.
#[derive(Default)]
pub struct LayerOrder {
    draws: u64,
    hits: u64,
    /// Drawing: coming in (a subtree's LayerCollectors children first) or Enabled turning true
    draw: FxHashMap<InstanceId, u64>,
    /// Input and GetGuiObjectsAtPosition: coming in (a subtree's LayerCollectors parents first)
    hit: FxHashMap<InstanceId, u64>,
}

impl LayerOrder {
    /// A subtree came in: its LayerCollectors parents first (`pre`) and children first (`post`)
    fn enter(&mut self, pre: &[InstanceId], post: &[InstanceId]) {
        for &id in pre {
            self.hits += 1;
            self.hit.insert(id, self.hits);
        }
        for &id in post {
            self.draws += 1;
            self.draw.insert(id, self.draws);
        }
    }

    fn enabled(&mut self, id: InstanceId) {
        self.draws += 1;
        self.draw.insert(id, self.draws);
    }

    /// The draw stamp: 0 for a LayerCollector that never came in or turned Enabled through the engine (a snapshot's)
    pub fn draw_of(&self, id: InstanceId) -> u64 {
        self.draw.get(&id).copied().unwrap_or(0)
    }

    /// The input stamp: 0 for a LayerCollector that never came in through the engine (a snapshot's)
    pub fn hit_of(&self, id: InstanceId) -> u64 {
        self.hit.get(&id).copied().unwrap_or(0)
    }
}

/// The classes the order needs.
struct Classes {
    layer_collector: ClassId,
    screen_gui: Option<ClassId>,
    base_player_gui: Option<ClassId>,
    gui_base: Option<ClassId>,
    folder: Option<ClassId>,
}

impl Classes {
    fn new(reflection: &Reflection) -> Option<Classes> {
        Some(Classes {
            layer_collector: reflection.class_id("LayerCollector")?,
            screen_gui: reflection.class_id("ScreenGui"),
            base_player_gui: reflection.class_id("BasePlayerGui"),
            gui_base: reflection.class_id("GuiBase2d"),
            folder: reflection.class_id("Folder"),
        })
    }
}

/// The PlayerGui, CoreGui or StarterGui `from` is (or is in)
fn container(state: &State, reflection: &Reflection, classes: &Classes, from: Option<InstanceId>) -> Option<InstanceId> {
    let base = classes.base_player_gui?;
    let mut current = from;
    while let Some(node) = current {
        let instance = state.dom.get(node)?;
        if reflection.class(instance.class).is_a(base) {
            return Some(node);
        }
        current = instance.parent;
    }
    None
}

/// The LayerCollectors of `root`'s subtree (itself included), parents first and children first
fn collectors(state: &State, reflection: &Reflection, classes: &Classes, root: InstanceId) -> (Vec<InstanceId>, Vec<InstanceId>) {
    let (mut pre, mut post) = (Vec::new(), Vec::new());
    let mut stack = vec![(root, false)];
    while let Some((node, done)) = stack.pop() {
        let collector = reflection.class(state.dom.at(node).class).is_a(classes.layer_collector);
        if done {
            if collector {
                post.push(node);
            }
            continue;
        }
        if collector {
            pre.push(node);
        }
        stack.push((node, true));
        stack.extend(state.dom.at(node).children.iter().rev().map(|&child| (child, false)));
    }
    (pre, post)
}

/// The hooks that keep [`LayerOrder`] (every engine, so a standby VM made before its run's options
/// are known has them).
pub fn install(engine: &Rc<Engine>) {
    let reflection = engine.reflection.clone();
    let Some(classes) = Classes::new(&reflection) else { return };
    let classes = Rc::new(classes);
    let mut hooks = engine.hooks.borrow_mut();
    let weak = Rc::downgrade(engine);
    let (known, types) = (reflection.clone(), classes.clone());
    hooks.hierarchy_changed.push(Box::new(move |state, id, old_parent, new_parent| {
        // (coming in: into a PlayerGui / CoreGui / StarterGui from outside it; a move inside one keeps the order)
        if new_parent.is_none() {
            return;
        }
        let into = container(state, &known, &types, new_parent);
        if into.is_none() || into == container(state, &known, &types, old_parent) {
            return;
        }
        let (pre, post) = collectors(state, &known, &types, id);
        if pre.is_empty() {
            return;
        }
        if let Some(engine) = weak.upgrade() {
            engine.gui().layer_order.borrow_mut().enter(&pre, &post);
        }
    }));
    let weak = Rc::downgrade(engine);
    let (known, types) = (reflection.clone(), classes.clone());
    hooks.property_changed.push(Box::new(move |state, id, name| {
        // (hooks run only when the value changed: Enabled false -> true)
        if name != K::Enabled as NameId || !matches!(state.dom.raw_get(id, name), Some(Value::Bool(true))) {
            return;
        }
        if !known.class(state.dom.at(id).class).is_a(types.layer_collector) {
            return;
        }
        if let Some(engine) = weak.upgrade() {
            engine.gui().layer_order.borrow_mut().enabled(id);
        }
    }));
    let weak = Rc::downgrade(engine);
    hooks.instance_freed.push(Box::new(move |_, id| {
        if let Some(engine) = weak.upgrade() {
            if let Ok(mut order) = engine.gui().layer_order.try_borrow_mut() {
                order.draw.remove(&id);
                order.hit.remove(&id);
            }
        }
    }));
}

/// The ScreenGuis inside a GuiObject (or a ScreenGui, a Model or anything but Folders) that are in a PlayerGui, CoreGui
/// or StarterGui, by creation. A client lays each out and draws it as a ScreenGui of its own, over the whole screen,
/// unclipped and unturned by what holds it, whether or not its holders are visible or enabled, and gives it input above
/// or below its holder by [`LayerOrder`] (notes/layout.md "ScreenGuis inside GuiObjects"). Found among the
/// LayerCollectors that came in through the engine.
pub fn nested_screen_guis(engine: &Engine, state: &State) -> Vec<InstanceId> {
    let reflection = &engine.reflection;
    let Some(classes) = Classes::new(reflection) else { return Vec::new() };
    let Some(screen_gui) = classes.screen_gui else { return Vec::new() };
    let order = engine.gui().layer_order.borrow();
    if order.hit.is_empty() {
        return Vec::new();
    }
    let mut result: Vec<InstanceId> = order
        .hit
        .keys()
        .copied()
        .filter(|&id| {
            let Some(instance) = state.dom.get(id) else { return false };
            if instance.class != screen_gui {
                return false;
            }
            // (nested: something other than Folders between it and its container)
            let mut current = instance.parent;
            let mut nested = false;
            while let Some(node) = current {
                let Some(holder) = state.dom.get(node) else { return false };
                let info = reflection.class(holder.class);
                if classes.base_player_gui.is_some_and(|base| info.is_a(base)) {
                    return nested;
                }
                if Some(holder.class) != classes.folder {
                    nested = true;
                }
                current = holder.parent;
            }
            false
        })
        .collect();
    result.sort_by_key(|&id| state.dom.at(id).serial);
    result
}

/// `roots` and the ScreenGuis nested in them ([`nested_screen_guis`]), by DisplayOrder's ties as a client draws them
/// (their draw stamps; the renderer sorts by DisplayOrder, stably). `roots` alone when none is nested in them.
pub fn with_nested(engine: &Engine, roots: &[InstanceId]) -> Vec<InstanceId> {
    let state = engine.state.borrow();
    let nested: Vec<InstanceId> = nested_screen_guis(engine, &state)
        .into_iter()
        .filter(|id| !roots.contains(id))
        .filter(|&id| std::iter::successors(state.dom.at(id).parent, |&node| state.dom.at(node).parent).any(|node| roots.contains(&node)))
        .collect();
    if nested.is_empty() {
        return roots.to_vec();
    }
    let order = engine.gui().layer_order.borrow();
    let mut result = roots.to_vec();
    result.extend(nested);
    // (stable: equal stamps keep the given order)
    result.sort_by_key(|&id| order.draw_of(id));
    result
}

/// The export roots of an `--all-layers` run: the enabled ScreenGuis (and plugin widgets) under the
/// local player's PlayerGui, then those under CoreGui, each list by [`LayerOrder`] (ties in child
/// order), looking through Folders and other non-GUI containers as Roblox does, and the enabled ScreenGuis nested in
/// GuiObjects there ([`nested_screen_guis`]; drawn whatever holds them). SurfaceGuis and BillboardGuis draw in the 3D
/// world and are left out. `roots` (the story gui and what the run already exports) are kept: one that is not found
/// there follows at the end.
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
    let nested = nested_screen_guis(engine, &state);
    let order = engine.gui().layer_order.borrow();
    let enabled = |id: InstanceId| !matches!(state.dom.raw_get(id, K::Enabled as NameId), Some(Value::Bool(false)));
    let mut result = Vec::new();
    for container in [player_gui, core_gui].into_iter().flatten() {
        let mut found = Vec::new();
        let mut stack: Vec<InstanceId> = state.dom.at(container).children.iter().rev().copied().collect();
        while let Some(id) = stack.pop() {
            let info = reflection.class(state.dom.at(id).class);
            if info.is_a(layer_collector) {
                let in_world = world_bound.iter().flatten().any(|&c| info.is_a(c));
                if !in_world && (enabled(id) || roots.contains(&id)) {
                    found.push(id);
                }
            } else if !info.is_a(gui_base) {
                stack.extend(state.dom.at(id).children.iter().rev().copied());
            }
        }
        for &id in &nested {
            let inside = std::iter::successors(state.dom.at(id).parent, |&node| state.dom.at(node).parent).any(|node| node == container);
            if inside && !found.contains(&id) && (enabled(id) || roots.contains(&id)) {
                found.push(id);
            }
        }
        // (stable: equal stamps keep child order)
        found.sort_by_key(|&id| order.draw_of(id));
        result.extend(found);
    }
    for &root in roots {
        if !result.contains(&root) {
            result.push(root);
        }
    }
    result
}
