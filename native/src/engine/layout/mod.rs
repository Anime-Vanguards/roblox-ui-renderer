//! Layout engine (phase 2 of the native engine): a bit-exact port of `runtime/layout/init.luau`
//! (sizes, AutomaticSize, constraints, list / grid / page / table layouts, flex, scrolling, text
//! bounds, float32 emulation, incremental invalidation and record versions), `install.luau`
//! (computed-property getters with lazy layout, invalidation hooks, the per-frame pass, CanvasPosition
//! clamping, UIPageLayout navigation) and `clipping.luau`.
//!
//! State lives in [`Layout`] (one per engine, `Engine::layout`). A pass borrows it together with the
//! engine state, hooks and text engine ([`Pass`]); property writes made by the pass reach the
//! invalidation hook while the layout is borrowed, which ignores them exactly as the Luau hook
//! ignores computed properties and `Layout.Writing` writes.
//!
//! Reading results: `engine.layout().borrow()` -> `Layout::record(id)` (the Luau `Layout.Records`),
//! [`record::absolute_position`], `Layout::changed_records` (for retained export).

pub mod ids;
mod install;
mod pass;
pub mod record;
#[cfg(test)]
mod tests;

use std::time::Instant;

use rustc_hash::{FxHashMap, FxHashSet};

pub use install::{displayed_roots, ensure, ensure_with_state, install, invalidate, mark_changed, pages, set_track_churn, stats_snapshot, take_churn, update_all};
pub use record::{Rect, Record, ScrollBars, TextInfo, Transform, absolute_position};

use crate::engine::GuiEngine;
use super::{InstanceId, NameId, Reflection, State};
use ids::{Ids, P};

/// Per-instance storage indexed by arena slot, guarded by the id's generation (slots may be reused
/// by instance GC: a stale entry never answers for a new instance).
///
/// The slots are kept in fixed-size chunks made as they are first written: storage grows by a chunk,
/// never by moving what is there. (One growing vector of records moved tens of megabytes whenever
/// the arena passed a power of two, in every per-instance store at once: a 20-40 ms layout pass in a
/// story whose rebuilt UI keeps its old instances alive.)
pub struct Slots<T> {
    chunks: Vec<Option<Box<[Option<(u32, T)>]>>>,
}

const SLOT_CHUNK_BITS: usize = 8;
const SLOT_CHUNK: usize = 1 << SLOT_CHUNK_BITS;

impl<T> Default for Slots<T> {
    fn default() -> Self {
        Slots { chunks: Vec::new() }
    }
}

impl<T> Slots<T> {
    #[inline]
    fn slot(&self, index: usize) -> Option<&Option<(u32, T)>> {
        self.chunks.get(index >> SLOT_CHUNK_BITS)?.as_ref().map(|chunk| &chunk[index & (SLOT_CHUNK - 1)])
    }

    #[inline]
    fn slot_mut(&mut self, index: usize) -> Option<&mut Option<(u32, T)>> {
        self.chunks.get_mut(index >> SLOT_CHUNK_BITS)?.as_mut().map(|chunk| &mut chunk[index & (SLOT_CHUNK - 1)])
    }

    #[inline]
    pub fn get(&self, id: InstanceId) -> Option<&T> {
        match self.slot(id.index as usize) {
            Some(Some((generation, value))) if *generation == id.generation => Some(value),
            _ => None,
        }
    }

    #[inline]
    pub fn get_mut(&mut self, id: InstanceId) -> Option<&mut T> {
        match self.slot_mut(id.index as usize) {
            Some(Some((generation, value))) if *generation == id.generation => Some(value),
            _ => None,
        }
    }

    #[inline]
    pub fn set(&mut self, id: InstanceId, value: T) {
        let index = id.index as usize;
        let chunk = index >> SLOT_CHUNK_BITS;
        if self.chunks.len() <= chunk {
            self.chunks.resize_with(chunk + 1, || None);
        }
        let chunk = self.chunks[chunk].get_or_insert_with(|| (0..SLOT_CHUNK).map(|_| None).collect());
        chunk[index & (SLOT_CHUNK - 1)] = Some((id.generation, value));
    }

