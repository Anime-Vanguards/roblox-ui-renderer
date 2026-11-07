//! One layout pass: the port of runtime/layout/init.luau's sizing (ComputeSize, AutomaticSize and
//! its measure passes, constraints), placement (PlaceChild / Locate, float32 emulation), layouts
//! (UIListLayout with flex, UIGridLayout, UIPageLayout, UITableLayout), ScrollingFrames, Store with
//! its incremental rules, roots and scrolling. Function names and comments follow the Luau; every
//! expression keeps the Luau's evaluation order and float32 steps so results are bit-identical.

use std::rc::Rc;

use rustc_hash::FxHashMap;

use super::{
    Carry, CarryEntry, Layout, Memo, MemoEntry, SizeArgs, Sized, Sticky, StickyKey,
    ids::{E, Ids},
    record::{Cells, Rect, Record, ScrollBars, TextInfo, Transform, absolute_position, compose, rotation_about},
};
use crate::engine::{
    ClassId, Hooks, InstanceId, K, NameId, Reflection, State,
    cache_counts::{self, Cache},
    instance as core,
    text::{
        SizeConstraint, TextEngine, TextObject, Truncate,
        num::{clamp, f32r, lmax, lmin},
    },
    value::Value,
};

const INF: f64 = f64::INFINITY;
const MEMO_LIMIT: usize = 8;
const SOURCE_SANS: &[u8] = b"rbxasset://fonts/families/SourceSansPro.json";

/// An offset scaled by UIScale where the engine stores the result in a UDim (int32 offset): the
/// float32 product truncated towards zero.
fn scaled_offset(offset: f64, scale: f64) -> f64 {
    let product = f32r(offset * f32r(scale));
    if product >= 0.0 { product.floor() } else { product.ceil() }
}

/// Measured (layout-probe-autoavail): the space an object may grow into along an axis blends the
/// space on both sides of its position by its AnchorPoint: (1 - a) x (available - p) + a x p.
fn growth_space(available: f64, near: f64, anchor: f64) -> f64 {
    let a = clamp(anchor, 0.0, 1.0);
    if a >= 1.0 {
        return near;
    }
    if available == INF {
        return INF;
    }
    if a <= 0.0 {
        return available - near;
    }
    (1.0 - a) * (available - near) + a * near
}

/// A record's content size in unscaled local units (what its children size and position against).
fn local_content(record: &Record) -> (f64, f64) {
    let scale = record.scale;
    let width = record.local_sizing_w.or(record.sizing_w.map(|v| v / scale)).or(record.local_w).unwrap_or(record.content.w / scale);
    let height = record.local_sizing_h.or(record.sizing_h.map(|v| v / scale)).or(record.local_h).unwrap_or(record.content.h / scale);
    (width, height)
}

/// What Locate reads from the parent record.
#[derive(Clone, Copy)]
struct ParentPlace {
    transform: Option<Transform>,
    rotation: f64,
    visible: bool,
}

impl ParentPlace {
    fn of(record: &Record) -> ParentPlace {
        ParentPlace { transform: record.transform, rotation: record.rotation, visible: record.visible }
    }
}

/// Positions a record inside its parent from its placement terms, its content origin and its
/// rotation transforms (Locate). Measured (layout-probe-f32): the engine works in float32 and stores
/// the centre; AbsolutePosition reads back through it.
fn locate(record: &mut Record, origin_x: f64, origin_y: f64, parent: ParentPlace, visible: bool) {
    let place = record.place.expect("placed record");
    let half_w = f32r(record.w * 0.5);
    let half_h = f32r(record.h * 0.5);
    let x = f32r(f32r(f32r(f32r(origin_x + place[0]) - place[1]) + half_w) - half_w);
    let y = f32r(f32r(f32r(f32r(origin_y + place[2]) - place[3]) + half_h) - half_h);
    record.x = x;
    record.y = y;
    record.content.x = f32r(x + place[4]);
    record.content.y = f32r(y + place[5]);
    let own_rotation = record.own_rotation.unwrap_or(0.0);
    let outer = parent.transform;
    let mut inner = outer;
    if own_rotation != 0.0 {
        let rotation = rotation_about(own_rotation, x + record.w / 2.0, y + record.h / 2.0);
        inner = Some(match outer {
            Some(outer) => compose(&outer, &rotation),
            None => rotation,
        });
    }
    record.outer = outer;
    record.transform = inner;
    record.rotation = parent.rotation + own_rotation;
    record.visible = parent.visible && visible;
}

/// Positions a ScrollingFrame's window and canvas from its record position and canvas terms;
/// returns the canvas content origin its children are placed from (LocateCanvas).
fn locate_canvas(record: &mut Record) -> (f64, f64) {
    let place = record.canvas_place.expect("canvas place");
    let (x, y) = (record.x, record.y);
    let window = record.window.get_or_insert_with(Rect::default);
    window.x = x + place[0];
    window.y = y;
    let canvas = record.canvas.get_or_insert_with(Rect::default);
    canvas.x = x + place[1] - place[2];
    canvas.y = y - place[3];
    (canvas.x + place[4], canvas.y + place[5])
}

/// Distributes `free` over `sizes`: growth by ratio, shrinkage weighted by ratio x size with items
/// frozen at zero (measured: CSS-like flex-shrink).
fn distribute(sizes: &mut [f64], grow: &[f64], shrink: &[f64], free: f64) {
    if free > 0.0 {
        let mut total = 0.0;
        for value in grow {
            total += value;
        }
        if total <= 0.0 {
            return;
        }
        for index in 0..sizes.len() {
            sizes[index] += free * grow[index] / total;
        }
        return;
    }
    if free >= 0.0 {
        return;
    }
    let base: Vec<f64> = sizes.to_vec();
    let mut frozen = vec![false; sizes.len()];
    loop {
        let mut weight = 0.0;
        let mut remaining = free;
        for index in 0..sizes.len() {
            if frozen[index] {
                remaining += base[index] - sizes[index];
            } else if shrink[index] > 0.0 {
                weight += shrink[index] * base[index];
            }
        }
        if weight <= 0.0 {
            return;
        }
        let mut violated = false;
        for index in 0..sizes.len() {
            if !frozen[index] && shrink[index] > 0.0 {
                let mut target = base[index] + remaining * shrink[index] * base[index] / weight;
                if target < 0.0 {
                    target = 0.0;
                    frozen[index] = true;
                    violated = true;
                }
                sizes[index] = target;
            }
        }
        if !violated {
            return;
        }
        for index in 0..sizes.len() {
            if !frozen[index] {
                sizes[index] = base[index];
            }
        }
    }
}

/// Start offset and gap for Space* distributions (None: not a Space* mode).
fn space_distribution(mode: u8, available: f64, sum: f64, count: f64, gap: f64) -> (Option<f64>, f64) {
    let free = available - sum;
    if mode == E::SPACE_BETWEEN {
        if count > 1.0 {
            return (Some(0.0), lmax(gap, free / (count - 1.0)));
        }
        return (Some(0.0), gap);
    } else if mode == E::SPACE_AROUND || mode == E::SPACE_EVENLY {
        let distributed = if mode == E::SPACE_AROUND { free / lmax(count, 1.0) } else { free / (count + 1.0) };
        let between = lmax(gap, distributed);
        return (Some((available - sum - (count - 1.0) * between) / 2.0), between);
    }
    (None, gap)
}

fn aligned_start(alignment: u8, available: f64, used: f64) -> f64 {
    if alignment == E::CENTER {
        (available - used) / 2.0
    } else if alignment == E::BOTTOM || alignment == E::RIGHT {
        available - used
    } else {
        0.0
    }
}

#[inline]
fn number_key(value: f64) -> u64 {
    if value.is_nan() { f64::NAN.to_bits() } else { value.to_bits() }
}

/// DisplayText: Text, or PlaceholderText for an empty TextBox.
fn display_text<'s>(state: &'s State, ids: &Ids, reflection: &Reflection, id: InstanceId) -> &'s [u8] {
    let text = match state.dom.raw_get(id, ids.n.Text) {
        Some(Value::String(s)) => s.as_bytes(),
        _ => b"",
    };
    if text.is_empty() && reflection.class(state.dom.at(id).class).is_a(ids.c.TextBox) {
        return match state.dom.raw_get(id, ids.n.PlaceholderText) {
            Some(Value::String(s)) => s.as_bytes(),
            _ => b"",
        };
    }
    text
}

/// FontOf (FontInternal.Parts): family, weight value, italic.
fn font_of<'s>(state: &'s State, ids: &Ids, reflection: &Reflection, id: InstanceId) -> (&'s [u8], f64, bool) {
    match state.dom.raw_get(id, ids.n.FontFace) {
        Some(Value::Font(font)) => {
            let weight = reflection.enum_item_info(font.weight).value;
            let italic = ids.code(Some(&Value::EnumItem(font.style))) == E::ITALIC;
            (font.family.as_bytes(), weight, italic)
        }
        _ => (SOURCE_SANS, 400.0, false),
    }
}

/// The text properties LayoutText reads (the first UITextSizeConstraint child included).
fn text_object<'s>(state: &'s State, ids: &Ids, reflection: &Reflection, id: InstanceId) -> TextObject<'s> {
    let n = &ids.n;
    let get = |name: NameId| state.dom.raw_get(id, name);
    let number = |target: InstanceId, name: NameId| state.dom.raw_get(target, name).and_then(Value::as_number);
    let is_true = |name: NameId| matches!(get(name), Some(Value::Bool(true)));
    let (family, weight, italic) = font_of(state, ids, reflection, id);
    let truncate = match ids.code(get(n.TextTruncate)) {
        E::AT_END => Truncate::AtEnd,
        E::SPLIT_WORD => Truncate::SplitWord,
        _ => Truncate::None,
    };
    let constraint = state
        .dom
        .at(id)
        .children
        .iter()
        .copied()
        .find(|&child| reflection.class(state.dom.at(child).class).is_a(ids.c.UITextSizeConstraint))
        .map(|constraint| SizeConstraint {
            min_text_size: number(constraint, n.MinTextSize).unwrap_or(1.0),
            max_text_size: number(constraint, n.MaxTextSize).unwrap_or(100.0),
        });
    TextObject {
        text: display_text(state, ids, reflection, id),
        rich: is_true(n.RichText),
        family: std::str::from_utf8(family).unwrap_or(""),
        weight,
        style: if italic { "italic" } else { "normal" },
        wrapped: is_true(n.TextWrapped),
        scaled: is_true(n.TextScaled),
        text_size: number(id, n.TextSize).unwrap_or(14.0),
        line_height: number(id, n.LineHeight).unwrap_or(1.0),
        truncate,
        constraint,
    }
}

/// A UIListLayout line: its items `start..end` (positions in the flow), sizes along both axes.
pub(crate) struct Line {
    start: usize,
    end: usize,
    main: f64,
    cross: f64,
    sum: f64,
    used: f64,
}

/// ListLayout's reused vectors.
#[derive(Default)]
pub(crate) struct ListScratch {
    lines: Vec<Line>,
    mains: Vec<f64>,
    grow: Vec<f64>,
    shrink: Vec<f64>,
    crosses: Vec<f64>,
}

/// Reused vectors of the list branch of LayoutChildren (taken while in use: lists do not nest
/// within one LayoutChildren call's list step).
#[derive(Default)]
pub(crate) struct ListVectors {
    pub list: ListScratch,
    pub sizes: Vec<[f64; 2]>,
    pub offsets: Vec<(f64, f64)>,
}

/// A child's resolved size in its parent's LayoutChildren.
#[derive(Clone, Copy)]
pub(crate) struct Resolved {
    w: f64,
    h: f64,
    base_w: f64,
    base_h: f64,
    measure: Option<(f64, f64)>,
}

type Collect<'c> = &'c mut dyn FnMut(&mut Pass, InstanceId, &Record);

pub(crate) struct Pass<'a> {
    pub l: &'a mut Layout,
    pub state: &'a mut State,
    pub hooks: &'a Hooks,
    pub reflection: &'a Reflection,
    pub text: &'a mut TextEngine,
}

impl<'a> Pass<'a> {
    // Property access

    #[inline]
    fn get(&self, id: InstanceId, name: NameId) -> Option<&Value> {
        self.state.dom.raw_get(id, name)
    }

    #[inline]
    fn num(&self, id: InstanceId, name: NameId) -> Option<f64> {
        self.get(id, name).and_then(Value::as_number)
    }

    /// `RawGet(...) == true`
    #[inline]
    fn is_true(&self, id: InstanceId, name: NameId) -> bool {
        matches!(self.get(id, name), Some(Value::Bool(true)))
    }

    /// `RawGet(...) ~= false`
    #[inline]
    fn not_false(&self, id: InstanceId, name: NameId) -> bool {
        !matches!(self.get(id, name), Some(Value::Bool(false)))
    }

    #[inline]
    fn visible(&self, id: InstanceId) -> bool {
        self.not_false(id, self.l.ids.n.Visible)
    }

    #[inline]
    fn code(&self, id: InstanceId, name: NameId) -> u8 {
        self.l.ids.code(self.get(id, name))
    }

    /// UDim2Terms: { X.Scale, X.Offset, Y.Scale, Y.Offset }
    #[inline]
    fn udim2(&self, id: InstanceId, name: NameId) -> [f64; 4] {
        match self.get(id, name) {
            Some(Value::UDim2(u)) => [u.x.scale as f64, u.x.offset as f64, u.y.scale as f64, u.y.offset as f64],
            _ => [0.0; 4],
        }
    }

    #[inline]
    fn udim(&self, id: InstanceId, name: NameId) -> (f64, f64) {
        match self.get(id, name) {
            Some(Value::UDim(u)) => (u.scale as f64, u.offset as f64),
            _ => (0.0, 0.0),
        }
    }

    #[inline]
    fn vec2(&self, id: InstanceId, name: NameId) -> Option<(f64, f64)> {
        match self.get(id, name) {
            Some(Value::Vector2(v)) => Some((v.x as f64, v.y as f64)),
            _ => None,
        }
    }

    #[inline]
    fn class(&self, id: InstanceId) -> ClassId {
        self.state.dom.at(id).class
    }

    #[inline]
    fn is_a(&self, id: InstanceId, class: ClassId) -> bool {
        self.reflection.class(self.class(id)).is_a(class)
    }

    #[inline]
    fn parent(&self, id: InstanceId) -> Option<InstanceId> {
        self.state.dom.at(id).parent
    }

    fn is_text_object(&self, id: InstanceId) -> bool {
        let class = self.reflection.class(self.class(id));
        let c = self.l.ids.c;
        class.is_a(c.TextLabel) || class.is_a(c.TextButton) || class.is_a(c.TextBox)
    }

    fn first_child_of(&self, id: InstanceId, class: ClassId) -> Option<InstanceId> {
        self.state.dom.at(id).children.iter().copied().find(|&child| self.is_a(child, class))
    }

    /// LayoutObject: the first UIGridStyleLayout child.
    fn layout_object(&self, id: InstanceId) -> Option<InstanceId> {
        self.first_child_of(id, self.l.ids.c.UIGridStyleLayout)
    }

