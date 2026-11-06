//! Retained export for live sessions (port of scene.luau's Scene.NewCache / Touch / Restructure /
//! CachedNode and runtime/session.luau's ReplyDelta bookkeeping).
//!
//! Each frame only nodes whose inputs changed are rebuilt: a node is reused while its instance
//! version (own properties and decoration children, bumped by the property / hierarchy hooks as
//! Scene.Touch / Scene.Restructure), the cache structure, its layout record version
//! (`Record::version`) and every walk input (parent, matrix, clip, ZInfo) are unchanged. A rebuilt
//! node reuses its appearance (every static key) while the style version and the record's size /
//! scale are unchanged, its text's static fields while the text layout is the same, and its glyph
//! entries while the text layout is the same. The walk skips a child subtree (per-sibling roots)
//! in which nothing changed since a build recorded it with every node reused: the hooks and the
//! layout's changed records mark changes on the instance and its ancestors (`Versions::mark`),
//! and a subtree reached with the same walk inputs adds the recorded draw entries and exported ids
//! again (export.rs `Subtree`), so a frame's walk costs what changed, not the whole tree.
//!
//! A build returns what the client does not hold yet ([`Update`]): the rebuilt nodes (a node the
//! client holds whose appearance was reused goes as a runtime/session.luau NodePatch of the keys a
//! rebuild recomputes, with `unset` for those it lost; others as whole nodes), the ids no longer exported (removed or culled), the draw lists / font registry when they
//! changed and the images not announced before. Nothing is compared with a copy of what was sent.
//! Outside presentation mode, nodes their clip hides entirely are culled (not exported), as the
//! Luau session cache does; the font registry lives as long as the session, so glyph font indices
//! stay valid across replies.

use std::{cell::RefCell, rc::Rc, sync::Arc};

use rustc_hash::{FxHashMap, FxHashSet};
use serde_json::{Map, Value as Json};

use crate::engine::GuiEngine;
use super::{Cached, Classes, Ctx, Export, Layer, LineCache, Names, Registry, Subtree, Variants, ZInfo, layers_json, traverse};
use crate::engine::{
    ClassId, Engine, InstanceId, K, NameId, Reflection, State,
    layout::{BoxSlots, Slots, Transform},
    text::TextLayout,
};

/// A node's cache entry (scene.luau Cache.Nodes[Proxy]).
#[derive(Default)]
pub(crate) struct Entry {
    /// Whether the entry holds a built node (a default entry does not)
    pub(super) built: bool,
    /// Whether the client holds the node as last built (false after it was culled / removed)
    pub(super) sent: bool,
    /// The build that last exported the node
    pub(super) exported: u64,
    pub(super) version: u32,
    pub(super) style_version: u32,
    pub(super) structure: u64,
    pub(super) record_version: u64,
    pub(super) parent: Option<u32>,
    pub(super) z: ZInfo,
    pub(super) matrix: Option<Transform>,
    pub(super) clip: Option<[f64; 4]>,
    pub(super) w: f64,
    pub(super) h: f64,
    pub(super) scale: f64,
    pub(super) has_scroll: bool,
    /// The DYNAMIC_KEYS the last build had (a patch unsets the ones gone since)
    pub(super) dynamic_keys: u16,
    /// Every static node key (Appearance); None once it moved into a whole node sent to the
    /// client (rebuilt, identically, if the node must be sent whole again while it is valid)
    pub(super) appearance: Option<Map<String, Json>>,
    /// The exported `path` and the path version it was computed at
    pub(super) path: Option<(u32, String)>,
    /// The text layout of the last build (ReuseText) and its static text fields
    pub(super) text_layout: Option<Arc<TextLayout>>,
    pub(super) text_static: Option<Map<String, Json>>,
    /// Glyph entries of the text layout's lines
    pub(super) lines: LineCache,
    /// What the node's subtree added to the build that last walked it with every node reused
    pub(super) subtree: Option<Box<Subtree>>,
}