    #[inline]
    pub fn take(&mut self, id: InstanceId) -> Option<T> {
        let slot = self.slot_mut(id.index as usize)?;
        match slot {
            Some((generation, _)) if *generation == id.generation => slot.take().map(|(_, value)| value),
            _ => None,
        }
    }

    /// The number of stored values (diagnostics and tests)
    pub fn count(&self) -> usize {
        self.chunks.iter().flatten().map(|chunk| chunk.iter().filter(|slot| slot.is_some()).count()).sum()
    }
}

/// [`Slots`] of large values that only some instances have (a layout record is 880 bytes and half
/// of a UI's instances are decorations and layouts without one; one text label in a hundred
/// instances keeps a sticky text; an export entry exists for visited GUI nodes): a slot holds a
/// pointer, so an instance without the value costs 16 bytes in the store, not the value's size.
/// (Inline, a story that kept the UIs it rebuilt paid ~1.4 KB of empty slots per instance.)
pub struct BoxSlots<T>(Slots<Box<T>>);

impl<T> Default for BoxSlots<T> {
    fn default() -> Self {
        BoxSlots(Slots::default())
    }
}

impl<T> BoxSlots<T> {
    #[inline]
    pub fn get(&self, id: InstanceId) -> Option<&T> {
        self.0.get(id).map(|value| &**value)
    }

    #[inline]
    pub fn get_mut(&mut self, id: InstanceId) -> Option<&mut T> {
        self.0.get_mut(id).map(|value| &mut **value)
    }

    #[inline]
    pub fn set(&mut self, id: InstanceId, value: T) {
        // (the slot's allocation is reused when it has one)
        match self.0.get_mut(id) {
            Some(held) => **held = value,
            None => self.0.set(id, Box::new(value)),
        }
    }

    #[inline]
    pub fn take(&mut self, id: InstanceId) -> Option<T> {
        self.0.take(id).map(|value| *value)
    }

    pub fn count(&self) -> usize {
        self.0.count()
    }
}

/// A ComputeSize result (width, height, text box, base size, child measure size, held size).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Sized {
    pub w: f64,
    pub h: f64,
    pub text_box: Option<f64>,
    pub base_w: f64,
    pub base_h: f64,
    pub measure_w: Option<f64>,
    pub measure_h: Option<f64>,
    /// What an automatic parent measures this object at on an automatic axis whose UISizeConstraint MaxSize
    /// cut it: its content clamped by the constraint plus its padding offsets (None when that is not wider).
    pub held_w: Option<f64>,
    pub held_h: Option<f64>,
    /// A text object's TextBounds as this measure laid its text out (absolute px).
    pub bounds: Option<(f64, f64)>,
    /// The room an object automatic on one axis with a UIAspectRatioConstraint gives its children to grow into on
    /// that axis (local units, inside its padding); see ComputeSize.
    pub grow: Option<(Option<f64>, Option<f64>)>,
    /// What the plain children of an automatic object with a UIAspectRatioConstraint size their scale Size against on
    /// its automatic axes, instead of its box less its padding (local units); see ComputeSize.
    pub basis: Option<(Option<f64>, Option<f64>)>,
}

/// ComputeSize's inputs (the memo key; compared with Luau `==`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SizeArgs {
    pub parent_w: f64,
    pub parent_h: f64,
    pub scale: f64,
    pub available: Option<f64>,
    pub measure_parent_w: Option<f64>,
    pub measure_parent_h: Option<f64>,
    pub available_h: Option<f64>,
    pub full_w: Option<f64>,
    pub full_h: Option<f64>,
}

/// TextCarries entry: the local box a text child's text stays laid out in, a kept AutomaticSize Y
/// height, or a kept automatic width.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct CarryEntry {
    pub height: Option<f64>,
    pub width: Option<f64>,
}

pub(crate) type Carry = std::rc::Rc<Vec<(InstanceId, CarryEntry)>>;

pub(crate) struct MemoEntry {
    pub args: SizeArgs,
    pub result: Sized,
    pub carry: Option<Carry>,
}