    /// OwnScale: float32 product of the UIScale children.
    fn own_scale(&self, id: InstanceId) -> f64 {
        let mut factor = 1.0;
        let ui_scale = self.l.ids.c.UIScale;
        for &child in &self.state.dom.at(id).children {
            if self.class(child) == ui_scale {
                factor = f32r(factor * self.num(child, self.l.ids.n.Scale).unwrap_or(1.0));
            }
        }
        factor
    }

    /// GuiChildren: GuiObject children, looking through Folders.
    fn gui_children_into(&self, id: InstanceId, out: &mut Vec<InstanceId>) {
        let gui_object = self.l.ids.c.GuiObject;
        let folder = self.l.ids.c.Folder;
        for &child in &self.state.dom.at(id).children {
            let class = self.class(child);
            if self.reflection.class(class).is_a(gui_object) {
                out.push(child);
            } else if class == folder {
                self.gui_children_into(child, out);
            }
        }
    }

    fn gui_children(&mut self, id: InstanceId) -> Vec<InstanceId> {
        let mut list = self.l.take_vec();
        self.gui_children_into(id, &mut list);
        list
    }

    /// `#GuiChildren(Proxy) > 0`
    fn has_gui_children(&self, id: InstanceId) -> bool {
        let gui_object = self.l.ids.c.GuiObject;
        let folder = self.l.ids.c.Folder;
        self.state.dom.at(id).children.iter().any(|&child| {
            let class = self.class(child);
            self.reflection.class(class).is_a(gui_object) || (class == folder && self.has_gui_children(child))
        })
    }

    // Computed property writes: only when the value differs, and through the change notification
    // only when something listens (the instance's Changed or the property's signal exists).

    fn write(&mut self, id: InstanceId, name: NameId, value: Value) {
        let listened = self
            .state
            .dom
            .at(id)
            .signals
            .as_deref()
            .is_some_and(|signals| signals.event(K::Changed as NameId).is_some() || signals.property(name).is_some());
        if listened {
            core::raw_set(self.state, self.reflection, self.hooks, id, name, value);
        } else {
            core::raw_set_silent(self.state, self.reflection, id, name, value);
        }
    }

    /// AbsolutePosition as scripts read it: Roblox measures it from below the top-bar inset, so a ScreenGui's content
    /// reads `inset` higher than where it is drawn (an IgnoreGuiInset ScreenGui at (0, -inset), one below the inset at
    /// (0, 0); measured in a playing client, Studio 0.744, with a 58 px inset: ree tools/parity/probes/ui_signals.luau).
    /// Layout records, drawing and hit tests stay in screen pixels; other layers' content is in its own canvas.
    fn set_absolute_position(&mut self, id: InstanceId, x: f64, y: f64) {
        let mut y = y;
        if self.l.top_inset != 0.0 {
            let (layer, screen) = (self.l.ids.c.LayerCollector, self.l.ids.c.ScreenGui);
            let mut current = Some(id);
            while let Some(node) = current {
                let class = self.class(node);
                if self.reflection.class(class).is_a(layer) {
                    if class == screen {
                        y -= self.l.top_inset;
                    }
                    break;
                }
                current = self.parent(node);
            }
        }
        let n_position = self.l.ids.n.AbsolutePosition;
        self.set_vector(id, n_position, x, y);
    }

    fn set_vector(&mut self, id: InstanceId, name: NameId, x: f64, y: f64) {
        if let Some(Value::Vector2(old)) = self.get(id, name) {
            if old.x as f64 == x && old.y as f64 == y {
                return;
            }
        }
        self.write(id, name, Value::Vector2(rbx_dom_weak::types::Vector2::new(x as f32, y as f32)));
    }

    fn set_number(&mut self, id: InstanceId, name: NameId, value: f64) {
        if let Some(Value::Number(old)) = self.get(id, name) {
            if *old == value {
                return;
            }
        }
        self.write(id, name, Value::Number(value));
    }

    fn set_bool(&mut self, id: InstanceId, name: NameId, value: bool) {
        if let Some(Value::Bool(old)) = self.get(id, name) {
            if *old == value {
                return;
            }
        }
        self.write(id, name, Value::Bool(value));
    }

    // Text

    /// LayoutText: lays out a text object's text for a content box of (width, height) local px.
    fn layout_text(&mut self, id: InstanceId, width: f64, height: f64, scale: f64, auto_width: bool) -> TextInfo {
        let dpi = self.l.pixel_scale;
        let object = text_object(self.state, &self.l.ids, self.reflection, id);
        Rc::new(self.text.layout_text(&object, width, height, scale, dpi, auto_width))
    }

    /// TextMeasureKey
    fn text_measure_key(&self, id: InstanceId, scale: f64) -> StickyKey {
        let n = self.l.ids.n;
        let (family, weight, italic) = font_of(self.state, &self.l.ids, self.reflection, id);
        let constraint = self.first_child_of(id, self.l.ids.c.UITextSizeConstraint).map(|constraint| {
            (self.num(constraint, n.MinTextSize).map(number_key), self.num(constraint, n.MaxTextSize).map(number_key))
        });
        StickyKey {
            text: display_text(self.state, &self.l.ids, self.reflection, id).into(),
            family: family.into(),
            weight: number_key(weight),
            italic,
            text_size: self.num(id, n.TextSize).map(number_key),
            rich: self.is_true(id, n.RichText),
            line_height: self.num(id, n.LineHeight).map(number_key),
            x_alignment: match self.get(id, n.TextXAlignment) {
                Some(Value::EnumItem(item)) => Some(*item),
                _ => None,
            },
            scale: number_key(scale),
            pixel_scale: number_key(self.l.pixel_scale),
            constraint,
        }
    }

    // Sizing

    /// Measured: BorderMode Inset insets the content by BorderSizePixel, Middle by half of it.
    fn border_inset(&self, id: InstanceId) -> f64 {
        if !self.is_a(id, self.l.ids.c.GuiObject) {
            return 0.0;
        }
        let n = self.l.ids.n;
        let mode = self.code(id, n.BorderMode);
        if mode == E::EMPTY || mode == E::OUTLINE {
            return 0.0;
        }
        let border = self.num(id, n.BorderSizePixel).unwrap_or(0.0);
        if border <= 0.0 {
            return 0.0;
        }
        if mode == E::INSET { border } else { border / 2.0 }
    }

    fn padding(&self, id: InstanceId, width: f64, height: f64) -> (f64, f64, f64, f64) {
        let inset = self.border_inset(id);
        let Some(pad) = self.first_child_of(id, self.l.ids.c.UIPadding) else {
            return (inset, inset, inset, inset);
        };
        let n = self.l.ids.n;
        let side = |name: NameId, basis: f64| {
            let (scale, offset) = self.udim(pad, name);
            f32r(f32r(f32r(scale * basis) + offset) + inset)
        };
        (side(n.PaddingLeft, width), side(n.PaddingRight, width), side(n.PaddingTop, height), side(n.PaddingBottom, height))
    }

    fn padding_offsets(&self, id: InstanceId) -> (f64, f64, f64, f64) {
        let inset = self.border_inset(id);
        let Some(pad) = self.first_child_of(id, self.l.ids.c.UIPadding) else {
            return (inset, inset, inset, inset);
        };
        let n = self.l.ids.n;
        (
            self.udim(pad, n.PaddingLeft).1 + inset,
            self.udim(pad, n.PaddingRight).1 + inset,
            self.udim(pad, n.PaddingTop).1 + inset,
            self.udim(pad, n.PaddingBottom).1 + inset,
        )
    }

    /// The UISizeConstraint children's (min, max) vectors.
    fn size_constraints(&self, id: InstanceId, mut visit: impl FnMut((f64, f64), (f64, f64))) {
        let class = self.l.ids.c.UISizeConstraint;
        let n = self.l.ids.n;
        for &child in &self.state.dom.at(id).children {
            if self.is_a(child, class) {
                let min = self.vec2(child, n.MinSize).unwrap_or((0.0, 0.0));
                let max = self.vec2(child, n.MaxSize).unwrap_or((INF, INF));
                visit(min, max);
            }
        }
    }

    /// The tightest UISizeConstraint bounds on the width (min, max).
    fn size_limits(&self, id: InstanceId) -> (f64, f64) {
        let (mut low, mut high) = (0.0, INF);
        self.size_constraints(id, |min, max| {
            low = lmax(low, min.0);
            high = lmin(high, lmax(min.0, max.0));
        });
        (low, high)
    }

    fn apply_size_constraint(&self, id: InstanceId, mut width: f64, mut height: f64) -> (f64, f64) {
        self.size_constraints(id, |min, max| {
            width = clamp(width, min.0, lmax(min.0, max.0));
            height = clamp(height, min.1, lmax(min.1, max.1));
        });
        (width, height)
    }

    fn apply_aspect_ratio(&self, id: InstanceId, mut width: f64, mut height: f64, parent_w: f64, parent_h: f64) -> (f64, f64) {
        let Some(constraint) = self.first_child_of(id, self.l.ids.c.UIAspectRatioConstraint) else {
            return (width, height);
        };
        let n = self.l.ids.n;
        let ratio = match self.num(constraint, n.AspectRatio) {
            Some(ratio) if ratio > 0.0 => ratio,
            _ => return (width, height),
        };
        let aspect_type = self.code(constraint, n.AspectType);
        let dominant = self.code(constraint, n.DominantAxis);
        if aspect_type == E::SCALE_WITH_PARENT_SIZE {
            // Measured: the dominant axis keeps the element's own size and the other follows the
            // ratio; when that exceeds the parent it becomes the largest box of the ratio inside it.
            if dominant == E::HEIGHT {
                width = height * ratio;
            } else {
                height = width / ratio;
            }
            if width > parent_w + 1e-9 || height > parent_h + 1e-9 {
                width = lmin(parent_w, parent_h * ratio);
                height = width / ratio;
            }
            return (width, height);
        }
        if dominant == E::HEIGHT {
            let from_height = height * ratio;
            if from_height <= width {
                return (from_height, height);
            }
            return (width, width / ratio);
        }
        let from_width = width / ratio;
        if from_width <= height {
            return (width, from_width);
        }
        (height * ratio, height)
    }

    /// Base size from Size / SizeConstraint against the parent's content size (all unscaled).
    fn base_size(&self, id: InstanceId, parent_w: f64, parent_h: f64) -> (f64, f64) {
        let n = self.l.ids.n;
        let size = self.udim2(id, n.Size);
        let constraint = self.code(id, n.SizeConstraint);
        let (mut relative_w, mut relative_h) = (parent_w, parent_h);
        if constraint == E::RELATIVE_XX {
            relative_h = parent_w;
        } else if constraint == E::RELATIVE_YY {
            relative_w = parent_h;
        }
        (f32r(f32r(size[0] * relative_w) + size[1]), f32r(f32r(size[2] * relative_h) + size[3]))
    }

    #[inline]
    fn automatic_axes(&self, id: InstanceId) -> (u8, bool, bool) {
        let automatic = self.code(id, self.l.ids.n.AutomaticSize);
        (automatic, automatic == E::X || automatic == E::XY, automatic == E::Y || automatic == E::XY)
    }

    // AutomaticSize measures

    /// Descendants count (DescendantExtent): returns the far edges in local px from `record`'s
    /// content origin of what `id` holds (see the Luau for the measured rules).
    fn descendant_extent(&mut self, id: InstanceId, record: &Record, zero_x: bool, zero_y: bool) -> (f64, f64) {
        let origin = (record.content.x, record.content.y, record.scale);
        let (max_x, max_y) = self.extent_visit(id, id, record, zero_x, zero_y, origin);
        (if zero_x { max_x } else { 0.0 }, if zero_y { max_y } else { 0.0 })
    }

    /// DescendantExtent's Visit: the far edges of what `parent` holds (its own far padding included
    /// below the top).
    fn extent_visit(&mut self, top_id: InstanceId, parent: InstanceId, parent_record: &Record, axis_x: bool, axis_y: bool, origin: (f64, f64, f64)) -> (f64, f64) {
        let (origin_x, origin_y, scale) = origin;
        let top = parent == top_id;
        let mut edge_x = if top { 0.0 } else { (parent_record.content.x - origin_x) / scale };
        let mut edge_y = if top { 0.0 } else { (parent_record.content.y - origin_y) / scale };
        {
            let mut collect = |pass: &mut Pass, child: InstanceId, child_record: &Record| {
                if !pass.visible(child) {
                    return;
                }
                let scrolling = pass.class(child) == pass.l.ids.c.ScrollingFrame;
                let (_, auto_x, auto_y) = pass.automatic_axes(child);
                let child_x = axis_x && !scrolling && !auto_x;
                let child_y = axis_y && !scrolling && !auto_y;
                let nested = pass.has_gui_children(child);
                let child_size = pass.udim2(child, pass.l.ids.n.Size);
                if !top {
                    // A scale-sized container counts by what it holds (visited below), not by its box
                    if !(nested && child_x && child_size[0] != 0.0) {
                        edge_x = lmax(edge_x, (child_record.x + child_record.w - origin_x) / scale);
                    }
                    if !(nested && child_y && child_size[2] != 0.0) {
                        edge_y = lmax(edge_y, (child_record.y + child_record.h - origin_y) / scale);
                    }
                }
                if !child_x && !child_y {
                    return;
                }
                if !nested && pass.first_child_of(child, pass.l.ids.c.UIPadding).is_none() {
                    return;
                }
                let mut inner = child_record.clone();
                if child_x {
                    inner.size_w = Some(inner.content.w);
                    inner.item_w = Some(inner.content.w);
                    inner.sizing_w = Some(0.0);
                    inner.local_sizing_w = Some(0.0);
                }
                if child_y {
                    inner.size_h = Some(inner.content.h);
                    inner.item_h = Some(inner.content.h);
                    inner.sizing_h = Some(0.0);
                    inner.local_sizing_h = Some(0.0);
                }
                // Measured (a slider prompt's amount buttons): a list in a container of offset size keeps its
                // alignment; only scale-sized containers measure their list from the padding.
                inner.align_start = Some((child_x && child_size[0] != 0.0, child_y && child_size[2] != 0.0));
                let (held_x, held_y) = pass.extent_visit(top_id, child, &inner, child_x, child_y, origin);
                if child_x {
                    edge_x = lmax(edge_x, held_x);
                }
                if child_y {
                    edge_y = lmax(edge_y, held_y);
                }
            };
            self.layout_children(parent, parent_record, true, Some(&mut collect));
        }
        if top {
            return (edge_x, edge_y);
        }
        let (_, pad_right, _, pad_bottom) = self.padding_offsets(parent);
        let ratio = parent_record.scale / scale;
        (edge_x + pad_right * ratio, edge_y + pad_bottom * ratio)
    }

