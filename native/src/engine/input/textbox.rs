//! TextBox focus and editing (port of `runtime/input/textbox.luau`). Measured in Studio
//! (tools/interaction/experiments/textbox-focus.mjs, caret.mjs, caret2.mjs; notes/interaction.md):
//!   - CaptureFocus: Focused, UIS.TextBoxFocused, CursorPosition = #Text + 1, SelectionStart -1;
//!     ClearTextOnFocus empties Text. Stealing focus with CaptureFocus fires only FocusLost(false)
//!     on the old box and leaves its CursorPosition / SelectionStart as they were.
//!   - ReleaseFocus(submitted) / a click elsewhere: FocusLost(submitted, input or nil),
//!     UIS.TextBoxFocusReleased, then CursorPosition = SelectionStart = -1.
//!   - A click places the caret at the character boundary whose threshold lies 2/3 into each
//!     character; fast clicks at one spot count up: a double click selects the word, a triple
//!     (and fourth) click the whole text, a fifth starts over. Dragging selects from the press.
//!   - Script writes: CursorPosition is clamped to [.., #Text + 1]; writing Text keeps it.
//!   - Caret: 1 logical px wide, the line box tall, TextColor3 (PlaceholderColor3 while the
//!     placeholder shows); it blinks 0.3 s on / 0.3 s off from focus. Selection: the line box
//!     between the ends in (107, 161, 249), selected glyphs white.
//! Keyboard editing could not be measured (arrows, Home/End, Backspace/Delete, Shift selection,
//! Ctrl word moves, Ctrl+A/C/X/V and Return follow common text-field behaviour).

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use mlua::prelude::*;
use serde_json::{Map, Value as Json, json};

use crate::engine::GuiEngine;
use super::{Cx, Input, Modifiers, clamp};
use crate::engine::{
    ClassId, Engine, InstanceId, State,
    dom::flags,
    export::number,
    instance as core,
    layout::Layout,
    lua::{none, raise, values},
    services::{Behaviors, SetterFn},
    text::utf8::codepoints,
    value::{Str, Value},
};

const BLINK_PERIOD: f64 = 0.3;
const MULTI_CLICK_TIME: f64 = 0.5;
const MULTI_CLICK_DISTANCE: f64 = 2.0;
const SELECTION_COLOR: [f64; 3] = [107.0, 161.0, 249.0];

#[derive(Clone, Copy)]
pub struct Click {
    time: f64,
    x: f64,
    y: f64,
    count: u32,
    pub target: InstanceId,
}

#[derive(Default)]
pub struct TextState {
    pub(crate) focused: Cell<Option<InstanceId>>,
    blink_start: Cell<f64>,
    last_blink_visible: Cell<Option<bool>>,
    clipboard: RefCell<Vec<u8>>,
    pub(crate) pending_clipboard: RefCell<Option<Vec<u8>>>,
    pub(crate) last_click: RefCell<Option<Click>>,
    /// Selection drag: the press anchor while the mouse button is held after pressing in the box
    drag_anchor: Cell<Option<f64>>,
}

/// TextBox.FocusedBox
pub(crate) fn focused_box(cx: &Cx) -> Option<InstanceId> {
    let focused = cx.i.text.focused.get()?;
    let destroyed = cx.engine.state.borrow().dom.get(focused).is_some_and(|i| i.has(flags::DESTROYED));
    if destroyed {
        cx.i.text.focused.set(None);
        return None;
    }
    Some(focused)
}

pub(crate) fn paste(cx: &Cx, value: &[u8]) {
    // A copy/cut earlier in this input batch precedes the host clipboard echo.
    if cx.i.text.pending_clipboard.borrow().is_none() {
        *cx.i.text.clipboard.borrow_mut() = value.to_vec();
    }
    let clipboard = cx.i.text.clipboard.borrow().clone();
    type_text(cx, &clipboard);
}

