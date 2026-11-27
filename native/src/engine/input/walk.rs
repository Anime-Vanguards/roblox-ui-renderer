//! Which of the objects under a point an input reaches, ScreenGui by ScreenGui from the top (measured in 0.747 clients,
//! P6d; notes/interaction.md "Stacked objects").
//!
//! A client hands a finger's or the mouse's input to the ScreenGuis under the point from the topmost down. A ScreenGui
//! with an Active object under the point (a sinker; a finger's ScrollingFrame counts too, touch.rs) is the last one
//! that gets it: the ScreenGuis below get nothing at all and keep the hover they had. Within the last one a finger's
//! touch reaches the objects down to the sinker; the ones below it are hovered (MouseEnter / MouseLeave / MouseMoved)
//! but get no InputBegan / InputChanged / InputEnded. The mouse's movement reaches every object of each ScreenGui it is
//! handed to.

use rustc_hash::{FxHashMap, FxHashSet};

use super::Cx;
use crate::engine::InstanceId;

/// The ranks of the displayed ScreenGuis: their place in the draw order, bottom first (a higher rank draws above)
pub(crate) struct Ranks(FxHashMap<InstanceId, usize>);

impl Ranks {
    pub fn new(cx: &Cx) -> Ranks {
        let state = cx.engine.state.borrow();
        let roots = cx.i.displayed_roots(cx.engine, &state);
        Ranks(roots.into_iter().enumerate().map(|(rank, root)| (root, rank)).collect())
    }

    /// The rank of the ScreenGui `id` is drawn in (None: not in a displayed one, or gone)
    pub fn of(&self, cx: &Cx, id: InstanceId) -> Option<usize> {
        if !cx.alive(id) {
            return None;
        }
        let state = cx.engine.state.borrow();
        let root = cx.i.find_root(&state, id);
        self.0.get(&root).copied()
    }
}

/// The objects under a point (topmost first) by ScreenGui, topmost ScreenGui first
pub(crate) fn groups(cx: &Cx, ranks: &Ranks, list: &[InstanceId]) -> Vec<(Option<usize>, Vec<InstanceId>)> {
    let mut out: Vec<(Option<usize>, Vec<InstanceId>)> = Vec::new();
    for &id in list {
        let rank = ranks.of(cx, id);
        match out.last_mut() {
            Some((last, objects)) if *last == rank => objects.push(id),
            _ => out.push((rank, vec![id])),
        }
    }
    out
}

/// Where an input stops: the ScreenGuis it is handed to and the objects there it only hovers
pub(crate) struct Walk {
    /// The lowest rank handed the input (None: every ScreenGui)
    pub floor: Option<usize>,
    /// The handed ScreenGuis' objects under the point, by ScreenGui from the top
    pub groups: Vec<(Option<usize>, Vec<InstanceId>)>,
    /// Objects under the point below a sinker of their ScreenGui (hover only)
    pub sunk: FxHashSet<InstanceId>,
}

/// A ScrollingFrame that holds a finger's touch (touch.rs): the objects of its ScreenGui drawn below it (all but its own
/// descendants when the finger is off it) are sunk, and `stops`: the ScreenGuis below it get nothing
#[derive(Clone, Copy)]
pub(crate) struct Holder {
    pub frame: InstanceId,
    pub stops: bool,
}

impl Walk {
    /// Whether a ScreenGui of `rank` is handed the input
    pub fn handles(&self, rank: Option<usize>) -> bool {
        match (rank, self.floor) {
            (Some(rank), Some(floor)) => rank >= floor,
            _ => true,
        }
    }

    pub fn is_sunk(&self, id: InstanceId) -> bool {
        self.sunk.contains(&id)
    }

    /// The handed objects under the point, topmost first
    pub fn objects(&self) -> Vec<InstanceId> {
        self.groups.iter().flat_map(|(_, objects)| objects.iter().copied()).collect()
    }

    /// The handed objects that are not sunk (they get Input* events), topmost first
    pub fn receivers(&self) -> Vec<InstanceId> {
        self.objects().into_iter().filter(|id| !self.sunk.contains(id)).collect()
    }

    /// The group of `rank`, empty if nothing of it is under the point
    pub fn group(&self, rank: Option<usize>) -> &[InstanceId] {
        self.groups.iter().find(|(r, _)| *r == rank).map_or(&[], |(_, objects)| objects.as_slice())
    }
}

/// The walk for a finger (`in_gui`: objects below a sinker of its ScreenGui are sunk) or the mouse's movement (not)
pub(crate) fn walk(cx: &Cx, ranks: &Ranks, list: &[InstanceId], holder: Option<Holder>, in_gui: bool) -> Walk {
    let holder_rank = holder.and_then(|h| ranks.of(cx, h.frame));
    let mut out = Walk { floor: None, groups: Vec::new(), sunk: FxHashSet::default() };
    for (rank, objects) in groups(cx, ranks, list) {
        if let (Some(h), Some(held), Some(r)) = (holder, holder_rank, rank) {
            if h.stops && r < held {
                out.floor = Some(held);
                break;
            }
        }
        let mut stops = false;
        let mut cut = None;
        if let (Some(h), true) = (holder, holder_rank.is_some() && holder_rank == rank) {
            match objects.iter().position(|&o| o == h.frame) {
                Some(index) => cut = Some(index),
                None => {
                    // (the finger is off the frame: everything of its ScreenGui under the point but the frame's own
                    // descendants is below it)
                    let state = cx.engine.state.borrow();
                    for &o in &objects {
                        if !state.dom.is_descendant_of(o, h.frame) {
                            out.sunk.insert(o);
                        }
                    }
                }
            }
            stops |= h.stops;
        }
        if let Some(index) = objects.iter().position(|&o| cx.is_true(o, cx.i.n.Active)) {
            cut = Some(cut.map_or(index, |c: usize| c.min(index)));
            stops = true;
        }
        if let (Some(index), true) = (cut, in_gui) {
            out.sunk.extend(objects[index + 1..].iter().copied());
        }
        // (the ScrollingFrame that holds the touch gets it below an Active object too: a drag that starts on a button
        // inside it scrolls it, and it gets InputChanged up to the drag's start, measured P6b)
        if let Some(h) = holder {
            out.sunk.remove(&h.frame);
        }
        out.groups.push((rank, objects));
        if stops {
            out.floor = rank;
            break;
        }
    }
    if out.floor.is_none() {
        if let (Some(h), Some(held)) = (holder, holder_rank) {
            if h.stops {
                out.floor = Some(held);
            }
        }
    }
    out
}

/// The ranks to visit for a walk: the handed ones under the point and those of `hovered` (objects hovered before),
/// topmost first (objects no longer in a displayed ScreenGui first)
pub(crate) fn visit_order(walk: &Walk, hovered: &[(InstanceId, Option<usize>)]) -> Vec<Option<usize>> {
    let mut ranks: Vec<Option<usize>> = walk.groups.iter().map(|(r, _)| *r).collect();
    for &(_, rank) in hovered {
        if walk.handles(rank) && !ranks.contains(&rank) {
            ranks.push(rank);
        }
    }
    // None (gone) first, then descending ranks
    ranks.sort_by(|a, b| match (a, b) {
        (None, None) => std::cmp::Ordering::Equal,
        (None, _) => std::cmp::Ordering::Less,
        (_, None) => std::cmp::Ordering::Greater,
        (Some(a), Some(b)) => b.cmp(a),
    });
    ranks
}