/// Names the invalidation rules test.
struct TouchNames {
    name: NameId,
    z_index: NameId,
    z_index_behavior: NameId,
    automatic_size: NameId,
    /// GeometryProperties: changes that keep the appearance (style version)
    geometry: FxHashSet<NameId>,
}

/// The versions the property / hierarchy hooks maintain (scene.luau Cache.Versions /
/// StyleVersions / Structure / AutomaticContent / Children).
pub(crate) struct Versions {
    /// (version, style version) per instance
    own: Slots<(u32, u32)>,
    /// Bumped with every InvalidateTree of the instance (its name or an ancestor's changed, or it
    /// moved): the exported `path` is cached per node until this changes
    paths: Slots<u32>,
    /// Subtree dirty marks: the build that first sees a change in the instance's subtree (the
    /// instance, a descendant, a record, the children), set on the instance and its ancestors
    dirty: Slots<u64>,
    /// The number of the next build (the value marks take)
    pub(super) pending: u64,
    pub(super) structure: u64,
    /// AutomaticContent memo (presentation's zeroSensitive)
    pub(super) automatic: FxHashMap<InstanceId, bool>,
    /// The ordered GuiChildren of visited instances
    pub(super) children: FxHashMap<InstanceId, Rc<[InstanceId]>>,
    /// Subtree roots that were moved and instances that were freed since the last build: the cache
    /// entries of those now outside the exported trees are dropped (a UI a story removed and still
    /// references keeps no export state; a node that comes back is built again, as a node the
    /// client does not hold is anyway)
    moved: Vec<InstanceId>,
    freed: Vec<InstanceId>,
    camera_users: FxHashMap<InstanceId, Vec<InstanceId>>,
    names: TouchNames,
    gui_base2d: Option<ClassId>,
    scrolling_frame: Option<ClassId>,
}

impl Versions {
    fn new(reflection: &Reflection) -> Versions {
        let geometry = [
            "Position",
            "AnchorPoint",
            "Size",
            "Rotation",
            "Visible",
            "AbsolutePosition",
            "AbsoluteSize",
            "AbsoluteRotation",
            "TextBounds",
            "TextFits",
            "AbsoluteContentSize",
            "AbsoluteCanvasSize",
            "AbsoluteWindowSize",
            "CanvasPosition",
        ]
        .iter()
        .filter_map(|name| reflection.name_id(name))
        .collect();
        Versions {
            own: Slots::default(),
            paths: Slots::default(),
            dirty: Slots::default(),
            pending: 1,
            structure: 0,
            automatic: FxHashMap::default(),
            children: FxHashMap::default(),
            moved: Vec::new(),
            freed: Vec::new(),
            camera_users: FxHashMap::default(),
            names: TouchNames {
                name: K::Name as NameId,
                z_index: K::ZIndex as NameId,
                z_index_behavior: K::ZIndexBehavior as NameId,
                automatic_size: K::AutomaticSize as NameId,
                geometry,
            },
            gui_base2d: reflection.class_id("GuiBase2d"),
            scrolling_frame: reflection.class_id("ScrollingFrame"),
        }
    }

    /// (version, style version) of `id`
    #[inline]
    pub(super) fn of(&self, id: InstanceId) -> (u32, u32) {
        self.own.get(id).copied().unwrap_or((0, 0))
    }

    /// The path version of `id` (see `paths`)
    #[inline]
    pub(super) fn path_of(&self, id: InstanceId) -> u32 {
        self.paths.get(id).copied().unwrap_or(0)
    }

    /// The build that first sees the latest change in `id`'s subtree (0: none since it existed)
    #[inline]
    pub(super) fn dirty_of(&self, id: InstanceId) -> u64 {
        self.dirty.get(id).copied().unwrap_or(0)
    }