pub(crate) struct Memo {
    pub stamp: u64,
    pub generation: u64,
    pub entries: Vec<MemoEntry>,
}

/// StickyText's key (TextMeasureKey): the text properties and measuring context a kept automatic
/// width depends on. Numbers compare by bits (Luau compares their `tostring`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StickyKey {
    pub text: Box<[u8]>,
    pub family: Box<[u8]>,
    pub weight: u64,
    pub italic: bool,
    pub text_size: Option<u64>,
    pub rich: bool,
    pub line_height: Option<u64>,
    pub x_alignment: Option<u32>,
    pub scale: u64,
    pub pixel_scale: u64,
    pub constraint: Option<(Option<u64>, Option<u64>)>,
}

pub(crate) struct Sticky {
    pub key: StickyKey,
    pub info: TextInfo,
    pub text_box: Option<f64>,
}

/// Reused per-call vectors of LayoutChildren (no allocation per laid-out container).
#[derive(Default)]
pub(crate) struct Scratch {
    pub resolved: Vec<Vec<pass::Resolved>>,
    pub pairs: Vec<Vec<Option<(f64, f64)>>>,
    pub boxes: Vec<Vec<Option<f64>>>,
    pub indices: Vec<Vec<usize>>,
}

/// `Layout.Stats` (RUR_LAYOUT_STATS=1 prints them after every UpdateAll that ran a pass).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stats {
    pub time: f64,
    pub passes: u64,
    pub computed: u64,
    pub memo: u64,
    pub measures: u64,
    pub stored: u64,
    pub skipped: u64,
    pub moved: u64,
    pub texts: u64,
    /// Number of scroll passes applied (`process_pending_scrolls` entries that moved at least one record)
    pub scroll_passes: u64,
    /// Records a scroll moved (`Pass::moved` while pending scrolls are applied)
    pub scroll_moved: u64,
    /// Skipped records a client lays out again anyway: in a pass after a geometry write in the tree (`Layout::relay`),
    /// every unchanged child of every object the pass lays out (`skipped` there)
    pub relaid: u64,
    /// The records under those (their subtrees, which a client walks again too)
    pub relaid_under: u64,
    /// Of `relaid`, the unchanged siblings of the objects written (children of their parents): what a script's read lays out
    /// again (install.rs `ensure`), where the frame's layout re-lays the whole tree
    pub relaid_near: u64,
    /// The records of trees whose objects a client sorts again after a ZIndex write (each such tree once a frame)
    pub resorted: u64,
}

impl Stats {
    pub fn add(&mut self, other: &Stats) {
        self.time += other.time;
        self.passes += other.passes;
        self.computed += other.computed;
        self.memo += other.memo;
        self.measures += other.measures;
        self.stored += other.stored;
        self.skipped += other.skipped;
        self.moved += other.moved;
        self.texts += other.texts;
        self.scroll_passes += other.scroll_passes;
        self.scroll_moved += other.scroll_moved;
        self.relaid += other.relaid;
        self.relaid_under += other.relaid_under;
        self.relaid_near += other.relaid_near;
        self.resorted += other.resorted;
    }

    pub fn delta_since(&self, before: &Stats) -> Stats {
        Stats {
            time: (self.time - before.time).max(0.0),
            passes: self.passes.saturating_sub(before.passes),
            computed: self.computed.saturating_sub(before.computed),
            memo: self.memo.saturating_sub(before.memo),
            measures: self.measures.saturating_sub(before.measures),
            stored: self.stored.saturating_sub(before.stored),
            skipped: self.skipped.saturating_sub(before.skipped),
            moved: self.moved.saturating_sub(before.moved),
            texts: self.texts.saturating_sub(before.texts),
            scroll_passes: self.scroll_passes.saturating_sub(before.scroll_passes),
            scroll_moved: self.scroll_moved.saturating_sub(before.scroll_moved),
            relaid: self.relaid.saturating_sub(before.relaid),
            relaid_under: self.relaid_under.saturating_sub(before.relaid_under),
            relaid_near: self.relaid_near.saturating_sub(before.relaid_near),
            resorted: self.resorted.saturating_sub(before.resorted),
        }
    }
}