fn reset_blink(cx: &Cx, id: InstanceId) {
    cx.i.text.blink_start.set(cx.now());
    cx.i.text.last_blink_visible.set(Some(true));
    cx.repaint(id);
}

fn cursor_value(cx: &Cx, id: InstanceId, name: crate::engine::NameId) -> Option<f64> {
    cx.number(id, name)
}

fn set_cursor(cx: &Cx, id: InstanceId, cursor: f64, selection: Option<f64>) {
    let length = cx.text(id).len() as f64;
    let cursor = clamp(cursor, 1.0, length + 1.0);
    let mut start = selection.unwrap_or(-1.0);
    if start != -1.0 {
        start = clamp(start, 1.0, length + 1.0);
        if start == cursor {
            start = -1.0;
        }
    }
    let n = &cx.i.n;
    if cursor_value(cx, id, n.CursorPosition) != Some(cursor) {
        cx.set(id, n.CursorPosition, Value::Number(cursor));
    }
    if cursor_value(cx, id, n.SelectionStart) != Some(start) {
        cx.set(id, n.SelectionStart, Value::Number(start));
    }
    reset_blink(cx, id);
}

// UTF-8 character boundaries

fn boundaries(value: &[u8]) -> Vec<f64> {
    let mut result: Vec<f64> = codepoints(value).into_iter().map(|(_, position)| position as f64).collect();
    result.push(value.len() as f64 + 1.0);
    result
}

fn previous_boundary(value: &[u8], cursor: f64) -> f64 {
    let mut previous = 1.0;
    for position in boundaries(value) {
        if position >= cursor {
            return previous;
        }
        previous = position;
    }
    previous
}

fn next_boundary(value: &[u8], cursor: f64) -> f64 {
    for position in boundaries(value) {
        if position > cursor {
            return position;
        }
    }
    value.len() as f64 + 1.0
}

/// Lua's %s (isspace in the C locale)
fn is_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | 0x0B | 0x0C | b'\r')
}

/// string.sub(Value, Position, Position) of a whole number position
fn byte_at(value: &[u8], position: f64) -> Option<u8> {
    let len = value.len() as f64;
    let mut index = position;
    if index < 0.0 {
        index = (len + index + 1.0).max(1.0);
    } else if index == 0.0 {
        index = 1.0;
    }
    if index > len || index < 1.0 {
        return None;
    }
    // (string.sub(s, i, i) with a negative i yields "" when i and j differ after adjustment)
    if position < 0.0 {
        let j = len + position + 1.0;
        if j < index {
            return None;
        }
    }
    value.get(index as usize - 1).copied()
}

fn char_class(value: &[u8], position: f64) -> u32 {
    let Some(byte) = byte_at(value, position) else { return 0 };
    if is_space(byte) {
        return 1;
    }
    if byte.is_ascii_alphanumeric() || byte == b'_' || byte >= 128 {
        return 2;
    }
    3
}

/// The run of characters of one class (word / whitespace / punctuation) containing `position`.
fn run_around(value: &[u8], position: f64) -> (f64, f64) {
    let len = value.len() as f64;
    if len == 0.0 {
        return (1.0, 1.0);
    }
    let position = clamp(position, 1.0, len);
    let class = char_class(value, position);
    let (mut start, mut finish) = (position, position);
    while start > 1.0 && char_class(value, start - 1.0) == class {
        start -= 1.0;
    }
    while finish <= len && char_class(value, finish) == class {
        finish += 1.0;
    }
    (start, finish)
}

/// Ctrl+Left (to the start of the previous word)
fn word_left(value: &[u8], cursor: f64) -> f64 {
    let mut position = cursor - 1.0;
    while position >= 1.0 && char_class(value, position) == 1 {
        position -= 1.0;
    }
    if position < 1.0 {
        return 1.0;
    }
    let class = char_class(value, position);
    while position > 1.0 && char_class(value, position - 1.0) == class {
        position -= 1.0;
    }
    position
}

