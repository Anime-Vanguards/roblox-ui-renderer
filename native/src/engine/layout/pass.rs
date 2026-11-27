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

/// `aligned_start` in float32 local units, as a client aligns list and grid content (fixtures/layout-f32-lists2.luau,
/// layout-f32-grids.luau): centred f32(f32(available - used) x 0.5), at the far end f32(available - used).
fn aligned_start_f32(alignment: u8, available: f64, used: f64) -> f64 {
    if alignment == E::CENTER {
        f32r(f32r(available - used) * 0.5)
    } else if alignment == E::BOTTOM || alignment == E::RIGHT {
        f32r(available - used)
    } else {
        0.0
    }
}

/// A UIGridLayout's cells in its container's local units (`Pass::grid_layout`).
pub(crate) struct GridPlan {
    /// each flow item's centre from the container's content origin
    centres: Vec<(f64, f64)>,
    /// the items' size: the stride less the padding
    size: (f64, f64),
    /// AbsoluteContentSize before the container's scale
    extent: (f64, f64),
    /// AbsoluteCellSize (a client reports it in local units, without UIScale)
    cell: (f64, f64),
    /// AbsoluteCellCount
    count: (f64, f64),
}

#[inline]
fn number_key(value: f64) -> u64 {
    if value.is_nan() { f64::NAN.to_bits() } else { value.to_bits() }
}

/// Whether a text object shows its PlaceholderText: an empty TextBox.
fn shows_placeholder(state: &State, ids: &Ids, reflection: &Reflection, id: InstanceId) -> bool {
    let empty = match state.dom.raw_get(id, ids.n.Text) {
        Some(Value::String(s)) => s.as_bytes().is_empty(),
        _ => true,
    };
    empty && reflection.class(state.dom.at(id).class).is_a(ids.c.TextBox)
}