    /// HeldWidth: the width of what `item` holds in its own local units (its children and their
    /// descendants laid out with the item 0 wide), plus its padding offsets.
    fn held_width(&mut self, item: InstanceId, scale: f64, available: f64) -> f64 {
        let item_scale = f32r(scale * self.own_scale(item));
        let (pad_left, pad_right, _, _) = self.padding_offsets(item);
        let probe = Record {
            scale: item_scale,
            content: Rect { x: pad_left * item_scale, y: 0.0, w: 0.0, h: 0.0 },
            visible: true,
            available: if available < INF { Some(lmax(available - pad_left - pad_right, 0.0) * item_scale) } else { None },
            measuring: Some((true, false)),
            size_w: Some(0.0),
            item_w: Some(0.0),
            sizing_w: Some(0.0),
            ..Record::default()
        };
        let (extent_x, _) = self.layout_children(item, &probe, true, None);
        let (deep_x, _) = self.descendant_extent(item, &probe, true, false);
        lmax(extent_x, deep_x) + pad_left + pad_right
    }

    /// Resolves a GuiObject's final unscaled size (constraints + automatic size) (ComputeSize).
    fn compute_size(&mut self, id: InstanceId, args: SizeArgs) -> Sized {
        let SizeArgs { parent_w, parent_h, scale, available, measure_parent_w, measure_parent_h, available_h, full_w, full_h } = args;
        let (automatic, auto_x, auto_y) = self.automatic_axes(id);
        let is_automatic = automatic != E::EMPTY && automatic != E::NONE;
        self.l.text_carries.set(id, None);

        let (mut width, mut height) = self.base_size(id, parent_w, parent_h);
        // Measured (layout-probe-autopad, -autopad2): text objects resolve the scale Size of their
        // automatic axes against the parent's whole size, not its content box.
        if is_automatic {
            if let Some(full_width) = full_w {
                if self.is_text_object(id) {
                    let (full_width, full_height) = self.base_size(id, full_width, full_h.unwrap_or(parent_h));
                    if auto_x {
                        width = full_width;
                    }
                    if auto_y {
                        height = full_height;
                    }
                }
            }
        }
        (width, height) = self.apply_size_constraint(id, width, height);
        if !is_automatic {
            (width, height) = self.apply_aspect_ratio(id, width, height, parent_w, parent_h);
        }
        let (mut base_w, mut base_h) = (width, height);
        let (mut child_measure_w, mut child_measure_h) = (None, None);

        let mut text_box = None;
        if is_automatic {
            // Measured: when an ancestor's UIAspectRatioConstraint shrank its automatic size,
            // automatic content is still measured against the ancestor's unconstrained size.
            let (mut measure_width, mut measure_height) = (width, height);
            if let Some(measure_parent_w) = measure_parent_w {
                let (measured_w, measured_h) = self.base_size(id, measure_parent_w, measure_parent_h.unwrap_or(parent_h));
                (measure_width, measure_height) = self.apply_size_constraint(id, measured_w, measured_h);
            }

            // Measured: UIPadding scale components do not count towards the automatic size.
            let (pad_left, pad_right, pad_top, pad_bottom) = self.padding_offsets(id);
            let inner_width = lmax(measure_width - pad_left - pad_right, 0.0);
            let inner_height = lmax(measure_height - pad_top - pad_bottom, 0.0);
            let inner_available = if auto_x {
                match available {
                    Some(available) => lmax(available - pad_left - pad_right, inner_width),
                    None => INF,
                }
            } else {
                inner_width
            };
            let inner_available_h = if auto_y {
                match available_h {
                    Some(available_h) => lmax(available_h - pad_top - pad_bottom, inner_height),
                    None => INF,
                }
            } else {
                inner_height
            };
            let (mut content_width, mut content_height) = (0.0, 0.0);

            if self.is_text_object(id) {
                // Measured: text with AutomaticSize X is laid out in the width available in the
                // parent, then the object takes the text's width (see the Luau).
                let mut text_available = inner_available;
                if auto_x {
                    if let Some(available) = available {
                        text_available = lmax(available / self.own_scale(id) - pad_left - pad_right, inner_width);
                    }
                }
                let mut text_available_h = inner_available_h;
                if auto_y {
                    if let Some(available_h) = available_h {
                        text_available_h = lmax(available_h / self.own_scale(id) - pad_top - pad_bottom, inner_height);
                    }
                }
                // Measured (Studio invalidation probes): an unwrapped, untruncated, unscaled label keeps
                // the width it was measured at until one of its text properties changes.
                let n = self.l.ids.n;
                let sticky_key = if auto_x
                    && !self.is_true(id, n.TextWrapped)
                    && !self.is_true(id, n.TextScaled)
                    && self.code(id, n.TextTruncate) == E::NONE
                {
                    Some(self.text_measure_key(id, scale))
                } else {
                    None
                };
                let kept = match (&sticky_key, self.l.sticky.get(id)) {
                    (Some(key), Some(sticky)) if sticky.key == *key => Some((sticky.info.clone(), sticky.text_box)),
                    _ => None,
                };
                let text_info = match kept {
                    Some((info, kept_box)) => {
                        text_box = kept_box;
                        info
                    }
                    None => {
                        let height_basis = if auto_y { text_available_h } else { inner_height };
                        let mut info = self.layout_text(id, if auto_x { text_available } else { inner_width }, height_basis, scale, false);
                        text_box = if auto_x { Some(text_available) } else { None };
                        // Measured (layout-probe-autotrunc 36/36): an automatic width capped by a
                        // UISizeConstraint lays the text out again at the cap and takes that text's width.
                        if auto_x {
                            let (_, max_width) = self.size_limits(id);
                            let cap = max_width - pad_left - pad_right;
                            if info.bounds_x > cap + 1e-9 && cap >= 0.0 {
                                info = self.layout_text(id, cap, height_basis, scale, false);
                                text_box = Some(cap);
                            }
                        }
                        if let Some(key) = sticky_key {
                            self.l.sticky.set(id, Sticky { key, info: info.clone(), text_box });
                        }
                        info
                    }
                };
                content_width = lmax(content_width, text_info.bounds_x);
                content_height = lmax(content_height, text_info.bounds_y);
            }

            // Measured: children size and position against a stable size (see the Luau).
            let measuring = (auto_x, auto_y);
            let probe = |content_w: f64, content_h: f64, measure_w: Option<f64>, measure_h: Option<f64>| Record {
                w: width * scale,
                h: height * scale,
                scale,
                content: Rect { x: pad_left * scale, y: pad_top * scale, w: content_w * scale, h: content_h * scale },
                visible: true,
                available: if auto_x { Some(inner_available * scale) } else { None },
                available_h: if auto_y { Some(inner_available_h * scale) } else { None },
                measuring: Some(measuring),
                measure_w: measure_w.map(|v| v * scale),
                measure_h: measure_h.map(|v| v * scale),
                ..Record::default()
            };
            let flow = self.layout_object(id);
            let flow_class = flow.map(|flow| self.class(flow));
            let c_list = self.l.ids.c.UIListLayout;
            let c_grid = self.l.ids.c.UIGridLayout;
            // Measured (round 3): text children with a scale width extend an AutomaticSize X
            // container by their text rather than by their box (see the Luau).
            let mut text_extent: f64 = 0.0;
            let mut carry: Vec<(InstanceId, CarryEntry)> = Vec::new();
            if auto_x && flow.is_none() && inner_available < INF {
                let children = self.gui_children(id);
                for &child in &children {
                    if self.visible(child) && self.is_text_object(child) {
                        let n = self.l.ids.n;
                        let child_size = self.udim2(child, n.Size);
                        let (_, child_auto_x, _) = self.automatic_axes(child);
                        let child_automatic = self.code(child, n.AutomaticSize);
                        if child_size[0] != 0.0 && !child_auto_x {
                            let child_auto_y = child_automatic == E::Y;
                            let wrapped = self.is_true(child, n.TextWrapped);
                            let child_scale = self.own_scale(child);
                            let child_position = self.udim2(child, n.Position);
                            let box_w = child_size[0] * inner_available + child_size[1];
                            let base_h = child_size[2] * inner_height + child_size[3];
                            let space_h = (if auto_y { inner_available_h } else { inner_height }) - child_position[3];
                            let box_h = if child_auto_y { lmax(space_h, base_h) } else { base_h };
                            let wrap_w = if wrapped && !child_auto_y { lmax(child_size[1], 0.0) } else { lmax(box_w, 0.0) };
                            let info = self.layout_text(child, wrap_w / child_scale, box_h / child_scale, scale * child_scale, false);
                            let mut text_w = info.bounds_x * child_scale;
                            if !wrapped {
                                text_w = lmin(text_w, lmax(box_w, 0.0));
                            }
                            text_extent = lmax(text_extent, child_position[1] + text_w);
                            if child_auto_y {
                                set_carry(&mut carry, child, CarryEntry {
                                    text_box: Some(wrap_w / child_scale),
                                    height: Some(lmax(base_h, info.bounds_y * child_scale)),
                                    width: None,
                                });
                            }
                        }
                    }
                }
                self.l.give_vec(children);
            } else if auto_x && flow_class == Some(c_list) && inner_available < INF {
                // Measured (round 3, list rows): text items with a scale width keep what the measure
                // against the available width gave them.
                let children = self.gui_children(id);
                for &child in &children {
                    if self.visible(child) && self.is_text_object(child) {
                        let n = self.l.ids.n;
                        let child_size = self.udim2(child, n.Size);
                        let child_automatic = self.code(child, n.AutomaticSize);
                        if child_size[0] != 0.0 && child_automatic != E::NONE && child_automatic != E::EMPTY {
                            let child_scale = self.own_scale(child);
                            if child_automatic == E::Y {
                                if self.is_true(child, n.TextWrapped) {
                                    let box_w = lmax(child_size[0] * inner_available + child_size[1], 0.0);
                                    let base_h = child_size[2] * inner_height + child_size[3];
                                    let space_h = if auto_y { inner_available_h } else { inner_height };
                                    let info = self.layout_text(child, box_w / child_scale, lmax(space_h, base_h) / child_scale, scale * child_scale, false);
                                    set_carry(&mut carry, child, CarryEntry {
                                        text_box: Some(box_w / child_scale),
                                        height: Some(lmax(base_h, info.bounds_y * child_scale)),
                                        width: None,
                                    });
                                }
                            } else {
                                let item = self.compute_size(child, SizeArgs {
                                    parent_w: inner_available,
                                    parent_h: inner_height,
                                    scale: scale * child_scale,
                                    available: Some(inner_available),
                                    measure_parent_w: None,
                                    measure_parent_h: None,
                                    available_h: Some(if auto_y { inner_available_h } else { inner_height }),
                                    full_w: Some(inner_available),
                                    full_h: Some(inner_height),
                                });
                                set_carry(&mut carry, child, CarryEntry { text_box: None, height: None, width: Some(item.w) });
                            }
                        }
                    }
                }
                self.l.give_vec(children);
            }
            let carry: Option<Carry> = if carry.is_empty() { None } else { Some(Rc::new(carry)) };
            self.l.text_carries.set(id, carry);
            let flow_items = flow_class == Some(c_list) || flow_class == Some(c_grid);
            let mut first = probe(if auto_x { 0.0 } else { inner_width }, if auto_y { 0.0 } else { inner_height }, None, None);
            if flow_items {
                if auto_x && inner_available < INF {
                    first.item_w = Some(inner_available * scale);
                }
                if auto_y && inner_available_h < INF {
                    first.item_h = Some(inner_available_h * scale);
                }
            }
            // (scale-width items of a vertical list that hold children count by what they hold)
            if flow_items && auto_x && flow_class == Some(c_list) && self.code(flow.unwrap(), self.l.ids.n.FillDirection) != E::HORIZONTAL {
                let children = self.gui_children(id);
                let mut held: Vec<(InstanceId, f64)> = Vec::new();
                for &child in &children {
                    let (_, child_auto_x, _) = self.automatic_axes(child);
                    let holds = self.has_gui_children(child) && self.class(child) != self.l.ids.c.ScrollingFrame;
                    if self.visible(child) && self.udim2(child, self.l.ids.n.Size)[0] != 0.0 && holds && !child_auto_x {
                        let width = self.held_width(child, scale, inner_available / self.own_scale(child));
                        match held.iter_mut().find(|(other, _)| *other == child) {
                            Some(entry) => entry.1 = width,
                            None => held.push((child, width)),
                        }
                    }
                }
                self.l.give_vec(children);
                if !held.is_empty() {
                    first.held_w = Some(Rc::new(held));
                }
            }
            // Measured (layout-probe-autolistgap, -autolistgap2): the first pass resolves a scale list
            // Padding against the object's own Size taken against the space it can grow into.
            let list = flow_class == Some(c_list);
            let padded = self.first_child_of(id, self.l.ids.c.UIPadding).is_some();
            if list {
                let size = self.udim2(id, self.l.ids.n.Size);
                if auto_x {
                    if let Some(grow_w) = available.filter(|v| *v < INF) {
                        first.gap_w = Some(lmax(size[0] * grow_w + size[1], 0.0) * scale);
                    }
                }
                if auto_y {
                    if let Some(grow_h) = available_h.filter(|v| *v < INF) {
                        first.gap_h = Some(lmax(size[2] * grow_h + size[3], 0.0) * scale);
                    }
                }
            }
            // Measured (layout-probe-autoaspect3, -autoaspect4, an options prompt): with a
            // UIAspectRatioConstraint the stable size is measured from the content on the other axis.
            let aspect = flow.is_none() && self.first_child_of(id, self.l.ids.c.UIAspectRatioConstraint).is_some() && self.has_gui_children(id);
            let (aspect_x, aspect_y) = (aspect && !auto_x, aspect && !auto_y);
            let (mut natural_x, mut natural_y): (f64, f64) = (0.0, 0.0);
            if aspect_x {
                first.size_w = Some(first.content.w);
                first.sizing_w = Some(0.0);
            }
            if aspect_y {
                first.size_h = Some(first.content.h);
                first.sizing_h = Some(0.0);
            }
            let first_content = first.content;
            let (mut offset_x, mut offset_y) = if aspect {
                let mut collect = |pass: &mut Pass, child: InstanceId, child_record: &Record| {
                    if !pass.visible(child) {
                        return;
                    }
                    let child_size = pass.udim2(child, pass.l.ids.n.Size);
                    let nested = pass.has_gui_children(child);
                    if aspect_x && !(nested && child_size[0] != 0.0) {
                        natural_x = lmax(natural_x, (child_record.x + child_record.w - first_content.x) / scale);
                    }
                    if aspect_y && !(nested && child_size[2] != 0.0) {
                        natural_y = lmax(natural_y, (child_record.y + child_record.h - first_content.y) / scale);
                    }
                };
                self.layout_children(id, &first, true, Some(&mut collect))
            } else {
                self.layout_children(id, &first, true, None)
            };
            if flow.is_none() {
                // Descendants below the children (plain containers): sized against the available size.
                let deep_w = if !auto_x { inner_width } else if inner_available < INF { inner_available } else { 0.0 };
                let deep_h = if !auto_y { inner_height } else if inner_available_h < INF { inner_available_h } else { 0.0 };
                let mut deep = probe(deep_w, deep_h, None, None);
                if auto_x || aspect_x {
                    deep.size_w = Some(deep.content.w);
                    deep.item_w = Some(deep.content.w);
                    deep.sizing_w = Some(0.0);
                }
                if auto_y || aspect_y {
                    deep.size_h = Some(deep.content.h);
                    deep.item_h = Some(deep.content.h);
                    deep.sizing_h = Some(0.0);
                }
                let (deep_x, deep_y) = self.descendant_extent(id, &deep, auto_x || aspect_x, auto_y || aspect_y);
                if aspect_x {
                    natural_x = lmax(natural_x, deep_x);
                }
                if aspect_y {
                    natural_y = lmax(natural_y, deep_y);
                }
                if auto_x {
                    offset_x = lmax(offset_x, deep_x);
                }
                if auto_y {
                    offset_y = lmax(offset_y, deep_y);
                }
                if text_extent > 0.0 {
                    offset_x = lmax(offset_x, text_extent);
                }
            }
            let mut stable_w = if auto_x { lmax(width, lmax(content_width, offset_x) + pad_left + pad_right) } else { width };
            let mut stable_h = if auto_y { lmax(height, lmax(content_height, offset_y) + pad_top + pad_bottom) } else { height };
            if aspect_x {
                stable_w = lmax(content_width, natural_x) + pad_left + pad_right;
            }
            if aspect_y {
                stable_h = lmax(content_height, natural_y) + pad_top + pad_bottom;
            }
            (stable_w, stable_h) = self.apply_size_constraint(id, stable_w, stable_h);
            if !auto_x && !aspect_x {
                stable_w = width;
            }
            if !auto_y && !aspect_y {
                stable_h = height;
            }

            // Measured: an AutomaticSize object's UIAspectRatioConstraint applies to this stable size.
            let (constrained_w, constrained_h) = self.apply_aspect_ratio(id, stable_w, stable_h, parent_w, parent_h);
            if constrained_w != stable_w || constrained_h != stable_h {
                child_measure_w = Some(lmax(stable_w - pad_left - pad_right, 0.0));
                child_measure_h = Some(lmax(stable_h - pad_top - pad_bottom, 0.0));
            }
            stable_w = constrained_w;
            stable_h = constrained_h;

            let mut second = probe(lmax(stable_w - pad_left - pad_right, 0.0), lmax(stable_h - pad_top - pad_bottom, 0.0), child_measure_w, child_measure_h);
            second.w = stable_w * scale;
            second.h = stable_h * scale;
            if list && padded {
                second.gap_w = if auto_x { Some(0.0) } else { None };
                second.gap_h = if auto_y { Some(0.0) } else { None };
            }
            let (extent_x, extent_y) = self.layout_children(id, &second, true, None);
            // Measured (layout-probe-autocenter): where children end up against the stable size
            // extends the frame without its padding.
            width = if auto_x { lmax(stable_w, extent_x) } else { stable_w };
            height = if auto_y { lmax(stable_h, extent_y) } else { stable_h };
            (width, height) = self.apply_size_constraint(id, width, height);
            base_w = stable_w;
            base_h = stable_h;
        }

        Sized { w: width, h: height, text_box, base_w, base_h, measure_w: child_measure_w, measure_h: child_measure_h }
    }