/// Ctrl+Right (to the start of the next word)
fn word_right(value: &[u8], cursor: f64) -> f64 {
    let len = value.len() as f64;
    let mut position = cursor;
    if position > len {
        return len + 1.0;
    }
    let class = char_class(value, position);
    while position <= len && char_class(value, position) == class {
        position += 1.0;
    }
    while position <= len && char_class(value, position) == 1 {
        position += 1.0;
    }
    position
}

struct GlyphBox {
    cluster: f64,
    x: f64,
    advance: f64,
}

struct LineBox {
    top: f64,
    height: f64,
    left: f64,
    glyphs: Vec<GlyphBox>,
}

struct Geometry {
    lines: Vec<LineBox>,
    placeholder: bool,
}

/// Line layout of the box's displayed text in physical px (the scene exporter's placement).
fn geometry_of(input: &Input, engine: &Engine, state: &State, layout: &Layout, id: InstanceId) -> Option<Geometry> {
    let record = layout.record(id)?;
    let info = record.text.as_ref()?;
    let result = &info.layout;
    let pixel_scale = layout.pixel_scale;
    let content = record.content;
    let (content_x, content_y) = (content.x * pixel_scale, content.y * pixel_scale);
    let (content_w, content_h) = (content.w * pixel_scale, content.h * pixel_scale);
    let align = |name| match state.dom.raw_get(id, name) {
        Some(Value::EnumItem(item)) => engine.reflection.enum_item_info(*item).name.to_string(),
        _ => "Center".to_string(),
    };
    let x_align = align(input.n.TextXAlignment);
    let y_align = align(input.n.TextYAlignment);
    let top = match y_align.as_str() {
        "Top" => content_y,
        "Bottom" => content_y + content_h - result.height,
        _ => content_y + (content_h - result.height) / 2.0,
    };
    let lines = result
        .lines
        .iter()
        .map(|line| {
            let left = match x_align.as_str() {
                "Left" => content_x,
                "Right" => content_x + content_w - line.width,
                _ => content_x + (content_w - line.width) / 2.0,
            };
            LineBox {
                top: top + line.top,
                height: line.height,
                left,
                glyphs: line.glyphs.iter().map(|g| GlyphBox { cluster: g.cluster as f64, x: g.x, advance: g.advance }).collect(),
            }
        })
        .collect();
    let text_empty = match state.dom.raw_get(id, input.n.Text) {
        Some(Value::String(s)) => s.as_bytes().is_empty(),
        _ => true,
    };
    Some(Geometry { lines, placeholder: text_empty })
}

fn geometry(cx: &Cx, id: InstanceId) -> Option<Geometry> {
    crate::engine::layout::ensure(cx.engine, id);
    let state = cx.engine.state.borrow();
    let guard = cx.engine.layout().borrow();
    geometry_of(cx.i, cx.engine, &state, guard.as_ref()?, id)
}

/// Caret x (physical px) and 1-based line index of byte offset `cursor` in the displayed layout.
fn caret_place(geometry: &Geometry, cursor: f64) -> (f64, usize) {
    let lines = &geometry.lines;
    if geometry.placeholder || lines.is_empty() {
        return (lines.first().map_or(0.0, |l| l.left), 1);
    }
    for (index, line) in lines.iter().enumerate() {
        for glyph in &line.glyphs {
            if glyph.cluster == cursor {
                return (line.left + glyph.x, index + 1);
            }
        }
    }
    // After the last glyph of a line (end of a paragraph or of the text)
    for (index, line) in lines.iter().enumerate().rev() {
        if let Some(last) = line.glyphs.last() {
            if last.cluster < cursor {
                return (line.left + last.x + last.advance, index + 1);
            }
        }
    }
    (lines[0].left, 1)
}

