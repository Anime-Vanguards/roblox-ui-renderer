//! Gamepad and keyboard UI navigation: GuiService.SelectedObject moved by the D-pad, the arrow keys and WASD,
//! ButtonSelect's selection toggle, which objects can be selected and when a selection is dropped.
//!
//! Measured in a 0.747 client (notes/interaction.md "Gamepad selection"): one-candidate sweeps for
//! what a press can reach, pairwise searches and maps for which candidate wins, and 7,208 presses over 300 random
//! scenes of 4-8 buttons (the model below picks the client's button in 7,189 of them; the rest are choices
//! between diagonal candidates). Rects are the unrotated AbsolutePosition / AbsoluteSize.

use super::{Cx, hit};
use crate::engine::{InstanceId, value::Value};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dir {
    Up,
    Down,
    Left,
    Right,
}

impl Dir {
    /// A key that moves the selection: the D-pad, the arrow keys and WASD (all measured).
    pub fn of_key(name: &str) -> Option<Dir> {
        match name {
            "DPadUp" | "Up" | "W" => Some(Dir::Up),
            "DPadDown" | "Down" | "S" => Some(Dir::Down),
            "DPadLeft" | "Left" | "A" => Some(Dir::Left),
            "DPadRight" | "Right" | "D" => Some(Dir::Right),
            _ => None,
        }
    }

    /// The GuiNavigationDirection item SelectionGained / SelectionLost carry
    pub fn item(self) -> &'static str {
        match self {
            Dir::Up => "Up",
            Dir::Down => "Down",
            Dir::Left => "Left",
            Dir::Right => "Right",
        }
    }

    fn vertical(self) -> bool {
        matches!(self, Dir::Up | Dir::Down)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Rect {
    l: f64,
    t: f64,
    r: f64,
    b: f64,
    cx: f64,
    cy: f64,
}

impl Rect {
    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect { l: x, t: y, r: x + w, b: y + h, cx: x + w / 2.0, cy: y + h / 2.0 }
    }
}

/// Whether a press toward `dir` from `a` can reach `b`. Measured with one candidate swept around a button:
/// - Up / Down: a candidate whose x range holds `a`'s centre line counts until its far edge passes `a`'s far edge
///   (Up: b.bottom <= a.bottom; at equal bottoms only one that starts below a.top); any other until its far edge
///   passes `a`'s centre line (Up: b.bottom <= a.cy).
/// - Left / Right, by the candidate's vertical place: entirely above or below `a`: its centre beyond `a`'s near edge
///   (Left: b.cx < a.left; on that edge only below `a`) and its far edge not past `a`'s centre (b.right <= a.cx);
///   holding `a`'s centre line: b.right <= a.right or b.left < a.left; overlapping it otherwise: b.right <= a.cx.
pub fn accepts(a: &Rect, b: &Rect, dir: Dir) -> bool {
    match dir {
        Dir::Up | Dir::Down => {
            let holds = b.l <= a.cx && a.cx <= b.r;
            if dir == Dir::Up {
                if holds { b.b < a.b || (b.b == a.b && b.t >= a.t) } else { b.b <= a.cy }
            } else if holds {
                b.t > a.t || (b.t == a.t && b.b <= a.b)
            } else {
                b.t >= a.cy
            }
        }
        Dir::Left | Dir::Right => {
            let overlaps = b.b > a.t && b.t < a.b;
            let holds = b.t <= a.cy && a.cy <= b.b;
            if dir == Dir::Left {
                if !overlaps {
                    (b.cx < a.l || (b.cx == a.l && b.t >= a.b)) && b.r <= a.cx
                } else if holds {
                    b.r <= a.r || b.l < a.l
                } else {
                    b.r <= a.cx
                }
            } else if !overlaps {
                (b.cx > a.r || (b.cx == a.r && b.t >= a.b)) && b.l >= a.cx
            } else if holds {
                b.l >= a.l || b.r > a.r
            } else {
                b.l >= a.cx
            }
        }
    }
}