    /// ComputeSize, memoized for AutomaticSize objects: a result stays valid while the object's
    /// subtree stamp and the inputs are the same.
    fn memoized_size(&mut self, id: InstanceId, args: SizeArgs) -> Sized {
        let stamp = self.l.stamps.get(id).copied().unwrap_or(0);
        let generation = self.l.generation;
        let valid = self.l.size_memo.get(id).is_some_and(|memo| memo.stamp == stamp && memo.generation == generation);
        if valid {
            let memo = self.l.size_memo.get(id).unwrap();
            if let Some(entry) = memo.entries.iter().find(|entry| entry.args == args) {
                let (result, carry) = (entry.result, entry.carry.clone());
                self.l.stats.memo += 1;
                cache_counts::hit(Cache::Sizes);
                self.l.text_carries.set(id, carry);
                return result;
            }
        } else {
            self.l.size_memo.set(id, Memo { stamp, generation, entries: Vec::new() });
        }
        self.l.stats.computed += 1;
        cache_counts::miss(Cache::Sizes);
        let result = self.compute_size(id, args);
        let carry = self.l.text_carries.get(id).cloned().flatten();
        if let Some(memo) = self.l.size_memo.get_mut(id) {
            if memo.entries.len() >= MEMO_LIMIT {
                memo.entries.remove(0);
                cache_counts::evicted(Cache::Sizes, 1);
            }
            memo.entries.push(MemoEntry { args, result, carry });
        }
        result
    }

    fn resolve_size(&mut self, id: InstanceId, args: SizeArgs) -> Sized {
        let automatic = self.code(id, self.l.ids.n.AutomaticSize);
        if automatic == E::EMPTY || automatic == E::NONE {
            return self.compute_size(id, args);
        }
        let mut result = self.memoized_size(id, args);
        if let Some(capture) = self.l.carry_capture {
            self.l.carry_maps[capture].insert(id, [result.w, result.h, result.base_w, result.base_h]);
        } else if let Some(apply) = self.l.carry_apply {
            if let Some(kept) = self.l.carry_maps[apply].get(&id).copied() {
                if automatic == E::X || automatic == E::XY {
                    result.w = kept[0];
                    result.base_w = kept[2];
                }
                if automatic == E::Y || automatic == E::XY {
                    result.h = kept[1];
                    result.base_h = kept[3];
                }
            }
        }
        result
    }

    // Placement

    /// Places a child: absolute rect from its unscaled size and the parent content rect. `offset`
    /// (children placed by a layout) is in unscaled local units from the content origin.
    fn place_child(&mut self, id: InstanceId, parent: &Record, unscaled_w: f64, unscaled_h: f64, offset: Option<(f64, f64)>, base: Option<(f64, f64)>) -> Record {
        let n = self.l.ids.n;
        let scale = f32r(parent.scale * self.own_scale(id));
        let (width, height) = (f32r(unscaled_w * scale), f32r(unscaled_h * scale));
        let (pad_left, pad_right, pad_top, pad_bottom) = self.padding(id, unscaled_w, unscaled_h);

        // Measured (layout-probe-negcontent): padding wider than the object leaves the content box
        // between the two padded edges, [W - right, left], |W - left - right| wide.
        let raw_w = f32r(f32r(unscaled_w - pad_left) - pad_right);
        let raw_h = f32r(f32r(unscaled_h - pad_top) - pad_bottom);
        let (content_w, content_h) = (raw_w.abs(), raw_h.abs());
        let start_x = if raw_w < 0.0 { f32r(unscaled_w - pad_right) } else { pad_left };
        let start_y = if raw_h < 0.0 { f32r(unscaled_h - pad_bottom) } else { pad_top };

        let place = match offset {
            Some((offset_x, offset_y)) => [
                f32r(offset_x * parent.scale),
                0.0,
                f32r(offset_y * parent.scale),
                0.0,
                f32r(start_x * scale),
                f32r(start_y * scale),
            ],
            None => {
                let position = self.udim2(id, n.Position);
                let anchor = self.vec2(id, n.AnchorPoint).unwrap_or((0.0, 0.0));
                let (sizing_w, sizing_h) = local_content(parent);
                let local_x = f32r(f32r(position[0] * sizing_w) + position[1]);
                let local_y = f32r(f32r(position[2] * sizing_h) + position[3]);
                // Measured: AnchorPoint components are clamped to [0, 1].
                [
                    f32r(local_x * parent.scale),
                    f32r(f32r(clamp(anchor.0, 0.0, 1.0) * unscaled_w) * scale),
                    f32r(local_y * parent.scale),
                    f32r(f32r(clamp(anchor.1, 0.0, 1.0) * unscaled_h) * scale),
                    f32r(start_x * scale),
                    f32r(start_y * scale),
                ]
            }
        };

        // Measured: children of an AutomaticSize object size and position against its base size.
        let (mut sizing_w, mut sizing_h, mut local_sizing_w, mut local_sizing_h) = (None, None, None, None);
        let (base_w, base_h) = base.unwrap_or((unscaled_w, unscaled_h));
        if base.is_some() && (base_w != unscaled_w || base_h != unscaled_h) {
            let lw = f32r(f32r(base_w - pad_left) - pad_right).abs();
            let lh = f32r(f32r(base_h - pad_top) - pad_bottom).abs();
            local_sizing_w = Some(lw);
            local_sizing_h = Some(lh);
            sizing_w = Some(f32r(lw * scale));
            sizing_h = Some(f32r(lh * scale));
        }

        // Measured: children placed by a UIListLayout/UIGridLayout ignore their own Rotation.
        let own_rotation = if offset.is_some() { 0.0 } else { self.num(id, n.Rotation).unwrap_or(0.0) };
        let visible = self.visible(id);

        let mut record = Record {
            w: width,
            h: height,
            scale,
            content: Rect { x: 0.0, y: 0.0, w: f32r(content_w * scale), h: f32r(content_h * scale) },
            visible: false,
            generation: self.l.generation,
            own_rotation: Some(own_rotation),
            parent_width: Some(parent.sizing_w.unwrap_or(parent.content.w)),
            parent_height: Some(parent.sizing_h.unwrap_or(parent.content.h)),
            sizing_w,
            sizing_h,
            local_w: Some(content_w),
            local_h: Some(content_h),
            local_sizing_w,
            local_sizing_h,
            local_outer_w: Some(if local_sizing_w.is_some() { base_w } else { unscaled_w }),
            local_outer_h: Some(if local_sizing_h.is_some() { base_h } else { unscaled_h }),
            // (the space children can grow into does not flip: a content box narrower than its padding
            // offers none, layout-probe-autoedge)
            local_space_w: Some(lmax(raw_w, 0.0)),
            local_space_h: Some(lmax(raw_h, 0.0)),
            place: Some(place),
            ..Record::default()
        };
        locate(&mut record, parent.content.x, parent.content.y, ParentPlace::of(parent), visible);
        record
    }

    // Layouts

    /// SortItems: SortOrder Name / LayoutOrder (ties keep the original order).
    fn sort_items(&self, layout: InstanceId, items: &mut Vec<InstanceId>) {
        let mut positions: Vec<usize> = (0..items.len()).collect();
        self.sort_positions(layout, items, &mut positions);
        let sorted: Vec<InstanceId> = positions.iter().map(|&position| items[position]).collect();
        *items = sorted;
    }

    /// Sorts `positions` (indices into `items`, ascending on entry) like SortItems sorts the items
    /// they point at (ties keep the ascending position order, so the order is total and an unstable,
    /// allocation-free sort gives the same result).
    fn sort_positions(&self, layout: InstanceId, items: &[InstanceId], positions: &mut [usize]) {
        let order = self.code(layout, self.l.ids.n.SortOrder);
        if order == E::NAME {
            positions.sort_unstable_by(|a, b| {
                let name_a = self.state.dom.at(items[*a]).name.as_bytes();
                let name_b = self.state.dom.at(items[*b]).name.as_bytes();
                name_a.cmp(name_b).then(a.cmp(b))
            });
        } else if order == E::LAYOUT_ORDER {
            let n = self.l.ids.n.LayoutOrder;
            positions.sort_unstable_by(|a, b| {
                let order_a = self.num(items[*a], n).unwrap_or(0.0);
                let order_b = self.num(items[*b], n).unwrap_or(0.0);
                if order_a != order_b {
                    if order_a < order_b {
                        return std::cmp::Ordering::Less;
                    }
                    if order_b < order_a {
                        return std::cmp::Ordering::Greater;
                    }
                }
                a.cmp(b)
            });
        }
    }

    /// Flex grow / shrink ratios of a list item (UIFlexItem, or the layout's main-axis Fill).
    fn flex_ratios(&self, item: InstanceId, main_flex: u8) -> (f64, f64) {
        let flex = self.first_child_of(item, self.l.ids.c.UIFlexItem);
        let n = self.l.ids.n;
        let mode = match flex {
            Some(flex) => self.code(flex, n.FlexMode),
            None => {
                if main_flex == E::FILL {
                    E::FILL
                } else {
                    E::NONE
                }
            }
        };
        match mode {
            E::GROW => (1.0, 0.0),
            E::SHRINK => (0.0, 1.0),
            E::FILL => (1.0, 1.0),
            E::CUSTOM if flex.is_some() => {
                let flex = flex.unwrap();
                (lmax(self.num(flex, n.GrowRatio).unwrap_or(0.0), 0.0), lmax(self.num(flex, n.ShrinkRatio).unwrap_or(0.0), 0.0))
            }
            _ => (0.0, 0.0),
        }
    }