/// Byte offset of the character boundary nearest to a screen point (logical px): measured
/// threshold 2/3 into each character.
fn cursor_at(cx: &Cx, id: InstanceId, x: f64, y: f64) -> f64 {
    let geo = geometry(cx, id);
    let value = cx.text(id);
    let Some(geo) = geo.filter(|g| !g.placeholder && !g.lines.is_empty()) else { return 1.0 };
    let pixel_scale = cx.engine.layout().borrow().as_ref().map_or(1.0, |l| l.pixel_scale);
    let (px, py) = (x * pixel_scale, y * pixel_scale);
    let mut line = &geo.lines[0];
    for candidate in &geo.lines {
        if py >= candidate.top {
            line = candidate;
        }
    }
    for glyph in &line.glyphs {
        if px < line.left + glyph.x + glyph.advance * (2.0 / 3.0) {
            return glyph.cluster;
        }
    }
    // Past the last character of the line: the end of the line's text
    match line.glyphs.last() {
        None => 1.0,
        Some(last) => next_boundary(&value, last.cluster),
    }
}

// Focus

/// TextBox.Focus(Box, Reason, Input)
pub(crate) fn focus(cx: &Cx, id: InstanceId, capture: bool, input_object: Option<InstanceId>) {
    let current = focused_box(cx);
    if current == Some(id) {
        return;
    }
    if let Some(current) = current {
        unfocus(cx, current, false, input_object, !capture);
    }
    let n = &cx.i.n;
    cx.i.text.focused.set(Some(id));
    cx.i.text.last_blink_visible.set(None);
    cx.fire(id, n.Focused, Vec::new());
    if let Some(uis) = cx.service("UserInputService") {
        cx.fire(uis, n.TextBoxFocused, vec![Value::Instance(id)]);
    }
    if !cx.is_false(id, n.ClearTextOnFocus) && !cx.is_false(id, n.TextEditable) {
        cx.set(id, n.Text, Value::String(Str::default()));
    }
    let length = cx.text(id).len() as f64;
    set_cursor(cx, id, length + 1.0, Some(-1.0));
}

/// `released` (ReleaseFocus, a click elsewhere, Return): UIS.TextBoxFocusReleased fires and the
/// cursor resets to -1; focus taken by another box's CaptureFocus fires only FocusLost.
pub(crate) fn unfocus(cx: &Cx, id: InstanceId, enter_pressed: bool, input_object: Option<InstanceId>, released: bool) {
    if cx.i.text.focused.get() != Some(id) {
        return;
    }
    let n = &cx.i.n;
    cx.i.text.focused.set(None);
    cx.i.text.drag_anchor.set(None);
    let mut args = vec![Value::Bool(enter_pressed)];
    if let Some(object) = input_object {
        args.push(Value::Instance(object));
    }
    cx.fire(id, n.FocusLost, args);
    if released {
        if let Some(uis) = cx.service("UserInputService") {
            cx.fire(uis, n.TextBoxFocusReleased, vec![Value::Instance(id)]);
        }
        cx.set(id, n.CursorPosition, Value::Number(-1.0));
        cx.set(id, n.SelectionStart, Value::Number(-1.0));
    }
    cx.repaint(id);
}

// Mouse

/// A mouse-button press whose sink is the TextBox `id` at logical (x, y).
pub(crate) fn press(cx: &Cx, id: InstanceId, x: f64, y: f64, input_object: InstanceId) -> LuaResult<()> {
    let now = cx.now();
    let mut count = 1;
    if let Some(previous) = *cx.i.text.last_click.borrow() {
        if previous.target == id
            && now - previous.time <= MULTI_CLICK_TIME
            && (x - previous.x).abs() <= MULTI_CLICK_DISTANCE
            && (y - previous.y).abs() <= MULTI_CLICK_DISTANCE
        {
            count = previous.count % 4 + 1;
        }
    }
    *cx.i.text.last_click.borrow_mut() = Some(Click { time: now, x, y, count, target: id });

    focus(cx, id, false, Some(input_object));
    let value = cx.text(id);
    let cursor = cursor_at(cx, id, x, y);
    let len = value.len() as f64;
    if count == 2 {
        let (start, finish) = run_around(&value, cursor.min(len));
        set_cursor(cx, id, finish, Some(start));
        cx.i.text.drag_anchor.set(None);
    } else if count >= 3 {
        set_cursor(cx, id, len + 1.0, Some(1.0));
        cx.i.text.drag_anchor.set(None);
    } else {
        set_cursor(cx, id, cursor, Some(-1.0));
        cx.i.text.drag_anchor.set(Some(cursor));
    }
    Ok(())
}