/// The ranking key of a reachable candidate (smaller wins), measured pairwise and fitted on random scenes:
/// 1. In `a`'s band (its x range for Up / Down, its y range for Left / Right, open edges): the nearest by the
///    gap between the facing edges, then the nearest to `a`'s centre line, then the top / left one.
/// 2. In the 45-degree cone: Up / Down from `a`'s centre, Left / Right from the middle of `a`'s facing edge (the
///    candidate's centre); ranked by hypot(along, 10 x across) between the centres, a lateral offset counting ten
///    times its distance.
/// 3. Anything else: the centre distance times (1 + angle^2), the angle off the direction in radians.
pub fn rank(a: &Rect, b: &Rect, dir: Dir) -> (u8, f64, f64, f64) {
    let (dx, dy) = (b.cx - a.cx, b.cy - a.cy);
    let (along, across) = match dir {
        Dir::Up => (-dy, dx),
        Dir::Down => (dy, dx),
        Dir::Left => (-dx, dy),
        Dir::Right => (dx, dy),
    };
    let in_band = if dir.vertical() { b.r > a.l && b.l < a.r } else { b.b > a.t && b.t < a.b };
    if in_band {
        let gap = match dir {
            Dir::Up => a.t - b.b,
            Dir::Down => b.t - a.b,
            Dir::Left => a.l - b.r,
            Dir::Right => b.l - a.r,
        };
        let first = if dir.vertical() { b.cx } else { b.cy };
        return (0, gap, across.abs(), first);
    }
    let from_edge = match dir {
        Dir::Up | Dir::Down => along,
        Dir::Left => a.l - b.cx,
        Dir::Right => b.cx - a.r,
    };
    if across.abs() <= from_edge {
        return (1, along.hypot(10.0 * across), 0.0, 0.0);
    }
    let angle = across.abs().atan2(along);
    (2, along.hypot(across) * (1.0 + angle * angle), 0.0, 0.0)
}

fn less(x: (u8, f64, f64, f64), y: (u8, f64, f64, f64)) -> bool {
    x.partial_cmp(&y) == Some(std::cmp::Ordering::Less)
}

/// The best reachable candidate from `a` toward `dir` among `candidates` (draw order).
pub fn choose(a: &Rect, dir: Dir, candidates: &[(InstanceId, Rect)]) -> Option<InstanceId> {
    let mut best: Option<((u8, f64, f64, f64), InstanceId)> = None;
    for (id, b) in candidates {
        if !accepts(a, b, dir) {
            continue;
        }
        let key = rank(a, b, dir);
        if best.is_none_or(|(held, _)| less(key, held)) {
            best = Some((key, *id));
        }
    }
    best.map(|(_, id)| id)
}