    /// Marks `id` and its ancestors changed for the next build (an ancestor already marked for it
    /// has its own ancestors marked).
    pub(super) fn mark(&mut self, state: &State, id: InstanceId) {
        let pending = self.pending;
        let mut node = Some(id);
        while let Some(current) = node {
            if self.dirty.get(current).copied() == Some(pending) {
                break;
            }
            let Some(instance) = state.dom.get(current) else { break };
            self.dirty.set(current, pending);
            node = instance.parent;
        }
    }

    fn bump(&mut self, state: &State, id: InstanceId, style: bool) {
        self.mark(state, id);
        match self.own.get_mut(id) {
            Some((version, style_version)) => {
                *version = version.wrapping_add(1);
                if style {
                    *style_version = style_version.wrapping_add(1);
                }
            }
            None => self.own.set(id, (1, style as u32)),
        }
    }

    fn is(&self, reflection: &Reflection, state: &State, id: InstanceId, class: Option<ClassId>) -> bool {
        class.is_some_and(|c| state.dom.get(id).is_some_and(|i| reflection.class(i.class).is_a(c)))
    }

    /// InvalidateAutomaticContent: the memo of `id` and its ancestors; their ScrollingFrames
    /// re-export (their zeroSensitive may change).
    fn invalidate_automatic_content(&mut self, reflection: &Reflection, state: &State, id: InstanceId) {
        if self.automatic.is_empty() {
            return;
        }
        let mut node = Some(id);
        while let Some(current) = node {
            self.automatic.remove(&current);
            let Some(instance) = state.dom.get(current) else { break };
            if self.scrolling_frame.is_some() && Some(instance.class) == self.scrolling_frame {
                self.bump(state, current, false);
            }
            node = instance.parent;
        }
        let _ = reflection;
    }

    /// InvalidateTree: every export of the subtree (paths, parents).
    fn invalidate_tree(&mut self, state: &State, id: InstanceId) {
        self.bump(state, id, true);
        match self.paths.get_mut(id) {
            Some(version) => *version = version.wrapping_add(1),
            None => self.paths.set(id, 1),
        }
        if let Some(instance) = state.dom.get(id) {
            for &child in &instance.children {
                self.invalidate_tree(state, child);
            }
        }
    }

    /// Scene.Touch: a property of `id` changed (None: the hierarchy under it changed).
    fn touch(&mut self, reflection: &Reflection, state: &State, id: InstanceId, name: Option<NameId>) {
        if let Some(users) = self.camera_users.get(&id).cloned() {
            for viewport in users {
                self.bump(state, viewport, true);
            }
        }
        let geometry = name.is_some_and(|n| self.names.geometry.contains(&n));
        self.bump(state, id, !geometry);
        if name == Some(self.names.automatic_size) {
            self.invalidate_automatic_content(reflection, state, id);
        }
        if name == Some(self.names.name) {
            self.invalidate_tree(state, id);
        }
        if name == Some(self.names.z_index) || name == Some(self.names.z_index_behavior) {
            // Folder descendants participate in the nearest GUI ancestor's sibling ordering
            self.children.clear();
        }
        // A decoration (UICorner, UIStroke, its UIGradient, ...) changes its owner's export
        let Some(instance) = state.dom.get(id) else { return };
        if self.is(reflection, state, id, self.gui_base2d) {
            return;
        }
        let mut owner = instance.parent;
        while let Some(current) = owner {
            if self.is(reflection, state, current, self.gui_base2d) {
                self.bump(state, current, true);
                return;
            }
            owner = state.dom.get(current).and_then(|i| i.parent);
        }
    }

    /// Scene.Restructure: reparenting changes paths below the moved instance and decorations on
    /// either owner.
    fn restructure(&mut self, reflection: &Reflection, state: &State, id: InstanceId, old: Option<InstanceId>, new: Option<InstanceId>) {
        self.invalidate_tree(state, id);
        for parent in [old, new].into_iter().flatten() {
            self.invalidate_automatic_content(reflection, state, parent);
        }
        for parent in [old, new].into_iter().flatten() {
            self.touch(reflection, state, parent, None);
        }
        self.children.clear();
    }
}