    /// UIListLayout: lines (Wraps), main-axis flex per line, cross-axis line distribution, item
    /// alignment inside lines. `sizes` are scaled (updated in place for flexed / stretched items).
    /// Returns offsets relative to the parent content origin and AbsoluteContentSize.
    fn list_layout(&self, layout: InstanceId, parent: &Record, items: &[InstanceId], sizes: &mut [[f64; 2]], offsets: &mut Vec<(f64, f64)>, scratch: &mut ListScratch) -> (f64, f64) {
        let n = self.l.ids.n;
        let horizontal = self.code(layout, n.FillDirection) == E::HORIZONTAL;
        let horizontal_alignment = self.code(layout, n.HorizontalAlignment);
        let vertical_alignment = self.code(layout, n.VerticalAlignment);
        let (padding_scale, padding_offset) = self.udim(layout, n.Padding);
        let mut wraps = self.is_true(layout, n.Wraps);
        let horizontal_flex = self.code(layout, n.HorizontalFlex);
        let vertical_flex = self.code(layout, n.VerticalFlex);
        let layout_line_alignment = self.code(layout, n.ItemLineAlignment);

        // Measured (layout-probe-invisible2): a list does not wrap along an automatic axis of its
        // container.
        if wraps {
            if let Some(container) = self.parent(layout) {
                let automatic = self.code(container, n.AutomaticSize);
                let automatic_main = if horizontal { automatic == E::X || automatic == E::XY } else { automatic == E::Y || automatic == E::XY };
                if automatic_main {
                    wraps = false;
                }
            }
        }

        let scale = parent.scale;
        // Measured (layout-probe-autolistgap): an AutomaticSize container lays its list out in its
        // stable size (gap, alignment), like its other children, not in the grown size.
        let (mut content_w, mut content_h) = (parent.sizing_w.unwrap_or(parent.content.w), parent.sizing_h.unwrap_or(parent.content.h));
        if parent.align_start.is_some() {
            content_w = parent.content.w;
            content_h = parent.content.h;
        }
        let main_size = if horizontal { content_w } else { content_h };
        let cross_size = if horizontal { content_h } else { content_w };
        let gap_basis = (if horizontal { parent.gap_w } else { parent.gap_h }).unwrap_or(main_size);
        let gap = padding_scale * gap_basis + padding_offset * scale;
        let main_flex = if horizontal { horizontal_flex } else { vertical_flex };
        let cross_flex = if horizontal { vertical_flex } else { horizontal_flex };
        let main_alignment = if horizontal { horizontal_alignment } else { vertical_alignment };
        let cross_alignment = if horizontal { vertical_alignment } else { horizontal_alignment };
        let main_index = if horizontal { 0 } else { 1 };
        let cross_index = if horizontal { 1 } else { 0 };
        let (main_measured, cross_measured) = match parent.measuring {
            Some((x, y)) => {
                if horizontal {
                    (x, y)
                } else {
                    (y, x)
                }
            }
            None => (false, false),
        };

        // Lines (wrapping uses the unflexed sizes)
        let ListScratch { lines, mains, grow, shrink, crosses } = scratch;
        lines.clear();
        lines.push(Line { start: 0, end: 0, main: 0.0, cross: 0.0, sum: 0.0, used: 0.0 });
        for index in 0..items.len() {
            let item_main = sizes[index][main_index];
            let line = lines.last_mut().unwrap();
            let count = line.end - line.start;
            let mut needed = if count > 0 { line.main + gap + item_main } else { item_main };
            if wraps && count > 0 && needed > main_size + 1e-6 {
                lines.push(Line { start: index, end: index, main: 0.0, cross: 0.0, sum: 0.0, used: 0.0 });
                needed = item_main;
            }
            let line = lines.last_mut().unwrap();
            line.end = index + 1;
            line.main = needed;
            line.cross = lmax(line.cross, sizes[index][cross_index]);
        }

        // Main-axis flex per line
        for line in lines.iter_mut() {
            mains.clear();
            grow.clear();
            shrink.clear();
            for index in line.start..line.end {
                mains.push(sizes[index][main_index]);
                let (g, s) = self.flex_ratios(items[index], main_flex);
                grow.push(g);
                shrink.push(s);
            }
            if !main_measured {
                distribute(mains, grow, shrink, main_size - line.main);
            }
            let mut sum = 0.0;
            for (offset, index) in (line.start..line.end).enumerate() {
                sizes[index][main_index] = mains[offset];
                sum += mains[offset];
            }
            line.sum = sum;
            let count = (line.end - line.start) as f64;
            line.used = sum + lmax(count - 1.0, 0.0) * gap;
        }

        // Cross-axis line distribution. Measured: a line is as thick as its thickest item (also
        // without Wraps); cross Fill grows lines equally and shrinks them weighted by thickness.
        let line_count = lines.len() as f64;
        let mut cross_sum = 0.0;
        for line in lines.iter() {
            cross_sum += line.cross;
        }
        let (mut cross_start, line_gap) = space_distribution(cross_flex, cross_size, cross_sum, line_count, gap);
        if cross_flex == E::FILL && !cross_measured {
            crosses.clear();
            crosses.extend(lines.iter().map(|line| line.cross));
            grow.clear();
            grow.resize(lines.len(), 1.0);
            distribute(crosses, grow, grow, cross_size - cross_sum - (line_count - 1.0) * gap);
            cross_sum = 0.0;
            for (line, cross) in lines.iter_mut().zip(crosses.iter()) {
                line.cross = *cross;
                cross_sum += line.cross;
            }
            cross_start = Some(0.0);
        } else if cross_start.is_none() {
            cross_start = Some(aligned_start(cross_alignment, cross_size, cross_sum + (line_count - 1.0) * gap));
        }
        // Measuring the descendants of an AutomaticSize frame (DescendantExtent): the block alignment
        // counts as 0 on the measured axes, like scale positions.
        let (main_zero, cross_zero) = match parent.align_start {
            Some((x, y)) => {
                if horizontal {
                    (x, y)
                } else {
                    (y, x)
                }
            }
            None => (false, false),
        };
        if cross_zero {
            cross_start = Some(0.0);
        }

        // Measured: items without an explicit ItemLineAlignment stretch in filled lines and otherwise
        // follow the cross-axis alignment.
        let mut default_line_alignment = layout_line_alignment;
        if default_line_alignment == E::AUTOMATIC || default_line_alignment == E::EMPTY {
            default_line_alignment = if cross_flex == E::FILL {
                E::STRETCH
            } else if cross_alignment == E::CENTER {
                E::CENTER
            } else if cross_alignment == E::BOTTOM || cross_alignment == E::RIGHT {
                E::END
            } else {
                E::START
            };
        }

        offsets.clear();
        offsets.resize(items.len(), (0.0, 0.0));
        let mut content_main: f64 = 0.0;
        let mut cross_cursor = cross_start.unwrap();
        for line in lines.iter() {
            let count = (line.end - line.start) as f64;
            let (start, between) = space_distribution(main_flex, main_size, line.sum, count, gap);
            let mut start = start.unwrap_or_else(|| aligned_start(main_alignment, main_size, line.used));
            if main_zero {
                start = 0.0;
            }

            let mut cursor = start;
            for index in line.start..line.end {
                let item = items[index];
                let flex = self.first_child_of(item, self.l.ids.c.UIFlexItem);
                let mut alignment = match flex {
                    Some(flex) => self.code(flex, n.ItemLineAlignment),
                    None => E::AUTOMATIC,
                };
                if alignment == E::AUTOMATIC || alignment == E::EMPTY {
                    alignment = default_line_alignment;
                }

                let mut cross = 0.0;
                if alignment == E::STRETCH {
                    sizes[index][cross_index] = line.cross;
                } else if alignment == E::CENTER {
                    cross = (line.cross - sizes[index][cross_index]) / 2.0;
                } else if alignment == E::END {
                    cross = line.cross - sizes[index][cross_index];
                }

                offsets[index] = if horizontal { (cursor, cross_cursor + cross) } else { (cross_cursor + cross, cursor) };
                cursor += sizes[index][main_index] + between;
            }

            content_main = lmax(content_main, line.used);
            cross_cursor += line.cross + line_gap;
        }

        let content_cross = cross_sum + (line_count - 1.0) * gap;
        if horizontal {
            (content_main, content_cross)
        } else {
            (content_cross, content_main)
        }
    }

    #[allow(clippy::type_complexity)]
    fn grid_layout(&self, layout: InstanceId, parent: &Record, items: &[InstanceId]) -> (Vec<(f64, f64)>, Vec<(f64, f64)>, f64, f64, f64, f64, f64, f64) {
        let n = self.l.ids.n;
        let scale = parent.scale;
        let cell_size = self.udim2(layout, n.CellSize);
        let cell_padding = self.udim2(layout, n.CellPadding);
        let (content_w, content_h) = (parent.content.w, parent.content.h);

        let (item_w, item_h) = (parent.item_w.unwrap_or(content_w), parent.item_h.unwrap_or(content_h));
        let mut cell_w = cell_size[0] * item_w + cell_size[1] * scale;
        let mut cell_h = cell_size[2] * item_h + cell_size[3] * scale;
        if let Some(aspect) = self.first_child_of(layout, self.l.ids.c.UIAspectRatioConstraint) {
            let ratio = self.num(aspect, n.AspectRatio).unwrap_or(1.0);
            if cell_w / lmax(cell_h, 1e-9) > ratio {
                cell_w = cell_h * ratio;
            } else {
                cell_h = cell_w / ratio;
            }
        }
        let pad_x = cell_padding[0] * content_w + cell_padding[1] * scale;
        let pad_y = cell_padding[2] * content_h + cell_padding[3] * scale;

        let horizontal = self.code(layout, n.FillDirection) != E::VERTICAL;
        let max_cells = self.num(layout, n.FillDirectionMaxCells).unwrap_or(0.0);
        // (relative tolerance: float32 inputs make three 0.3-wide cells + 0.05 gaps overshoot 1)
        let mut per_line = if horizontal {
            lmax(1.0, ((item_w + pad_x) / lmax(cell_w + pad_x, 1e-9) + 1e-5).floor())
        } else {
            lmax(1.0, ((item_h + pad_y) / lmax(cell_h + pad_y, 1e-9) + 1e-5).floor())
        };
        if max_cells > 0.0 {
            per_line = lmin(per_line, max_cells);
        }

        let count = items.len() as f64;
        let lines = lmax(1.0, (count / per_line).ceil());
        let columns = if horizontal { lmin(per_line, lmax(count, 1.0)) } else { lines };
        let rows = if horizontal { lines } else { lmin(per_line, lmax(count, 1.0)) };
        let grid_w = columns * cell_w + lmax(columns - 1.0, 0.0) * pad_x;
        let grid_h = rows * cell_h + lmax(rows - 1.0, 0.0) * pad_y;

        let horizontal_alignment = self.code(layout, n.HorizontalAlignment);
        let vertical_alignment = self.code(layout, n.VerticalAlignment);
        let mut start_x = if horizontal_alignment == E::CENTER {
            (content_w - grid_w) / 2.0
        } else if horizontal_alignment == E::RIGHT {
            content_w - grid_w
        } else {
            0.0
        };
        let mut start_y = if vertical_alignment == E::CENTER {
            (content_h - grid_h) / 2.0
        } else if vertical_alignment == E::BOTTOM {
            content_h - grid_h
        } else {
            0.0
        };
        if let Some((x, y)) = parent.align_start {
            if x {
                start_x = 0.0;
            }
            if y {
                start_y = 0.0;
            }
        }

        let corner = self.code(layout, n.StartCorner);
        let flip_x = corner == E::TOP_RIGHT || corner == E::BOTTOM_RIGHT;
        let flip_y = corner == E::BOTTOM_LEFT || corner == E::BOTTOM_RIGHT;

        let mut offsets = Vec::with_capacity(items.len());
        let mut sizes = Vec::with_capacity(items.len());
        for index in 0..items.len() {
            let position = index as f64;
            let line = (position / per_line).floor();
            let slot = position - (position / per_line).floor() * per_line;
            let mut column = if horizontal { slot } else { line };
            let mut row = if horizontal { line } else { slot };
            if flip_x {
                column = columns - 1.0 - column;
            }
            if flip_y {
                row = rows - 1.0 - row;
            }
            offsets.push((start_x + column * (cell_w + pad_x), start_y + row * (cell_h + pad_y)));
            sizes.push((cell_w, cell_h));
        }
        let (cells_x, cells_y) = if count > 0.0 { (columns, rows) } else { (0.0, 0.0) };
        (offsets, sizes, grid_w, grid_h, cell_w, cell_h, cells_x, cells_y)
    }

    /// UIPageLayout (layout-probe-page): pages side by side along FillDirection, the current page at
    /// the origin; each page aligned inside its container-sized slot.
    fn page_layout(&self, layout: InstanceId, parent: &Record, items: &[InstanceId], sizes: &[(f64, f64)]) -> (Vec<(f64, f64)>, f64, f64) {
        let n = self.l.ids.n;
        let horizontal = self.code(layout, n.FillDirection) != E::VERTICAL;
        let horizontal_alignment = self.code(layout, n.HorizontalAlignment);
        let vertical_alignment = self.code(layout, n.VerticalAlignment);
        let (padding_scale, padding_offset) = self.udim(layout, n.Padding);
        let (content_w, content_h) = (parent.content.w, parent.content.h);
        let main_size = if horizontal { content_w } else { content_h };
        let gap = padding_scale * main_size + padding_offset * parent.scale;
        let stride = main_size + gap;

        let mut current = 1.0;
        let current_page = self.get(layout, n.CurrentPage).and_then(Value::as_instance);
        for (index, item) in items.iter().enumerate() {
            if Some(*item) == current_page {
                current = (index + 1) as f64;
            }
        }

        let mut offsets = Vec::with_capacity(items.len());
        for (index, size) in sizes.iter().enumerate() {
            let position = (index + 1) as f64;
            let slot_x = if horizontal { (position - current) * stride } else { 0.0 };
            let slot_y = if horizontal { 0.0 } else { (position - current) * stride };
            offsets.push((
                slot_x + aligned_start(horizontal_alignment, content_w, size.0),
                slot_y + aligned_start(vertical_alignment, content_h, size.1),
            ));
        }

        let count = items.len() as f64;
        let total = count * main_size + lmax(count - 1.0, 0.0) * gap;
        if horizontal {
            (offsets, total, content_h)
        } else {
            (offsets, content_w, total)
        }
    }

    /// UITableLayout (layout-probe-page): children are rows (RowMajor) or columns (ColumnMajor)
    /// whose children are the cells (see the Luau). Returns offsets, sizes, block size and each
    /// row / column's cell rects.
    #[allow(clippy::type_complexity)]
    fn table_layout(&mut self, layout: InstanceId, parent: &Record, items: &[InstanceId], content_w_local: f64, content_h_local: f64) -> (Vec<(f64, f64)>, Vec<(f64, f64)>, f64, f64, Vec<Cells>) {
        let n = self.l.ids.n;
        let scale = parent.scale;
        let (content_w, content_h) = (parent.content.w, parent.content.h);
        let column_major = self.code(layout, n.MajorAxis) == E::COLUMN_MAJOR;
        let padding = self.udim2(layout, n.Padding);
        let pad_x = padding[0] * content_w + padding[1] * scale;
        let pad_y = padding[2] * content_h + padding[3] * scale;

        let mut groups: Vec<Vec<InstanceId>> = Vec::with_capacity(items.len());
        for &item in items {
            let children = self.gui_children(item);
            let mut cells: Vec<InstanceId> = children.iter().copied().filter(|&cell| self.visible(cell)).collect();
            self.l.give_vec(children);
            self.sort_items(layout, &mut cells);
            groups.push(cells);
        }

        // Widths per column and heights per row (scaled px)
        let mut widths: Vec<Option<f64>> = Vec::new();
        let mut heights: Vec<Option<f64>> = Vec::new();
        for (group_index, cells) in groups.iter().enumerate() {
            for (cell_index, &cell) in cells.iter().enumerate() {
                let cell_scale = scale * self.own_scale(cell);
                let sized = self.resolve_size(cell, SizeArgs {
                    parent_w: content_w_local,
                    parent_h: content_h_local,
                    scale: cell_scale,
                    available: None,
                    measure_parent_w: None,
                    measure_parent_h: None,
                    available_h: None,
                    full_w: None,
                    full_h: None,
                });
                let (column, row) = if column_major { (group_index, cell_index) } else { (cell_index, group_index) };
                if widths.len() <= column {
                    widths.resize(column + 1, None);
                }
                if heights.len() <= row {
                    heights.resize(row + 1, None);
                }
                widths[column] = Some(lmax(widths[column].unwrap_or(0.0), sized.w * scale));
                heights[row] = Some(lmax(heights[row].unwrap_or(0.0), sized.h * scale));
            }
        }
        let (mut columns, mut rows) = (widths.len(), heights.len());
        if column_major {
            columns = columns.max(groups.len());
        } else {
            rows = rows.max(groups.len());
        }
        let mut widths: Vec<f64> = (0..columns).map(|index| widths.get(index).copied().flatten().unwrap_or(0.0)).collect();
        let mut heights: Vec<f64> = (0..rows).map(|index| heights.get(index).copied().flatten().unwrap_or(0.0)).collect();

        let fill = |values: &mut [f64], available: f64, pad: f64| {
            let count = values.len() as f64;
            let mut sum = 0.0;
            for value in values.iter() {
                sum += value;
            }
            let target = available - lmax(count - 1.0, 0.0) * pad;
            if sum > 0.0 && target > 0.0 {
                for value in values.iter_mut() {
                    *value = *value * target / sum;
                }
            }
        };
        if self.is_true(layout, n.FillEmptySpaceColumns) {
            fill(&mut widths, content_w, pad_x);
        }
        if self.is_true(layout, n.FillEmptySpaceRows) {
            fill(&mut heights, content_h, pad_y);
        }

        let mut column_x = Vec::with_capacity(columns);
        let mut cursor = 0.0;
        for width in &widths {
            column_x.push(cursor);
            cursor += width + pad_x;
        }
        let block_w = lmax(cursor - pad_x, 0.0);
        let mut row_y = Vec::with_capacity(rows);
        cursor = 0.0;
        for height in &heights {
            row_y.push(cursor);
            cursor += height + pad_y;
        }
        let block_h = lmax(cursor - pad_y, 0.0);
        let start_x = aligned_start(self.code(layout, n.HorizontalAlignment), content_w, block_w);
        let start_y = aligned_start(self.code(layout, n.VerticalAlignment), content_h, block_h);

        let mut offsets = Vec::with_capacity(items.len());
        let mut sizes = Vec::with_capacity(items.len());
        let mut cell_rects = Vec::with_capacity(items.len());
        for (group_index, cells) in groups.iter().enumerate() {
            if column_major {
                offsets.push((start_x + column_x[group_index], start_y));
                sizes.push((widths[group_index], block_h));
            } else {
                offsets.push((start_x, start_y + row_y[group_index]));
                sizes.push((block_w, heights[group_index]));
            }
            let mut rects: Vec<(InstanceId, [f64; 4])> = Vec::with_capacity(cells.len());
            for (cell_index, &cell) in cells.iter().enumerate() {
                let (column, row) = if column_major { (group_index, cell_index) } else { (cell_index, group_index) };
                let rect = if column_major {
                    [0.0, row_y[row], widths[column], heights[row]]
                } else {
                    [column_x[column], 0.0, widths[column], heights[row]]
                };
                match rects.iter_mut().find(|(other, _)| *other == cell) {
                    Some(entry) => entry.1 = rect,
                    None => rects.push((cell, rect)),
                }
            }
            cell_rects.push(Rc::new(rects));
        }
        (offsets, sizes, block_w, block_h, cell_rects)
    }