/// Per-instance layout churn counters recorded when `Layout::track_churn` is enabled (`rur cost`).
/// `stored` and `moved` hold fractional attributed shares when an ancestor's record is shared by
/// multiple changed descendants in the same pass.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ObjectChurn {
    pub stored: f64,
    pub ancestor_stored: f64,
    pub moved: f64,
    pub scroll_passes: u64,
    pub scroll_moved: u64,
    /// `Stats::relaid` / `relaid_under` of the passes this object's geometry writes caused (shared by a pass's sources)
    pub relaid: f64,
    pub relaid_under: f64,
}

/// Resolved per-instance layout churn record emitted per sequence frame for `rur cost`.
#[derive(Clone, Debug, PartialEq)]
pub struct LayoutChurnEntry {
    pub id: u64,
    pub name: String,
    pub path: String,
    pub class: String,
    pub sibling_index: u32,
    pub path_count: u32,
    pub pos: [f64; 2],
    pub stored: f64,
    pub ancestor_stored: f64,
    pub moved: f64,
    pub scroll_passes: u64,
    pub scroll_moved: u64,
    pub relaid: f64,
    pub relaid_under: f64,
}

/// The layout's persistent state (the module-level tables of runtime/layout/init.luau).
pub struct Layout {
    pub ids: Ids,
    pub pixel_scale: f64,
    pub edit_mode: bool,
    pub viewport_width: f64,
    pub viewport_height: f64,
    pub top_inset: f64,
    pub clips_descendants_supports_rotation: bool,
    /// `Layout.Generation` (Invalidate forces a full pass)
    pub generation: u64,
    clock: u64,
    stamps: Slots<u64>,
    stored_stamps: Slots<u64>,
    size_memo: BoxSlots<Memo>,
    root_stamps: Slots<u64>,
    root_generations: Slots<u64>,
    /// ScrollingFrames whose CanvasPosition changed (insertion order)
    scrolled: Vec<InstanceId>,
    scrolled_set: FxHashSet<InstanceId>,
    /// ScrollingFrames a finger drags or that coast / spring back after a touch drag: their CanvasPosition may lie past
    /// the scroll range (a touch overscroll, input/touch.rs) and is not clamped to it
    pub overscroll: FxHashSet<InstanceId>,
    changed: FxHashSet<InstanceId>,
    record_version: u64,
    records: BoxSlots<Record>,
    sticky: BoxSlots<Sticky>,
    text_carries: Slots<Option<Carry>>,
    carry_stores: Slots<usize>,
    carry_maps: Vec<FxHashMap<InstanceId, [f64; 6]>>,
    carry_capture: Option<usize>,
    carry_apply: Option<usize>,
    /// The objects whose children are being laid out (outermost first): id, rect, scroll window, rotation (their
    /// records are out of `records` meanwhile)
    open: Vec<(InstanceId, [f64; 4], Option<Rect>, f64)>,
    pub stats: Stats,
    /// Counters already flushed by `print_stats` so [`Self::total_stats`] stays monotonic
    cumulative: Stats,
    pub print_stats: bool,
    /// The layout clock at the frame's layout point (`mark_frame_layout`): a tree whose last change is later has changed since a
    /// client laid it out, and the read that lays it out pays the pass (install.rs `ensure`)
    frame_layout_clock: u64,
    /// Each pending scroll's mark (`scroll_seq` when it was marked), and the mark at the frame's layout point
    scroll_marks: FxHashMap<InstanceId, u64>,
    scroll_seq: u64,
    frame_scroll_seq: u64,
    /// By root, what the per-frame pass laid out of a change made after the frame's layout point: a client lays that out at the
    /// tree's next read before the next point, which pays it (install.rs `ensure`)
    pub(crate) owed: FxHashMap<InstanceId, f64>,
    /// Scratch vectors (children lists) reused across calls
    pool: Vec<Vec<InstanceId>>,
    /// LayoutChildren's per-call vectors, reused (a stack: calls nest)
    scratch: Scratch,
    list_scratch: pass::ListVectors,
    /// Total time spent in layout passes (seconds), for profiling
    pub profile_time: f64,
    /// Per-instance churn attribution for `rur cost` sequences (off by default; zero cost when false)
    pub(crate) track_churn: bool,
    pub(crate) churn: FxHashMap<InstanceId, ObjectChurn>,
    /// Instances directly mutated since the last layout pass (causes of pass work when `track_churn` is true)
    pub(crate) dirty_sources: FxHashSet<InstanceId>,
    /// Raw `(stored, moved)` counts recorded per node during the active `Pass::run`, attributed to
    /// `dirty_sources` when the pass completes.
    pub(crate) pass_churn: FxHashMap<InstanceId, (u64, u64)>,
    /// Roots whose tree had a geometry write since their last pass (`mark_relay`), each with the parents of the objects
    /// written: a client lays every object of such a tree out again, the unchanged ones too (`Stats::relaid`)
    relay: FxHashMap<InstanceId, FxHashSet<InstanceId>>,
    /// The running pass's entry of `relay` (None: not a pass after a geometry write)
    pub(crate) relaying: Option<FxHashSet<InstanceId>>,
    /// Roots whose tree had a ZIndex write since the frame's layout (`Stats::resorted`)
    resort_roots: FxHashSet<InstanceId>,
    /// The record counts of the children laid out so far, one entry per object being laid out (`Record::count`)
    pub(crate) count_stack: Vec<u32>,
}