/// What one build changed for the client (runtime/session.luau ReplyDelta's contents).
pub struct Update {
    /// Rebuilt nodes (whole nodes or patches of their moving keys, new ones included)
    pub nodes: Vec<Json>,
    /// Ids the client holds that this build did not export
    pub removed: Vec<u32>,
    /// The draw lists, when they changed
    pub layers: Option<Json>,
    /// The font registry, when it grew
    pub fonts: Option<Json>,
    /// Image URIs not announced before (sorted)
    pub images: Vec<String>,
    /// Nodes exported this build
    pub exported: usize,
    /// Nodes rebuilt this build
    pub misses: usize,
}

/// The session's export cache (Scene.NewCache).
pub struct Retained {
    versions: Rc<RefCell<Versions>>,
    entries: BoxSlots<Entry>,
    registry: Registry,
    variants: Variants,
    announced: FxHashSet<String>,
    frame: u64,
    /// Nodes the client holds: those exported by the previous build
    sent: Vec<(u32, InstanceId)>,
    layers: Option<Vec<Layer>>,
    fonts: usize,
    presentation: bool,
}

impl Retained {
    /// A cache for `engine`'s session; its invalidation hooks stay installed while it lives.
    pub fn new(engine: &Rc<Engine>) -> Retained {
        let versions = Rc::new(RefCell::new(Versions::new(&engine.reflection)));
        let mut hooks = engine.hooks.borrow_mut();
        let weak = Rc::downgrade(&versions);
        let reflection = engine.reflection.clone();
        hooks.property_changed.push(Box::new(move |state, id, name| {
            if let Some(versions) = weak.upgrade() {
                versions.borrow_mut().touch(&reflection, state, id, Some(name));
            }
        }));
        let weak = Rc::downgrade(&versions);
        let reflection = engine.reflection.clone();
        hooks.hierarchy_changed.push(Box::new(move |state, id, old, new| {
            if let Some(versions) = weak.upgrade() {
                let mut versions = versions.borrow_mut();
                versions.restructure(&reflection, state, id, old, new);
                if old.is_some() {
                    versions.moved.push(id);
                }
            }
        }));
        let weak = Rc::downgrade(&versions);
        hooks.instance_freed.push(Box::new(move |_, id| {
            if let Some(versions) = weak.upgrade() {
                let mut versions = versions.borrow_mut();
                versions.own.take(id);
                versions.paths.take(id);
                versions.dirty.take(id);
                versions.automatic.remove(&id);
                versions.children.remove(&id);
                versions.freed.push(id);
            }
        }));
        Retained {
            versions,
            entries: BoxSlots::default(),
            registry: Registry::default(),
            variants: Variants::default(),
            announced: FxHashSet::default(),
            frame: 0,
            sent: Vec::new(),
            layers: None,
            fonts: 0,
            presentation: false,
        }
    }

    /// Cached node entries (diagnostics and tests)
    pub fn entry_count(&self) -> usize {
        self.entries.count()
    }

    /// Cache.Presentation: clip owners, rotation culls and scroll ranges on the nodes, no clip
    /// culling. Switching rebuilds every node (Structure).
    pub fn set_presentation(&mut self, enabled: bool) {
        if enabled != self.presentation {
            self.presentation = enabled;
            self.versions.borrow_mut().structure += 1;
        }
    }