    /// Lays out the children of `id` whose own record is `record`. In measure mode nothing is
    /// stored; returns the unscaled extent of the visible content (for AutomaticSize).
    pub(crate) fn layout_children(&mut self, id: InstanceId, record: &Record, measure: bool, mut collect: Option<Collect>) -> (f64, f64) {
        if measure {
            self.l.stats.measures += 1;
        }
        let items = self.gui_children(id);
        let layout = self.layout_object(id);
        let scale = record.scale;
        let (content_w, content_h) = local_content(record);
        // Basis of the children's scale sizes: list / grid items may use the available size while
        // measuring (ItemW/H); plain children of an AutomaticCanvasSize ScrollingFrame use SizeW/H.
        let (basis_w, basis_h) = if layout.is_some() {
            (record.item_w.map_or(content_w, |v| v / scale), record.item_h.map_or(content_h, |v| v / scale))
        } else {
            (record.size_w.map_or(content_w, |v| v / scale), record.size_h.map_or(content_h, |v| v / scale))
        };
        // (in local units: the content box's own local size where the record has one)
        let available_w = match record.available {
            Some(available) => available / scale,
            None => record.local_space_w.unwrap_or(record.content.w / scale),
        };
        let available_height = match record.available_h {
            Some(available) => available / scale,
            None => record.local_space_h.unwrap_or(record.content.h / scale),
        };
        // The parent's whole size (automatic axes of text objects, see ComputeSize); none for canvases.
        let (mut full_w, mut full_h) = (None, None);
        let class = self.class(id);
        if class != self.l.ids.c.ScrollingFrame && self.reflection.class(class).is_a(self.l.ids.c.GuiObject) {
            full_w = Some(record.local_outer_w.unwrap_or(record.w / scale));
            full_h = Some(record.local_outer_h.unwrap_or(record.h / scale));
            if layout.is_some() {
                if let Some(item_w) = record.item_w {
                    full_w = Some(item_w / scale);
                }
                if let Some(item_h) = record.item_h {
                    full_h = Some(item_h / scale);
                }
            }
        }
        let (mut extent_x, mut extent_y): (f64, f64) = (0.0, 0.0);

        let count = items.len();
        let mut resolved = self.l.scratch.resolved.pop().unwrap_or_default();
        resolved.clear();
        let mut text_boxes = self.l.scratch.boxes.pop().unwrap_or_default();
        text_boxes.clear();
        let text_carry = self.l.text_carries.get(id).cloned().flatten();
        let n_position = self.l.ids.n.Position;
        let n_anchor = self.l.ids.n.AnchorPoint;
        for &child in &items {
            // Width a child may grow to (AutomaticSize X): the parent's available content width minus
            // the child's X position; children placed by a layout get the whole width.
            let child_scale = self.own_scale(child);
            let position = self.udim2(child, n_position);
            let anchor = self.vec2(child, n_anchor).unwrap_or((0.0, 0.0));
            let available = growth_space(available_w, position[0] * content_w + position[1], anchor.0);
            let available_h = growth_space(available_height, position[2] * content_h + position[3], anchor.1);
            let sized = self.resolve_size(child, SizeArgs {
                parent_w: basis_w,
                parent_h: basis_h,
                scale: scale * child_scale,
                available: Some(available),
                measure_parent_w: record.measure_w.map(|v| v / scale),
                measure_parent_h: record.measure_h.map(|v| v / scale),
                available_h: Some(available_h),
                full_w,
                full_h,
            });
            let (mut width, mut height, mut text_box, mut base_w, mut base_h) = (sized.w, sized.h, sized.text_box, sized.base_w, sized.base_h);
            if let Some(kept) = text_carry.as_ref().and_then(|carry| carry.iter().find(|(c, _)| *c == child).map(|(_, e)| *e)) {
                if let Some(kept_box) = kept.text_box {
                    text_box = Some(kept_box);
                }
                if let Some(kept_height) = kept.height {
                    height = kept_height;
                    base_h = kept_height;
                }
                if let Some(kept_width) = kept.width {
                    width = kept_width;
                    base_w = kept_width;
                }
            }
            let mut entry = Resolved {
                w: width,
                h: height,
                base_w,
                base_h,
                measure: sized.measure_w.map(|w| (w, sized.measure_h.unwrap_or(0.0))),
            };
            if let Some(held) = record.held_w.as_ref().and_then(|held| held.iter().find(|(c, _)| *c == child).map(|(_, w)| *w)) {
                entry.w = held;
                entry.base_w = held;
            }
            resolved.push(entry);
            text_boxes.push(text_box);
        }

        let mut offsets = self.l.scratch.pairs.pop().unwrap_or_default();
        offsets.clear();
        offsets.resize(count, None);
        let mut forced = self.l.scratch.pairs.pop().unwrap_or_default();
        forced.clear();
        forced.resize(count, None);
        let mut cell_rects: Option<Vec<Option<Cells>>> = None;
        if layout.is_none() {
            if let Some(cells) = &record.cells {
                // Cells of a UITableLayout row/column get the rects the table assigned them.
                for (index, &child) in items.iter().enumerate() {
                    if let Some((_, cell)) = cells.iter().find(|(c, _)| *c == child) {
                        offsets[index] = Some((cell[0], cell[1]));
                        forced[index] = Some((cell[2], cell[3]));
                    }
                }
            }
        }
        if let Some(layout) = layout {
            // Flow: visible direct children, sorted
            let mut flow_index = self.l.scratch.indices.pop().unwrap_or_default();
            flow_index.clear();
            for (index, &child) in items.iter().enumerate() {
                if self.visible(child) && self.parent(child) == Some(id) {
                    flow_index.push(index);
                }
            }
            self.sort_positions(layout, &items, &mut flow_index);
            // positions in `items` of the sorted flow
            let flow_positions = flow_index;
            let mut flow = self.l.take_vec();
            flow.extend(flow_positions.iter().map(|&index| items[index]));
            let layout_class = self.class(layout);
            let c = self.l.ids.c;
            let n = self.l.ids.n;
            if layout_class == c.UIGridLayout {
                let (grid_offsets, grid_sizes, grid_w, grid_h, cell_w, cell_h, grid_columns, grid_rows) = self.grid_layout(layout, record, &flow);
                // Measured: a UIScale'd grid item is its cell scaled about the cell centre.
                for (flow_at, &position) in flow_positions.iter().enumerate() {
                    let child_scale = self.own_scale(flow[flow_at]);
                    let size = grid_sizes[flow_at];
                    let offset = grid_offsets[flow_at];
                    offsets[position] = Some((offset.0 + size.0 * (1.0 - child_scale) / 2.0, offset.1 + size.1 * (1.0 - child_scale) / 2.0));
                    forced[position] = Some((size.0 * child_scale, size.1 * child_scale));
                }
                // Measured: invisible children get the cell size at the content origin.
                for (index, &child) in items.iter().enumerate() {
                    if forced[index].is_none() && self.parent(child) == Some(id) {
                        let child_scale = self.own_scale(child);
                        forced[index] = Some((cell_w * child_scale, cell_h * child_scale));
                        offsets[index] = Some((0.0, 0.0));
                    }
                }
                if !measure {
                    self.set_vector(layout, n.AbsoluteContentSize, grid_w, grid_h);
                    self.set_vector(layout, n.AbsoluteCellSize, cell_w, cell_h);
                    self.set_vector(layout, n.AbsoluteCellCount, grid_columns, grid_rows);
                }
                extent_x = grid_w / scale;
                extent_y = grid_h / scale;
            } else if layout_class == c.UIPageLayout {
                let mut page_sizes = Vec::with_capacity(flow.len());
                for (flow_at, &position) in flow_positions.iter().enumerate() {
                    let child_scale = scale * self.own_scale(flow[flow_at]);
                    page_sizes.push((resolved[position].w * child_scale, resolved[position].h * child_scale));
                }
                let (page_offsets, content_width, content_height) = self.page_layout(layout, record, &flow, &page_sizes);
                for (flow_at, &position) in flow_positions.iter().enumerate() {
                    offsets[position] = Some(page_offsets[flow_at]);
                }
                if !measure {
                    self.set_vector(layout, n.AbsoluteContentSize, content_width, content_height);
                    let current_page = n.CurrentPage;
                    if self.get(layout, current_page).is_none() && !flow.is_empty() {
                        // (Layout.Writing: the invalidation hook ignores this write)
                        core::raw_set(self.state, self.reflection, self.hooks, layout, current_page, Value::Instance(flow[0]));
                    }
                }
                extent_x = content_width / scale;
                extent_y = content_height / scale;
            } else if layout_class == c.UITableLayout {
                let (table_offsets, table_sizes, table_w, table_h, table_cells) = self.table_layout(layout, record, &flow, content_w, content_h);
                let mut rects: Vec<Option<Cells>> = vec![None; count];
                for (flow_at, &position) in flow_positions.iter().enumerate() {
                    offsets[position] = Some(table_offsets[flow_at]);
                    forced[position] = Some(table_sizes[flow_at]);
                    rects[position] = Some(table_cells[flow_at].clone());
                }
                cell_rects = Some(rects);
                if !measure {
                    self.set_vector(layout, n.AbsoluteContentSize, table_w, table_h);
                }
                extent_x = table_w / scale;
                extent_y = table_h / scale;
            } else {
                // Measured quirk (layout-probe-flexscale 72/72): in a flex list a UIScale'd item keeps
                // its unscaled cross-axis size.
                let flex_list = self.is_true(layout, n.Wraps)
                    || self.code(layout, n.HorizontalFlex) != E::NONE
                    || self.code(layout, n.VerticalFlex) != E::NONE
                    || self.code(layout, n.ItemLineAlignment) != E::AUTOMATIC;
                let horizontal_fill = self.code(layout, n.FillDirection) == E::HORIZONTAL;
                let mut scaled_sizes = std::mem::take(&mut self.l.list_scratch.sizes);
                scaled_sizes.clear();
                for (flow_at, &position) in flow_positions.iter().enumerate() {
                    let child_scale = self.own_scale(flow[flow_at]);
                    let mut width = resolved[position].w * scale * child_scale;
                    let mut height = resolved[position].h * scale * child_scale;
                    if flex_list && child_scale != 1.0 {
                        if horizontal_fill {
                            height = resolved[position].h * scale;
                        } else {
                            width = resolved[position].w * scale;
                        }
                    }
                    scaled_sizes.push([width, height]);
                }
                let mut list_scratch = std::mem::take(&mut self.l.list_scratch.list);
                let mut list_offsets = std::mem::take(&mut self.l.list_scratch.offsets);
                let (content_width, content_height) = self.list_layout(layout, record, &flow, &mut scaled_sizes, &mut list_offsets, &mut list_scratch);
                self.l.list_scratch.list = list_scratch;
                for (flow_at, &position) in flow_positions.iter().enumerate() {
                    offsets[position] = Some(list_offsets[flow_at]);
                }
                // Measured: invisible children keep their size and sit at the content origin; in a
                // flex list they are 0 x 0 there (layout-probe-invisible2).
                for (index, &child) in items.iter().enumerate() {
                    if offsets[index].is_none() && self.parent(child) == Some(id) {
                        offsets[index] = Some((0.0, 0.0));
                        if flex_list {
                            forced[index] = Some((0.0, 0.0));
                        }
                    }
                }
                for (flow_at, &position) in flow_positions.iter().enumerate() {
                    let child_scale = scale * self.own_scale(flow[flow_at]);
                    let size = scaled_sizes[flow_at];
                    if (size[0] - resolved[position].w * child_scale).abs() > 1e-9 || (size[1] - resolved[position].h * child_scale).abs() > 1e-9 {
                        forced[position] = Some((size[0], size[1]));
                    }
                }
                self.l.list_scratch.sizes = scaled_sizes;
                self.l.list_scratch.offsets = list_offsets;
                if !measure {
                    self.set_vector(layout, n.AbsoluteContentSize, content_width, content_height);
                }
                extent_x = content_width / scale;
                extent_y = content_height / scale;
            }
            self.l.give_vec(flow);
            self.l.scratch.indices.push(flow_positions);
        }

        let has_layout = layout.is_some();
        for (index, &child) in items.iter().enumerate() {
            let entry = resolved[index];
            let (mut width, mut height) = (entry.w, entry.h);
            let child_scale = self.own_scale(child);
            let forced_size = forced[index];
            if let Some(size) = forced_size {
                width = size.0 / (scale * child_scale);
                height = size.1 / (scale * child_scale);
            }
            let offset = offsets[index];
            let (mut base_w, mut base_h) = (entry.base_w, entry.base_h);
            if forced_size.is_some() {
                base_w = width;
                base_h = height;
            }
            // (layout offsets are scaled px from the content origin; PlaceChild takes local units)
            let mut child_record = self.place_child(child, record, width, height, offset.map(|o| (o.0 / scale, o.1 / scale)), Some((base_w, base_h)));
            if let Some((measure_w, measure_h)) = entry.measure {
                child_record.measure_w = Some(measure_w * child_record.scale);
                child_record.measure_h = Some(measure_h * child_record.scale);
            }
            if let Some(rects) = &cell_rects {
                child_record.cells = rects[index].clone();
            }

            if !has_layout && self.visible(child) {
                let right = (child_record.x + child_record.w - record.content.x) / scale;
                let bottom = (child_record.y + child_record.h - record.content.y) / scale;
                extent_x = lmax(extent_x, right);
                extent_y = lmax(extent_y, bottom);
            }

            if !measure {
                self.store(child, child_record, if forced_size.is_some() { None } else { text_boxes[index] });
            } else if let Some(collect) = collect.as_mut() {
                collect(self, child, &child_record);
            }
        }
        self.l.give_vec(items);
        self.l.scratch.resolved.push(resolved);
        self.l.scratch.boxes.push(text_boxes);
        self.l.scratch.pairs.push(offsets);
        self.l.scratch.pairs.push(forced);
        (extent_x, extent_y)
    }