impl Layout {
    pub fn new(reflection: &Reflection, options: &super::Options) -> Layout {
        Layout {
            ids: Ids::new(reflection),
            pixel_scale: options.pixel_scale,
            edit_mode: options.edit_mode(),
            viewport_width: options.profile.device.viewport().0,
            viewport_height: options.profile.device.viewport().1,
            top_inset: options.profile.device.topbar_inset(),
            clips_descendants_supports_rotation: options.clips_descendants_supports_rotation,
            generation: 1,
            clock: 0,
            stamps: Slots::default(),
            stored_stamps: Slots::default(),
            size_memo: BoxSlots::default(),
            root_stamps: Slots::default(),
            root_generations: Slots::default(),
            scrolled: Vec::new(),
            scrolled_set: FxHashSet::default(),
            overscroll: FxHashSet::default(),
            changed: FxHashSet::default(),
            record_version: 0,
            records: BoxSlots::default(),
            sticky: BoxSlots::default(),
            text_carries: Slots::default(),
            carry_stores: Slots::default(),
            carry_maps: Vec::new(),
            carry_capture: None,
            carry_apply: None,
            open: Vec::new(),
            stats: Stats::default(),
            cumulative: Stats::default(),
            frame_layout_clock: 0,
            scroll_marks: FxHashMap::default(),
            scroll_seq: 0,
            frame_scroll_seq: 0,
            owed: FxHashMap::default(),
            print_stats: std::env::var("RUR_LAYOUT_STATS").as_deref() == Ok("1"),
            pool: Vec::new(),
            scratch: Scratch::default(),
            list_scratch: Default::default(),
            profile_time: 0.0,
            track_churn: false,
            churn: FxHashMap::default(),
            dirty_sources: FxHashSet::default(),
            pass_churn: FxHashMap::default(),
            relay: FxHashMap::default(),
            relaying: None,
            resort_roots: FxHashSet::default(),
            count_stack: Vec::new(),
        }
    }

    /// A geometry write in `id`'s tree (`property_changed`, `hierarchy_changed`): measured in a 0.747 client
    /// (notes/cost-calibration.md "Re-laid objects"), the tree's next layout lays every object of the tree out again
    /// (its LayerCollector's; another ScreenGui's tree is not), which a write that leaves the geometry alone does not.
    /// `parent`: the object whose children the write makes a script's read lay out again (the written object's parent).
    pub fn mark_relay(&mut self, state: &State, reflection: &Reflection, id: InstanceId, parent: Option<InstanceId>) {
        if state.dom.get(id).is_some() {
            let root = self.find_root(state, reflection, id);
            let parents = self.relay.entry(root).or_default();
            if let Some(parent) = parent {
                parents.insert(parent);
            }
        }
    }