/// Mouse moved with the button held after a press in the focused box: extend the selection.
pub(crate) fn drag(cx: &Cx, x: f64, y: f64) -> LuaResult<()> {
    let Some(id) = focused_box(cx) else { return Ok(()) };
    let Some(anchor) = cx.i.text.drag_anchor.get() else { return Ok(()) };
    let cursor = cursor_at(cx, id, x, y);
    set_cursor(cx, id, cursor, Some(anchor));
    Ok(())
}

pub(crate) fn release(cx: &Cx) {
    cx.i.text.drag_anchor.set(None);
}

// Keyboard

fn selection(cx: &Cx, id: InstanceId) -> Option<(f64, f64)> {
    let start = cx.number(id, cx.i.n.SelectionStart).unwrap_or(-1.0);
    let cursor = cx.number(id, cx.i.n.CursorPosition).unwrap_or(-1.0);
    if start == -1.0 || start == cursor {
        return None;
    }
    Some((start.min(cursor), start.max(cursor)))
}

/// Lua string.sub(s, i, j) (j = None: to the end)
fn lua_sub(s: &[u8], i: f64, j: Option<f64>) -> &[u8] {
    let len = s.len() as f64;
    let mut i = i;
    let mut j = j.unwrap_or(-1.0);
    if i < 0.0 {
        i = (len + i + 1.0).max(0.0);
    }
    if j < 0.0 {
        j = len + j + 1.0;
    }
    if i < 1.0 {
        i = 1.0;
    }
    if j > len {
        j = len;
    }
    if i > j {
        return &[];
    }
    &s[(i as usize - 1)..(j as usize)]
}

fn replace(cx: &Cx, id: InstanceId, from: f64, to: f64, insert: &[u8]) {
    let value = cx.text(id);
    let mut new_text = lua_sub(&value, 1.0, Some(from - 1.0)).to_vec();
    new_text.extend_from_slice(insert);
    new_text.extend_from_slice(lua_sub(&value, to, None));
    cx.set(id, cx.i.n.Text, Value::String(Str::new(&new_text)));
    set_cursor(cx, id, from + insert.len() as f64, Some(-1.0));
}

fn editable(cx: &Cx, id: InstanceId) -> bool {
    !cx.is_false(id, cx.i.n.TextEditable)
}

/// Typed text (the `text` event): replaces the selection or inserts at the caret.
pub(crate) fn type_text(cx: &Cx, typed: &[u8]) -> bool {
    let Some(id) = focused_box(cx) else { return false };
    if !editable(cx, id) || typed.is_empty() {
        return false;
    }
    let mut typed = typed.to_vec();
    if !cx.is_true(id, cx.i.n.MultiLine) {
        typed.retain(|&b| b != b'\r' && b != b'\n');
        if typed.is_empty() {
            return true;
        }
    }
    let (from, to) = match selection(cx, id) {
        Some(range) => range,
        None => {
            let cursor = cx.number(id, cx.i.n.CursorPosition).unwrap_or(f64::NAN);
            (cursor, cursor)
        }
    };
    replace(cx, id, from, to, &typed);
    true
}