/// RichText as the text is laid out. Measured (fixtures/text-probe-placeholder.luau, Roblox 0.747 client): a
/// placeholder is plain text under RichText too, its tags shown and measured ("<b>Search</b> for <i>items</i>" is 257 px
/// wide at TextSize 24, where the rich text would be 134).
fn lays_out_rich(state: &State, ids: &Ids, reflection: &Reflection, id: InstanceId) -> bool {
    matches!(state.dom.raw_get(id, ids.n.RichText), Some(Value::Bool(true))) && !shows_placeholder(state, ids, reflection, id)
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
        rich: lays_out_rich(state, ids, reflection, id),
        family: std::str::from_utf8(family).unwrap_or(""),
        weight,
        style: if italic { "italic" } else { "normal" },
        wrapped: is_true(n.TextWrapped),
        scaled: is_true(n.TextScaled),
        text_size: number(id, n.TextSize).unwrap_or(14.0),
        line_height: number(id, n.LineHeight).unwrap_or(1.0),
        truncate,
        constraint,
        // Measured (fixtures/text-probe-boxwrap.luau, 0.747 client): a TextBox wraps its plain text (its placeholder too)
        // keeping only the whitespace that fits on a line; rich text wraps as in a label
        box_wrap: reflection.class(state.dom.at(id).class).is_a(ids.c.TextBox) && !lays_out_rich(state, ids, reflection, id),
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
    grow: Option<(Option<f64>, Option<f64>)>,
    basis: Option<(Option<f64>, Option<f64>)>,
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
            rich: lays_out_rich(self.state, &self.l.ids, self.reflection, id),
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

    /// The UISizeConstraint children's (min, max) vectors. Measured (fixtures/layout-autosize-constraint.luau,
    /// Studio 0.747): a constraint whose MinSize exceeds its MaxSize on either axis constrains neither axis
    /// (MinSize (40, 25) with MaxSize (30, 100) leaves a 10 x 10 frame 10 x 10), whichever was set first.
    fn size_constraints(&self, id: InstanceId, mut visit: impl FnMut((f64, f64), (f64, f64))) {
        let class = self.l.ids.c.UISizeConstraint;
        let n = self.l.ids.n;
        for &child in &self.state.dom.at(id).children {
            if self.is_a(child, class) {
                let min = self.vec2(child, n.MinSize).unwrap_or((0.0, 0.0));
                let max = self.vec2(child, n.MaxSize).unwrap_or((INF, INF));
                if min.0 > max.0 || min.1 > max.1 {
                    continue;
                }
                visit(min, max);
            }
        }
    }

    fn has_size_constraint(&self, id: InstanceId) -> bool {
        let mut any = false;
        self.size_constraints(id, |_, _| any = true);
        any
    }

    /// The tightest UISizeConstraint bounds on the width (min, max).
    fn size_limits(&self, id: InstanceId) -> (f64, f64) {
        let (mut low, mut high) = (0.0, INF);
        self.size_constraints(id, |min, max| {
            low = lmax(low, min.0);
            high = lmin(high, max.0);
        });
        (low, high)
    }

    fn apply_size_constraint(&self, id: InstanceId, mut width: f64, mut height: f64) -> (f64, f64) {
        self.size_constraints(id, |min, max| {
            width = clamp(width, min.0, max.0);
            height = clamp(height, min.1, max.1);
        });
        (width, height)
    }

    /// The AspectRatio of the object's UIAspectRatioConstraint, if it has one that applies.
    fn aspect_ratio(&self, id: InstanceId) -> Option<f64> {
        let constraint = self.first_child_of(id, self.l.ids.c.UIAspectRatioConstraint)?;
        self.num(constraint, self.l.ids.n.AspectRatio).filter(|ratio| *ratio > 0.0)
    }

    /// Whether the object's UIAspectRatioConstraint is of AspectType ScaleWithParentSize.
    fn aspect_scales_with_parent(&self, id: InstanceId) -> bool {
        self.first_child_of(id, self.l.ids.c.UIAspectRatioConstraint)
            .is_some_and(|constraint| self.code(constraint, self.l.ids.n.AspectType) == E::SCALE_WITH_PARENT_SIZE)
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
    /// `leaf_axes`: the axes (automatic ones) on which a container's childless descendants size against its Size.
    fn descendant_extent(&mut self, id: InstanceId, record: &Record, zero_x: bool, zero_y: bool, leaf_axes: (bool, bool)) -> (f64, f64) {
        let origin = (record.content.x, record.content.y, record.scale);
        let (max_x, max_y) = self.extent_visit(id, id, record, None, zero_x, zero_y, origin, leaf_axes);
        (if zero_x { max_x } else { 0.0 }, if zero_y { max_y } else { 0.0 })
    }

    /// DescendantExtent's Visit: the far edges of what `parent` holds (its own far padding included
    /// below the top). Below the top, `leaves` is the record the parent's childless children size
    /// against; the children that hold children size against `parent_record`.
    #[allow(clippy::too_many_arguments)]
    fn extent_visit(
        &mut self,
        top_id: InstanceId,
        parent: InstanceId,
        parent_record: &Record,
        leaves: Option<&Record>,
        axis_x: bool,
        axis_y: bool,
        origin: (f64, f64, f64),
        leaf_axes: (bool, bool),
    ) -> (f64, f64) {
        let (origin_x, origin_y, scale) = origin;
        let top = parent == top_id;
        let start_x = if top { 0.0 } else { (parent_record.content.x - origin_x) / scale };
        let start_y = if top { 0.0 } else { (parent_record.content.y - origin_y) / scale };
        let (mut edge_x, mut edge_y) = (start_x, start_y);
        let mut holds = false;
        {
            // (`only`: Some(true) visits the childless children, Some(false) the others, None all)
            let mut visit = |pass: &mut Pass, child: InstanceId, child_record: &Record, only: Option<bool>| {
                if !pass.visible(child) {
                    return;
                }
                let nested = pass.has_gui_children(child);
                if only.is_some_and(|leaf| leaf == nested) {
                    return;
                }
                holds = true;
                let scrolling = pass.class(child) == pass.l.ids.c.ScrollingFrame;
                let (_, auto_x, auto_y) = pass.automatic_axes(child);
                let child_x = axis_x && !scrolling && !auto_x;
                let child_y = axis_y && !scrolling && !auto_y;
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
                // Measured (fixtures/layout-autonested-scalepos.luau, a 0.747 client): inside a plain container the
                // children that hold children size against its measured content box, but its childless children
                // size against its own Size resolved against that box, as an automatic frame's children do in its
                // first pass, and the edges count its padding's offsets only. A (0.5, 0.5) frame measured 95 x 77.5
                // holding a (1, 1) frame counts 38.75 high, not 77.5; (0.8, 0.8) frames three deep holding a (1, 1)
                // frame count 0.8^4 of the space; a 60 x 60 frame with 10 px of padding holding a (1, 1) frame counts
                // 10 + 60 + 10.
                // (measured on automatic axes only; a UIAspectRatioConstraint's measure of the other axis keeps the
                // container's content box for every child)
                let plain = pass.layout_object(child).is_none();
                let (leaf_x, leaf_y) = (child_x && leaf_axes.0, child_y && leaf_axes.1);
                if let (true, Some(pad)) = (plain, pass.first_child_of(child, pass.l.ids.c.UIPadding)) {
                    // (a near padding with a scale part starts the edges at its offset part)
                    let n = pass.l.ids.n;
                    let (offset_left, _, offset_top, _) = pass.padding_offsets(child);
                    if leaf_x && pass.udim(pad, n.PaddingLeft).0 != 0.0 {
                        inner.content.x = f32r(child_record.x + offset_left * inner.scale);
                    }
                    if leaf_y && pass.udim(pad, n.PaddingTop).0 != 0.0 {
                        inner.content.y = f32r(child_record.y + offset_top * inner.scale);
                    }
                }
                let child_scale = inner.scale;
                let leaf_record = (plain && (leaf_x || leaf_y)).then(|| {
                    let resolve = |scale_part: f64, offset: f64, content: f64| lmax(f32r(f32r(scale_part * (content / child_scale)) + offset), 0.0) * child_scale;
                    let mut leaf = inner.clone();
                    if leaf_x {
                        let width = resolve(child_size[0], child_size[1], inner.content.w);
                        leaf.size_w = Some(width);
                        leaf.item_w = Some(width);
                    }
                    if leaf_y {
                        let height = resolve(child_size[2], child_size[3], inner.content.h);
                        leaf.size_h = Some(height);
                        leaf.item_h = Some(height);
                    }
                    leaf
                });
                let (held_x, held_y) = pass.extent_visit(top_id, child, &inner, leaf_record.as_ref(), child_x, child_y, origin, leaf_axes);
                if child_x {
                    edge_x = lmax(edge_x, held_x);
                }
                if child_y {
                    edge_y = lmax(edge_y, held_y);
                }
            };
            match leaves {
                Some(leaf_record) if !top => {
                    self.layout_children(parent, parent_record, true, Some(&mut |pass: &mut Pass, child: InstanceId, record: &Record| visit(pass, child, record, Some(false))));
                    self.layout_children(parent, leaf_record, true, Some(&mut |pass: &mut Pass, child: InstanceId, record: &Record| visit(pass, child, record, Some(true))));
                }
                _ => {
                    self.layout_children(parent, parent_record, true, Some(&mut |pass: &mut Pass, child: InstanceId, record: &Record| visit(pass, child, record, None)));
                }
            }
        }
        if top {
            return (edge_x, edge_y);
        }
        let ratio = parent_record.scale / scale;
        // Measured (fixtures/layout-autosize-constraint.luau): what a UISizeConstraint'd descendant holds counts
        // clamped by its constraint inside its padding (a 30 px capped frame holding 40 px inside 6 + 6 px of
        // padding counts 42; one with MinSize 22 holding 4 px counts 34); one that holds nothing counts by its box.
        if holds && self.has_size_constraint(parent) {
            let (held_x, held_y) = self.apply_size_constraint(parent, (edge_x - start_x) / ratio, (edge_y - start_y) / ratio);
            edge_x = start_x + held_x * ratio;
            edge_y = start_y + held_y * ratio;
        }
        let (_, pad_right, _, pad_bottom) = self.padding_offsets(parent);
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
        let (deep_x, _) = self.descendant_extent(item, &probe, true, false, (false, false));
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
        // An automatic object's UIAspectRatioConstraint first fits its base Size, as on any object (see the stable
        // size below).
        let aspect_holder = is_automatic && self.aspect_ratio(id).is_some();
        let (fit_w, fit_h) = if aspect_holder { self.apply_aspect_ratio(id, width, height, parent_w, parent_h) } else { (width, height) };
        let (mut base_w, mut base_h) = (width, height);
        let (mut child_measure_w, mut child_measure_h) = (None, None);
        let (mut held_w, mut held_h) = (None, None);

        let mut text_box = None;
        let mut measured_bounds = None;
        let mut grow = None;
        let mut basis = None;
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
            let mut inner_available = if auto_x {
                match available {
                    Some(available) => lmax(available - pad_left - pad_right, inner_width),
                    None => INF,
                }
            } else {
                inner_width
            };
            let mut inner_available_h = if auto_y {
                match available_h {
                    Some(available_h) => lmax(available_h - pad_top - pad_bottom, inner_height),
                    None => INF,
                }
            } else {
                inner_height
            };
            // Measured (fixtures/layout-autosize-aspect2.luau gap01_*, layout-autosize-prompt.luau popups; a 0.747
            // client): an object automatic on one axis with a UIAspectRatioConstraint lets its children grow on that
            // axis only into the box its constraint makes of its base Size with the room it has (capped by its
            // MaxSize) on that axis, less its padding: a 240 x 60 frame of ratio 3 with 170 px of room below offers
            // 80 (a 0.8 scale panel's list gaps measure against 0.8 x (80 - 16)), 56 with 56 px of room, 50 under a
            // MaxSize of 50; of ratio 3 by height in a 296 px parent 296 / 3. A prompt holder of 500 x 158 at ratio
            // 3 offers 166.67, so its panel's 0.1 gaps measure against 0.8862 x (166.67 - 30).
            if aspect_holder && auto_x != auto_y {
                let (mut max_w, mut max_h) = (INF, INF);
                self.size_constraints(id, |_, max| {
                    max_w = lmin(max_w, max.0);
                    max_h = lmin(max_h, max.1);
                });
                if auto_y {
                    let room = lmin(available_h.unwrap_or(INF), max_h);
                    let (_, grow_h) = self.apply_aspect_ratio(id, width, room, parent_w, parent_h);
                    inner_available_h = lmax(grow_h - pad_top - pad_bottom, 0.0);
                    grow = Some((None, Some(inner_available_h)));
                } else {
                    let room = lmin(available.unwrap_or(INF), max_w);
                    let (grow_w, _) = self.apply_aspect_ratio(id, room, height, parent_w, parent_h);
                    inner_available = lmax(grow_w - pad_left - pad_right, 0.0);
                    grow = Some((Some(inner_available), None));
                }
            }
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
                        // Measured (fixtures/layout-autotext-matrix.luau narrow_w40_nowrap, layout-autoavail-chain.luau;
                        // a 0.747 client): unwrapped AutomaticSize X text drops its last space run only where that
                        // fits the room it has, not its box: "Hi there" (53.33 px) in a (0.5, 30) label 50 px wide
                        // with 40 px of room keeps 53.33 (TextBounds too), laid out in a box that wide.
                        let room = match available {
                            Some(available) if auto_x && sticky_key.is_some() => lmax(available / self.own_scale(id) - pad_left - pad_right, 0.0),
                            _ => INF,
                        };
                        let measure_at = if auto_x { lmin(text_available, room) } else { inner_width };
                        let mut info = self.layout_text(id, measure_at, height_basis, scale, false);
                        text_box = if auto_x { Some(if room < text_available { lmax(text_available, info.bounds_x) } else { text_available }) } else { None };
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
                measured_bounds = Some((text_info.absolute_x, text_info.absolute_y));
                // Measured (fixtures/layout-autotext-matrix.luau narrow_*, a 0.747 client): wrapped AutomaticSize XY
                // text with less room than its box measures its height with the text laid out in that room, while
                // its width, TextBounds and drawn text keep the box: a (0.5, 30) label "Hi there" in a 0 px wide
                // AutomaticSize Y frame is 30 wide and 100 tall (one glyph per line, capped by the 100 px it may
                // grow into) with TextBounds 28 x 60; in a 20 px frame 40 x 80.
                if auto_x && auto_y && self.is_true(id, n.TextWrapped) {
                    if let Some(available) = available {
                        let room = lmax(available / self.own_scale(id) - pad_left - pad_right, 0.0);
                        if room < text_available {
                            let height_basis = text_available_h;
                            let narrow = self.layout_text(id, room, height_basis, scale, false);
                            content_height = lmax(content_height, narrow.bounds_y);
                        }
                    }
                }
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
                                set_carry(&mut carry, child, CarryEntry { height: None, width: Some(item.w) });
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
            // Measured (fixtures/layout-autosize-constraint3.luau, layout-autolist-scalepad.luau; a 0.747 client): the
            // first pass measures against the object's own Size, its scale part resolved against the space the
            // object can grow into (capped by its MaxSize) less its padding (the padding's scale resolved against
            // the base size). A plain container's children size against it but still position against the empty
            // content box: a scale 1 child of a Size (0, 20) frame with 6 + 6 px of padding is 20 wide and the frame
            // 32; a 0.5 child of a Size (0, 50) frame capped at 30 asks for 25 and ends 12.5 wide; a Size (0.1, 0)
            // frame with 136 px to grow into gives 12.4. A list's scale Padding resolves against it: a Size
            // (1, 0, 0.2, 0) list with 0.1 + 4 and 6 px of padding in 150 px measures its gaps against
            // 0.2 x (150 - 7 - 6) = 27.4 (the frame 62.74).
            let list = flow_class == Some(c_list);
            let (first_w, first_h) = {
                let size = self.udim2(id, self.l.ids.n.Size);
                let (mut max_w, mut max_h) = (INF, INF);
                self.size_constraints(id, |_, max| {
                    max_w = lmin(max_w, max.0);
                    max_h = lmin(max_h, max.1);
                });
                let (base_left, base_right, base_top, base_bottom) = self.padding(id, width, height);
                let basis = |scale_part: f64, offset: f64, grow: Option<f64>, cap: f64, pads: f64| -> Option<f64> {
                    let grow = grow.filter(|v| *v < INF)?;
                    let space = lmax(lmin(grow, cap) - pads, 0.0);
                    Some(lmax(f32r(f32r(scale_part * space) + offset), 0.0))
                };
                (
                    if auto_x { basis(size[0], size[1], available, max_w, base_left + base_right) } else { None },
                    if auto_y { basis(size[2], size[3], available_h, max_h, base_top + base_bottom) } else { None },
                )
            };
            // (with a UIAspectRatioConstraint too: a (1, 1) frame in a 60 x 60 frame of ratio 0.5 automatic on X counts
            // 60 wide, fixtures/layout-autosize-aspect4.luau x_full_b20x100)
            if flow.is_none() {
                let size = self.udim2(id, self.l.ids.n.Size);
                // (with nothing to grow into, only the offset)
                if auto_x {
                    first.size_w = Some(first_w.unwrap_or(lmax(size[1], 0.0)) * scale);
                }
                if auto_y {
                    first.size_h = Some(first_h.unwrap_or(lmax(size[3], 0.0)) * scale);
                }
            } else if list {
                first.gap_w = first_w.map(|v| v * scale);
                first.gap_h = first_h.map(|v| v * scale);
            }
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
            // Measured (layout-probe-autoaspect3, -autoaspect4, an options prompt): with a
            // UIAspectRatioConstraint the stable size is measured from the content on the other axis.
            let has_aspect = self.first_child_of(id, self.l.ids.c.UIAspectRatioConstraint).is_some() && self.has_gui_children(id);
            // Measured (fixtures/layout-autosize-aspect7.luau, -aspect8, -aspect9; a 0.747 client): a holder with a
            // UIListLayout or UIGridLayout of its own takes the FitWithinMaxSize rule below too, its content being the
            // layout's content measured in the base Size: a 240 x 60 frame of ratio 3 with two 100 x 30 items 5 px apart
            // is 180 x 65 (not 195 x 65), with eight 50 x 40 cells (four to a row at 240) 215 x 85.
            let flow_aspect = has_aspect && (flow_class == Some(c_list) || flow_class == Some(c_grid)) && !self.aspect_scales_with_parent(id);
            let aspect = (flow.is_none() && has_aspect) || flow_aspect;
            let (aspect_x, aspect_y) = (aspect && !auto_x, aspect && !auto_y);
            let (mut natural_x, mut natural_y): (f64, f64) = (0.0, 0.0);
            // (at the base Size itself, not less the padding: a (1, 0.5) frame in a 240 x 60 frame of ratio 3 with 20 + 20 px
            // of side padding counts 240 wide, so beside a 70 px box the frame is 3 x 70 = 210 wide; aspect9 y_l20r20_*)
            if aspect_x && !flow_aspect {
                first.size_w = Some(width * scale);
                first.sizing_w = Some(0.0);
            }
            if aspect_y && !flow_aspect {
                first.size_h = Some(height * scale);
                first.sizing_h = Some(0.0);
            }
            let first_content = first.content;
            let (mut offset_x, mut offset_y) = if aspect && !flow_aspect {
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
            if flow_aspect {
                natural_x = offset_x;
                natural_y = offset_y;
            }
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
                // (the leaf rule also on the axis a UIAspectRatioConstraint measures: a 0.8 item holding a (1, 0) frame
                // in a 240 px frame counts 0.8 x 0.8 x 240, fixtures/layout-autosize-aspect5.luau)
                let (deep_x, deep_y) = self.descendant_extent(id, &deep, auto_x || aspect_x, auto_y || aspect_y, (auto_x || aspect_x, auto_y || aspect_y));
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
            // Measured (fixtures/layout-autosize-constraint.luau, Studio 0.747): on an automatic axis the
            // UISizeConstraint bounds the content inside the padding offsets, so MinSize 22 with 6 + 6 px of
            // padding makes a label holding 8 px of text 34 wide, not 22. The object is then constrained as any
            // object is (MaxSize caps it), its children still lay out in the content box its content asked for,
            // and an automatic parent measures it at that asked size (`held`).
            let (asked_x, asked_y) = self.apply_size_constraint(id, lmax(content_width, offset_x), lmax(content_height, offset_y));
            let asked_w = asked_x + pad_left + pad_right;
            let asked_h = asked_y + pad_top + pad_bottom;
            let (grow_w, grow_h) = if aspect_holder { (fit_w, fit_h) } else { (width, height) };
            let mut stable_w = if auto_x { lmax(grow_w, asked_w) } else { width };
            let mut stable_h = if auto_y { lmax(grow_h, asked_h) } else { height };
            let ratio = self.aspect_ratio(id).unwrap_or(1.0);
            let fit_within = aspect && !self.aspect_scales_with_parent(id);
            // (ScaleWithParentSize keeps its dominant axis: on the other axis the stable size is what the content
            // holds, at least the fitted base and at most the base, as below)
            if aspect_x && !fit_within {
                stable_w = lmin(width, lmax(fit_w, lmax(content_width, natural_x) + ratio * (pad_top + pad_bottom)));
            }
            if aspect_y && !fit_within {
                stable_h = lmin(height, lmax(fit_h, lmax(content_height, natural_y) + (pad_left + pad_right) / ratio));
            }
            let (asked_box_w, asked_box_h) = (stable_w, stable_h);
            (stable_w, stable_h) = self.apply_size_constraint(id, stable_w, stable_h);
            if !auto_x && !aspect_x {
                stable_w = width;
            }
            if !auto_y && !aspect_y {
                stable_h = height;
            }
            // (the box the children lay out in: what MaxSize cut stays on automatic axes)
            let mut box_w = if auto_x { asked_box_w } else { stable_w };
            let mut box_h = if auto_y { asked_box_h } else { stable_h };

            // Measured (fixtures/layout-autosize-aspect.luau, -aspect2 to -aspect5, layout-autosize-prompt.luau; a
            // 0.747 client): a UIAspectRatioConstraint (FitWithinMaxSize, either DominantAxis) on an object that holds
            // children takes the width from what they hold, fitted to the ratio, and the height from the width. The
            // content is measured inside the padding, scale leaves at the base Size; its fitted height, plus the top
            // and bottom padding, times the ratio is the width, at least the base Size's fitted width and, when the
            // width is not automatic, at most the base width; the height is the width over the ratio, at most the
            // base height when it is not automatic. Then the children lay out in that box and push the automatic
            // axes further. A 240 x 60 frame of ratio 3 (fitted 180 x 60) holding a 100 x 40 box is 180 x 60, a
            // 100 x 100 box 180 x 100, a 200 x 100 box 200 x 100, a 260 x 100 box 240 x 100; with 30 + 30 px of top
            // and bottom padding a 100 x 40 box makes it 3 x (33.33 + 60) = 280, so 240 x 80; left and right padding
            // count nowhere. A 240 x 0 frame (fitted 0 x 0) holding a 100 x 40 box is 100 x 40 (the options prompt's
            // 518 x 154). A 60 x 60 frame of ratio 0.5 automatic on X holding a (1, 1) frame and a 20 x 100 box is
            // 50 x 60 (the content 60 x 100 fitted to 50 x 100); the prompt holder of 500 x 158 at 2.87 is 453.46
            // wide whatever its content's height.
            // Measured (fixtures/layout-autosize-aspect8.luau x_l0r0t30b0_b20x100, -aspect9; a 0.747 client): the top and
            // bottom padding count only when the height is automatic: a 60 x 120 frame of ratio 0.5 automatic on X with
            // 30 px of top padding holding a (1, 1) frame and a 20 x 100 box is 60 wide, not 0.5 x (120 + 30) = 75.
            // (Not reproduced: a 200 x 60 frame of ratio 2 automatic on X and Y with 10 + 10 px of side and 20 + 20 px of
            // top and bottom padding around a (1, 1) frame and a 60 x 80 box is 200 x 100 in the client, here
            // 2 x (80 + 40) = 240 wide; frames of a 0 x 0 base take the full width, aspect9 xy_l10r10t20b20_full_b60x80.)
            // Measured (fixtures/layout-autosize-aspect3.luau, -aspect6, -aspect8, -aspect9): on an automatic axis a
            // scale child of such an object sizes against the box less the padding or, when the content holds more,
            // against what it holds up to the box: a (1, 1) frame in a 240 x 60 frame of ratio 3 with 10 + 10 px of top
            // and bottom padding is 60 tall beside a 20 px box and 80 beside an 80 px one (the frame 240 x 80 either way),
            // a lone (0.5, 0.5) frame there 20 (half of 60 - 20); likewise on X with the height fixed (a (1, 1) frame in a
            // 60 x 120 frame of ratio 0.5 with 10 + 10 px of side padding is 60 wide, a (0.5, 1) one 20); automatic on
            // both axes the width is max(ratio x that height, the box less the side padding).
            let (constrained_w, constrained_h) = if fit_within {
                let held_x = if auto_x { lmax(content_width, offset_x) } else { lmax(content_width, natural_x) };
                let held_y = if auto_y { lmax(content_height, offset_y) } else { lmax(content_height, natural_y) };
                let fitted_h = lmin(held_y, held_x / ratio);
                let pad_y = if auto_y { pad_top + pad_bottom } else { 0.0 };
                let raw_w = ratio * (fitted_h + pad_y);
                let fitted_w = if auto_x { lmax(fit_w, raw_w) } else { lmin(width, lmax(fit_w, raw_w)) };
                let fitted_h = if auto_y { fitted_w / ratio } else { lmin(fitted_w / ratio, height) };
                // (a MaxSize the ratio's box passes cuts it and the ratio applies again: fixtures/layout-autosize-aspect7.luau
                // max_*, measured)
                let (capped_w, capped_h) = self.apply_size_constraint(id, fitted_w, fitted_h);
                let (box_w, box_h) = if capped_w != fitted_w || capped_h != fitted_h {
                    self.apply_aspect_ratio(id, capped_w, capped_h, parent_w, parent_h)
                } else {
                    (fitted_w, fitted_h)
                };
                if flow.is_none() {
                    let (inner_w, inner_h) = (lmax(box_w - pad_left - pad_right, 0.0), lmax(box_h - pad_top - pad_bottom, 0.0));
                    let basis_h = lmax(inner_h, lmin(held_y, box_h));
                    basis = Some(match (auto_x, auto_y) {
                        (true, true) => (Some(lmax(ratio * basis_h, inner_w)), Some(basis_h)),
                        (true, false) => (Some(lmax(inner_w, lmin(held_x, box_w))), None),
                        _ => (None, Some(basis_h)),
                    });
                }
                (box_w, box_h)
            } else {
                self.apply_aspect_ratio(id, stable_w, stable_h, parent_w, parent_h)
            };
            if constrained_w != stable_w || constrained_h != stable_h {
                // Measured (fixtures/layout-autosize-aspect2.luau text_*): on the axis that is not automatic, content
                // measures against the base Size: a wrapped (1, -20) label in a 240 x 60 frame of ratio 3 keeps the
                // two lines it has at 220 px while the frame is 180 wide.
                child_measure_w = Some(lmax(if aspect_x { width } else { stable_w } - pad_left - pad_right, 0.0));
                child_measure_h = Some(lmax(if aspect_y { height } else { stable_h } - pad_top - pad_bottom, 0.0));
                (box_w, box_h) = (constrained_w, constrained_h);
            }
            stable_w = constrained_w;
            stable_h = constrained_h;

            let mut second = probe(lmax(box_w - pad_left - pad_right, 0.0), lmax(box_h - pad_top - pad_bottom, 0.0), child_measure_w, child_measure_h);
            second.w = box_w * scale;
            second.h = box_h * scale;
            if let Some((basis_w, basis_h)) = basis {
                second.size_w = basis_w.map(|v| v * scale);
                second.size_h = basis_h.map(|v| v * scale);
            }
            // Measured (fixtures/layout-autolist-scalepad.luau): the second pass resolves a scale list Padding against
            // the box less the whole padding, its scale resolved against the constrained box (a 0.1 scale top pad
            // and a 0.1 gap make 50 px of items 54.5 tall; with MaxSize 40 and 0.1 + 4 and 6 px of padding the gaps
            // measure against 50 - 8 - 6 = 36), and where the items end up extends the frame past its stable size.
            if list {
                let (capped_w, capped_h) = self.apply_size_constraint(id, box_w, box_h);
                let (left, right, top, bottom) = self.padding(id, capped_w, capped_h);
                second.gap_w = if auto_x { Some(lmax(box_w - left - right, 0.0) * scale) } else { None };
                second.gap_h = if auto_y { Some(lmax(box_h - top - bottom, 0.0) * scale) } else { None };
            }
            let (extent_x, extent_y) = self.layout_children(id, &second, true, None);
            // Measured (layout-probe-autocenter; fixtures/layout-autosize-constraint.luau): where children end
            // up extends the frame without its padding, past a MaxSize too (a 40 px child of a frame capped at
            // 30 makes it 40).
            width = if auto_x { lmax(stable_w, extent_x) } else { stable_w };
            height = if auto_y { lmax(stable_h, extent_y) } else { stable_h };
            base_w = box_w;
            base_h = box_h;
            held_w = if auto_x && asked_w > width { Some(asked_w) } else { None };
            held_h = if auto_y && asked_h > height { Some(asked_h) } else { None };
        }

        Sized { w: width, h: height, text_box, base_w, base_h, measure_w: child_measure_w, measure_h: child_measure_h, held_w, held_h, bounds: measured_bounds, grow, basis }
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
            let (bounds_x, bounds_y) = result.bounds.unwrap_or((f64::NAN, f64::NAN));
            self.l.carry_maps[capture].insert(id, [result.w, result.h, result.base_w, result.base_h, bounds_x, bounds_y]);
        } else if let Some(apply) = self.l.carry_apply {
            if let Some(kept) = self.l.carry_maps[apply].get(&id).copied() {
                if automatic == E::X || automatic == E::XY {
                    result.w = kept[0];
                    result.base_w = kept[2];
                    result.held_w = None;
                }
                if automatic == E::Y || automatic == E::XY {
                    result.h = kept[1];
                    result.base_h = kept[3];
                    result.held_h = None;
                }
            }
        }
        result
    }

    // Placement

    /// Places a child: absolute rect from its unscaled size and the parent content rect. `offset`
    /// (children placed by a layout) is in unscaled local units from the content origin.
    /// `offset`: a layout's placement from the parent's content origin in its local units, instead of Position;
    /// `centred`: that is the child's centre (AnchorPoint 0.5), as a UIGridLayout places its items.
    #[allow(clippy::too_many_arguments)]
    fn place_child(&mut self, id: InstanceId, parent: &Record, unscaled_w: f64, unscaled_h: f64, offset: Option<(f64, f64)>, centred: bool, base: Option<(f64, f64)>) -> Record {
        let n = self.l.ids.n;
        let scale = f32r(parent.scale * self.own_scale(id));
        let (width, height) = (f32r(unscaled_w * scale), f32r(unscaled_h * scale));
        // Measured (fixtures/layout-autosize-constraint.luau, layout-autolist-scalepad.luau): an automatic object
        // its children pushed past its box resolves its padding's scale against that box, constrained: 0.1 of a
        // frame capped at 30 is 3 px while it reads 40 wide; a 0.1 top padding of a list frame whose 50 px of
        // items and 4.5 px gap make it 54.5 tall is 5.
        let (_, auto_x, auto_y) = self.automatic_axes(id);
        let (pad_basis_w, pad_basis_h) = if auto_x || auto_y {
            let (box_w, box_h) = base.unwrap_or((unscaled_w, unscaled_h));
            self.apply_size_constraint(id, box_w, box_h)
        } else {
            (unscaled_w, unscaled_h)
        };
        let (pad_left, pad_right, pad_top, pad_bottom) = self.padding(id, pad_basis_w, pad_basis_h);

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
                if centred { f32r(f32r(0.5 * unscaled_w) * scale) } else { 0.0 },
                f32r(offset_y * parent.scale),
                if centred { f32r(f32r(0.5 * unscaled_h) * scale) } else { 0.0 },
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
    /// alignment inside lines, in the parent's local units: `sizes` are the items' sizes there (their own UIScale
    /// included; updated in place for flexed / stretched items). Returns offsets from the parent's content origin and
    /// the content size, both local.
    /// Measured (fixtures/layout-f32-lists.luau, layout-f32-lists2.luau; a 0.747 client, UIScale 1 / 0.885 / 1.3: 840
    /// of 840 item positions, 96 of 96 AbsoluteContentSizes bit for bit): a client walks the items in float32, the
    /// next item at f32(f32(cursor + size) + gap) from 0 with the gap f32(f32(Padding.Scale x main) + Offset), the
    /// content is where the last item ends, the block starts at f32(f32(main - content) x 0.5) when centred and
    /// f32(main - content) at the far end, an item at f32(start + cursor), and on the cross axis an item of a single
    /// line is aligned in the container in one step, f32(f32(cross - size) x 0.5).
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
        let (mut content_w, mut content_h) = local_content(parent);
        if parent.align_start.is_some() {
            content_w = parent.local_w.unwrap_or(parent.content.w / scale);
            content_h = parent.local_h.unwrap_or(parent.content.h / scale);
        }
        let main_size = if horizontal { content_w } else { content_h };
        let cross_size = if horizontal { content_h } else { content_w };
        let gap_basis = (if horizontal { parent.gap_w } else { parent.gap_h }).map_or(main_size, |v| v / scale);
        let gap = f32r(f32r(padding_scale * gap_basis) + padding_offset);
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
            // (the content is where the last item ends, walked in float32)
            let mut used = 0.0;
            for (offset, index) in (line.start..line.end).enumerate() {
                sizes[index][main_index] = mains[offset];
                sum += mains[offset];
                if offset > 0 {
                    used = f32r(used + gap);
                }
                used = f32r(used + mains[offset]);
            }
            line.sum = sum;
            line.used = used;
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

        // (one line aligned by the layout's own cross alignment: the item is aligned in the container in one step)
        let one_step = lines.len() == 1
            && cross_flex != E::FILL
            && !cross_zero
            && space_distribution(cross_flex, cross_size, cross_sum, line_count, gap).0.is_none()
            && (layout_line_alignment == E::AUTOMATIC || layout_line_alignment == E::EMPTY);

        offsets.clear();
        offsets.resize(items.len(), (0.0, 0.0));
        let mut content_main: f64 = 0.0;
        let mut cross_cursor = cross_start.unwrap();
        for line in lines.iter() {
            let count = (line.end - line.start) as f64;
            let (start, between) = space_distribution(main_flex, main_size, line.sum, count, gap);
            let mut start = start.map_or_else(|| aligned_start_f32(main_alignment, main_size, line.used), f32r);
            if main_zero {
                start = 0.0;
            }

            let mut cursor = 0.0;
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
                let item_cross = sizes[index][cross_index];
                if alignment == E::STRETCH {
                    sizes[index][cross_index] = line.cross;
                } else if alignment == E::CENTER {
                    cross = f32r(f32r(line.cross - item_cross) * 0.5);
                } else if alignment == E::END {
                    cross = f32r(line.cross - item_cross);
                }
                let cross_at = if one_step && alignment == default_line_alignment && alignment != E::STRETCH {
                    match alignment {
                        E::CENTER => f32r(f32r(cross_size - item_cross) * 0.5),
                        E::END => f32r(cross_size - item_cross),
                        _ => 0.0,
                    }
                } else {
                    f32r(cross_cursor + cross)
                };
                let main_at = f32r(start + cursor);

                offsets[index] = if horizontal { (main_at, cross_at) } else { (cross_at, main_at) };
                cursor = f32r(f32r(cursor + sizes[index][main_index]) + between);
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

    /// UIGridLayout, in float32 local units as a 0.747 client lays it out (fixtures/layout-f32-grids.luau: 1,008 of
    /// 1,008 item positions, 504 of 504 sizes and 72 of 72 AbsoluteContentSizes bit for bit under UIScale 1, 0.885 and
    /// 1.3): cell = f32(f32(CellSize.Scale x W) + Offset) and pad = f32(f32(CellPadding.Scale x W) + Offset) against
    /// the container's local size; the stride is f32(cell + pad) and each item is f32(stride - pad) (a 14.33309 cell
    /// with a 3.45337 pad makes 14.33309 - 1 ulp items); the content is f32(f32(n x stride) - pad); cell i starts at
    /// f32(start + f32(i x stride)), and the item is placed at the cell's centre f32(start_i + f32(size x 0.5)) with
    /// an AnchorPoint of 0.5 (which also scales a UIScale'd item about the cell centre). AbsoluteCellSize is the cell
    /// in local units: a client reports 16.837 under UIScale 0.885 as under 1.
    fn grid_layout(&self, layout: InstanceId, parent: &Record, items: &[InstanceId]) -> GridPlan {
        let n = self.l.ids.n;
        let scale = parent.scale;
        let cell_size = self.udim2(layout, n.CellSize);
        let cell_padding = self.udim2(layout, n.CellPadding);
        // (an automatic container lays its grid out in its stable size, as its list: fixtures/layout-autosize-aspect7.luau
        // grid_scale_n6, a 226 x 80.33 frame's (0.3, 0.5) cells against its 226 x 75.33 box: 67.8 x 37.67)
        let (content_w, content_h) = if parent.align_start.is_some() {
            (parent.local_w.unwrap_or(parent.content.w / scale), parent.local_h.unwrap_or(parent.content.h / scale))
        } else {
            local_content(parent)
        };

        let (item_w, item_h) = (parent.item_w.map_or(content_w, |v| v / scale), parent.item_h.map_or(content_h, |v| v / scale));
        let mut cell_w = f32r(f32r(cell_size[0] * item_w) + cell_size[1]);
        let mut cell_h = f32r(f32r(cell_size[2] * item_h) + cell_size[3]);
        if let Some(aspect) = self.first_child_of(layout, self.l.ids.c.UIAspectRatioConstraint) {
            let ratio = self.num(aspect, n.AspectRatio).unwrap_or(1.0);
            if cell_w / lmax(cell_h, 1e-9) > ratio {
                cell_w = f32r(cell_h * ratio);
            } else {
                cell_h = f32r(cell_w / ratio);
            }
        }
        let pad_x = f32r(f32r(cell_padding[0] * content_w) + cell_padding[1]);
        let pad_y = f32r(f32r(cell_padding[2] * content_h) + cell_padding[3]);
        let (stride_x, stride_y) = (f32r(cell_w + pad_x), f32r(cell_h + pad_y));
        let size = (f32r(stride_x - pad_x), f32r(stride_y - pad_y));

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
        let grid_w = f32r(f32r(columns * stride_x) - pad_x);
        let grid_h = f32r(f32r(rows * stride_y) - pad_y);

        let horizontal_alignment = self.code(layout, n.HorizontalAlignment);
        let vertical_alignment = self.code(layout, n.VerticalAlignment);
        let mut start_x = if horizontal_alignment == E::CENTER || horizontal_alignment == E::RIGHT {
            aligned_start_f32(horizontal_alignment, content_w, grid_w)
        } else {
            0.0
        };
        let mut start_y = if vertical_alignment == E::CENTER || vertical_alignment == E::BOTTOM {
            aligned_start_f32(vertical_alignment, content_h, grid_h)
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

        let (half_x, half_y) = (f32r(size.0 * 0.5), f32r(size.1 * 0.5));
        let mut centres = Vec::with_capacity(items.len());
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
            let left = f32r(start_x + f32r(column * stride_x));
            let top = f32r(start_y + f32r(row * stride_y));
            centres.push((f32r(left + half_x), f32r(top + half_y)));
        }
        let count = if count > 0.0 { (columns, rows) } else { (0.0, 0.0) };
        GridPlan { centres, size, extent: (grid_w, grid_h), cell: (cell_w, cell_h), count }
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
            let (mut width, mut height, text_box, mut base_w, mut base_h) = (sized.w, sized.h, sized.text_box, sized.base_w, sized.base_h);
            if let Some(kept) = text_carry.as_ref().and_then(|carry| carry.iter().find(|(c, _)| *c == child).map(|(_, e)| *e)) {
                // Measured (fixtures/layout-autotext-matrix.luau, a 0.747 client): a carried label keeps the measure's
                // height but draws (and reports TextBounds for) its text laid out in its own box: "Which units would
                // you like to unequip" measured 129.33 px wide is drawn wrapped in its 65 px label (TextBounds 57 x 20).
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
                grow: sized.grow,
                basis: sized.basis,
            };
            // Measured (fixtures/layout-autosize-constraint.luau): an automatic parent measures a child its
            // MaxSize cut at the size the child's content asked for (a 30 px capped frame holding 24 px inside
            // 6 + 6 px of padding makes its AutomaticSize X parent 36 wide).
            if let (true, Some((measuring_x, measuring_y))) = (measure, record.measuring) {
                if let (true, Some(held)) = (measuring_x, sized.held_w) {
                    entry.w = lmax(entry.w, held);
                }
                if let (true, Some(held)) = (measuring_y, sized.held_h) {
                    entry.h = lmax(entry.h, held);
                }
            }
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
        // (`offsets`: from the content origin in local units; `forced`: the child's own unscaled size; `centred`: placed by
        // its centre with an AnchorPoint of 0.5, as a grid places its items)
        let mut forced = self.l.scratch.pairs.pop().unwrap_or_default();
        forced.clear();
        forced.resize(count, None);
        let mut centred: Option<Vec<bool>> = None;
        let mut cell_rects: Option<Vec<Option<Cells>>> = None;
        if layout.is_none() {
            if let Some(cells) = &record.cells {
                // Cells of a UITableLayout row/column get the rects the table assigned them.
                for (index, &child) in items.iter().enumerate() {
                    if let Some((_, cell)) = cells.iter().find(|(c, _)| *c == child) {
                        let child_scale = scale * self.own_scale(child);
                        offsets[index] = Some((cell[0] / scale, cell[1] / scale));
                        forced[index] = Some((cell[2] / child_scale, cell[3] / child_scale));
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
                let plan = self.grid_layout(layout, record, &flow);
                // Measured: a UIScale'd grid item is its cell scaled about the cell centre (the centre placement).
                let mut by_centre = vec![false; count];
                for (flow_at, &position) in flow_positions.iter().enumerate() {
                    offsets[position] = Some(plan.centres[flow_at]);
                    forced[position] = Some(plan.size);
                    by_centre[position] = true;
                }
                // Measured: invisible children get the cell size at the content origin.
                for (index, &child) in items.iter().enumerate() {
                    if forced[index].is_none() && self.parent(child) == Some(id) {
                        forced[index] = Some(plan.cell);
                        offsets[index] = Some((0.0, 0.0));
                    }
                }
                centred = Some(by_centre);
                if !measure {
                    self.set_vector(layout, n.AbsoluteContentSize, f32r(plan.extent.0 * scale), f32r(plan.extent.1 * scale));
                    self.set_vector(layout, n.AbsoluteCellSize, plan.cell.0, plan.cell.1);
                    self.set_vector(layout, n.AbsoluteCellCount, plan.count.0, plan.count.1);
                }
                extent_x = plan.extent.0;
                extent_y = plan.extent.1;
            } else if layout_class == c.UIPageLayout {
                let mut page_sizes = Vec::with_capacity(flow.len());
                for (flow_at, &position) in flow_positions.iter().enumerate() {
                    let child_scale = scale * self.own_scale(flow[flow_at]);
                    page_sizes.push((resolved[position].w * child_scale, resolved[position].h * child_scale));
                }
                let (page_offsets, content_width, content_height) = self.page_layout(layout, record, &flow, &page_sizes);
                for (flow_at, &position) in flow_positions.iter().enumerate() {
                    offsets[position] = Some((page_offsets[flow_at].0 / scale, page_offsets[flow_at].1 / scale));
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
                    let child_scale = scale * self.own_scale(flow[flow_at]);
                    offsets[position] = Some((table_offsets[flow_at].0 / scale, table_offsets[flow_at].1 / scale));
                    forced[position] = Some((table_sizes[flow_at].0 / child_scale, table_sizes[flow_at].1 / child_scale));
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
                // (the items' sizes in this frame's local units, their own UIScale included)
                let mut local_sizes = std::mem::take(&mut self.l.list_scratch.sizes);
                local_sizes.clear();
                for (flow_at, &position) in flow_positions.iter().enumerate() {
                    let child_scale = self.own_scale(flow[flow_at]);
                    let own = |value: f64| if child_scale == 1.0 { value } else { f32r(value * child_scale) };
                    let mut width = own(resolved[position].w);
                    let mut height = own(resolved[position].h);
                    if flex_list && child_scale != 1.0 {
                        if horizontal_fill {
                            height = resolved[position].h;
                        } else {
                            width = resolved[position].w;
                        }
                    }
                    local_sizes.push([width, height]);
                }
                let mut list_scratch = std::mem::take(&mut self.l.list_scratch.list);
                let mut list_offsets = std::mem::take(&mut self.l.list_scratch.offsets);
                let (content_width, content_height) = self.list_layout(layout, record, &flow, &mut local_sizes, &mut list_offsets, &mut list_scratch);
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
                    let child_scale = self.own_scale(flow[flow_at]);
                    let size = local_sizes[flow_at];
                    let own = |value: f64| if child_scale == 1.0 { value } else { f32r(value * child_scale) };
                    if (size[0] - own(resolved[position].w)).abs() > 1e-9 || (size[1] - own(resolved[position].h)).abs() > 1e-9 {
                        forced[position] = Some((size[0] / child_scale, size[1] / child_scale));
                    }
                }
                self.l.list_scratch.sizes = local_sizes;
                self.l.list_scratch.offsets = list_offsets;
                if !measure {
                    self.set_vector(layout, n.AbsoluteContentSize, f32r(content_width * scale), f32r(content_height * scale));
                }
                extent_x = content_width;
                extent_y = content_height;
            }
            self.l.give_vec(flow);
            self.l.scratch.indices.push(flow_positions);
        }

        let has_layout = layout.is_some();
        for (index, &child) in items.iter().enumerate() {
            let entry = resolved[index];
            let (mut width, mut height) = (entry.w, entry.h);
            let forced_size = forced[index];
            if let Some(size) = forced_size {
                (width, height) = size;
            }
            let offset = offsets[index];
            let (mut base_w, mut base_h) = (entry.base_w, entry.base_h);
            if forced_size.is_some() {
                base_w = width;
                base_h = height;
            }
            let by_centre = centred.as_ref().is_some_and(|centred| centred[index]);
            let mut child_record = self.place_child(child, record, width, height, offset, by_centre, Some((base_w, base_h)));
            if let Some((measure_w, measure_h)) = entry.measure {
                child_record.measure_w = Some(measure_w * child_record.scale);
                child_record.measure_h = Some(measure_h * child_record.scale);
            }
            // (an aspect holder's children grow into the room its constraint leaves, after the measure too)
            if let Some((grow_w, grow_h)) = entry.grow {
                if let Some(grow_w) = grow_w {
                    child_record.available = Some(grow_w * child_record.scale);
                }
                if let Some(grow_h) = grow_h {
                    child_record.available_h = Some(grow_h * child_record.scale);
                }
            }
            // (and its plain children size against the basis its constraint gives them)
            if let Some((basis_w, basis_h)) = entry.basis {
                child_record.size_w = basis_w.map(|v| v * child_record.scale);
                child_record.size_h = basis_h.map(|v| v * child_record.scale);
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
    /// Adds a laid-out child's record count to the object being laid out (`Record::count`)
    fn add_count(&mut self, count: u32) {
        if let Some(top) = self.l.count_stack.last_mut() {
            *top = top.saturating_add(count);
        }
    }

    fn version(&mut self, id: InstanceId, mut record: Record, old: Option<Record>) {
        if let Some(mut old) = old {
            if old.same_output(&record) {
                old.generation = record.generation;
                old.place = record.place;
                old.canvas_place = record.canvas_place;
                old.count = record.count;
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
    /// descendants moved along. `from_scroll` distinguishes scroll shifts from `store` same-shape moves.
    fn moved(&mut self, id: InstanceId, from_scroll: bool) {
        let Some(record) = self.l.records.get(id) else { return };
        if from_scroll {
            self.l.stats.scroll_moved += 1;
        }
        let (absolute_x, absolute_y) = absolute_position(record);
        let rotation = record.rotation;
        let n = self.l.ids.n;
        let n_rotation = n.AbsoluteRotation;
        self.set_absolute_position(id, absolute_x, absolute_y);
        self.set_number(id, n_rotation, rotation);
        self.bump(id);
        self.shift_children(id, from_scroll);
    }

    /// Re-places the children of a moved record in place from their stored placement terms.
    fn shift_children(&mut self, id: InstanceId, from_scroll: bool) {
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
                if !from_scroll {
                    self.l.stats.moved += 1;
                }
                self.moved(child, from_scroll);
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
        // (a touch overscroll keeps its position past the range, input/touch.rs)
        let (scroll_x, scroll_y) = if self.l.overscroll.contains(&id) {
            position
        } else {
            (clamp(position.0, 0.0, limits[0]), clamp(position.1, 0.0, limits[1]))
        };
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
    /// Applies the pending scrolls; returns each scrolled frame applied with the records it moved and whether it was scrolled after
    /// the frame's layout point (install.rs prices those).
    pub(crate) fn process_scrolls(&mut self) -> Vec<(InstanceId, u64, bool)> {
        if self.l.scrolled.is_empty() {
            return Vec::new();
        }
        // Profile: counted, and its time is layout time (Layout.ProfileTime), like a pass's
        crate::engine::profile::count("scrollLayoutPasses", 1);
        let started = std::time::Instant::now();
        let applied = self.process_pending_scrolls();
        self.l.profile_time += super::elapsed(started);
        applied
    }

    fn process_pending_scrolls(&mut self) -> Vec<(InstanceId, u64, bool)> {
        let pending: Vec<InstanceId> = self.l.scrolled.clone();
        let mut applied = Vec::new();
        for id in pending {
            let after = self.l.scrolled_after_frame_layout(id);
            let before = self.l.stats.scroll_moved;
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
                self.shift_children(id, true);
                let delta = self.l.stats.scroll_moved - before;
                if delta > 0 {
                    self.l.stats.scroll_passes += 1;
                    if self.l.track_churn {
                        let entry = self.l.churn.entry(id).or_default();
                        entry.scroll_passes += 1;
                        entry.scroll_moved += delta;
                    }
                }
                applied.push((id, delta, after));
            }
        }
        applied
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
                let count = old.count.max(1);
                if let Some(parents) = &self.l.relaying {
                    // (a client lays it out again after a geometry write in the tree, and walks its subtree; a read lays out
                    // the written objects' siblings only)
                    self.l.stats.relaid += 1;
                    self.l.stats.relaid_under += u64::from(count - 1);
                    if self.state.dom.at(id).parent.is_some_and(|parent| parents.contains(&parent)) {
                        self.l.stats.relaid_near += 1;
                    }
                }
                self.add_count(count);
                cache_counts::hit(Cache::Records);
                return;
            }
            if old.same_shape(&record) {
                // Only moved: keep the record (and everything computed for it), take the new place.
                let before_moved = self.l.stats.moved;
                self.l.stats.moved += 1;
                self.add_count(old.count.max(1));
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
                self.moved(id, false);
                if self.l.track_churn {
                    let delta = self.l.stats.moved.saturating_sub(before_moved);
                    self.l.pass_churn.entry(id).or_default().1 += delta;
                }
                self.l.stored_stamps.set(id, stamp);
                return;
            }
        }

        self.l.stats.stored += 1;
        if self.l.track_churn {
            self.l.pass_churn.entry(id).or_default().0 += 1;
        }
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
                    // Measured (fixtures/layout-autotext-matrix.luau, a 0.747 client): a drawn label lays its text out in
                    // its content box, |W - padding| wide also where that is negative (a (1, -20) AutomaticSize Y label in
                    // a 0 px wide frame wraps at 20: TextBounds 15 x 80, where Studio's undrawn probe read the measure's
                    // one glyph per line).
                    let (text_w, text_h) = (record.content.w / record.scale, record.content.h / record.scale);
                    self.layout_text(id, text_box.unwrap_or(text_w), text_h, record.scale, false)
                }
            };
            // Measured (a clipped column of AutomaticSize cards, a 0.747 client): an AutomaticSize label wholly outside
            // its clip is not laid out for drawing and keeps the TextBounds of the AutomaticCanvasSize measure
            // (129.33 x 100, laid out 143 px wide in the padded canvas) where one any part of which shows reports
            // its 154 px box's (151.33 x 80), its centre in the window or not.
            let mut bounds = (info.absolute_x, info.absolute_y);
            if let Some(apply) = self.l.carry_apply {
                let automatic = self.code(id, n_auto);
                if automatic != E::NONE && automatic != E::EMPTY {
                    if let Some(kept) = self.l.carry_maps[apply].get(&id) {
                        if !kept[4].is_nan() && (kept[4], kept[5]) != bounds && self.outside_clip(id, &record) {
                            bounds = (kept[4], kept[5]);
                        }
                    }
                }
            }
            self.set_vector(id, n_bounds, bounds.0, bounds.1);
            let box_w = record.content.w / record.scale;
            let box_h = record.content.h / record.scale;
            let slack_w = lmax(1e-6, box_w * 4.0 * f64::from(f32::EPSILON));
            let slack_h = lmax(1e-6, box_h * 4.0 * f64::from(f32::EPSILON));
            // (the text as laid out, not TextBounds rounded up to whole logical px: measure.rs text_bounds)
            let fits = info.layout.fits
                && info.fit_x <= box_w + slack_w
                && info.fit_y <= box_h + slack_h;
            self.set_bool(id, n_fits, fits);
            record.text = Some(info);
        }

        self.l.count_stack.push(0);
        if self.class(id) == self.l.ids.c.ScrollingFrame {
            self.layout_scrolling(id, &mut record);
        } else {
            self.l.open.push((id, [record.x, record.y, record.w, record.h], None, record.rotation));
            self.layout_children(id, &record, false, None);
            self.l.open.pop();
        }
        record.count = 1 + self.l.count_stack.pop().unwrap_or(0);
        self.add_count(record.count);

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
        // (in float32: fixtures/layout-f32-lists2.luau, a 0.747 client's AbsoluteCanvasSize under UIScale 0.885 and 1.3 is
        // f32(f32(1.7 x 849.6) + 9), 1453.31995)
        let raw_w = f32r(f32r(canvas_size[0] * parent_w) + scaled_offset(canvas_size[1], scale));
        let raw_h = f32r(f32r(canvas_size[2] * parent_h) + scaled_offset(canvas_size[3], scale));

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
                // Measured (Studio 0.747, layout-probe-autocanvas5 and constraint probes):
                // UISizeConstraint and UIAspectRatioConstraint on the ScrollingFrame constrain the
                // measured extent before UIPadding offsets and any inset scrollbar thickness are
                // added (ScaleWithParentSize clamps against the frame's own local box).
                let (extent_x, extent_y) = self.apply_size_constraint(id, extent_x, extent_y);
                let (extent_x, extent_y) = self.apply_aspect_ratio(id, extent_x, extent_y, frame_w / scale, frame_h / scale);
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
        // (a touch overscroll keeps its position past the range, input/touch.rs)
        let (scroll_x, scroll_y) = if self.l.overscroll.contains(&id) {
            position
        } else {
            (clamp(position.0, 0.0, max_x), clamp(position.1, 0.0, max_y))
        };
        if scroll_x != position.0 || scroll_y != position.1 {
            // (Layout.Writing: the invalidation hook ignores this write)
            self.set_vector(id, n_canvas_position, scroll_x, scroll_y);
        }

        // Measured (fixtures/layout-f32-lists2.luau, layout-f32-lists.luau; a 0.747 client, 51 of 51 values): AbsoluteCanvasSize
        // reads back through the canvas's far corner from the frame's AbsolutePosition, f32(f32(x + canvas) - x) (1643 at
        // x = 543.20007 reads 1643.00012).
        self.set_vector(id, n_canvas, f32r(f32r(record.x + canvas_w) - record.x), f32r(f32r(record.y + canvas_h) - record.y));
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
        // (children of a canvas size against it in float32 local units, f32(canvas / scale))
        if !auto_x {
            canvas_record.local_w = Some(f32r(canvas_record.content.w / scale));
        }
        if !auto_y {
            canvas_record.local_h = Some(f32r(canvas_record.content.h / scale));
        }
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
        self.l.open.push((id, [record.x, record.y, record.w, record.h], record.window, record.rotation));
        self.layout_children(id, &canvas_record, false, None);
        self.l.open.pop();
        self.l.carry_apply = outer_apply;
        self.l.carry_capture = outer_capture;
    }

    /// Whether `id`'s box lies wholly outside the clip its ancestors give it (ClipsDescendants frames, scroll
    /// windows, CanvasGroups; export.rs `outside`), unturned and in no scrolled ScrollingFrame: the client does not
    /// lay such a label out for drawing.
    fn outside_clip(&self, id: InstanceId, record: &Record) -> bool {
        let n = self.l.ids.n;
        let c = self.l.ids.c;
        if record.rotation != 0.0 {
            return false;
        }
        let mut chain: Vec<InstanceId> = Vec::new();
        let mut at = self.parent(id);
        while let Some(ancestor) = at {
            if !self.is_a(ancestor, c.GuiObject) {
                break;
            }
            chain.push(ancestor);
            at = self.parent(ancestor);
        }
        let mut clip: Option<[f64; 4]> = None;
        for &ancestor in chain.iter().rev() {
            // (an ancestor whose children are being laid out has its record out of `records`)
            let open = self.l.open.iter().rev().find(|(open, ..)| *open == ancestor).map(|&(_, rect, window, rotation)| (rect, window, rotation));
            let Some((rect, window, rotation)) = open.or_else(|| self.l.records.get(ancestor).map(|r| ([r.x, r.y, r.w, r.h], r.window, r.rotation))) else {
                return false;
            };
            if rotation != 0.0 {
                return false;
            }
            if self.class(ancestor) == c.ScrollingFrame {
                if let Some((x, y)) = self.vec2(ancestor, n.CanvasPosition) {
                    if x != 0.0 || y != 0.0 {
                        return false;
                    }
                }
            }
            if self.is_true(ancestor, n.ClipsDescendants) || self.is_a(ancestor, c.CanvasGroup) {
                // (unturned: AbsolutePosition is the record's corner; a located scroll window is absolute)
                let rect = match window {
                    Some(window) => [window.x, window.y, window.w, window.h],
                    None => rect,
                };
                let (mut x1, mut y1, mut x2, mut y2) = (rect[0], rect[1], rect[0] + rect[2], rect[1] + rect[3]);
                if let Some(outer) = clip {
                    x1 = x1.max(outer[0]);
                    y1 = y1.max(outer[1]);
                    x2 = x2.min(outer[0] + outer[2]);
                    y2 = y2.min(outer[1] + outer[3]);
                }
                clip = Some([f32r(x1), f32r(y1), f32r((x2 - x1).max(0.0)), f32r((y2 - y1).max(0.0))]);
            }
        }
        let Some(clip) = clip else { return false };
        if clip[2] <= 0.0 || clip[3] <= 0.0 {
            return true;
        }
        record.x + record.w <= clip[0] || record.x >= clip[0] + clip[2] || record.y + record.h <= clip[1] || record.y >= clip[1] + clip[3]
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
            // (a ScreenGui that is not displayed, in ReplicatedStorage or inside a frame there, reads the screen's size too:
            // measured in a 0.747 client, notes/layout.md "ScreenGuis inside GuiObjects")
            if class == c.ScreenGui {
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
            let mut record = Record {
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
            self.l.count_stack.push(0);
            self.layout_children(root, &record, false, None);
            record.count = 1 + self.l.count_stack.pop().unwrap_or(0);
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
            let record = self.place_child(root, &parent, sized.w, sized.h, None, false, Some((sized.base_w, sized.base_h)));
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
        if self.l.track_churn {
            self.l.pass_churn.clear();
        }
        // (a pass after a geometry write in this tree: a client re-lays its unchanged objects too, Stats::relaid)
        self.l.relaying = self.l.take_relay(root);
        let relaid_before = (self.l.stats.relaid, self.l.stats.relaid_under);
        self.l.count_stack.clear();
        self.layout_root(root);
        self.l.relaying = None;
        if self.l.track_churn {
            let relaid = (self.l.stats.relaid - relaid_before.0, self.l.stats.relaid_under - relaid_before.1);
            self.attribute_pass_churn(root, relaid);
        }
        let elapsed = super::elapsed(started);
        self.l.stats.time += elapsed;
        self.l.profile_time += elapsed;
        self.l.stats.passes += 1;
    }

    /// Attributes every `stored` and `moved` record produced by `layout_root(root)` back to the
    /// object(s) in `root`'s tree whose change dirtied the pass (`dirty_sources`), so ancestors
    /// re-stored only because a descendant changed transfer their records to the changed
    /// descendant(s), and reflowed siblings/descendants transfer theirs to the nearest changed
    /// container/source. Conserves exact totals across all attributed drivers. `relaid`: the pass's re-laid records (and
    /// those under them), shared by the pass's sources.
    fn attribute_pass_churn(&mut self, root: InstanceId, relaid: (u64, u64)) {
        let raw = std::mem::take(&mut self.l.pass_churn);
        let mut sources_in_root: Vec<InstanceId> = Vec::new();
        let mut remove_sources: Vec<InstanceId> = Vec::new();
        for &src in &self.l.dirty_sources {
            if self.state.dom.get(src).is_none() {
                remove_sources.push(src);
            } else if self.find_root(src) == root {
                sources_in_root.push(src);
                remove_sources.push(src);
            }
        }
        for src in remove_sources {
            self.l.dirty_sources.remove(&src);
        }
        if relaid != (0, 0) {
            let targets: Vec<InstanceId> = if sources_in_root.is_empty() { vec![root] } else { sources_in_root.clone() };
            let share = 1.0 / targets.len() as f64;
            for src in targets {
                let entry = self.l.churn.entry(src).or_default();
                entry.relaid += relaid.0 as f64 * share;
                entry.relaid_under += relaid.1 as f64 * share;
            }
        }
        if raw.is_empty() {
            return;
        }
        if sources_in_root.is_empty() {
            // Full invalidation (`Layout::invalidate`) with no individual dirty source
            for (id, (stored, moved)) in raw {
                let entry = self.l.churn.entry(id).or_default();
                entry.stored += stored as f64;
                entry.moved += moved as f64;
            }
            return;
        }
        let source_set: rustc_hash::FxHashSet<InstanceId> = sources_in_root.iter().copied().collect();
        let mut descendants: rustc_hash::FxHashMap<InstanceId, Vec<InstanceId>> = rustc_hash::FxHashMap::default();
        for &src in &sources_in_root {
            let mut cur = self.parent(src);
            while let Some(ancestor) = cur {
                descendants.entry(ancestor).or_default().push(src);
                if ancestor == root {
                    break;
                }
                cur = self.parent(ancestor);
            }
        }
        for (id, (stored, moved)) in raw {
            if stored == 0 && moved == 0 {
                continue;
            }
            let (s_f, m_f) = (stored as f64, moved as f64);
            if source_set.contains(&id) {
                let entry = self.l.churn.entry(id).or_default();
                entry.stored += s_f;
                entry.moved += m_f;
                continue;
            }
            if let Some(ds) = descendants.get(&id) {
                let share = 1.0 / ds.len() as f64;
                for &src in ds {
                    let entry = self.l.churn.entry(src).or_default();
                    entry.stored += s_f * share;
                    entry.ancestor_stored += s_f * share;
                    entry.moved += m_f * share;
                }
                continue;
            }
            let mut attributed = false;
            let mut cur = self.parent(id);
            while let Some(ancestor) = cur {
                if source_set.contains(&ancestor) {
                    let entry = self.l.churn.entry(ancestor).or_default();
                    entry.stored += s_f;
                    entry.moved += m_f;
                    attributed = true;
                    break;
                }
                if let Some(ds) = descendants.get(&ancestor) {
                    let share = 1.0 / ds.len() as f64;
                    for &src in ds {
                        let entry = self.l.churn.entry(src).or_default();
                        entry.stored += s_f * share;
                        entry.moved += m_f * share;
                    }
                    attributed = true;
                    break;
                }
                if ancestor == root {
                    break;
                }
                cur = self.parent(ancestor);
            }
            if !attributed {
                let entry = self.l.churn.entry(id).or_default();
                entry.stored += s_f;
                entry.moved += m_f;
            }
        }
    }

    /// DisplayedRoots: LayerCollectors under CoreGui / StarterGui (edit mode) / the players'
    /// PlayerGuis, and the displayed ones of `nested` (layers.rs `nested_screen_guis`: ScreenGuis inside GuiObjects, each a
    /// root of its own), by DisplayOrder then creation.
    pub(crate) fn displayed_roots(&self, game: InstanceId, nested: &[InstanceId]) -> Vec<InstanceId> {
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
        for &id in nested {
            if self.state.dom.get(id).is_some() && !result.contains(&id) && self.is_displayed(id) {
                result.push(id);
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