    /// Whether `root`'s next pass re-lays the unchanged objects too, and the parents of the objects written (forgotten
    /// here): `Pass::run`
    pub(crate) fn take_relay(&mut self, root: InstanceId) -> Option<FxHashSet<InstanceId>> {
        self.relay.remove(&root)
    }

    /// The frame's ZIndex writes: a client sorts every object of each tree that had one again (measured in a 0.747 client:
    /// 0.18 us an object, by the tree's size, not the parent's: one ZIndex write among 10 siblings beside 1,000 frames cost
    /// as much as one among 1,000 siblings). Counted into `Stats::resorted` once a frame (install.rs `update_all`).
    pub(crate) fn count_resorts(&mut self) {
        if self.resort_roots.is_empty() {
            return;
        }
        let mut total = 0u64;
        for root in std::mem::take(&mut self.resort_roots) {
            if let Some(record) = self.records.get(root) {
                total += u64::from(record.count.max(1));
            }
        }
        self.stats.resorted += total;
    }

    /// `Layout.Records[Proxy]`
    #[inline]
    pub fn record(&self, id: InstanceId) -> Option<&Record> {
        self.records.get(id)
    }

    /// Monotonic lifetime layout pass counters (unaffected by `RUR_LAYOUT_STATS=1` resets)
    pub fn total_stats(&self) -> Stats {
        let mut total = self.cumulative;
        total.add(&self.stats);
        total
    }

    /// Stored (records, size memos, sticky texts) (diagnostics)
    pub fn counts(&self) -> (usize, usize, usize) {
        (self.records.count(), self.size_memo.count(), self.sticky.count())
    }

    /// `Layout.ChangedRecords()`: instances whose record changed since the previous call.
    pub fn changed_records(&mut self) -> FxHashSet<InstanceId> {
        std::mem::take(&mut self.changed)
    }

    /// `Layout.Invalidate()`
    pub fn invalidate(&mut self) {
        self.generation += 1;
    }

    /// The frame's layout point: a client lays every changed GUI out once a frame, after the render step's bindings and before
    /// PreRender, for nothing a script pays (install.rs `ensure` charges only a tree changed after this point).
    pub fn mark_frame_layout(&mut self) {
        self.frame_layout_clock = self.clock;
        self.frame_scroll_seq = self.scroll_seq;
        self.owed.clear();
    }

    /// Whether the pending scroll of `id` was made after the frame's layout point
    pub fn scrolled_after_frame_layout(&self, id: InstanceId) -> bool {
        self.scroll_marks.get(&id).is_some_and(|&seq| seq > self.frame_scroll_seq)
    }

    /// Whether `root`'s tree changed after the frame's layout point (`mark_frame_layout`)
    pub fn changed_since_frame_layout(&self, root: InstanceId) -> bool {
        self.stamps.get(root).copied().unwrap_or(0) > self.frame_layout_clock
    }

    fn mark_changed_internal(&mut self, state: &State, id: InstanceId) {
        self.clock += 1;
        let clock = self.clock;
        let mut node = Some(id);
        while let Some(current) = node {
            let Some(instance) = state.dom.get(current) else { break };
            self.stamps.set(current, clock);
            node = instance.parent;
        }
    }

    /// `Layout.MarkChanged`: stamps `id` and every ancestor with a new clock value.
    pub fn mark_changed(&mut self, state: &State, id: InstanceId) {
        if self.track_churn {
            self.dirty_sources.insert(id);
        }
        self.mark_changed_internal(state, id);
    }

    pub(crate) fn scrolled_is_empty(&self) -> bool {
        self.scrolled.is_empty()
    }

    /// `Layout.MarkScrolled`
    pub fn mark_scrolled(&mut self, id: InstanceId) {
        if self.scrolled_set.insert(id) {
            self.scrolled.push(id);
        }
        self.scroll_seq += 1;
        self.scroll_marks.insert(id, self.scroll_seq);
    }

    fn unmark_scrolled(&mut self, id: InstanceId) {
        self.scroll_marks.remove(&id);
        if self.scrolled_set.remove(&id) {
            if let Some(index) = self.scrolled.iter().position(|&s| s == id) {
                self.scrolled.remove(index);
            }
        }
    }