impl Cx<'_> {
    fn rect(&self, id: InstanceId) -> Option<Rect> {
        self.record(id, |r| Rect::new(r.x, r.y, r.w, r.h))
    }

    fn screen(&self) -> (f64, f64) {
        self.engine.options.profile.device.viewport()
    }

    /// Selectable, shown and interactable, with its centre on the screen and inside every clipping ancestor
    /// other than a ScrollingFrame (measured: a button a clipping Frame hides is skipped, one a ScrollingFrame
    /// scrolled away is not). Active does not matter.
    fn selectable_at(&self, id: InstanceId) -> Option<Rect> {
        if !self.alive(id) || !self.is(id, self.i.c.GuiObject) || !self.is_true(id, self.i.n.Selectable) {
            return None;
        }
        if !self.record(id, |r| r.visible).unwrap_or(false) || !self.i.interactable(&self.engine.state.borrow(), id) {
            return None;
        }
        let rect = self.rect(id)?;
        let (w, h) = self.screen();
        if rect.cx < 0.0 || rect.cy < 0.0 || rect.cx > w || rect.cy > h {
            return None;
        }
        let state = self.engine.state.borrow();
        let mut current = state.dom.at(id).parent;
        while let Some(node) = current {
            let class = state.dom.at(node).class;
            if !self.i.class_is(class, self.i.c.GuiObject) {
                break;
            }
            let clips = matches!(state.dom.raw_get(node, self.i.n.ClipsDescendants), Some(Value::Bool(true)))
                || self.i.class_is(class, self.i.c.CanvasGroup);
            if clips && !self.i.class_is(class, self.i.c.ScrollingFrame) {
                let inside = self.record(node, |r| rect.cx >= r.x && rect.cy >= r.y && rect.cx <= r.x + r.w && rect.cy <= r.y + r.h);
                if inside == Some(false) {
                    return None;
                }
            }
            current = state.dom.at(node).parent;
        }
        Some(rect)
    }

    /// The objects a press can move to, in draw order: shown in an enabled ScreenGui, selectable (above), and not
    /// under a GuiButton or TextBox drawn above them at their centre (measured: a button over the centre hides a
    /// candidate, even a transparent or unselectable one; a Frame, Active or not, does not; one beside the centre
    /// does not).
    pub fn selectables(&self) -> Vec<(InstanceId, Rect)> {
        let order = self.i.draw_order(self.engine);
        let state_interactable = |id| self.i.interactable(&self.engine.state.borrow(), id);
        let mut out = Vec::new();
        for (index, entry) in order.iter().enumerate() {
            if entry.culled {
                continue;
            }
            let Some(rect) = self.selectable_at(entry.id) else { continue };
            let covered = order[index + 1..].iter().any(|above| {
                (self.is(above.id, self.i.c.GuiButton) || self.is(above.id, self.i.c.TextBox))
                    && state_interactable(above.id)
                    && hit::contains(above, rect.cx, rect.cy)
            });
            if !covered {
                out.push((entry.id, rect));
            }
        }
        out
    }

    /// Whether the selection stays selected (checked every frame). Measured: one made invisible, unselectable or
    /// moved off the screen is dropped, and so is a script's selection of something that cannot be selected (a
    /// TextLabel); one made inactive, or scrolled out of its ScrollingFrame's window, stays.
    /// (An object the layout has not placed yet, one just made, is judged once it has a record.)
    pub fn selection_holds(&self, id: InstanceId) -> bool {
        let attached = self.alive(id) && self.engine.state.borrow().dom.at(id).parent.is_some();
        if !attached || !self.is(id, self.i.c.GuiObject) || !self.is_true(id, self.i.n.Selectable) {
            return false;
        }
        let layer_enabled = {
            let state = self.engine.state.borrow();
            std::iter::successors(state.dom.at(id).parent, |&node| state.dom.at(node).parent)
                .find(|&node| self.i.class_is(state.dom.at(node).class, self.i.c.LayerCollector))
                .map(|layer| !matches!(state.dom.raw_get(layer, self.i.n.Enabled), Some(Value::Bool(false))))
        };
        if layer_enabled != Some(true) {
            return false;
        }
        let Some((visible, rect)) = self.record(id, |r| (r.visible, Rect::new(r.x, r.y, r.w, r.h))) else { return true };
        let (w, h) = self.screen();
        visible && rect.cx >= 0.0 && rect.cy >= 0.0 && rect.cx <= w && rect.cy <= h
    }

    /// Where a press toward `dir` moves the selection from `from` (None: it stays).
    /// - NextSelectionUp / Down / Left / Right win when they name a selectable object, whatever its direction
    ///   (itself too); otherwise the search runs as if unset.
    /// - Inside a SelectionGroup (innermost first) the search looks at the group's descendants first; with nothing
    ///   there it leaves the group (SelectionBehavior Escape) or stays (Stop).
    pub fn navigate(&self, from: InstanceId, dir: Dir) -> Option<InstanceId> {
        let candidates = self.selectables();
        let next = match dir {
            Dir::Up => "NextSelectionUp",
            Dir::Down => "NextSelectionDown",
            Dir::Left => "NextSelectionLeft",
            Dir::Right => "NextSelectionRight",
        };
        if let Some(name) = self.engine.reflection.name_id(next) {
            if let Some(target) = self.get(from, name).and_then(|v| v.as_instance()) {
                if candidates.iter().any(|(id, _)| *id == target) {
                    return Some(target);
                }
            }
        }
        let a = self.rect(from)?;
        let others: Vec<(InstanceId, Rect)> = candidates.into_iter().filter(|(id, _)| *id != from).collect();
        let (group, behavior) = match (self.engine.reflection.name_id("SelectionGroup"), self.engine.reflection.name_id(&format!("SelectionBehavior{}", dir.item()))) {
            (Some(group), Some(behavior)) => (group, behavior),
            _ => return choose(&a, dir, &others),
        };
        let groups: Vec<InstanceId> = {
            let state = self.engine.state.borrow();
            std::iter::successors(state.dom.at(from).parent, |&id| state.dom.at(id).parent)
                .take_while(|&id| self.i.class_is(state.dom.at(id).class, self.i.c.GuiBase2d))
                .filter(|&id| matches!(state.dom.raw_get(id, group), Some(Value::Bool(true))))
                .collect()
        };
        for container in groups {
            let inside: Vec<(InstanceId, Rect)> = {
                let state = self.engine.state.borrow();
                others.iter().filter(|(id, _)| state.dom.is_descendant_of(*id, container)).copied().collect()
            };
            if let Some(found) = choose(&a, dir, &inside) {
                return Some(found);
            }
            if self.enum_name(container, behavior) == "Stop" {
                return None;
            }
        }
        choose(&a, dir, &others)
    }

    /// ButtonSelect with nothing selected: the lowest SelectionOrder, then the highest layer (DisplayOrder, then the
    /// ZIndex path in draw order), then the topmost, then the leftmost (measured; creation order does not matter).
    pub fn auto_select(&self) -> Option<InstanceId> {
        let order_name = self.i.n.SelectionOrder;
        let mut best: Option<(Vec<f64>, InstanceId)> = None;
        for (id, rect) in self.selectables() {
            let mut key = vec![self.number(id, order_name).unwrap_or(0.0)];
            let (display, path) = self.layer_path(id);
            key.push(-display);
            key.extend(path.iter().map(|z| -z));
            // a shorter ZIndex path is drawn below a longer one that shares it
            key.push(f64::INFINITY);
            key.extend([rect.t, rect.l]);
            let better = best.as_ref().is_none_or(|(held, _)| {
                // keys of different lengths: compare the shared part, then the layer marker
                key.partial_cmp(held) == Some(std::cmp::Ordering::Less)
            });
            if better {
                best = Some((key, id));
            }
        }
        best.map(|(_, id)| id)
    }

    /// The DisplayOrder of the object's ScreenGui and the ZIndex of each GuiObject from the top down
    /// (ZIndexBehavior Global: the object's own ZIndex)
    fn layer_path(&self, id: InstanceId) -> (f64, Vec<f64>) {
        let state = self.engine.state.borrow();
        let mut path = Vec::new();
        let mut display = 0.0;
        let mut global = false;
        let mut current = Some(id);
        while let Some(node) = current {
            let class = state.dom.at(node).class;
            if self.i.class_is(class, self.i.c.GuiObject) {
                path.push(state.dom.raw_get(node, self.i.n.ZIndex).and_then(Value::as_number).unwrap_or(1.0));
            } else if self.i.class_is(class, self.i.c.LayerCollector) {
                display = state.dom.raw_get(node, self.i.n.DisplayOrder).and_then(Value::as_number).unwrap_or(0.0);
                global = state
                    .dom
                    .raw_get(node, self.i.n.ZIndexBehavior)
                    .and_then(Value::as_enum)
                    .is_some_and(|item| &*self.engine.reflection.enum_item_info(item).name == "Global");
                break;
            }
            current = state.dom.at(node).parent;
        }
        path.reverse();
        if global {
            path = path.last().copied().into_iter().collect();
        }
        (display, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect::new(x, y, w, h)
    }

    fn id(k: u32) -> InstanceId {
        InstanceId { index: k, generation: 0 }
    }

    /// random_scenes_follow_the_client's count of presses the model gets right, of those both captures agree on
    const MATCHED: usize = 7187;
    const TOTAL: usize = 7201;

    /// One-candidate sweeps around a 100 x 60 button at (350, 270) (the client's last accepted and first refused
    /// positions, 2 px apart)
    #[test]
    fn reach_follows_the_client_sweeps() {
        let a = r(350.0, 270.0, 100.0, 60.0);
        // Up, beside: until the bottom passes the centre line, whatever the height
        for h in [10.0, 80.0, 160.0] {
            assert!(accepts(&a, &r(100.0, 300.0 - h, 40.0, h), Dir::Up));
            assert!(!accepts(&a, &r(100.0, 302.0 - h, 40.0, h), Dir::Up));
        }
        // Up, over the centre column: until the bottom passes a's bottom; at equal bottoms only from inside
        assert!(accepts(&a, &r(380.0, 320.0, 40.0, 10.0), Dir::Up));
        assert!(!accepts(&a, &r(380.0, 322.0, 40.0, 10.0), Dir::Up));
        assert!(accepts(&a, &r(380.0, 290.0, 40.0, 40.0), Dir::Up));
        assert!(!accepts(&a, &r(380.0, 250.0, 40.0, 80.0), Dir::Up));
        assert!(accepts(&a, &r(380.0, 248.0, 40.0, 80.0), Dir::Up));
        // Down mirrors Up
        assert!(accepts(&a, &r(100.0, 300.0, 40.0, 10.0), Dir::Down));
        assert!(!accepts(&a, &r(100.0, 298.0, 40.0, 10.0), Dir::Down));
        assert!(accepts(&a, &r(380.0, 270.0, 40.0, 10.0), Dir::Down));
        assert!(!accepts(&a, &r(380.0, 268.0, 40.0, 10.0), Dir::Down));
        // Left, above: centre left of a's left edge and right edge not past a's centre
        for (w, last) in [(10.0, 344.0), (20.0, 338.0), (40.0, 328.0), (80.0, 308.0), (160.0, 240.0), (300.0, 100.0)] {
            assert!(accepts(&a, &r(last, 150.0, w, 20.0), Dir::Left), "{w}");
            assert!(!accepts(&a, &r(last + 2.0, 150.0, w, 20.0), Dir::Left), "{w}");
        }
        // ... below, the centre on the edge counts
        assert!(accepts(&a, &r(330.0, 400.0, 40.0, 20.0), Dir::Left));
        assert!(!accepts(&a, &r(332.0, 400.0, 40.0, 20.0), Dir::Left));
        // Left, holding the centre line: right edge not past a's right edge, or left edge past a's left edge
        assert!(accepts(&a, &r(440.0, 290.0, 10.0, 20.0), Dir::Left));
        assert!(!accepts(&a, &r(442.0, 290.0, 10.0, 20.0), Dir::Left));
        assert!(accepts(&a, &r(348.0, 290.0, 160.0, 20.0), Dir::Left));
        assert!(!accepts(&a, &r(350.0, 290.0, 160.0, 20.0), Dir::Left));
        // Left, overlapping without the centre line: right edge not past a's centre
        assert!(accepts(&a, &r(360.0, 250.0, 40.0, 30.0), Dir::Left));
        assert!(!accepts(&a, &r(362.0, 250.0, 40.0, 30.0), Dir::Left));
        // Right mirrors Left
        assert!(accepts(&a, &r(432.0, 150.0, 40.0, 20.0), Dir::Right));
        assert!(!accepts(&a, &r(430.0, 150.0, 40.0, 20.0), Dir::Right));
        assert!(accepts(&a, &r(350.0, 290.0, 10.0, 20.0), Dir::Right));
        assert!(!accepts(&a, &r(348.0, 290.0, 10.0, 20.0), Dir::Right));
    }

    /// Pairwise: a candidate in the band beats any outside it however near; within the band the nearer facing
    /// edge wins, then the one nearer the centre line, then the left one
    #[test]
    fn the_band_wins_then_the_nearest_edge() {
        let a = r(350.0, 270.0, 100.0, 60.0);
        let ids = |list: &[Rect]| list.iter().enumerate().map(|(k, rect)| (id(k as u32 + 1), *rect)).collect::<Vec<_>>();
        let pick = |list: &[Rect]| choose(&a, Dir::Up, &ids(list)).map(|found| found.index);
        // the band's candidate far up vs a nearer one just outside the band (right edge on a's left edge)
        assert_eq!(pick(&[r(390.0, 40.0, 20.0, 20.0), r(330.0, 240.0, 20.0, 20.0)]), Some(1));
        assert_eq!(pick(&[r(390.0, 40.0, 20.0, 20.0), r(331.0, 240.0, 20.0, 20.0)]), Some(2));
        // a tall one whose bottom is 2 px nearer beats a small one at the centre
        assert_eq!(pick(&[r(390.0, 190.0, 20.0, 20.0), r(420.0, 112.0, 20.0, 100.0)]), Some(2));
        assert_eq!(pick(&[r(390.0, 190.0, 20.0, 20.0), r(420.0, 110.0, 20.0, 100.0)]), Some(1));
        // equal gaps below a wide button: nearer the centre line, then the left one
        let wide = r(300.0, 100.0, 200.0, 40.0);
        let down = |list: &[Rect]| choose(&wide, Dir::Down, &ids(list)).map(|found| found.index);
        assert_eq!(down(&[r(440.0, 200.0, 60.0, 40.0), r(300.0, 200.0, 60.0, 40.0)]), Some(2));
        assert_eq!(down(&[r(300.0, 200.0, 60.0, 40.0), r(380.0, 200.0, 60.0, 40.0)]), Some(2));
    }

    /// The client's picks over 300 random scenes (data/studio/navigation-random-scenes.json; "?" marks a press two
    /// captures disagreed on). The misses are choices between diagonal candidates the model ranks the other way:
    /// pinned so a change to the model shows what it gains or loses.
    #[test]
    fn random_scenes_follow_the_client() {
        const CODES: &str = "123456789abcdefghijklmnopqrstuvwxyz";
        let data: serde_json::Value = serde_json::from_str(include_str!("../../../../data/studio/navigation-random-scenes.json")).expect("dataset");
        let (mut matched, mut total, mut misses) = (0, 0, Vec::new());
        for line in data["scenes"].as_array().expect("scenes") {
            let line = line.as_str().expect("line");
            let (head, results) = line.split_once(" | ").expect("results");
            let (scene, rects) = head.split_once(' ').expect("rects");
            let candidates: Vec<(InstanceId, Rect)> = rects
                .split(';')
                .enumerate()
                .map(|(k, rect)| {
                    let v: Vec<f64> = rect.split(',').map(|n| n.parse().expect("number")).collect();
                    (id(k as u32), r(v[0], v[1], v[2], v[3]))
                })
                .collect();
            for cell in results.split(' ') {
                let (from, picks) = cell.split_once(':').expect("cell");
                let from = CODES.find(from).expect("button");
                for (dir, pick) in [Dir::Up, Dir::Down, Dir::Left, Dir::Right].into_iter().zip(picks.chars()) {
                    if pick == '?' {
                        continue;
                    }
                    let want = if pick == '.' { None } else { Some(CODES.find(pick).expect("pick") as u32) };
                    let others: Vec<(InstanceId, Rect)> = candidates.iter().filter(|(c, _)| c.index != from as u32).copied().collect();
                    let got = choose(&candidates[from].1, dir, &others).map(|found| found.index);
                    total += 1;
                    if got == want {
                        matched += 1;
                    } else {
                        misses.push(format!("{scene} from {} {dir:?}: client {want:?} model {got:?}", from + 1));
                    }
                }
            }
        }
        assert!(total > 7000, "{total}");
        assert_eq!((matched, total), (MATCHED, TOTAL), "misses:\n{}", misses.join("\n"));
    }

    /// Outside the band: the 45-degree cone beats the rest (maps against a fixed rival), lateral offset first
    #[test]
    fn the_cone_ranks_by_lateral_offset() {
        let a = r(350.0, 270.0, 100.0, 60.0);
        let rival = (id(1), r(240.0, 190.0, 20.0, 20.0)); // centre (-150, -100): out of the cone
        let x = |cx: f64, cy: f64| (id(2), r(cx - 10.0, cy - 10.0, 20.0, 20.0));
        let pick = |c: (InstanceId, Rect)| choose(&a, Dir::Up, &[rival, c]).map(|found| found.index);
        assert_eq!(pick(x(160.0, 60.0)), Some(2)); // (-240, -240) on the cone's edge
        assert_eq!(pick(x(140.0, 60.0)), Some(1));
        let rival = (id(1), r(290.0, 90.0, 20.0, 20.0)); // (-100, -200), in the cone
        let pick = |c: (InstanceId, Rect)| choose(&a, Dir::Up, &[rival, c]).map(|found| found.index);
        assert_eq!(pick(x(320.0, 15.0)), Some(2)); // 80 across, farther up
        assert_eq!(pick(x(320.0, 210.0)), Some(2)); // 80 across, nearer
        assert_eq!(pick(x(300.0, 15.0)), Some(1));
    }
}