    // Records (incremental layout)

    /// Gives `record` a new version (and reports it changed) unless it shows the same as `old`;
    /// then the old record stays (stable identity) with the fields the comparison skips.
    fn version(&mut self, id: InstanceId, mut record: Record, old: Option<Record>) {
        if let Some(mut old) = old {
            if old.same_output(&record) {
                old.generation = record.generation;
                old.place = record.place;
                old.canvas_place = record.canvas_place;
                self.l.records.set(id, old);
                return;
            }
        }
        self.l.record_version += 1;
        record.version = self.l.record_version;
        self.l.changed.insert(id);
        self.l.records.set(id, record);
    }

    fn bump(&mut self, id: InstanceId) {
        self.l.record_version += 1;
        let version = self.l.record_version;
        if let Some(record) = self.l.records.get_mut(id) {
            record.version = version;
        }
        self.l.changed.insert(id);
    }

    /// Finishes a record that moved in place: computed properties, a new version, and the
    /// descendants moved along.
    fn moved(&mut self, id: InstanceId) {
        let Some(record) = self.l.records.get(id) else { return };
        let (absolute_x, absolute_y) = absolute_position(record);
        let rotation = record.rotation;
        let n = self.l.ids.n;
        let n_rotation = n.AbsoluteRotation;
        self.set_absolute_position(id, absolute_x, absolute_y);
        self.set_number(id, n_rotation, rotation);
        self.bump(id);
        self.shift_children(id);
    }

    /// Re-places the children of a moved record in place from their stored placement terms.
    fn shift_children(&mut self, id: InstanceId) {
        let Some(record) = self.l.records.get_mut(id) else { return };
        let (origin_x, origin_y) = if record.canvas_place.is_some() { locate_canvas(record) } else { (record.content.x, record.content.y) };
        let parent = ParentPlace::of(record);
        if self.state.dom.at(id).children.is_empty() {
            return;
        }
        let children = self.gui_children(id);
        for &child in &children {
            let visible = self.visible(child);
            let Some(child_record) = self.l.records.get_mut(child) else { continue };
            if child_record.place.is_some() {
                locate(child_record, origin_x, origin_y, parent, visible);
                self.moved(child);
            }
        }
        self.l.give_vec(children);
    }

    /// Applies a ScrollingFrame's current CanvasPosition to its laid-out record (clamped to the
    /// limits of its last layout); returns whether the canvas moved.
    fn apply_scroll(&mut self, id: InstanceId) -> bool {
        self.l.unmark_scrolled(id);
        let n = self.l.ids.n;
        let n_canvas_position = n.CanvasPosition;
        let limits = self.l.records.get(id).and_then(|r| r.scroll_max).unwrap_or([0.0, 0.0]);
        let position = self.vec2(id, n_canvas_position).unwrap_or((0.0, 0.0));
        let scroll_x = clamp(position.0, 0.0, limits[0]);
        let scroll_y = clamp(position.1, 0.0, limits[1]);
        if scroll_x != position.0 || scroll_y != position.1 {
            self.set_vector(id, n_canvas_position, scroll_x, scroll_y);
        }
        let dpi = self.l.pixel_scale;
        let offset_x = (scroll_x * dpi + 0.5).floor() / dpi;
        let offset_y = (scroll_y * dpi + 0.5).floor() / dpi;
        let Some(record) = self.l.records.get_mut(id) else { return false };
        let place = record.canvas_place.unwrap();
        if place[2] == offset_x && place[3] == offset_y {
            return false;
        }
        record.canvas_place = Some([place[0], place[1], offset_x, offset_y, place[4], place[5]]);
        if let Some(bars) = record.scroll_bars.as_mut() {
            bars.scroll_x = offset_x;
            bars.scroll_y = offset_y;
        }
        true
    }

    /// Scrolls ScrollingFrames whose CanvasPosition changed and whose layout is otherwise current:
    /// the canvas subtree only moves.
    pub(crate) fn process_scrolls(&mut self) {
        if self.l.scrolled.is_empty() {
            return;
        }
        // Profile: counted, and its time is layout time (Layout.ProfileTime), like a pass's
        crate::engine::profile::count("scrollLayoutPasses", 1);
        let started = std::time::Instant::now();
        self.process_pending_scrolls();
        self.l.profile_time += super::elapsed(started);
    }

    fn process_pending_scrolls(&mut self) {
        let pending: Vec<InstanceId> = self.l.scrolled.clone();
        for id in pending {
            let current = match self.l.records.get(id) {
                Some(record) => {
                    record.canvas_place.is_some()
                        && record.scroll_max.is_some()
                        && record.generation == self.l.generation
                        && self.l.stored_stamps.get(id).copied() == Some(self.l.stamps.get(id).copied().unwrap_or(0))
                }
                None => false,
            };
            if !current {
                self.l.unmark_scrolled(id);
                continue;
            }
            if self.apply_scroll(id) {
                self.bump(id);
                self.shift_children(id);
            }
        }
    }

    /// Stores a record, writes computed properties, handles text / scrolling specifics, recurses.
    /// When nothing in the subtree changed since it was stored, an identical record is skipped and a
    /// moved one (SameShape) just moves the subtree. `text_box`: for AutomaticSize X text, the local
    /// width its text was measured in.
    pub(crate) fn store(&mut self, id: InstanceId, mut record: Record, text_box: Option<f64>) {
        record.text_box = text_box;
        let stamp = self.l.stamps.get(id).copied().unwrap_or(0);
        let clean = match self.l.records.get(id) {
            Some(old) => self.l.stored_stamps.get(id).copied() == Some(stamp) && old.generation == self.l.generation,
            None => false,
        };
        if clean {
            let old = self.l.records.get(id).unwrap();
            if old.same_inputs(&record) {
                self.l.stats.skipped += 1;
                cache_counts::hit(Cache::Records);
                return;
            }
            if old.same_shape(&record) {
                // Only moved: keep the record (and everything computed for it), take the new place.
                self.l.stats.moved += 1;
                cache_counts::hit(Cache::Records);
                self.l.stored_stamps.take(id);
                let kept = self.l.records.get_mut(id).unwrap();
                kept.x = record.x;
                kept.y = record.y;
                kept.content.x = record.content.x;
                kept.content.y = record.content.y;
                kept.outer = record.outer;
                kept.transform = record.transform;
                kept.rotation = record.rotation;
                kept.visible = record.visible;
                kept.place = record.place;
                self.moved(id);
                self.l.stored_stamps.set(id, stamp);
                return;
            }
        }

        self.l.stats.stored += 1;
        cache_counts::miss(Cache::Records);
        self.l.stored_stamps.take(id);
        let old = self.l.records.take(id);
        let n = self.l.ids.n;
        let (n_size, n_rotation, n_bounds, n_fits, n_auto) = (n.AbsoluteSize, n.AbsoluteRotation, n.TextBounds, n.TextFits, n.AutomaticSize);
        let (absolute_x, absolute_y) = absolute_position(&record);
        self.set_absolute_position(id, absolute_x, absolute_y);
        self.set_vector(id, n_size, record.w, record.h);
        self.set_number(id, n_rotation, record.rotation);

        if self.is_text_object(id) {
            let mut info = None;
            if clean {
                // Same text inputs as last time: keep its layout.
                let old = old.as_ref().unwrap();
                if let Some(previous) = &old.text {
                    if old.content.w == record.content.w && old.content.h == record.content.h && old.scale == record.scale && old.text_box == text_box {
                        info = Some(previous.clone());
                    }
                }
            }
            let info = match info {
                Some(info) => info,
                None => {
                    self.l.stats.texts += 1;
                    // Measured (round 3, layout-probe-autotext2 Y rows): an AutomaticSize text box
                    // narrower than its padding keeps the text its measure laid out 0 wide.
                    let automatic = self.code(id, n_auto);
                    let negative = automatic != E::NONE && automatic != E::EMPTY;
                    let text_w = match (record.local_space_w, record.local_w) {
                        (Some(space), Some(local)) if negative && space < local => space,
                        _ => record.content.w / record.scale,
                    };
                    let text_h = match (record.local_space_h, record.local_h) {
                        (Some(space), Some(local)) if negative && space < local => space,
                        _ => record.content.h / record.scale,
                    };
                    self.layout_text(id, text_box.unwrap_or(text_w), text_h, record.scale, false)
                }
            };
            self.set_vector(id, n_bounds, info.absolute_x, info.absolute_y);
            let fits = info.layout.fits
                && info.bounds_x <= record.content.w / record.scale + 1e-6
                && info.bounds_y <= record.content.h / record.scale + 1e-6;
            self.set_bool(id, n_fits, fits);
            record.text = Some(info);
        }

        if self.class(id) == self.l.ids.c.ScrollingFrame {
            self.layout_scrolling(id, &mut record);
        } else {
            self.layout_children(id, &record, false, None);
        }

        self.version(id, record, old);
        // Recorded last with the stamp it was laid out for.
        self.l.stored_stamps.set(id, stamp);
    }