    /// `Layout.FindRoot`: topmost GuiBase2d ancestor-or-self (through Folders).
    pub fn find_root(&self, state: &State, reflection: &Reflection, id: InstanceId) -> InstanceId {
        let mut root = id;
        let mut current = state.dom.at(id).parent;
        while let Some(node) = current {
            let instance = state.dom.at(node);
            if reflection.class(instance.class).is_a(self.ids.c.GuiBase2d) {
                root = node;
            } else if instance.class != self.ids.c.Folder {
                break;
            }
            current = instance.parent;
        }
        root
    }

    /// `Layout.MarkPlaced`
    pub fn mark_placed(&mut self, state: &State, reflection: &Reflection, id: InstanceId) {
        if self.track_churn {
            self.dirty_sources.insert(id);
        }
        let parent = state.dom.at(id).parent;
        match parent {
            Some(parent) if self.find_root(state, reflection, id) != id => self.mark_changed_internal(state, parent),
            _ => self.mark_changed_internal(state, id),
        }
    }

    /// install.luau's PropertyChanged hook.
    pub fn property_changed(&mut self, state: &State, reflection: &Reflection, id: InstanceId, name: NameId) {
        let Some(instance) = state.dom.get(id) else { return };
        let (n, c) = (self.ids.n, self.ids.c);
        let flags = self.ids.name_flags(name);
        if flags & P::COMPUTED != 0 {
            return;
        }
        let class = reflection.class(instance.class);
        if name == n.ZIndex && class.is_a(c.GuiObject) {
            let root = self.find_root(state, reflection, id);
            self.resort_roots.insert(root);
            return;
        }
        let appearance = flags & P::APPEARANCE != 0
            && !(name == n.TextXAlignment
                && class.is_a(c.GuiObject)
                && !matches!(state.dom.raw_get(id, n.AutomaticSize), Some(super::Value::EnumItem(item)) if Some(*item) == self.ids.automatic_none));
        if appearance {
            return;
        }
        // IsRelevant: GuiBase2d / UIComponent / UIBase, except appearance-only components
        if instance.class == c.UICorner || instance.class == c.UIDragDetector || instance.class == c.UIGradient || instance.class == c.UIStroke {
            return;
        }
        if !(class.is_a(c.GuiBase2d) || class.is_a(c.UIComponent) || class.is_a(c.UIBase)) {
            return;
        }
        // (a geometry write, `relays`; a layout object's or modifier's write lays its parent's tree out the same way, measured
        // for UIPadding and UIScale)
        if class.is_a(c.GuiObject) && self.relays(state, reflection, class, id, name) {
            let parent = instance.parent;
            self.mark_relay(state, reflection, id, parent);
        } else if class.is_a(c.UIComponent) && instance.class != c.UIShadow {
            if let Some(parent) = instance.parent {
                let grand = state.dom.get(parent).and_then(|p| p.parent);
                self.mark_relay(state, reflection, parent, grand);
            }
        }
        if name == n.CanvasPosition && instance.class == c.ScrollingFrame {
            self.mark_scrolled(id);
        } else if flags & P::PLACEMENT != 0 && class.is_a(c.GuiObject) {
            self.mark_placed(state, reflection, id);
        } else {
            self.mark_changed(state, id);
        }
    }