    /// Scene.Build with the cache, then ReplyDelta's bookkeeping: what the client needs to hold
    /// this build's scene.
    pub fn build(&mut self, engine: &Rc<Engine>, roots: &[InstanceId]) -> Update {
        let _memory = crate::memtrace::scope(crate::memtrace::Tag::Export);
        for &root in roots {
            crate::engine::layout::ensure(engine, root);
        }
        // Records the layout changed since the last build mark their subtrees (Subtree reuse)
        let changed = engine.layout().borrow_mut().as_mut().map(|layout| layout.changed_records());
        if let Some(changed) = changed {
            let state = engine.state.borrow();
            let mut versions = self.versions.borrow_mut();
            for id in changed {
                versions.mark(&state, id);
            }
        }
        let state = engine.state.borrow();
        // Entries of freed instances and of trees that left the exported ones
        let (moved, freed) = {
            let mut versions = self.versions.borrow_mut();
            (std::mem::take(&mut versions.moved), std::mem::take(&mut versions.freed))
        };
        for id in freed {
            self.entries.take(id);
        }
        for root in moved {
            if state.dom.get(root).is_none() {
                continue;
            }
            let mut top = root;
            while let Some(parent) = state.dom.at(top).parent {
                top = parent;
            }
            if roots.contains(&top) || crate::engine::services::in_data_model(&state, top) {
                continue;
            }
            let mut stack = vec![root];
            while let Some(node) = stack.pop() {
                self.entries.take(node);
                stack.extend_from_slice(&state.dom.at(node).children);
            }
        }
        let layout_guard = engine.layout().borrow();
        let Some(layout) = layout_guard.as_ref() else {
            return Update { nodes: Vec::new(), removed: Vec::new(), layers: None, fonts: None, images: Vec::new(), exported: 0, misses: 0 };
        };
        let mut text = super::text_engine(engine);
        let content_root = text.fonts.content_root().map(str::to_string);
        let ctx = Ctx {
            state: &state,
            reflection: &engine.reflection,
            engine,
            layout,
            c: Classes::new(&engine.reflection),
            n: Names::new(&engine.reflection),
        };
        let mut versions = self.versions.borrow_mut();
        self.frame += 1;
        let frame = self.frame;
        let mut cached = Cached {
            entries: &mut self.entries,
            versions: &mut versions,
            frame,
            presentation: self.presentation,
            exported: Vec::with_capacity(self.sent.len()),
            announced: &mut self.announced,
            new_images: Vec::new(),
            misses: 0,
        };
        let mut out = Export {
            text: &mut *text,
            registry: &mut self.registry,
            variants: &mut self.variants,
            content_root,
            pixel_scale: engine.options.pixel_scale,
        };
        let (nodes, layers) = traverse(&ctx, &mut out, roots, Some(&mut cached));
        let Cached { exported, mut new_images, misses, .. } = cached;
        versions.camera_users.clear();
        if let Some(property) = engine.reflection.name_id("CurrentCamera") {
            for &(_, id) in &exported {
                if ctx.is_exactly(id, ctx.c.viewport_frame) {
                    if let Some(camera) = ctx.get(id, property).and_then(crate::engine::value::Value::as_instance) {
                        versions.camera_users.entry(camera).or_default().push(id);
                    }
                }
            }
        }
        // Changes from here on are the next build's
        versions.pending = frame + 1;
        drop(versions);


        // Nodes the client holds that this build did not export: removed from the tree or culled
        let mut removed = Vec::new();
        for &(serial, id) in &self.sent {
            match self.entries.get_mut(id) {
                Some(entry) if entry.exported == frame => {}
                Some(entry) => {
                    entry.sent = false;
                    removed.push(serial);
                }
                None => removed.push(serial),
            }
        }
        let count = exported.len();
        self.sent = exported;
        let layers = if self.layers.as_ref() != Some(&layers) {
            let json = layers_json(&layers);
            self.layers = Some(layers);
            Some(json)
        } else {
            None
        };
        let fonts = (self.registry.len() != self.fonts).then(|| {
            self.fonts = self.registry.len();
            self.registry.json()
        });
        new_images.sort();
        Update { nodes, removed, layers, fonts, images: new_images, exported: count, misses }
    }
}