/// Line-relative caret moves for MultiLine boxes (Up/Down keep the caret x).
fn vertical_move(cx: &Cx, id: InstanceId, cursor: f64, direction: i64) -> f64 {
    let Some(geo) = geometry(cx, id).filter(|g| !g.placeholder) else { return cursor };
    let value = cx.text(id);
    let (x, line_index) = caret_place(&geo, cursor);
    let target = line_index as i64 + direction;
    let Some(target) = (target >= 1).then(|| geo.lines.get(target as usize - 1)).flatten() else {
        return if direction < 0 { 1.0 } else { value.len() as f64 + 1.0 };
    };
    let pixel_scale = cx.engine.layout().borrow().as_ref().map_or(1.0, |l| l.pixel_scale);
    let (tx, ty) = (x / pixel_scale, (target.top + target.height / 2.0) / pixel_scale);
    cursor_at(cx, id, tx, ty)
}

fn line_bounds(value: &[u8], cursor: f64) -> (f64, f64) {
    let len = value.len() as f64;
    let mut start = cursor;
    while start > 1.0 && byte_at(value, start - 1.0) != Some(b'\n') {
        start -= 1.0;
    }
    let mut finish = cursor;
    while finish <= len && byte_at(value, finish) != Some(b'\n') {
        finish += 1.0;
    }
    (start, finish)
}