    /// Whether a write of `name` on the GuiObject `id` is a geometry write (`mark_relay`). Measured in a 0.747 client, one
    /// object's write among 1,000 siblings: Size (also of the same absolute size), Position, AnchorPoint, Rotation, Visible,
    /// SizeConstraint and AutomaticSize lay all 1,000 out again (0.7 to 2 ms a frame), and so does any text input of a text
    /// object that sizes itself to its text (AutomaticSize; Text also when its width stays, TextSize, Font), and LayoutOrder
    /// beside a UIGridLayout (300 cards: 0.48 ms); its Text, TextSize, Font or TextWrapped in a fixed box, TextScaled text,
    /// colours, Name, and LayoutOrder without a layout object do not.
    fn relays(&self, state: &State, reflection: &Reflection, class: &crate::engine::reflection::ClassInfo, id: InstanceId, name: NameId) -> bool {
        let n = &self.ids.n;
        let c = &self.ids.c;
        if name == n.Size || name == n.Position || name == n.AnchorPoint || name == n.Rotation || name == n.Visible || name == n.SizeConstraint || name == n.AutomaticSize {
            return true;
        }
        if name == n.LayoutOrder {
            let parent = state.dom.at(id).parent.and_then(|parent| state.dom.get(parent));
            return parent.is_some_and(|parent| {
                parent.children.iter().any(|&child| state.dom.get(child).is_some_and(|child| reflection.class(child.class).is_a(c.UIGridStyleLayout)))
            });
        }
        // (Name: placement only to a layout object sorting by name, not measured)
        if self.ids.name_flags(name) & P::PLACEMENT != 0 {
            return false;
        }
        (class.is_a(c.TextLabel) || class.is_a(c.TextButton) || class.is_a(c.TextBox))
            && matches!(state.dom.raw_get(id, n.AutomaticSize), Some(super::Value::EnumItem(item)) if Some(*item) != self.ids.automatic_none)
    }

    /// Drops every piece of layout state of a freed instance.
    pub fn forget(&mut self, id: InstanceId) {
        self.records.take(id);
        self.stamps.take(id);
        self.stored_stamps.take(id);
        self.size_memo.take(id);
        self.root_stamps.take(id);
        self.root_generations.take(id);
        self.sticky.take(id);
        self.text_carries.take(id);
        if let Some(index) = self.carry_stores.take(id) {
            self.carry_maps[index].clear();
        }
        self.unmark_scrolled(id);
        self.overscroll.remove(&id);
        self.changed.remove(&id);
        self.churn.remove(&id);
        self.dirty_sources.remove(&id);
        self.pass_churn.remove(&id);
        self.relay.remove(&id);
        self.resort_roots.remove(&id);
    }

    /// install.luau's HierarchyChanged hook.
    pub fn hierarchy_changed(&mut self, state: &State, reflection: &Reflection, id: InstanceId, old_parent: Option<InstanceId>) {
        if self.track_churn {
            if state.dom.get(id).and_then(|i| i.parent).is_some() {
                self.dirty_sources.insert(id);
            }
            if let Some(old) = old_parent {
                self.dirty_sources.insert(old);
            }
        }
        // (a GuiObject put in or taken out: a geometry write in both trees, measured as Parent = nil and back among 1,000
        // siblings)
        let relevant = state.dom.get(id).is_some_and(|instance| {
            let class = reflection.class(instance.class);
            let c = &self.ids.c;
            let appearance_only = [c.UICorner, c.UIDragDetector, c.UIGradient, c.UIStroke, c.UIShadow].contains(&instance.class);
            class.is_a(c.GuiObject) || (class.is_a(c.UIComponent) && !appearance_only)
        });
        if relevant {
            if let Some(parent) = state.dom.at(id).parent {
                self.mark_relay(state, reflection, id, Some(parent));
            }
            if let Some(old) = old_parent {
                self.mark_relay(state, reflection, old, Some(old));
            }
        }
        self.mark_changed_internal(state, id);
        if let Some(old) = old_parent {
            self.mark_changed_internal(state, old);
        }
    }

    fn take_vec(&mut self) -> Vec<InstanceId> {
        let mut list = self.pool.pop().unwrap_or_default();
        list.clear();
        list
    }

    fn give_vec(&mut self, list: Vec<InstanceId>) {
        if self.pool.len() < 64 {
            self.pool.push(list);
        }
    }
}

pub(crate) fn elapsed(started: Instant) -> f64 {
    started.elapsed().as_secs_f64()
}

/// `Layout.ProfileTime`: seconds spent in layout passes so far (0 before install); None while a
/// pass holds the layout state.
pub fn profile_time(engine: &super::Engine) -> Option<f64> {
    let guard = engine.layout().try_borrow().ok()?;
    Some(guard.as_ref().map_or(0.0, |layout| layout.profile_time))
}