    /// ScrollingFrame: canvas size (incl. AutomaticCanvasSize), scroll bar insets, canvas position.
    fn layout_scrolling(&mut self, id: InstanceId, record: &mut Record) {
        self.l.unmark_scrolled(id);
        let n = self.l.ids.n;
        let scale = record.scale;
        let thickness = self.num(id, n.ScrollBarThickness).unwrap_or(12.0) * scale;
        // Measured (layout-probe-scrollborder): a visible bar takes its thickness plus
        // BorderSizePixel off the window.
        let border = lmax(self.num(id, n.BorderSizePixel).unwrap_or(0.0), 0.0) * scale;
        let gutter = thickness + border;
        let canvas_size = self.udim2(id, n.CanvasSize);
        let direction = self.code(id, n.ScrollingDirection);
        let auto_canvas = self.code(id, n.AutomaticCanvasSize);
        let vertical_inset = self.code(id, n.VerticalScrollBarInset);
        let horizontal_inset = self.code(id, n.HorizontalScrollBarInset);
        let inset_x = vertical_inset != E::NONE && vertical_inset != E::EMPTY;
        let inset_y = horizontal_inset != E::NONE && horizontal_inset != E::EMPTY;
        let left_bar = self.code(id, n.VerticalScrollBarPosition) == E::LEFT;
        let auto_x = auto_canvas == E::X || auto_canvas == E::XY;
        let auto_y = auto_canvas == E::Y || auto_canvas == E::XY;
        let n_canvas_position = n.CanvasPosition;
        let n_canvas = n.AbsoluteCanvasSize;
        let n_window = n.AbsoluteWindowSize;

        // Measured: CanvasSize scale is relative to the ScrollingFrame's parent content size;
        // offsets scale with UIScale and are truncated to whole px.
        let (frame_w, frame_h) = (record.w, record.h);
        let parent_w = record.parent_width.unwrap_or(frame_w);
        let parent_h = record.parent_height.unwrap_or(frame_h);
        let raw_w = canvas_size[0] * parent_w + scaled_offset(canvas_size[1], scale);
        let raw_h = canvas_size[2] * parent_h + scaled_offset(canvas_size[3], scale);

        let has_flow = self.layout_object(id).is_some();

        let (mut window_w, mut window_h) = (frame_w, frame_h);
        let (mut min_w, mut min_h);
        let (mut content_w, mut content_h) = (raw_w, raw_h);
        let (mut need_vertical, mut need_horizontal) = (false, false);
        let (mut show_vertical, mut show_horizontal) = (false, false);
        let mut carried: Option<usize> = None;
        for _ in 0..4 {
            // Measured: the canvas is at least the frame size, or the window size on an inset axis.
            min_w = if inset_x { window_w } else { frame_w };
            min_h = if inset_y { window_h } else { frame_h };
            content_w = raw_w;
            content_h = raw_h;
            if auto_x || auto_y {
                // Measured (layout-probe-autocanvas*): an automatic axis is max(CanvasSize, content
                // extent + UIPadding offsets) (see the Luau).
                // Measured (round 9, layout-probe-autocanvas6/7): on an inset automatic axis whose
                // cross bar is shown the measure works in the frame less the bar's thickness (not its
                // BorderSizePixel, not the window), the UIPadding still resolved against the frame,
                // and the canvas is that extent plus the thickness: max(CanvasSize, extent + UIPadding
                // offsets + thickness).
                let cut_w = if inset_x && window_w < frame_w { thickness } else { 0.0 };
                let cut_h = if inset_y && window_h < frame_h { thickness } else { 0.0 };
                let measure_w = if auto_x { frame_w - cut_w } else { min_w };
                let measure_h = if auto_y { frame_h - cut_h } else { min_h };
                let (canvas_base_w, canvas_base_h) = (lmax(raw_w, measure_w), lmax(raw_h, measure_h));
                let (frame_left, frame_right, frame_top, frame_bottom) = self.padding(id, frame_w / scale, frame_h / scale);
                let (canvas_left, canvas_right, canvas_top, canvas_bottom) = self.padding(id, canvas_base_w / scale, canvas_base_h / scale);
                let (offset_left, offset_right, offset_top, offset_bottom) = self.padding_offsets(id);
                let mut probe = record.clone();
                probe.sizing_w = None;
                probe.sizing_h = None;
                probe.available = None;
                probe.available_h = None;
                probe.measure_w = None;
                probe.measure_h = None;
                probe.local_w = None;
                probe.local_h = None;
                probe.local_sizing_w = None;
                probe.local_sizing_h = None;
                probe.local_outer_w = None;
                probe.local_outer_h = None;
                probe.local_space_w = None;
                probe.local_space_h = None;
                let padded_w = lmax(canvas_base_w - (canvas_left + canvas_right) * scale, 0.0);
                let padded_h = lmax(canvas_base_h - (canvas_top + canvas_bottom) * scale, 0.0);
                let (box_w, box_h);
                if has_flow {
                    box_w = if auto_x { lmax(measure_w - (frame_left + frame_right) * scale, 0.0) } else { padded_w };
                    box_h = if auto_y { lmax(measure_h - (frame_top + frame_bottom) * scale, 0.0) } else { padded_h };
                    // Measured (round 9, layout-probe-autocanvas4/5/6, a leaderboard window): the
                    // list Padding's scale resolves against the frame's own Size UDim applied to that
                    // measure box, Size.Scale * box + Size.Offset (float32, local units), not against
                    // the frame: a 0.912-scale frame 214.72 px high measures its 0.03 gaps against
                    // 195.83 (canvas 323.499, the rows are still placed 6.44 apart). With an offset
                    // Size this is the frame, as the earlier probes had it.
                    let size = self.udim2(id, n.Size);
                    let basis = |scale_part: f64, offset: f64, measured: f64| f32r(f32r(scale_part * f32r(measured / scale)) + offset) * scale;
                    probe.gap_w = if auto_x { Some(basis(size[0], size[1], box_w)) } else { None };
                    probe.gap_h = if auto_y { Some(basis(size[2], size[3], box_h)) } else { None };
                } else {
                    box_w = if auto_x { canvas_base_w } else { padded_w };
                    box_h = if auto_y { canvas_base_h } else { padded_h };
                    // Measured (round 3): along a non-automatic axis the measure sizes plain children
                    // against the padded canvas; only the final placement uses the frame.
                    probe.size_w = if auto_x { Some(measure_w) } else { None };
                    probe.size_h = if auto_y { Some(measure_h) } else { None };
                }
                // (an automatic canvas axis leaves its content unbounded room)
                probe.available_h = if auto_y { Some(INF) } else { None };
                probe.content = Rect { x: 0.0, y: 0.0, w: box_w, h: box_h };
                // (kept per frame across passes: a memoized object's descendants are not measured again)
                let captured = match self.l.carry_stores.get(id) {
                    Some(&index) => index,
                    None => {
                        self.l.carry_maps.push(FxHashMap::default());
                        let index = self.l.carry_maps.len() - 1;
                        self.l.carry_stores.set(id, index);
                        index
                    }
                };
                let outer_capture = self.l.carry_capture;
                self.l.carry_capture = Some(captured);
                let (extent_x, extent_y) = self.layout_children(id, &probe, true, None);
                self.l.carry_capture = outer_capture;
                carried = Some(captured);
                if auto_x {
                    content_w = lmax(raw_w, (extent_x + offset_left + offset_right) * scale + cut_w);
                }
                if auto_y {
                    content_h = lmax(raw_h, (extent_y + offset_top + offset_bottom) * scale + cut_h);
                }
            }

            // Measured: bars are decided on the unclamped canvas (canvas > frame); a visible bar makes
            // the other axis need its bar when canvas >= frame - thickness (non-strict).
            need_vertical = content_h > frame_h + 1e-6;
            need_horizontal = content_w > frame_w + 1e-6;
            for _ in 0..2 {
                need_vertical = need_vertical || (need_horizontal && content_h >= frame_h - gutter - 1e-6);
                need_horizontal = need_horizontal || (need_vertical && content_w >= frame_w - gutter - 1e-6);
            }
            show_vertical = need_vertical && direction != E::X;
            show_horizontal = need_horizontal && direction != E::Y;

            let new_w = frame_w - if show_vertical { gutter } else { 0.0 };
            let new_h = frame_h - if show_horizontal { gutter } else { 0.0 };
            if new_w == window_w && new_h == window_h {
                break;
            }
            window_w = new_w;
            window_h = new_h;
        }
        // Measured (round 9, layout-probe-autocanvas7): the final placement on an automatic axis uses
        // the frame even when that axis is inset (list gaps, item scale sizes, anchored children).
        min_w = if inset_x && !auto_x { window_w } else { frame_w };
        min_h = if inset_y && !auto_y { window_h } else { frame_h };

        let canvas_w = lmax(content_w, if inset_x { window_w } else { frame_w });
        let canvas_h = lmax(content_h, if inset_y { window_h } else { frame_h });

        // Measured: an axis scrolls when its bar is needed; with a single ScrollingDirection the other
        // axis scrolls only when the canvas exceeds the frame itself.
        let scrollable_x = if direction == E::Y { content_w > frame_w + 1e-6 } else { need_horizontal };
        let scrollable_y = if direction == E::X { content_h > frame_h + 1e-6 } else { need_vertical };
        let max_x = if scrollable_x { lmax(canvas_w - window_w, 0.0) } else { 0.0 };
        let max_y = if scrollable_y { lmax(canvas_h - window_h, 0.0) } else { 0.0 };
        record.scroll_max = Some([max_x, max_y]);
        let position = self.vec2(id, n_canvas_position).unwrap_or((0.0, 0.0));
        let scroll_x = clamp(position.0, 0.0, max_x);
        let scroll_y = clamp(position.1, 0.0, max_y);
        if scroll_x != position.0 || scroll_y != position.1 {
            // (Layout.Writing: the invalidation hook ignores this write)
            self.set_vector(id, n_canvas_position, scroll_x, scroll_y);
        }

        self.set_vector(id, n_canvas, canvas_w, canvas_h);
        self.set_vector(id, n_window, window_w, window_h);

        // Measured: the canvas offset is rounded to whole physical pixels (half up). A left bar shifts
        // the canvas only when the vertical axis is inset; the content clip always excludes bars.
        let dpi = self.l.pixel_scale;
        let offset_x = (scroll_x * dpi + 0.5).floor() / dpi;
        let offset_y = (scroll_y * dpi + 0.5).floor() / dpi;
        let left_width = if left_bar && show_vertical { gutter } else { 0.0 };
        let canvas_shift = if inset_x { left_width } else { 0.0 };

        // Measured (layout-probe-autocanvas*): UIPadding scale resolves against the canvas.
        let (pad_left, pad_right, pad_top, pad_bottom) = self.padding(id, canvas_w / scale, canvas_h / scale);
        record.canvas_place = Some([left_width, canvas_shift, offset_x, offset_y, pad_left * scale, pad_top * scale]);
        record.window = Some(Rect { x: 0.0, y: 0.0, w: window_w, h: window_h });
        record.canvas = Some(Rect { x: 0.0, y: 0.0, w: canvas_w, h: canvas_h });
        record.scroll_bars = Some(ScrollBars {
            vertical: show_vertical,
            horizontal: show_horizontal,
            thickness,
            border,
            scroll_x: offset_x,
            scroll_y: offset_y,
            left: left_bar,
        });
        let (origin_x, origin_y) = locate_canvas(record);

        let mut canvas_record = record.clone();
        canvas_record.sizing_w = None;
        canvas_record.sizing_h = None;
        canvas_record.available = None;
        canvas_record.measure_w = None;
        canvas_record.measure_h = None;
        canvas_record.local_w = None;
        canvas_record.local_h = None;
        canvas_record.local_sizing_w = None;
        canvas_record.local_sizing_h = None;
        canvas_record.local_outer_w = None;
        canvas_record.local_outer_h = None;
        canvas_record.local_space_w = None;
        canvas_record.local_space_h = None;
        canvas_record.content = Rect {
            x: origin_x,
            y: origin_y,
            w: if auto_x { min_w } else { lmax(canvas_w - (pad_left + pad_right) * scale, 0.0) },
            h: if auto_y { min_h } else { lmax(canvas_h - (pad_top + pad_bottom) * scale, 0.0) },
        };
        if (auto_x || auto_y) && !has_flow {
            canvas_record.size_w = Some(min_w);
            canvas_record.size_h = Some(min_h);
        }
        if auto_y {
            canvas_record.available_h = Some(INF);
        }
        // Measured (round 3, a clipped list of AutomaticSize cards): the automatic sizes the canvas measure gave its
        // descendants stay for the final placement.
        let (outer_apply, outer_capture) = (self.l.carry_apply, self.l.carry_capture);
        self.l.carry_apply = carried;
        self.l.carry_capture = None;
        self.layout_children(id, &canvas_record, false, None);
        self.l.carry_apply = outer_apply;
        self.l.carry_capture = outer_capture;
    }

    // Roots

    fn container(&self, id: InstanceId) -> Option<InstanceId> {
        let c = self.l.ids.c;
        let mut current = self.parent(id);
        while let Some(node) = current {
            let class = self.class(node);
            if class == c.PlayerGui || class == c.CoreGui || class == c.StarterGui || class == c.PluginGui || class == c.DockWidgetPluginGui {
                return Some(node);
            }
            current = self.parent(node);
        }
        None
    }

    pub(crate) fn is_displayed(&self, root: InstanceId) -> bool {
        match self.container(root) {
            None => false,
            Some(holder) => {
                if self.class(holder) == self.l.ids.c.StarterGui {
                    self.l.edit_mode
                } else {
                    true
                }
            }
        }
    }

    pub(crate) fn layout_root(&mut self, root: InstanceId) {
        let stamp = self.l.stamps.get(root).copied().unwrap_or(0);
        let class = self.class(root);
        let info = self.reflection.class(class);
        let c = self.l.ids.c;
        let n = self.l.ids.n;

        if info.is_a(c.ScreenGui) || info.is_a(c.LayerCollector) {
            let (x, y, width, height);
            if class == c.ScreenGui && self.is_displayed(root) {
                let below_topbar = match self.get(root, n.ScreenInsets) {
                    Some(value) => value.as_enum() == self.l.ids.core_ui_safe_insets,
                    None => true,
                };
                let inset = if below_topbar { self.l.top_inset } else { 0.0 };
                x = 0.0;
                y = inset;
                width = self.l.viewport_width;
                height = self.l.viewport_height - inset;
            } else if class == c.DockWidgetPluginGui || class == c.PluginGui {
                // `--plugin` dock widgets (story.rs install_plugin): docked over the whole viewport
                x = 0.0;
                y = 0.0;
                width = self.l.viewport_width;
                height = self.l.viewport_height;
            } else if class == c.SurfaceGui {
                let canvas = self.vec2(root, n.CanvasSize).unwrap_or((800.0, 600.0));
                x = 0.0;
                y = 0.0;
                width = canvas.0;
                height = canvas.1;
            } else {
                x = 0.0;
                y = 0.0;
                width = 800.0;
                height = 600.0;
            }
            let record = Record {
                x,
                y,
                w: width,
                h: height,
                rotation: 0.0,
                scale: 1.0,
                content: Rect { x, y, w: width, h: height },
                visible: self.not_false(root, n.Enabled),
                generation: self.l.generation,
                ..Record::default()
            };
            let old = self.l.records.take(root);
            let n_size = n.AbsoluteSize;
            self.set_absolute_position(root, x, y);
            self.set_vector(root, n_size, width, height);
            self.layout_children(root, &record, false, None);
            self.version(root, record, old);
            self.l.root_stamps.set(root, stamp);
            self.l.root_generations.set(root, self.l.generation);
            return;
        }

        if info.is_a(c.GuiObject) {
            let own = self.own_scale(root);
            let sized = self.resolve_size(root, SizeArgs {
                parent_w: 0.0,
                parent_h: 0.0,
                scale: own,
                available: None,
                measure_parent_w: None,
                measure_parent_h: None,
                available_h: None,
                full_w: None,
                full_h: None,
            });
            let parent = Record { scale: 1.0, visible: true, generation: self.l.generation, ..Record::default() };
            let record = self.place_child(root, &parent, sized.w, sized.h, None, Some((sized.base_w, sized.base_h)));
            self.store(root, record, sized.text_box);
        }
        self.l.root_stamps.set(root, stamp);
        self.l.root_generations.set(root, self.l.generation);
    }

    pub(crate) fn is_laid_out(&self, root: InstanceId) -> bool {
        self.l.root_generations.get(root).copied() == Some(self.l.generation)
            && self.l.root_stamps.get(root).copied() == Some(self.l.stamps.get(root).copied().unwrap_or(0))
    }

    pub(crate) fn run(&mut self, root: InstanceId) {
        crate::engine::profile::count("layoutPasses", 1);
        let _scope = crate::microprofile::scope("Layout pass");
        let started = std::time::Instant::now();
        self.layout_root(root);
        let elapsed = super::elapsed(started);
        self.l.stats.time += elapsed;
        self.l.profile_time += elapsed;
        self.l.stats.passes += 1;
    }

    /// DisplayedRoots: LayerCollectors under CoreGui / StarterGui (edit mode) / the players'
    /// PlayerGuis, by DisplayOrder then creation.
    pub(crate) fn displayed_roots(&self, game: InstanceId) -> Vec<InstanceId> {
        let c = self.l.ids.c;
        let mut result = Vec::new();
        fn collect(pass: &Pass, holder: InstanceId, out: &mut Vec<InstanceId>) {
            for &child in &pass.state.dom.at(holder).children {
                if pass.is_a(child, pass.l.ids.c.LayerCollector) {
                    out.push(child);
                } else if pass.class(child) == pass.l.ids.c.Folder {
                    collect(pass, child, out);
                }
            }
        }
        for &child in &self.state.dom.at(game).children {
            let class = self.class(child);
            if class == c.CoreGui || (class == c.StarterGui && self.l.edit_mode) {
                collect(self, child, &mut result);
            } else if class == c.Players {
                for &player in &self.state.dom.at(child).children {
                    for &holder in &self.state.dom.at(player).children {
                        if self.class(holder) == c.PlayerGui {
                            collect(self, holder, &mut result);
                        }
                    }
                }
            }
        }
        let order = self.l.ids.n.DisplayOrder;
        result.sort_by(|a, b| {
            let order_a = self.num(*a, order).unwrap_or(0.0);
            let order_b = self.num(*b, order).unwrap_or(0.0);
            if order_a != order_b {
                if order_a < order_b {
                    return std::cmp::Ordering::Less;
                }
                if order_b < order_a {
                    return std::cmp::Ordering::Greater;
                }
            }
            self.state.dom.at(*a).serial.cmp(&self.state.dom.at(*b).serial)
        });
        result
    }

    /// `Layout.Pages`: pages of a UIPageLayout in order.
    pub(crate) fn pages(&mut self, layout: InstanceId) -> Vec<InstanceId> {
        let Some(parent) = self.parent(layout) else { return Vec::new() };
        let children = self.gui_children(parent);
        let mut pages: Vec<InstanceId> = children.iter().copied().filter(|&child| self.visible(child) && self.parent(child) == Some(parent)).collect();
        self.l.give_vec(children);
        self.sort_items(layout, &mut pages);
        pages
    }

    /// ClampCanvasPosition's clamp (after the frame was laid out).
    pub(crate) fn clamp_canvas(&self, id: InstanceId, x: f64, y: f64) -> Option<(f64, f64)> {
        let limits = self.l.records.get(id)?.scroll_max?;
        Some((clamp(x, 0.0, limits[0]), clamp(y, 0.0, limits[1])))
    }

    pub(crate) fn find_root(&self, id: InstanceId) -> InstanceId {
        self.l.find_root(self.state, self.reflection, id)
    }
}

fn set_carry(carry: &mut Vec<(InstanceId, CarryEntry)>, child: InstanceId, entry: CarryEntry) {
    match carry.iter_mut().find(|(other, _)| *other == child) {
        Some(existing) => existing.1 = entry,
        None => carry.push((child, entry)),
    }
}