/// A key press while a box is focused; true when the box used it.
pub(crate) fn key(cx: &Cx, key_name: &str, modifiers: &Modifiers, input_object: InstanceId) -> LuaResult<bool> {
    let Some(id) = focused_box(cx) else { return Ok(false) };
    let n = &cx.i.n;
    let value = cx.text(id);
    let len = value.len() as f64;
    let cursor = cx.number(id, n.CursorPosition).unwrap_or(len + 1.0);
    let start = cx.number(id, n.SelectionStart).unwrap_or(-1.0);
    let range = selection(cx, id);
    let (shift, ctrl) = (modifiers.shift, modifiers.ctrl);
    let edit = editable(cx, id);
    let move_to = |target: f64| {
        if shift {
            set_cursor(cx, id, target, Some(if start != -1.0 { start } else { cursor }));
        } else {
            set_cursor(cx, id, target, Some(-1.0));
        }
    };
    match key_name {
        "Return" | "KeypadEnter" => {
            if cx.is_true(id, n.MultiLine) && edit {
                type_text(cx, b"\n");
                return Ok(true);
            }
            unfocus(cx, id, true, Some(input_object), true);
            Ok(true)
        }
        "Backspace" => {
            if !edit {
                return Ok(true);
            }
            if let Some((from, to)) = range {
                replace(cx, id, from, to, b"");
            } else if cursor > 1.0 {
                let target = if ctrl { word_left(&value, cursor) } else { previous_boundary(&value, cursor) };
                replace(cx, id, target, cursor, b"");
            }
            Ok(true)
        }
        "Delete" => {
            if !edit {
                return Ok(true);
            }
            if let Some((from, to)) = range {
                replace(cx, id, from, to, b"");
            } else if cursor <= len {
                let target = if ctrl { word_right(&value, cursor) } else { next_boundary(&value, cursor) };
                replace(cx, id, cursor, target, b"");
            }
            Ok(true)
        }
        "Left" => {
            match range {
                Some((from, _)) if !shift => set_cursor(cx, id, from, Some(-1.0)),
                _ => move_to(if ctrl { word_left(&value, cursor) } else { previous_boundary(&value, cursor) }),
            }
            Ok(true)
        }
        "Right" => {
            match range {
                Some((_, to)) if !shift => set_cursor(cx, id, to, Some(-1.0)),
                _ => move_to(if ctrl { word_right(&value, cursor) } else { next_boundary(&value, cursor) }),
            }
            Ok(true)
        }
        "Home" => {
            let (line_start, _) = line_bounds(&value, cursor);
            move_to(if ctrl { 1.0 } else { line_start });
            Ok(true)
        }
        "End" => {
            let (_, line_end) = line_bounds(&value, cursor);
            move_to(if ctrl { len + 1.0 } else { line_end });
            Ok(true)
        }
        "Up" | "Down" => {
            if cx.is_true(id, n.MultiLine) {
                move_to(vertical_move(cx, id, cursor, if key_name == "Up" { -1 } else { 1 }));
            }
            Ok(true)
        }
        "A" if ctrl => {
            set_cursor(cx, id, len + 1.0, Some(1.0));
            Ok(true)
        }
        "C" | "X" if ctrl => {
            if let Some((from, to)) = range {
                let copied = lua_sub(&value, from, Some(to - 1.0)).to_vec();
                *cx.i.text.clipboard.borrow_mut() = copied.clone();
                *cx.i.text.pending_clipboard.borrow_mut() = Some(copied);
                if key_name == "X" && edit {
                    replace(cx, id, from, to, b"");
                }
            }
            Ok(true)
        }
        "V" if ctrl => {
            let clipboard = cx.i.text.clipboard.borrow().clone();
            type_text(cx, &clipboard);
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn blink_visible(now: f64, start: f64) -> bool {
    (((now - start) / BLINK_PERIOD + 1e-9).floor()).rem_euclid(2.0) == 0.0
}

/// Visuals for the scene exporter: caret and selection of the focused box (physical px),
/// blinking on virtual time.
pub(crate) fn visual(input: &Input, engine: &Engine, state: &State, layout: &Layout, id: InstanceId) -> Option<Map<String, Json>> {
    let focused = input.text.focused.get()?;
    if focused != id || state.dom.get(id).is_none_or(|i| i.has(flags::DESTROYED)) {
        return None;
    }
    let geo = geometry_of(input, engine, state, layout, id)?;
    let value_len = match state.dom.raw_get(id, input.n.Text) {
        Some(Value::String(s)) => s.as_bytes().len() as f64,
        _ => 0.0,
    };
    let pixel_scale = layout.pixel_scale;
    let cursor = state.dom.raw_get(id, input.n.CursorPosition).and_then(Value::as_number).unwrap_or(value_len + 1.0);
    let (x, line_index) = caret_place(&geo, cursor);
    let line = geo.lines.get(line_index.wrapping_sub(1));
    let color = state
        .dom
        .raw_get(id, if geo.placeholder { input.n.PlaceholderColor3 } else { input.n.TextColor3 })
        .and_then(Value::as_color3);
    let visible = blink_visible(state.sched.now, input.text.blink_start.get());
    let color = match color {
        Some(c) => vec![number(c.r as f64), number(c.g as f64), number(c.b as f64)],
        None => vec![number(0.0), number(0.0), number(0.0)],
    };
    let mut result = Map::new();
    result.insert(
        "caret".into(),
        json!({
            "x": number(x),
            "y": number(line.map_or(0.0, |l| l.top)),
            "w": number(pixel_scale),
            "h": number(line.map_or(0.0, |l| l.height)),
            "color": color,
            "visible": visible,
        }),
    );
    let start = state.dom.raw_get(id, input.n.SelectionStart).and_then(Value::as_number).unwrap_or(-1.0);
    let cursor_raw = state.dom.raw_get(id, input.n.CursorPosition).and_then(Value::as_number).unwrap_or(-1.0);
    if start != -1.0 && start != cursor_raw && !geo.placeholder {
        let (from, to) = (start.min(cursor_raw), start.max(cursor_raw));
        let mut rects = Vec::new();
        let (start_x, start_line) = caret_place(&geo, from);
        let (end_x, end_line) = caret_place(&geo, to);
        for index in start_line..=end_line {
            let Some(current) = geo.lines.get(index - 1) else { continue };
            let line_end = current.left + current.glyphs.last().map_or(0.0, |g| g.x + g.advance);
            let x0 = if index == start_line { start_x } else { current.left };
            let x1 = if index == end_line { end_x } else { line_end };
            if x1 > x0 {
                rects.push(json!([number(x0), number(current.top), number(x1 - x0), number(current.height)]));
            }
        }
        result.insert(
            "selection".into(),
            json!({
                "rects": rects,
                "color": SELECTION_COLOR.iter().map(|v| number(*v)).collect::<Vec<_>>(),
                "textColor": [1, 1, 1],
                "from": number(from),
                "to": number(to),
            }),
        );
    }
    Some(result)
}

/// Per frame: repaint the focused box when its caret blink phase flips.
pub(crate) fn step(cx: &Cx) {
    let Some(id) = focused_box(cx) else { return };
    let visible = blink_visible(cx.now(), cx.i.text.blink_start.get());
    if Some(visible) != cx.i.text.last_blink_visible.get() {
        cx.i.text.last_blink_visible.set(Some(visible));
        cx.repaint(id);
    }
}

/// TextBox.Changed("Text") (from the input's hook): a Text write that leaves CursorPosition /
/// SelectionStart past the end clamps them to #Text + 1 (measured, replay scenario textbox).
pub(crate) fn text_changed(engine: &Engine, input: &Input, state: &mut State, id: InstanceId) {
    let limit = match state.dom.raw_get(id, input.n.Text) {
        Some(Value::String(s)) => s.as_bytes().len() as f64 + 1.0,
        _ => 1.0,
    };
    for name in [input.n.CursorPosition, input.n.SelectionStart] {
        let value = state.dom.raw_get(id, name).and_then(Value::as_number);
        if value.is_some_and(|v| v > limit) {
            let hooks = engine.hooks.borrow();
            core::raw_set(state, &engine.reflection, &hooks, id, name, Value::Number(limit));
        }
    }
    if input.text.focused.get() == Some(id) {
        let hooks = engine.hooks.borrow();
        for hook in &hooks.property_changed {
            hook(state, id, input.n.GuiState);
        }
    }
}

/// Script API: CaptureFocus, ReleaseFocus, IsFocused, the CursorPosition setter.
pub(crate) fn install(behaviors: &mut Behaviors, text_box: ClassId, input: &Rc<Input>) {
    let n = &input.n;
    let i = input.clone();
    behaviors.add_method(text_box, n.CaptureFocus, Rc::new(move |lua, engine, id, _| {
        let cx = Cx { lua, engine, i: &i };
        focus(&cx, id, true, None);
        i.sync_pins(engine);
        Ok(none())
    }));
    let i = input.clone();
    behaviors.add_method(text_box, n.ReleaseFocus, Rc::new(move |lua, engine, id, args| {
        let cx = Cx { lua, engine, i: &i };
        unfocus(&cx, id, matches!(args.get(1), LuaValue::Boolean(true)), None, true);
        i.sync_pins(engine);
        Ok(none())
    }));
    let i = input.clone();
    behaviors.add_method(text_box, n.IsFocused, Rc::new(move |lua, engine, id, _| {
        let focused = i.focused_text_box(engine) == Some(id);
        let _ = lua;
        Ok(values([LuaValue::Boolean(focused)]))
    }));
    // Measured: CursorPosition writes clamp to #Text + 1 (100 -> 14 for a 13-byte text)
    let i = input.clone();
    let setter: SetterFn = Rc::new(move |lua, engine, id, value| {
        let number = match &value {
            LuaValue::Number(n) => Some(*n),
            LuaValue::Integer(n) => Some(*n as f64),
            LuaValue::String(s) => lua.coerce_number(LuaValue::String(s.clone())).ok().flatten(),
            _ => None,
        };
        let Some(number) = number else {
            return raise(format!(
                "Unable to assign property CursorPosition. number expected, got {}",
                crate::engine::lua::typeof_value(engine, &value)
            ));
        };
        let cx = Cx { lua, engine, i: &i };
        let mut number = number.floor();
        if number != -1.0 {
            number = clamp(number, 1.0, cx.text(id).len() as f64 + 1.0);
        }
        cx.set(id, i.n.CursorPosition, Value::Number(number));
        if focused_box(&cx) == Some(id) {
            reset_blink(&cx, id);
        }
        Ok(())
    });
    behaviors.add_setter(text_box, n.CursorPosition, setter);
}
