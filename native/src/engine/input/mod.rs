//! Input emulation (port of `runtime/input/init.luau`): hit-testing, GuiObject / GuiButton / TextBox
//! / ScrollingFrame input and UserInputService / legacy Mouse state, following rules measured in
//! Roblox Studio (notes/interaction.md, experiments in tools/interaction/).
//!
//!   install(engine)                once, from Engine.Start after the layout (lua::globals::start)
//!   dispatch(lua, engine, &event)  one input event in logical viewport pixels (session.rs, story.rs
//!                                  timelines, UserInputService:CreateVirtualInput):
//!     {type="move", x, y}  {type="down"|"up", button=1|2|3, x, y}  {type="leave"}
//!     {type="wheel", notches (+1 = one notch away from the user) | dy (px, 100 per notch), x, y}
//!     {type="key", key=<Enum.KeyCode name>, down, shift?, ctrl?, alt?}  {type="text", text}
//!     {type="paste", text}
//!
//! Handlers run deferred like the engine's signals, in firing order; the deferred queue is flushed
//! after every event (measured: a FocusLost handler for a click sees the click's input still in
//! state Begin although the release came in the same frame).
//!
//! State lives in [`Input`] (`engine.input`), in cells borrowed only briefly: nothing is borrowed
//! across a property write (its hooks come back here), a Luau call or a deferred flush.
//! Iteration over the Luau runtime's hash tables keyed by instances (Touched, Motions, Presented,
//! KeysDown) follows insertion order here; the Luau order depends on addresses.

pub mod actions;
mod context_actions;
pub mod drag;
pub mod hit;
pub mod scrolling;
pub mod textbox;

#[cfg(test)]
mod tests;

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use mlua::prelude::*;
use rbx_dom_weak::types as dom;
use rustc_hash::{FxHashMap, FxHashSet};
use serde_json::{Map, Value as Json, json};

use crate::engine::GuiEngine;
use super::{
    ClassId, Engine, InstanceId, NameId, Reflection, State,
    dom::flags,
    instance as core,
    lua::{Args, Flow, Ret, convert, enum_item_value, instance_value, none, raise, values},
    reflection::EnumItemId,
    profile,
    scheduler::{self, Phase},
    services::{self, GetterFn, Method, MethodFn, SetterFn},
    signal::SignalId,
    value::{Str, Value},
};

macro_rules! names {
    ($($field:ident),* $(,)?) => {
        /// Property / event / method names the input reads (interned once).
        #[allow(non_snake_case)]
        pub struct Names { $(pub $field: NameId,)* }
        impl Names {
            fn new(r: &Reflection) -> Names {
                Names { $($field: r.name_id(stringify!($field)).unwrap_or(NameId::MAX),)* }
            }
        }
    };
}

names!(
    Name, UserInputType, UserInputState, KeyCode, Position, Delta, Active, GuiState, Interactable, ZIndex,
    ZIndexBehavior, Rotation, ClipsDescendants, Enabled, DisplayOrder, ScrollingEnabled, ScrollingDirection,
    AbsoluteCanvasSize, AbsoluteWindowSize, CanvasPosition, Text, CursorPosition, SelectionStart, ClearTextOnFocus,
    TextEditable, MultiLine, TextXAlignment, TextYAlignment, PlaceholderColor3, TextColor3, BackgroundColor3,
    AutoButtonColor, PressedImage, HoverImage, SelectedObject, Draggable, DragStyle, DragAxis, BoundingUI,
    MinDragTranslation, MaxDragTranslation, ResponseStyle, DragUDim2, Visible, MouseMoved, InputChanged, MouseLeave,
    InputEnded, MouseEnter, InputBegan, MouseButton1Down, MouseButton2Down, MouseButton1Click, MouseButton1Up,
    Activated, MouseButton2Click, SecondaryActivated, MouseButton2Up, MouseWheelForward, MouseWheelBackward,
    LastInputTypeChanged, SelectionLost, SelectionGained, Focused, FocusLost, TextBoxFocused, TextBoxFocusReleased,
    DragBegin, DragStopped, DragStart, DragContinue, DragEnd, GetMouseLocation, GetMouseDelta, IsMouseButtonPressed,
    GetMouseButtonsPressed, IsKeyDown, GetKeysPressed, GetFocusedTextBox, GetLastInputType, CreateVirtualInput,
    IsModifierKeyDown, GetGuiObjectsAtPosition, ScrollToTop, GetScrollVelocity, GetSampledInertialVelocity,
    ResetScrollVelocity, ClearInertialScrolling, CaptureFocus, ReleaseFocus, IsFocused,
    BindAction, BindActionAtPriority, UnbindAction, UnbindAllActions, Select, Selectable, SelectionOrder, PreferredInput,
    IsGamepadButtonDown, GetGamepadConnected, GetImageForKeyCode,
);

macro_rules! classes {
    ($($field:ident),* $(,)?) => {
        #[allow(non_snake_case)]
        pub struct Classes { $(pub $field: Option<ClassId>,)* }
        impl Classes {
            fn new(r: &Reflection) -> Classes {
                Classes { $($field: r.class_id(stringify!($field)),)* }
            }
        }
    };
}

classes!(
    GuiObject, GuiButton, TextBox, ScrollingFrame, ImageButton, CanvasGroup, ScreenGui, LayerCollector, Folder,
    UIDragDetector, InputObject, GuiBase2d, UIComponent, UIBase, CoreGui, StarterGui, Players, PlayerGui, PluginGui,
    DockWidgetPluginGui, UserInputService, GuiService, BasePlayerGui, ContextActionService,
);

/// The latest mouse position (GetMouseLocation); the hover state follows it once per frame
/// (processed = where it was last applied, pending = a newer position waits for the frame).
#[derive(Clone, Copy, Default)]
struct Mouse {
    x: f64,
    y: f64,
    inside: bool,
    last_move_delta: (f32, f32),
    processed_x: f64,
    processed_y: f64,
    pending: bool,
}

/// Per mouse button: the GuiButton that took the press (MouseButton*Down), the first GuiButton /
/// TextBox under the press (sink), whether it is still a click, the objects that got InputBegan.
#[derive(Clone)]
struct Press {
    button: Option<InstanceId>,
    sink: Option<InstanceId>,
    clickable: bool,
    receivers: Vec<InstanceId>,
}

/// Activated's clickCount (measured, clicks2.mjs): consecutive activations of a button less than
/// 0.5 s apart count up 0, 1, 2, ... without a limit, whatever the distance between the clicks.
const CLICK_TIMEOUT: f64 = 0.5;

const BUTTON_TYPES: [&str; 3] = ["MouseButton1", "MouseButton2", "MouseButton3"];

/// The cached draw order is stale after any change to a GUI object or component, or to the
/// hierarchy, except properties that only change how an object looks.
const VISUAL_ONLY: &[&str] = &[
    "GuiState", "BackgroundColor3", "BackgroundTransparency", "BorderColor3", "Transparency", "ImageColor3",
    "ImageTransparency", "TextColor3", "TextTransparency", "TextStrokeColor3", "TextStrokeTransparency", "GroupColor3",
    "GroupTransparency", "ScrollBarImageColor3", "ScrollBarImageTransparency", "AutoButtonColor", "Selected", "Active",
    "Color", "Offset", "Thickness", "LineJoinMode", "CornerRadius", "TopLeftRadius", "TopRightRadius",
    "BottomLeftRadius", "BottomRightRadius", "PlaceholderColor3", "CursorPosition", "SelectionStart",
];

pub struct Input {
    reflection: std::sync::Arc<Reflection>,
    pub n: Names,
    pub c: Classes,
    visual_only: FxHashSet<NameId>,
    mouse: Cell<Mouse>,
    /// GuiObjects the cursor is over (MouseEnter fired, MouseLeave not yet), topmost first
    hovered: RefCell<Vec<InstanceId>>,
    hovered_set: RefCell<FxHashSet<InstanceId>>,
    /// Presses by button number (ascending, like the Luau array)
    presses: RefCell<Vec<(f64, Press)>>,
    last_activation: RefCell<FxHashMap<InstanceId, (f64, f64)>>,
    objects: RefCell<FxHashMap<String, InstanceId>>,
    /// The input objects' userdata, held like the Luau runtime's InputObjects table holds their
    /// proxies (identity in scripts' weak-keyed tables survives between events)
    object_values: RefCell<Vec<LuaValue>>,
    keys_down: RefCell<Vec<(EnumItemId, InstanceId)>>,
    key_receivers: RefCell<FxHashMap<EnumItemId, Vec<InstanceId>>>,
    last_input_type: Cell<Option<EnumItemId>>,
    /// Objects whose GuiState may need recomputing after an event (insertion order)
    touched: RefCell<Vec<InstanceId>>,
    /// GuiService.SelectedObject and a selected button held by Return
    selected: Cell<Option<InstanceId>>,
    key_pressed: Cell<Option<InstanceId>>,
    /// Objects that "have" the wheel input
    wheel_set: RefCell<FxHashSet<InstanceId>>,
    wheel_list: RefCell<Vec<InstanceId>>,
    hit_dirty: Cell<bool>,
    hit_order: RefCell<Option<Rc<Vec<hit::Entry>>>>,
    /// The previous Hit.At query (draw order, point): the Luau same-point cache's hit counter
    hit_last: RefCell<Option<(Rc<Vec<hit::Entry>>, f64, f64)>>,
    pub(crate) scroll: scrolling::Scroll,
    pub(crate) text: textbox::TextState,
    drag: RefCell<Option<drag::Active>>,
    /// Instances the input holds strongly (the Luau tables keep them alive): pinned in the dom
    pinned: RefCell<FxHashSet<InstanceId>>,
    /// The legacy Mouse (Player:GetMouse) table and its signals, looked up on first use
    legacy: RefCell<Option<(LuaTable, FxHashMap<&'static str, SignalId>)>>,
    /// ContextActionService binds
    actions: context_actions::Actions,
}

/// The input of an engine (installed by Engine.Start).
pub fn get(engine: &Engine) -> Option<Rc<Input>> {
    engine.input().borrow().clone()
}

fn flow_error(flow: Flow) -> LuaError {
    match flow {
        Flow::Error(error) => LuaError::runtime(error.message),
        Flow::Fail(error) => error,
        Flow::Yield => LuaError::runtime("unexpected yield"),
    }
}

pub(crate) fn vector3(x: f64, y: f64, z: f64) -> Value {
    Value::Vector3(dom::Vector3::new(x as f32, y as f32, z as f32))
}

pub(crate) fn vector2(x: f64, y: f64) -> Value {
    Value::Vector2(dom::Vector2::new(x as f32, y as f32))
}

/// tonumber of an event field (numbers, numeric strings)
pub(crate) fn json_number(value: Option<&Json>) -> Option<f64> {
    match value? {
        Json::Number(n) => n.as_f64(),
        Json::String(text) => {
            let text = text.trim();
            if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
                return u64::from_str_radix(hex, 16).ok().map(|v| v as f64);
            }
            text.parse::<f64>().ok().filter(|_| !text.is_empty() && !text.eq_ignore_ascii_case("inf") && !text.eq_ignore_ascii_case("nan"))
        }
        _ => None,
    }
}

/// Luau `math.clamp`
pub(crate) fn clamp(value: f64, min: f64, max: f64) -> f64 {
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}

/// The running context of an input operation.
pub(crate) struct Cx<'a> {
    pub lua: &'a Lua,
    pub engine: &'a Rc<Engine>,
    pub i: &'a Input,
}

impl Cx<'_> {
    pub fn get(&self, id: InstanceId, name: NameId) -> Option<Value> {
        let state = self.engine.state.borrow();
        state.dom.get(id)?;
        state.dom.raw_get(id, name).cloned()
    }

    pub fn number(&self, id: InstanceId, name: NameId) -> Option<f64> {
        self.get(id, name).and_then(|v| v.as_number())
    }

    pub fn vector2(&self, id: InstanceId, name: NameId) -> (f64, f64) {
        match self.get(id, name) {
            Some(Value::Vector2(v)) => (v.x as f64, v.y as f64),
            _ => (0.0, 0.0),
        }
    }

    pub fn is_false(&self, id: InstanceId, name: NameId) -> bool {
        matches!(self.get(id, name), Some(Value::Bool(false)))
    }

    pub fn is_true(&self, id: InstanceId, name: NameId) -> bool {
        matches!(self.get(id, name), Some(Value::Bool(true)))
    }

    pub fn enum_name(&self, id: InstanceId, name: NameId) -> String {
        match self.get(id, name) {
            Some(Value::EnumItem(item)) => self.engine.reflection.enum_item_info(item).name.to_string(),
            _ => String::new(),
        }
    }

    pub fn text(&self, id: InstanceId) -> Vec<u8> {
        match self.get(id, self.i.n.Text) {
            Some(Value::String(s)) => s.as_bytes().to_vec(),
            _ => Vec::new(),
        }
    }

    /// RawSet with change notifications
    pub fn set(&self, id: InstanceId, name: NameId, value: Value) -> bool {
        super::lua::instance::set_property(self.engine, id, name, value)
    }

    pub fn set_silent(&self, id: InstanceId, name: NameId, value: Value) {
        let mut state = self.engine.state.borrow_mut();
        core::raw_set_silent(&mut state, &self.engine.reflection, id, name, value);
    }

    /// FireSignal: an instance event, if it was ever created (deferred handlers)
    pub fn fire(&self, id: InstanceId, event: NameId, args: Vec<Value>) {
        let mut state = self.engine.state.borrow_mut();
        if state.dom.get(id).is_some() {
            core::fire_event(&mut state, id, event, args);
        }
    }

    pub fn alive(&self, id: InstanceId) -> bool {
        self.engine.state.borrow().dom.get(id).is_some_and(|instance| !instance.has(flags::DESTROYED))
    }

    pub fn is(&self, id: InstanceId, class: Option<ClassId>) -> bool {
        let state = self.engine.state.borrow();
        state.dom.get(id).is_some_and(|instance| self.i.class_is(instance.class, class))
    }

    pub fn class_name_is(&self, id: InstanceId, class: Option<ClassId>) -> bool {
        let state = self.engine.state.borrow();
        class.is_some() && state.dom.get(id).map(|instance| instance.class) == class
    }

    pub fn now(&self) -> f64 {
        self.engine.state.borrow().sched.now
    }

    pub fn item(&self, enum_name: &str, item: &str) -> Value {
        self.engine.reflection.enum_item(enum_name, item).map_or(Value::Nil, Value::EnumItem)
    }

    pub fn inset(&self) -> f64 {
        self.engine.options.topbar_inset
    }

    /// Game:GetService(name)
    pub fn service(&self, name: &str) -> Option<InstanceId> {
        services::get_service(self.engine, self.engine.world().game, name).ok()
    }

    /// Marks an instance's export stale without a script-visible property change (caret blink,
    /// selection): the PropertyChanged hooks see "GuiState" (the session's dirty flag; the
    /// layout and the hit cache ignore it).
    pub fn repaint(&self, id: InstanceId) {
        let hooks = self.engine.hooks.borrow();
        let mut state = self.engine.state.borrow_mut();
        if state.dom.get(id).is_none() {
            return;
        }
        for hook in &hooks.property_changed {
            hook(&mut state, id, self.i.n.GuiState);
        }
    }

    /// `instance[key] = value` (setters, coercion, errors)
    pub fn assign(&self, id: InstanceId, key: &str, value: LuaValue) -> LuaResult<()> {
        let key = LuaValue::String(self.lua.create_string(key)?);
        super::lua::instance::assign(self.lua, self.engine, id, &key, &value).map_err(flow_error)
    }

    pub fn to_lua(&self, value: &Value) -> LuaResult<LuaValue> {
        convert::to_lua(self.lua, self.engine, value)
    }

    /// A Luau datatype constructor (Datatypes.<library>.<function>(...)), for values scripts see
    pub fn datatype(&self, library: &str, function: &str, args: impl IntoLuaMulti) -> LuaResult<LuaValue> {
        let datatypes: LuaTable = self.engine.prelude().exports.get("Datatypes")?;
        let library: LuaTable = datatypes.get(library)?;
        library.get::<LuaFunction>(function)?.call(args)
    }

    /// Layout.Records[Proxy] (a copy of what the caller needs)
    pub fn record<T>(&self, id: InstanceId, f: impl FnOnce(&super::layout::Record) -> T) -> Option<T> {
        let guard = self.engine.layout().try_borrow().ok()?;
        let layout = guard.as_ref()?;
        layout.record(id).map(f)
    }

    pub fn update_layout(&self) {
        super::layout::update_all(self.engine);
    }
}

impl Input {
    fn new(reflection: &std::sync::Arc<Reflection>) -> Input {
        let n = Names::new(reflection);
        let c = Classes::new(reflection);
        let visual_only = VISUAL_ONLY.iter().filter_map(|name| reflection.name_id(name)).collect();
        Input {
            reflection: reflection.clone(),
            n,
            c,
            visual_only,
            mouse: Cell::new(Mouse::default()),
            hovered: RefCell::default(),
            hovered_set: RefCell::default(),
            presses: RefCell::default(),
            last_activation: RefCell::default(),
            objects: RefCell::default(),
            object_values: RefCell::default(),
            keys_down: RefCell::default(),
            key_receivers: RefCell::default(),
            last_input_type: Cell::new(None),
            touched: RefCell::default(),
            selected: Cell::new(None),
            key_pressed: Cell::new(None),
            wheel_set: RefCell::default(),
            wheel_list: RefCell::default(),
            hit_dirty: Cell::new(true),
            hit_order: RefCell::new(None),
            hit_last: RefCell::new(None),
            scroll: scrolling::Scroll::default(),
            text: textbox::TextState::default(),
            drag: RefCell::new(None),
            pinned: RefCell::default(),
            legacy: RefCell::new(None),
            actions: context_actions::Actions::default(),
        }
    }

    pub(crate) fn class_is(&self, class: ClassId, ancestor: Option<ClassId>) -> bool {
        let Some(ancestor) = ancestor else { return false };
        self.reflection.class(class).is_a(ancestor)
    }

    fn touch(&self, id: InstanceId) {
        let mut touched = self.touched.borrow_mut();
        if !touched.contains(&id) {
            touched.push(id);
        }
    }

    /// The mouse location (GetMouseLocation)
    pub fn mouse_location(&self) -> (f64, f64) {
        let mouse = self.mouse.get();
        (mouse.x, mouse.y)
    }

    pub fn hovered(&self) -> Vec<InstanceId> {
        self.hovered.borrow().clone()
    }
}

// Input objects, GuiState

impl Cx<'_> {
    /// Input objects: one per mouse input type / key, reused like the engine's.
    pub fn input_object(&self, key: &str, input_type: &str) -> InstanceId {
        if let Some(&existing) = self.i.objects.borrow().get(key) {
            return existing;
        }
        let n = &self.i.n;
        let created = {
            let mut state = self.engine.state.borrow_mut();
            let class = self.i.c.InputObject.expect("InputObject class");
            let id = core::create(&mut state, &self.engine.reflection, class);
            state.dom.pin(id);
            id
        };
        self.set_silent(created, n.Name, Value::String(Str::from("InputObject")));
        self.set_silent(created, n.UserInputType, self.item("UserInputType", input_type));
        self.set_silent(created, n.UserInputState, self.item("UserInputState", "None"));
        self.set_silent(created, n.KeyCode, self.item("KeyCode", "Unknown"));
        self.set_silent(created, n.Position, vector3(0.0, 0.0, 0.0));
        self.set_silent(created, n.Delta, vector3(0.0, 0.0, 0.0));
        self.i.objects.borrow_mut().insert(key.to_string(), created);
        if let Ok(value) = instance_value(self.lua, self.engine, created) {
            self.i.object_values.borrow_mut().push(value);
        }
        created
    }

    fn set_input(&self, object: InstanceId, state_name: &str, position: Value, delta: Option<Value>) {
        let n = &self.i.n;
        self.set(object, n.UserInputState, self.item("UserInputState", state_name));
        if let Some(delta) = delta {
            self.set(object, n.Delta, delta);
        }
        self.set(object, n.Position, position);
    }

    fn set_last_input_type(&self, type_name: &str) {
        let Some(item) = self.engine.reflection.enum_item("UserInputType", type_name) else { return };
        if self.i.last_input_type.get() == Some(item) {
            return;
        }
        self.i.last_input_type.set(Some(item));
        if let Some(uis) = self.service("UserInputService") {
            self.fire(uis, self.i.n.LastInputTypeChanged, vec![Value::EnumItem(item)]);
        }
    }

    fn fire_uis(&self, event: NameId, args: Vec<Value>) {
        if let Some(uis) = self.service("UserInputService") {
            self.fire(uis, event, args);
        }
    }

    /// The first GuiButton / TextBox under the cursor takes mouse buttons; objects below it get none.
    fn sink_of(&self, list: &[InstanceId]) -> (Option<InstanceId>, usize) {
        for (index, &id) in list.iter().enumerate() {
            if self.is(id, self.i.c.GuiButton) || self.is(id, self.i.c.TextBox) {
                return (Some(id), index + 1);
            }
        }
        (None, list.len())
    }

    /// Measured: gameProcessedEvent is true for mouse movement when an Active object is under the
    /// cursor, for mouse buttons also when a GuiButton / TextBox is (Active or not).
    fn any_active(&self, list: &[InstanceId]) -> bool {
        list.iter().any(|&id| self.is_true(id, self.i.n.Active))
    }

    fn pressed_by(&self, id: InstanceId) -> bool {
        self.i.presses.borrow().iter().any(|(_, press)| press.button == Some(id) && press.clickable)
    }

    fn compute_state(&self, id: InstanceId) -> &'static str {
        if !self.i.interactable(&self.engine.state.borrow(), id) {
            return "NonInteractable";
        }
        // Measured: hiding a hovered object sets Idle at once (MouseLeave waits for the next move)
        let shown = self.record(id, |r| r.visible).unwrap_or(true);
        if !shown {
            return "Idle";
        }
        // Measured (select-press.mjs, edges.mjs): a selected object shows Hover, a selected button
        // held by Return shows Press; Press is a GuiButton state
        if self.i.key_pressed.get() == Some(id) {
            return "Press";
        }
        let hovered = self.i.hovered_set.borrow().contains(&id);
        if hovered || self.i.selected.get() == Some(id) {
            if self.is(id, self.i.c.GuiButton) && self.pressed_by(id) && hovered {
                return "Press";
            }
            return "Hover";
        }
        "Idle"
    }

    pub fn update_state(&self, id: InstanceId) {
        if !self.alive(id) || !self.is(id, self.i.c.GuiObject) {
            return;
        }
        let item = self.item("GuiState", self.compute_state(id));
        let current = self.get(id, self.i.n.GuiState).unwrap_or(Value::Nil);
        if !current.same(&item) {
            self.set(id, self.i.n.GuiState, item);
        }
    }

    fn flush_states(&self) {
        let touched = std::mem::take(&mut *self.i.touched.borrow_mut());
        for id in touched {
            self.update_state(id);
        }
    }

    fn legacy_signal(&self, name: &'static str) -> LuaResult<Option<SignalId>> {
        self.legacy_mouse()?;
        Ok(self.i.legacy.borrow().as_ref().and_then(|(_, signals)| signals.get(name).copied()))
    }

    fn legacy_mouse(&self) -> LuaResult<Option<LuaTable>> {
        if let Some((table, _)) = self.i.legacy.borrow().as_ref() {
            return Ok(Some(table.clone()));
        }
        let shared = self.engine.lua.borrow().shared.clone();
        let Some(shared) = shared else { return Ok(None) };
        let LuaValue::Table(mouse) = shared.raw_get::<LuaValue>("__rurMouse")? else { return Ok(None) };
        let mut signals = FxHashMap::default();
        for name in ["Button1Down", "Button1Up", "Button2Down", "Button2Up", "Move", "WheelForward", "WheelBackward", "KeyDown", "KeyUp"] {
            if let LuaValue::UserData(ud) = mouse.raw_get::<LuaValue>(name)? {
                if let Ok(signal) = ud.borrow::<super::lua::types::LSignal>() {
                    signals.insert(name, signal.0);
                }
            }
        }
        *self.i.legacy.borrow_mut() = Some((mouse.clone(), signals));
        Ok(Some(mouse))
    }

    /// Signal.Fire(Engine.Mouse.<name>, ...)
    fn fire_legacy(&self, name: &'static str, args: Vec<Value>) -> LuaResult<()> {
        if let Some(signal) = self.legacy_signal(name)? {
            core::fire(&mut self.engine.state.borrow_mut(), signal, args);
        }
        Ok(())
    }

    fn update_legacy_mouse(&self) -> LuaResult<()> {
        let Some(legacy) = self.legacy_mouse()? else { return Ok(()) };
        let mouse = self.i.mouse.get();
        let inset = self.inset();
        legacy.raw_set("X", mouse.x)?;
        legacy.raw_set("Y", mouse.y - inset)?;
        legacy.raw_set("ViewSizeX", self.engine.options.viewport_width)?;
        legacy.raw_set("ViewSizeY", self.engine.options.viewport_height - inset)?;
        Ok(())
    }

    fn hit_list(&self, x: f64, y: f64) -> Vec<InstanceId> {
        self.update_layout();
        self.i.hit_at(self.engine, x, y)
    }
}

// Mouse movement

fn screen_point(cx: &Cx, event: &Json) -> (f64, f64) {
    // Studio reports whole logical pixels (Qt rounds the device-independent position)
    let mouse = cx.i.mouse.get();
    let x = json_number(event.get("x")).unwrap_or(mouse.x);
    let y = json_number(event.get("y")).unwrap_or(mouse.y);
    ((x + 0.5).floor(), (y + 0.5).floor())
}

/// A new mouse position: visible to scripts at once, applied to the hover state once per frame.
fn set_position(cx: &Cx, x: f64, y: f64) -> LuaResult<()> {
    let mut mouse = cx.i.mouse.get();
    mouse.x = x;
    mouse.y = y;
    mouse.pending = true;
    cx.i.mouse.set(mouse);
    cx.update_legacy_mouse()
}

/// Hover update for the cursor at (X, Y): measured order (all topmost first) MouseMoved on every
/// object under the cursor, InputChanged on those already hovered, MouseLeave + InputEnded on those
/// left, MouseEnter + InputBegan on those entered; then UserInputService.
fn move_to(cx: &Cx, x: f64, y: f64) -> LuaResult<()> {
    let n = &cx.i.n;
    let movement = cx.input_object("MouseMovement", "MouseMovement");
    let mut mouse = cx.i.mouse.get();
    let first = !mouse.inside;
    let (dx, dy) = if first { (0.0, 0.0) } else { (x - mouse.processed_x, y - mouse.processed_y) };
    mouse.x = x;
    mouse.y = y;
    mouse.processed_x = x;
    mouse.processed_y = y;
    mouse.pending = false;
    mouse.inside = true;
    mouse.last_move_delta = (dx as f32, dy as f32);
    cx.i.mouse.set(mouse);
    cx.update_legacy_mouse()?;
    cx.set_input(movement, if first { "Begin" } else { "Change" }, vector3(x, y - cx.inset(), 0.0), Some(vector3(dx, dy, 0.0)));

    let list = cx.hit_list(x, y);
    let now: FxHashSet<InstanceId> = list.iter().copied().collect();
    let position = || vec![Value::Number(x), Value::Number(y)];
    for &id in &list {
        cx.fire(id, n.MouseMoved, position());
    }
    for &id in &list {
        if cx.i.hovered_set.borrow().contains(&id) {
            cx.fire(id, n.InputChanged, vec![Value::Instance(movement)]);
        }
    }
    let previous = cx.i.hovered.borrow().clone();
    for id in previous {
        if !now.contains(&id) {
            cx.i.hovered_set.borrow_mut().remove(&id);
            cx.i.touch(id);
            if cx.alive(id) {
                cx.fire(id, n.MouseLeave, position());
                cx.fire(id, n.InputEnded, vec![Value::Instance(movement)]);
            }
        }
    }
    for &id in &list {
        let entered = cx.i.hovered_set.borrow_mut().insert(id);
        if entered {
            cx.i.touch(id);
            cx.fire(id, n.MouseEnter, position());
            cx.fire(id, n.InputBegan, vec![Value::Instance(movement)]);
        }
    }
    *cx.i.hovered.borrow_mut() = list.clone();

    // A press stops being a click once the cursor leaves the pressed button
    let mut left = Vec::new();
    for (_, press) in cx.i.presses.borrow_mut().iter_mut() {
        if let Some(button) = press.button {
            if !now.contains(&button) {
                press.clickable = false;
                left.push(button);
            }
        }
    }
    for id in left {
        cx.i.touch(id);
    }
    cx.flush_states();

    let primary = cx.i.presses.borrow().iter().any(|(key, _)| *key == 1.0);
    if primary {
        textbox::drag(cx, x, y)?;
        scrolling::drag(cx, x, y)?;
        drag::move_to(cx, x, y)?;
    }

    let active = cx.any_active(&list);
    if cx.engine.options.device == "touch" {
        if primary {
            let finger = cx.input_object("Touch", "Touch");
            let (position, delta) = position_delta(cx, finger, x, y);
            cx.set_input(finger, "Change", position, Some(delta));
            cx.fire_uis(n.InputChanged, vec![Value::Instance(finger), Value::Bool(active)]);
        }
    } else {
        cx.set_last_input_type("MouseMovement");
        cx.fire_uis(if first { n.InputBegan } else { n.InputChanged }, vec![Value::Instance(movement), Value::Bool(active)]);
    }
    cx.fire_legacy("Move", Vec::new())
}

/// A button / wheel event at (X, Y): the mouse position follows, the hover state does not.
fn ensure_at(cx: &Cx, x: f64, y: f64) -> LuaResult<()> {
    let mut mouse = cx.i.mouse.get();
    if x != mouse.x || y != mouse.y {
        mouse.x = x;
        mouse.y = y;
        cx.i.mouse.set(mouse);
        cx.update_legacy_mouse()?;
    }
    Ok(())
}

/// Applies a waiting mouse position (per frame, and before the pointer leaves).
fn process_move(cx: &Cx) -> LuaResult<()> {
    let mouse = cx.i.mouse.get();
    if mouse.pending {
        move_to(cx, mouse.x, mouse.y)?;
    }
    Ok(())
}

/// The pointer left the viewport: hovered objects are left, the movement input ends.
fn leave(cx: &Cx) -> LuaResult<()> {
    process_move(cx)?;
    let mut mouse = cx.i.mouse.get();
    if !mouse.inside {
        return Ok(());
    }
    let n = &cx.i.n;
    let movement = cx.input_object("MouseMovement", "MouseMovement");
    cx.set_input(movement, "End", vector3(mouse.x, mouse.y - cx.inset(), 0.0), Some(vector3(0.0, 0.0, 0.0)));
    mouse.inside = false;
    cx.i.mouse.set(mouse);
    let hovered = std::mem::take(&mut *cx.i.hovered.borrow_mut());
    for id in hovered {
        cx.i.hovered_set.borrow_mut().remove(&id);
        cx.i.touch(id);
        if cx.alive(id) {
            cx.fire(id, n.MouseLeave, vec![Value::Number(mouse.x), Value::Number(mouse.y)]);
            cx.fire(id, n.InputEnded, vec![Value::Instance(movement)]);
        }
    }
    for (_, press) in cx.i.presses.borrow_mut().iter_mut() {
        press.clickable = false;
    }
    cx.flush_states();
    cx.fire_uis(n.InputEnded, vec![Value::Instance(movement), Value::Bool(false)]);
    Ok(())
}

// Mouse buttons

/// --device touch: the primary pointer is a finger. UserInputService and GuiObject input events carry
/// one Touch InputObject from down through its moves to up; GuiButton click events stay as they are.
fn pointer_type(cx: &Cx, button: f64) -> &'static str {
    if button == 1.0 && cx.engine.options.device == "touch" { "Touch" } else { button_type(button) }
}

/// A gamepad's buttons, D-pad and sticks arrive as Gamepad1, a keyboard's keys as Keyboard.
fn key_input_type(code_name: &str) -> &'static str {
    const GAMEPAD: [&str; 18] = ["ButtonA", "ButtonB", "ButtonX", "ButtonY", "ButtonL1", "ButtonL2", "ButtonL3", "ButtonR1", "ButtonR2", "ButtonR3",
        "ButtonStart", "ButtonSelect", "DPadUp", "DPadDown", "DPadLeft", "DPadRight", "Thumbstick1", "Thumbstick2"];
    if GAMEPAD.contains(&code_name) { "Gamepad1" } else { "Keyboard" }
}

fn button_type(button: f64) -> &'static str {
    if button == 2.0 {
        BUTTON_TYPES[1]
    } else if button == 3.0 {
        BUTTON_TYPES[2]
    } else {
        BUTTON_TYPES[0]
    }
}

fn position_delta(cx: &Cx, object: InstanceId, x: f64, y: f64) -> (Value, Value) {
    let previous = match cx.get(object, cx.i.n.Position) {
        Some(Value::Vector3(v)) => v,
        _ => dom::Vector3::new(0.0, 0.0, 0.0),
    };
    let position = dom::Vector3::new(x as f32, (y - cx.inset()) as f32, 0.0);
    let delta = dom::Vector3::new(position.x - previous.x, position.y - previous.y, position.z - previous.z);
    (Value::Vector3(position), Value::Vector3(delta))
}

fn button_down(cx: &Cx, button: f64, x: f64, y: f64) -> LuaResult<()> {
    ensure_at(cx, x, y)?;
    let n = &cx.i.n;
    let type_name = pointer_type(cx, button);
    let object = cx.input_object(type_name, type_name);
    let (position, delta) = position_delta(cx, object, x, y);
    cx.set_input(object, "Begin", position, Some(delta));

    let list = cx.hit_list(x, y);
    let (sink, sink_index) = cx.sink_of(&list);
    let receivers: Vec<InstanceId> = list[..sink_index].to_vec();
    let processed = cx.any_active(&list) || sink.is_some();
    {
        let mut presses = cx.i.presses.borrow_mut();
        presses.retain(|(key, _)| *key != button);
        presses.push((button, Press { button: None, sink, clickable: false, receivers: receivers.clone() }));
        presses.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    }

    // Measured: a click elsewhere releases the focused TextBox first (FocusLost(false, input))
    if let Some(focused) = textbox::focused_box(cx) {
        if sink != Some(focused) && button <= 2.0 {
            textbox::unfocus(cx, focused, false, Some(object), true);
        }
    }

    if let Some(sink) = sink {
        if cx.is(sink, cx.i.c.GuiButton) && button <= 2.0 {
            if let Some((_, press)) = cx.i.presses.borrow_mut().iter_mut().find(|(key, _)| *key == button) {
                press.button = Some(sink);
                press.clickable = true;
            }
            cx.i.touch(sink);
            let event = if button == 1.0 { n.MouseButton1Down } else { n.MouseButton2Down };
            cx.fire(sink, event, vec![Value::Number(x), Value::Number(y)]);
        } else if cx.is(sink, cx.i.c.TextBox) && button == 1.0 {
            textbox::press(cx, sink, x, y, object)?;
        }
    }

    // Scroll bar thumbs and dragging (Draggable fires DragBegin before InputBegan, measured)
    if button == 1.0 && !scrolling::press(cx, &list, x, y) {
        drag::press(cx, &receivers, x, y, cx.engine.options.edit_mode());
    }

    for &id in &receivers {
        cx.fire(id, n.InputBegan, vec![Value::Instance(object)]);
    }
    cx.flush_states();

    cx.set_last_input_type(type_name);
    cx.fire_uis(n.InputBegan, vec![Value::Instance(object), Value::Bool(processed)]);
    if button == 1.0 {
        cx.fire_legacy("Button1Down", Vec::new())?;
    } else if button == 2.0 {
        cx.fire_legacy("Button2Down", Vec::new())?;
    }
    Ok(())
}

fn activation_count(cx: &Cx, button: InstanceId) -> f64 {
    let now = cx.now();
    let mut table = cx.i.last_activation.borrow_mut();
    let count = match table.get(&button) {
        Some(&(time, count)) if now - time < CLICK_TIMEOUT => count + 1.0,
        _ => 0.0,
    };
    table.insert(button, (now, count));
    count
}

fn button_up(cx: &Cx, button: f64, x: f64, y: f64) -> LuaResult<()> {
    ensure_at(cx, x, y)?;
    let n = &cx.i.n;
    let type_name = pointer_type(cx, button);
    let object = cx.input_object(type_name, type_name);
    let (position, delta) = position_delta(cx, object, x, y);
    cx.set_input(object, "End", position, Some(delta));

    let current = {
        let mut presses = cx.i.presses.borrow_mut();
        let index = presses.iter().position(|(key, _)| *key == button);
        index.map(|index| presses.remove(index).1)
    };
    if button == 1.0 {
        // Measured: a drag selection in a TextBox ends at the release point even when the moves of
        // the drag had not been applied yet (one-frame drag)
        textbox::drag(cx, x, y)?;
        textbox::release(cx);
        scrolling::release(cx);
        // Measured: DragStopped(x, y) fires before the button events and InputEnded
        drag::release(cx, x, y);
    }
    let list = cx.hit_list(x, y);
    let (sink, sink_index) = cx.sink_of(&list);

    // Measured order: MouseButton1Click, MouseButton1Up, Activated (MB2: Click, SecondaryActivated,
    // Up). MouseButton*Up fires on the button under the cursor wherever the press began. Activated
    // needs the press to have been taken by this button or by no button / TextBox; leaving the
    // button during the press does not stop it. The click needs the press on the same button
    // without leaving it.
    if let Some(sink) = sink {
        if cx.is(sink, cx.i.c.GuiButton) && button <= 2.0 {
            let clicked = current.as_ref().is_some_and(|c| c.button == Some(sink) && c.clickable);
            let activates = !cx.is_false(sink, n.Active)
                && current.as_ref().is_none_or(|c| c.sink.is_none() || c.sink == Some(sink));
            cx.i.touch(sink);
            if button == 1.0 {
                if clicked {
                    cx.fire(sink, n.MouseButton1Click, Vec::new());
                }
                cx.fire(sink, n.MouseButton1Up, vec![Value::Number(x), Value::Number(y)]);
                if activates {
                    let count = activation_count(cx, sink);
                    cx.fire(sink, n.Activated, vec![Value::Instance(object), Value::Number(count)]);
                }
            } else {
                if clicked {
                    cx.fire(sink, n.MouseButton2Click, Vec::new());
                }
                if activates {
                    cx.fire(sink, n.SecondaryActivated, vec![Value::Instance(object)]);
                }
                cx.fire(sink, n.MouseButton2Up, vec![Value::Number(x), Value::Number(y)]);
            }
        }
    }
    if let Some(pressed) = current.as_ref().and_then(|c| c.button) {
        cx.i.touch(pressed);
    }

    // Measured: InputEnded goes to the objects that got InputBegan for the press; a press no
    // button / TextBox took ends on the objects under the release point instead
    let ended: Vec<InstanceId> = match &current {
        Some(c) if c.sink.is_some() => c.receivers.clone(),
        _ => {
            let mut ended = Vec::new();
            let mut seen = FxHashSet::default();
            if let Some(c) = &current {
                for &id in &c.receivers {
                    seen.insert(id);
                    ended.push(id);
                }
            }
            for &id in &list[..sink_index] {
                if !seen.contains(&id) {
                    ended.push(id);
                }
            }
            ended
        }
    };
    for id in ended {
        if cx.alive(id) {
            cx.fire(id, n.InputEnded, vec![Value::Instance(object)]);
        }
    }
    cx.flush_states();

    // Measured: gameProcessedEvent of the release follows what is under the release point
    cx.set_last_input_type(type_name);
    let processed = cx.any_active(&list) || sink.is_some();
    cx.fire_uis(n.InputEnded, vec![Value::Instance(object), Value::Bool(processed)]);
    if button == 1.0 {
        cx.fire_legacy("Button1Up", Vec::new())?;
    } else if button == 2.0 {
        cx.fire_legacy("Button2Up", Vec::new())?;
    }
    Ok(())
}

// Mouse wheel

/// Measured (wheel.mjs replayed in Studio, keys-wheel.mjs): the wheel goes to the topmost
/// ScrollingFrame under the cursor that can still move (frames at the end of their range fire a
/// MouseWheel event of their own and pass it on). The objects under the cursor down to that frame
/// are the receivers: they fire MouseWheelForward / Backward (once per whole notch;
/// ScrollingEnabled = false frames excepted), then get InputChanged(wheel) if they already had the
/// wheel input, objects that had it and are no longer under the cursor get InputEnded(wheel), and
/// receivers new to it get InputBegan(wheel); then UserInputService.InputChanged(wheel, processed).
fn wheel(cx: &Cx, event: &Json) -> LuaResult<()> {
    let (x, y) = screen_point(cx, event);
    ensure_at(cx, x, y)?;
    let notches = match json_number(event.get("notches")) {
        Some(notches) => notches,
        None => json_number(event.get("dy")).map_or(0.0, |dy| -dy / 100.0),
    };
    if notches == 0.0 {
        return Ok(());
    }
    let n = &cx.i.n;
    let object = cx.input_object("MouseWheel", "MouseWheel");
    cx.set_input(object, "Change", vector3(x, y - cx.inset(), notches), Some(vector3(0.0, 0.0, 0.0)));

    let list = cx.hit_list(x, y);
    let (taker, failed) = scrolling::route(cx, &list, notches, event.get("presentationTaker"));
    let event_name = if notches > 0.0 { n.MouseWheelForward } else { n.MouseWheelBackward };
    let repeats = (notches.abs() + 0.5).floor().max(1.0) as usize;

    let mut receivers = Vec::new();
    for &id in &list {
        receivers.push(id);
        if Some(id) == taker {
            break;
        }
    }
    for &id in &failed {
        cx.fire(id, event_name, vec![Value::Number(x), Value::Number(y)]);
    }
    for _ in 0..repeats {
        for &id in &receivers {
            if !(cx.class_name_is(id, cx.i.c.ScrollingFrame) && cx.is_false(id, n.ScrollingEnabled)) {
                cx.fire(id, event_name, vec![Value::Number(x), Value::Number(y)]);
            }
        }
    }

    let under: FxHashSet<InstanceId> = list.iter().copied().collect();
    for &id in &receivers {
        if cx.i.wheel_set.borrow().contains(&id) {
            cx.fire(id, n.InputChanged, vec![Value::Instance(object)]);
        }
    }
    let previous = cx.i.wheel_list.borrow().clone();
    for id in previous {
        if cx.i.wheel_set.borrow().contains(&id) && !under.contains(&id) {
            cx.i.wheel_set.borrow_mut().remove(&id);
            if cx.alive(id) {
                cx.fire(id, n.InputEnded, vec![Value::Instance(object)]);
            }
        }
    }
    for &id in &receivers {
        if !cx.i.wheel_set.borrow().contains(&id) {
            cx.fire(id, n.InputBegan, vec![Value::Instance(object)]);
        }
    }
    {
        let mut set = cx.i.wheel_set.borrow_mut();
        for &id in &list {
            set.insert(id);
        }
    }
    *cx.i.wheel_list.borrow_mut() = list.clone();

    cx.set_last_input_type("MouseWheel");
    let processed = taker.is_some() || cx.any_active(&list);
    cx.fire_uis(n.InputChanged, vec![Value::Instance(object), Value::Bool(processed)]);
    cx.fire_legacy(if notches > 0.0 { "WheelForward" } else { "WheelBackward" }, Vec::new())
}

// Keyboard

fn key_code_of(engine: &Engine, name: Option<&Json>) -> Option<EnumItemId> {
    let Some(Json::String(name)) = name else { return None };
    engine.reflection.enum_item("KeyCode", name)
}

/// Shift / Ctrl / Alt of a key event (the event's flags, else the keys held).
pub(crate) struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
}

fn modifiers(cx: &Cx, event: &Json) -> Modifiers {
    let held = |left: &str, right: &str| {
        let keys = cx.i.keys_down.borrow();
        [left, right].iter().any(|name| {
            cx.engine.reflection.enum_item("KeyCode", name).is_some_and(|item| keys.iter().any(|(k, _)| *k == item))
        })
    };
    let flag = |key: &str, left: &str, right: &str| match event.get(key) {
        Some(Json::Null) | None => held(left, right),
        Some(value) => value == &Json::Bool(true),
    };
    Modifiers { shift: flag("shift", "LeftShift", "RightShift"), ctrl: flag("ctrl", "LeftControl", "RightControl") }
}

/// Measured: a key press reaches the objects under the cursor (InputBegan / InputEnded with the
/// key's InputObject, Position = mouse position; one InputObject per KeyCode, reused).
fn key(cx: &Cx, event: &Json) -> LuaResult<()> {
    let Some(code) = key_code_of(cx.engine, event.get("key")) else { return Ok(()) };
    let n = &cx.i.n;
    let code_name = cx.engine.reflection.enum_item_info(code).name.to_string();
    let input_type = key_input_type(&code_name);
    let object = cx.input_object(&format!("Key:{code_name}"), input_type);
    cx.set(object, n.KeyCode, Value::EnumItem(code));
    let mouse = cx.i.mouse.get();
    let position = vector3(mouse.x, mouse.y - cx.inset(), 0.0);
    cx.set_last_input_type(input_type);

    let activation_key = code_name == "Return" || code_name == "KeypadEnter" || code_name == "ButtonA";
    let selected = cx.i.selected.get();
    let can_activate = activation_key
        && selected.is_some_and(|s| cx.is(s, cx.i.c.GuiButton) && cx.alive(s))
        && textbox::focused_box(cx).is_none();
    let is_down = event.get("down") != Some(&Json::Bool(false));
    let center = |id: InstanceId| {
        cx.record(id, |r| (r.x + r.w / 2.0, r.y + r.h / 2.0)).unwrap_or((mouse.x, mouse.y))
    };
    if is_down {
        let repeat = cx.i.keys_down.borrow().iter().any(|(k, _)| *k == code);
        let mut used = false;
        if textbox::focused_box(cx).is_some() {
            let modifiers = modifiers(cx, event);
            textbox::key(cx, &code_name, &modifiers, object)?;
            used = true;
        }
        if repeat {
            return Ok(());
        }
        cx.i.keys_down.borrow_mut().push((code, object));
        cx.set_input(object, "Begin", position.clone(), Some(vector3(0.0, 0.0, 0.0)));
        let sunk = context_actions::run(cx, code, input_type, "Begin", object)?;
        used = used || sunk;
        let activation = can_activate && !sunk;
        let mut receivers = Vec::new();
        // Measured: Return on a selected button fires MouseButton1Down at the button's centre and
        // the button gets the key's InputBegan; the key is processed
        if activation {
            let selected = selected.unwrap();
            let (cx_, cy_) = center(selected);
            cx.i.key_pressed.set(Some(selected));
            cx.i.touch(selected);
            cx.fire(selected, n.MouseButton1Down, vec![Value::Number(cx_), Value::Number(cy_)]);
            receivers.push(selected);
            cx.fire(selected, n.InputBegan, vec![Value::Instance(object)]);
            used = true;
        }
        let hovered = cx.i.hovered.borrow().clone();
        for id in hovered {
            if cx.alive(id) && Some(id) != cx.i.selected.get() {
                receivers.push(id);
                cx.fire(id, n.InputBegan, vec![Value::Instance(object)]);
            }
        }
        cx.i.key_receivers.borrow_mut().insert(code, receivers);
        cx.flush_states();
        cx.fire_uis(n.InputBegan, vec![Value::Instance(object), Value::Bool(used)]);
        if code_name.len() == 1 {
            cx.fire_legacy("KeyDown", vec![Value::String(Str::from(code_name.to_lowercase()))])?;
        }
    } else {
        let index = cx.i.keys_down.borrow().iter().position(|(k, _)| *k == code);
        let Some(index) = index else { return Ok(()) };
        cx.i.keys_down.borrow_mut().remove(index);
        cx.set_input(object, "End", position, Some(vector3(0.0, 0.0, 0.0)));
        let sunk = context_actions::run(cx, code, input_type, "End", object)?;
        let mut processed = textbox::focused_box(cx).is_some() || sunk;
        let pressed = if activation_key { cx.i.key_pressed.get() } else { None };
        if let Some(button) = pressed {
            // Measured order: MouseButton1Up (centre), InputEnded, MouseButton1Click, Activated
            cx.i.key_pressed.set(None);
            cx.i.touch(button);
            let (cx_, cy_) = center(button);
            cx.fire(button, n.MouseButton1Up, vec![Value::Number(cx_), Value::Number(cy_)]);
        }
        let receivers = cx.i.key_receivers.borrow_mut().remove(&code).unwrap_or_default();
        for id in receivers {
            if cx.alive(id) {
                cx.fire(id, n.InputEnded, vec![Value::Instance(object)]);
            }
        }
        if let Some(button) = pressed.filter(|&b| cx.alive(b)) {
            cx.fire(button, n.MouseButton1Click, Vec::new());
            if !cx.is_false(button, n.Active) {
                let count = activation_count(cx, button);
                cx.fire(button, n.Activated, vec![Value::Instance(object), Value::Number(count)]);
            }
            processed = true;
        }
        cx.flush_states();
        cx.fire_uis(n.InputEnded, vec![Value::Instance(object), Value::Bool(processed)]);
        if code_name.len() == 1 {
            cx.fire_legacy("KeyUp", vec![Value::String(Str::from(code_name.to_lowercase()))])?;
        }
    }
    Ok(())
}

// Dispatch

/// Runs `f` in profile bucket `name` (Profile.Enter / Leave around it).
fn profiled<T>(name: &'static str, f: impl FnOnce() -> T) -> T {
    profile::enter(name);
    let result = f();
    profile::leave();
    result
}

/// SettlePresentation: a presented click must see the rows created by CanvasPosition listeners.
/// Profile: the "presentationSettle" bucket, its passes, and hitting the pass limit.
fn settle_presentation(lua: &Lua, engine: &Rc<Engine>) -> LuaResult<()> {
    profile::enter("presentationSettle");
    let result = (|| {
        for _ in 0..8 {
            profile::count("presentationSettlePasses", 1);
            scheduler::flush_deferred(lua, engine)?;
            super::layout::update_all(engine);
            if !engine.state.borrow().sched.has_deferred() {
                return Ok(());
            }
        }
        profile::count("presentationSettleLimit", 1);
        Ok(())
    })();
    profile::leave();
    result
}

impl Input {
    /// Input.Dispatch: one input event, then the deferred queue is flushed.
    pub fn dispatch(self: &Rc<Self>, lua: &Lua, engine: &Rc<Engine>, event: &Json) -> LuaResult<()> {
        if !event.is_object() {
            return Ok(());
        }
        let cx = Cx { lua, engine, i: self };
        let result = (|| -> LuaResult<bool> {
            match event.get("type").and_then(Json::as_str) {
                Some("move") => {
                    // Measured: the engine handles a move to the same position like any move
                    let (x, y) = screen_point(&cx, event);
                    set_position(&cx, x, y)?;
                }
                Some("down") => {
                    let (x, y) = screen_point(&cx, event);
                    button_down(&cx, json_number(event.get("button")).unwrap_or(1.0), x, y)?;
                }
                Some("up") => {
                    let (x, y) = screen_point(&cx, event);
                    button_up(&cx, json_number(event.get("button")).unwrap_or(1.0), x, y)?;
                }
                Some("wheel") => wheel(&cx, event)?,
                Some("leave") => leave(&cx)?,
                Some("key") => key(&cx, event)?,
                Some("text") => {
                    if let Some(Json::String(text)) = event.get("text") {
                        textbox::type_text(&cx, text.as_bytes());
                    }
                }
                Some("paste") if matches!(event.get("text"), Some(Json::String(_))) => {
                    textbox::paste(&cx, event["text"].as_str().unwrap().as_bytes());
                }
                _ => return Ok(false),
            }
            Ok(true)
        })();
        self.sync_pins(engine);
        if result? {
            scheduler::flush_deferred(lua, engine)?;
        }
        Ok(())
    }

    /// Per frame (PreRender, virtual time): presented poses, the pending move, smooth scrolling,
    /// caret blink, hidden hovered objects.
    fn step(self: &Rc<Self>, lua: &Lua, engine: &Rc<Engine>, delta: f64) -> LuaResult<()> {
        let cx = Cx { lua, engine, i: self };
        // A selected object that left the tree stops being selected
        if let (Some(selected), Some(gui_service)) = (self.selected.get(), cx.service("GuiService")) {
            let attached = cx.alive(selected) && engine.state.borrow().dom.at(selected).parent.is_some();
            if !attached {
                set_selected(&cx, gui_service, None);
            }
        }
        // Profile buckets: presentationApply, pointer, scrolling, hover (Input Step)
        let result = (|| -> LuaResult<()> {
            if profiled("presentationApply", || scrolling::flush_presentation(&cx))? {
                settle_presentation(lua, engine)?;
            }
            profiled("pointer", || process_move(&cx))?;
            profiled("scrolling", || scrolling::step(&cx, delta))?;
            profiled("hover", || {
                textbox::step(&cx);
                let hovered = self.hovered.borrow().clone();
                for id in hovered {
                    cx.update_state(id);
                }
            });
            Ok(())
        })();
        self.sync_pins(engine);
        result
    }

    /// Input.EnablePresentation
    pub fn enable_presentation(&self, engine: &Engine, enabled: bool) {
        scrolling::enable_presentation(self, engine, enabled);
    }

    /// Input.ApplyPresentation(packet, phase)
    pub fn apply_presentation(self: &Rc<Self>, lua: &Lua, engine: &Rc<Engine>, packet: &Json, phase: &str) -> LuaResult<bool> {
        let cx = Cx { lua, engine, i: self };
        let applied = profiled("presentationApply", || scrolling::apply_presentation_phase(&cx, packet, phase))?;
        if applied && phase != "frame" && phase != "wheel" {
            settle_presentation(lua, engine)?;
        }
        Ok(applied)
    }

    /// Input.PresentationSnapshot
    pub fn presentation_snapshot(self: &Rc<Self>, lua: &Lua, engine: &Rc<Engine>) -> Option<Json> {
        let cx = Cx { lua, engine, i: self };
        scrolling::snapshot(&cx)
    }

    /// Input.TakeClipboard
    pub fn take_clipboard(&self) -> Option<Vec<u8>> {
        self.text.pending_clipboard.borrow_mut().take()
    }

    /// Input.FocusedTextBox
    pub fn focused_text_box(&self, engine: &Engine) -> Option<InstanceId> {
        let focused = self.text.focused.get()?;
        let state = engine.state.borrow();
        match state.dom.get(focused) {
            Some(instance) if instance.has(flags::DESTROYED) => None,
            _ => Some(focused),
        }
    }

    /// Pins the instances the input holds (the Luau tables keep them alive) so the instance
    /// collector (lua/raw.rs) does not free them while referenced here.
    fn sync_pins(&self, engine: &Engine) {
        let mut held: FxHashSet<InstanceId> = FxHashSet::default();
        held.extend(self.hovered.borrow().iter().copied());
        held.extend(self.wheel_list.borrow().iter().copied());
        for (_, press) in self.presses.borrow().iter() {
            held.extend(press.button);
            held.extend(press.sink);
            held.extend(press.receivers.iter().copied());
        }
        for receivers in self.key_receivers.borrow().values() {
            held.extend(receivers.iter().copied());
        }
        held.extend(self.selected.get());
        held.extend(self.key_pressed.get());
        held.extend(self.text.focused.get());
        if let Some(click) = self.text.last_click.borrow().as_ref() {
            held.insert(click.target);
        }
        if let Some(active) = self.drag.borrow().as_ref() {
            held.insert(active.target);
            held.extend(active.detector);
        }
        if let Some(grab) = self.scroll.grabbed.borrow().as_ref() {
            held.insert(grab.target);
        }
        let mut pinned = self.pinned.borrow_mut();
        if *pinned == held {
            return;
        }
        let mut state = engine.state.borrow_mut();
        for &id in pinned.iter() {
            if !held.contains(&id) && state.dom.get(id).is_some() {
                state.dom.unpin(id);
            }
        }
        for &id in held.iter() {
            if !pinned.contains(&id) && state.dom.get(id).is_some() {
                state.dom.pin(id);
            }
        }
        *pinned = held;
    }
}

// Scene export (runtime/export/scene.luau ExportInteraction)

/// Measured AutoButtonColor tint (hover-round / press-tint experiments): per channel of
/// BackgroundColor3, Hover = min(255, floor(c * 0.7 * 256)), Press = min(255, floor((c * 0.7 +
/// 0.3) * 256)).
fn quantize(value: f64) -> f64 {
    clamp((value * 256.0).floor(), 0.0, 255.0)
}

pub fn tint(color: dom::Color3, press: bool) -> [f64; 3] {
    let channel = |c: f32| {
        let c = c as f64;
        if press { quantize(c * 0.7 + 0.3) } else { quantize(c * 0.7) }
    };
    [channel(color.r), channel(color.g), channel(color.b)]
}

impl Input {
    /// Input.Visual: scene fields for an instance's interaction visuals: `interaction` = { state,
    /// bg = exact 8-bit background colour } for a tinted GuiButton, `imageUri` (ImageButton
    /// HoverImage / PressedImage); `caret` / `selection` for the focused TextBox. Reads the state
    /// the export holds (the layout records are current).
    pub fn visual(&self, engine: &Engine, state: &State, layout: &super::layout::Layout, id: InstanceId) -> Option<Map<String, Json>> {
        let class = state.dom.at(id).class;
        if self.class_is(class, self.c.TextBox) {
            return textbox::visual(self, engine, state, layout, id);
        }
        if !self.class_is(class, self.c.GuiButton) {
            return None;
        }
        let state_name = match state.dom.raw_get(id, self.n.GuiState) {
            Some(Value::EnumItem(item)) => engine.reflection.enum_item_info(*item).name.to_string(),
            _ => "Idle".to_string(),
        };
        if state_name != "Hover" && state_name != "Press" {
            return None;
        }
        let press = state_name == "Press";
        let mut result = Map::new();
        // Measured (hover-image.mjs): an ImageButton shows HoverImage while hovered and
        // PressedImage while pressed (AutoButtonColor or not); the Image property is unchanged
        if self.class_is(class, self.c.ImageButton) {
            let swap = state.dom.raw_get(id, if press { self.n.PressedImage } else { self.n.HoverImage });
            let uri = match swap {
                Some(Value::String(s)) => Some(s.to_str_lossy().into_owned()),
                _ => None,
            };
            if let Some(uri) = uri.filter(|u| !u.is_empty()) {
                result.insert("imageUri".into(), Json::String(uri));
            }
        }
        let color = state.dom.raw_get(id, self.n.BackgroundColor3).and_then(Value::as_color3);
        if let Some(color) = color {
            if !matches!(state.dom.raw_get(id, self.n.AutoButtonColor), Some(Value::Bool(false))) {
                let bg = tint(color, press);
                result.insert(
                    "interaction".into(),
                    json!({ "state": state_name, "bg": bg.iter().map(|v| super::export::number(*v)).collect::<Vec<_>>() }),
                );
            }
        }
        if result.is_empty() { None } else { Some(result) }
    }
}

// Install

/// Input.Install: behaviours (UserInputService live state, GuiService.SelectedObject, GuiState,
/// GetGuiObjectsAtPosition, ScrollingFrame and TextBox methods), the invalidation hooks and the
/// PreRender step. Idempotent.
pub fn install(engine: &Rc<Engine>) {
    if engine.input().borrow().is_some() {
        return;
    }
    let reflection = engine.reflection.clone();
    let input = Rc::new(Input::new(&reflection));
    *engine.input().borrow_mut() = Some(input.clone());
    install_hooks(engine, &input);
    install_behaviors(engine, &input);
    let weak = Rc::downgrade(&input);
    engine.state.borrow_mut().sched.add_hook(
        Phase::PreRender,
        Rc::new(move |lua, engine, delta| match weak.upgrade() {
            Some(input) => input.step(lua, engine, delta),
            None => Ok(()),
        }),
    );
}

fn install_hooks(engine: &Rc<Engine>, input: &Rc<Input>) {
    let mut hooks = engine.hooks.borrow_mut();
    let weak = Rc::downgrade(input);
    let weak_engine = Rc::downgrade(engine);
    hooks.property_changed.push(Box::new(move |state, id, name| {
        let (Some(input), Some(engine)) = (weak.upgrade(), weak_engine.upgrade()) else { return };
        let Some(instance) = state.dom.get(id) else { return };
        let class = instance.class;
        let n = &input.n;
        // (behaviour Changed hooks: GuiObject Interactable, TextBox Text)
        if name == n.Interactable && input.class_is(class, input.c.GuiObject) {
            interactable_changed(&engine, &input, state, id);
        }
        if name == n.Text && input.class_is(class, input.c.TextBox) {
            textbox::text_changed(&engine, &input, state, id);
        }
        scrolling::property_changed(&engine, &input, state, id, name);
        let gui = input.class_is(class, input.c.GuiBase2d)
            || input.class_is(class, input.c.UIComponent)
            || input.class_is(class, input.c.UIBase);
        if gui && !input.visual_only.contains(&name) && !(name == n.Rotation && !input.class_is(class, input.c.GuiBase2d)) {
            input.invalidate_hit();
        }
    }));
    let weak = Rc::downgrade(input);
    hooks.hierarchy_changed.push(Box::new(move |_, _, _, _| {
        if let Some(input) = weak.upgrade() {
            input.invalidate_hit();
        }
    }));
    // (instance GC, lua/raw.rs: a freed ScrollingFrame's presentation state goes with it)
    let weak = Rc::downgrade(input);
    hooks.instance_freed.push(Box::new(move |_, id| {
        if let Some(input) = weak.upgrade() {
            input.scroll.forget(id);
        }
    }));
}

/// UpdateState from inside a hook (the state is borrowed by the caller).
pub(crate) fn update_state_in(engine: &Engine, input: &Input, state: &mut State, id: InstanceId) {
    let Some(instance) = state.dom.get(id) else { return };
    if instance.has(flags::DESTROYED) || !input.class_is(instance.class, input.c.GuiObject) {
        return;
    }
    let name = if !input.interactable(state, id) {
        "NonInteractable"
    } else {
        let shown = engine
            .layout()
            .try_borrow()
            .ok()
            .and_then(|guard| guard.as_ref().and_then(|layout| layout.record(id).map(|r| r.visible)))
            .unwrap_or(true);
        if !shown {
            "Idle"
        } else if input.key_pressed.get() == Some(id) {
            "Press"
        } else {
            let hovered = input.hovered_set.borrow().contains(&id);
            if hovered || input.selected.get() == Some(id) {
                let pressed = input.presses.borrow().iter().any(|(_, p)| p.button == Some(id) && p.clickable);
                if input.class_is(instance.class, input.c.GuiButton) && pressed && hovered { "Press" } else { "Hover" }
            } else {
                "Idle"
            }
        }
    };
    let Some(item) = engine.reflection.enum_item("GuiState", name) else { return };
    let same = matches!(state.dom.raw_get(id, input.n.GuiState), Some(Value::EnumItem(current)) if *current == item);
    if !same {
        let hooks = engine.hooks.borrow();
        core::raw_set(state, &engine.reflection, &hooks, id, input.n.GuiState, Value::EnumItem(item));
    }
}

/// GuiObject.Changed("Interactable"): the object and its descendants recompute GuiState.
fn interactable_changed(engine: &Engine, input: &Input, state: &mut State, id: InstanceId) {
    let mut stack = vec![id];
    while let Some(current) = stack.pop() {
        update_state_in(engine, input, state, current);
        if let Some(instance) = state.dom.get(current) {
            stack.extend(instance.children.iter().copied());
        }
    }
}

fn set_selected(cx: &Cx, gui_service: InstanceId, target: Option<InstanceId>) {
    let i = cx.i;
    let old = cx.get(gui_service, i.n.SelectedObject).and_then(|v| v.as_instance());
    if old == target {
        return;
    }
    cx.set(gui_service, i.n.SelectedObject, target.map_or(Value::Nil, Value::Instance));
    i.selected.set(target);
    if i.key_pressed.get().is_some() && i.key_pressed.get() != target {
        i.key_pressed.set(None);
    }
    if let Some(old) = old.filter(|&o| cx.alive(o)) {
        cx.fire(old, i.n.SelectionLost, Vec::new());
        cx.update_state(old);
    }
    if let Some(target) = target {
        cx.fire(target, i.n.SelectionGained, Vec::new());
        cx.update_state(target);
    }
    i.sync_pins(cx.engine);
}

fn lowest_selectable(cx: &Cx, parent: InstanceId) -> Option<InstanceId> {
    let gui_children = |id: InstanceId| {
        let state = cx.engine.state.borrow();
        let mut children = cx.i.gui_children(&state, id);
        children.reverse();
        children
    };
    let mut best: Option<(f64, InstanceId)> = None;
    let mut stack = gui_children(parent);
    while let Some(id) = stack.pop() {
        if cx.is_false(id, cx.i.n.Visible) {
            continue;
        }
        if cx.is_true(id, cx.i.n.Selectable) {
            let order = match cx.get(id, cx.i.n.SelectionOrder) {
                Some(Value::Number(order)) => order,
                _ => 0.0,
            };
            if best.is_none_or(|(lowest, _)| order < lowest) {
                best = Some((order, id));
            }
        }
        stack.extend(gui_children(id));
    }
    best.map(|(_, id)| id)
}

fn enum_named(engine: &Rc<Engine>, value: &LuaValue, wanted: &str) -> bool {
    enum_arg(value).is_some_and(|item| &*engine.reflection.enum_item_info(item).name == wanted)
}

fn enum_arg(value: &LuaValue) -> Option<EnumItemId> {
    match convert::from_lua(value) {
        Value::EnumItem(item) => Some(item),
        _ => None,
    }
}

fn vector2_lua(lua: &Lua, x: f64, y: f64) -> LuaResult<LuaValue> {
    let vector = crate::engine::lune_roblox::datatypes::types::Vector2::from(dom::Vector2::new(x as f32, y as f32));
    Ok(LuaValue::UserData(lua.create_userdata(vector)?))
}

fn method(f: impl Fn(&Lua, &Rc<Engine>, InstanceId, &Args) -> Ret + 'static) -> MethodFn {
    Rc::new(f)
}

/// The UserInputService:CreateVirtualInput() object: scripted input through the same dispatcher.
const VIRTUAL_INPUT: &str = r#"
local Dispatch, MouseLocation = ...
return function()
	local Virtual = {}
	local function Point(Position)
		local X, Y = MouseLocation()
		return (Position and Position.X) or X, (Position and Position.Y) or Y
	end
	function Virtual.SendMousePosition(_, Position)
		local X, Y = Point(Position)
		Dispatch({ type = "move", x = X, y = Y })
	end
	function Virtual.SendMouseButton(_, Position, InputType, IsDown, _Repeat)
		local X, Y = Point(Position)
		local Name = if type(InputType) == "string" then InputType else (InputType and InputType.Name) or "MouseButton1"
		local Button = if Name == "MouseButton2" then 2 elseif Name == "MouseButton3" then 3 else 1
		Dispatch({ type = if IsDown then "down" else "up", button = Button, x = X, y = Y })
	end
	function Virtual.SendMouseDelta(_, Delta)
		local X, Y = MouseLocation()
		Dispatch({ type = "move", x = X + ((Delta and Delta.X) or 0), y = Y + ((Delta and Delta.Y) or 0) })
	end
	function Virtual.SendKey(_, IsPressed, KeyCode, _IsRepeat)
		local Name = if type(KeyCode) == "string" then KeyCode else (KeyCode and KeyCode.Name)
		Dispatch({ type = "key", key = Name, down = IsPressed == true })
	end
	function Virtual.SendTextInput(_, Text)
		Dispatch({ type = "text", text = Text })
	end
	function Virtual.SendPointerAction(_, Position, Action)
		local X, Y = Point(Position)
		local Wheel = type(Action) == "table" and tonumber(Action.Wheel) or 0
		if Wheel ~= 0 then
			Dispatch({ type = "wheel", notches = Wheel, x = X, y = Y })
		end
	end
	return setmetatable(Virtual, { __tostring = function()
		return "VirtualInput"
	end })
end
"#;

fn install_behaviors(engine: &Rc<Engine>, input: &Rc<Input>) {
    let reflection = engine.reflection.clone();
    let mut behaviors = engine.behaviors.borrow_mut();
    let n = &input.n;
    let c = &input.c;
    let valid = |name: NameId| name != NameId::MAX;

    // UserInputService
    if let Some(uis) = c.UserInputService {
        let i = input.clone();
        behaviors.add_method(uis, n.GetMouseLocation, method(move |lua, _, _, _| {
            let (x, y) = i.mouse_location();
            Ok(values([vector2_lua(lua, x, y)?]))
        }));
        let i = input.clone();
        behaviors.add_method(uis, n.GetMouseDelta, method(move |lua, _, _, _| {
            let (x, y) = i.mouse.get().last_move_delta;
            Ok(values([vector2_lua(lua, x as f64, y as f64)?]))
        }));
        let i = input.clone();
        behaviors.add_method(uis, n.IsMouseButtonPressed, method(move |_, engine, _, args| {
            let wanted = args.get(1);
            for (index, type_name) in BUTTON_TYPES.iter().enumerate() {
                let item = engine.reflection.enum_item("UserInputType", type_name);
                let matches = match wanted {
                    LuaValue::String(s) => &*s.as_bytes() == type_name.as_bytes(),
                    other => enum_arg(other).is_some() && enum_arg(other) == item,
                };
                if matches {
                    let pressed = i.presses.borrow().iter().any(|(key, _)| *key == (index + 1) as f64);
                    return Ok(values([LuaValue::Boolean(pressed)]));
                }
            }
            Ok(values([LuaValue::Boolean(false)]))
        }));
        let i = input.clone();
        behaviors.add_method(uis, n.GetMouseButtonsPressed, method(move |lua, engine, _, _| {
            let table = lua.create_table()?;
            for (index, type_name) in BUTTON_TYPES.iter().enumerate() {
                let pressed = i.presses.borrow().iter().any(|(key, _)| *key == (index + 1) as f64);
                if pressed {
                    let cx = Cx { lua, engine, i: &i };
                    let object = cx.input_object(type_name, type_name);
                    table.raw_push(instance_value(lua, engine, object)?)?;
                }
            }
            Ok(values([LuaValue::Table(table)]))
        }));
        let i = input.clone();
        behaviors.add_method(uis, n.IsKeyDown, method(move |_, engine, _, args| {
            let code = match args.get(1) {
                LuaValue::String(s) => engine.reflection.enum_item("KeyCode", &s.to_string_lossy()),
                other => enum_arg(other),
            };
            let down = code.is_some_and(|code| i.keys_down.borrow().iter().any(|(k, _)| *k == code));
            Ok(values([LuaValue::Boolean(down)]))
        }));
        let i = input.clone();
        behaviors.add_method(uis, n.GetKeysPressed, method(move |lua, engine, _, _| {
            let table = lua.create_table()?;
            let keys = i.keys_down.borrow().clone();
            for (_, object) in keys {
                table.raw_push(instance_value(lua, engine, object)?)?;
            }
            Ok(values([LuaValue::Table(table)]))
        }));
        let i = input.clone();
        behaviors.add_method(uis, n.GetFocusedTextBox, method(move |lua, engine, _, _| {
            Ok(values([match i.focused_text_box(engine) {
                Some(id) => instance_value(lua, engine, id)?,
                None => LuaValue::Nil,
            }]))
        }));
        let previous = behaviors.find_method(&reflection, uis, n.GetLastInputType).map(|(_, m)| m.clone());
        let i = input.clone();
        behaviors.add_method(uis, n.GetLastInputType, method(move |lua, engine, id, args| {
            if let Some(item) = i.last_input_type.get() {
                return Ok(values([enum_item_value(lua, engine, item)?]));
            }
            // Measured by STORIES: an edit-mode Studio without input reports Focus
            match &previous {
                Some(Method::Rust(f)) => f(lua, engine, id, args),
                Some(Method::Lua(f)) => Ok(f.call::<LuaMultiValue>(args.0.clone())?),
                None => Ok(values([engine.reflection.enum_item("UserInputType", "None").map_or(Ok(LuaValue::Nil), |item| enum_item_value(lua, engine, item))?])),
            }
        }));
        let i = input.clone();
        behaviors.add_method(uis, n.CreateVirtualInput, method(move |lua, engine, _, _| {
            let weak_input = Rc::downgrade(&i);
            let e = engine.clone();
            let dispatch = lua.create_function(move |lua, event: LuaValue| {
                let json: Json = lua.from_value(event)?;
                if let Some(input) = weak_input.upgrade() {
                    input.dispatch(lua, &e, &json)?;
                }
                Ok(())
            })?;
            let weak_input = Rc::downgrade(&i);
            let location = lua.create_function(move |_, ()| {
                Ok(weak_input.upgrade().map_or((0.0, 0.0), |input| input.mouse_location()))
            })?;
            let factory: LuaFunction = lua.load(VIRTUAL_INPUT).set_name("=rur.input").call((dispatch, location))?;
            Ok(values([factory.call::<LuaValue>(())?]))
        }));
    }

    // InputObject:IsModifierKeyDown(ModifierKey)
    if let Some(object_class) = c.InputObject {
        let i = input.clone();
        behaviors.add_method(object_class, n.IsModifierKeyDown, method(move |_, engine, _, args| {
            let name = match args.get(1) {
                LuaValue::String(s) => s.to_string_lossy(),
                other => enum_arg(other).map(|item| engine.reflection.enum_item_info(item).name.to_string()).unwrap_or_default(),
            };
            let keys: &[&str] = match name.as_str() {
                "Shift" => &["LeftShift", "RightShift"],
                "Ctrl" => &["LeftControl", "RightControl"],
                "Alt" => &["LeftAlt", "RightAlt"],
                "Meta" => &["LeftSuper", "RightSuper"],
                _ => &[],
            };
            let down = keys.iter().any(|key| {
                engine.reflection.enum_item("KeyCode", key).is_some_and(|item| i.keys_down.borrow().iter().any(|(k, _)| *k == item))
            });
            Ok(values([LuaValue::Boolean(down)]))
        }));
    }

    // ContextActionService dispatch (context_actions.rs)
    if let Some(actions) = c.ContextActionService {
        context_actions::install(&mut behaviors, input, actions);
    }

    // The device (--device): what scripts branch on for controller and touch paths
    if let Some(uis) = c.UserInputService {
        if valid(n.PreferredInput) {
            let item = match engine.options.device.as_str() {
                "touch" => "Touch",
                "gamepad" => "Gamepad",
                _ => "KeyboardAndMouse",
            };
            let getter: GetterFn = Rc::new(move |lua, engine, _| {
                Ok(engine.reflection.enum_item("PreferredInput", item).map_or(Ok(LuaValue::Nil), |item| enum_item_value(lua, engine, item))?)
            });
            behaviors.add_getter(uis, n.PreferredInput, getter);
        }
        let connected = engine.options.device == "gamepad";
        // Where a connected controller's glyphs live in Studio's content (Xbox art, the default)
        behaviors.add_method(uis, n.GetImageForKeyCode, method(move |lua, engine, _, args| {
            let code = enum_arg(args.get(1)).map(|item| engine.reflection.enum_item_info(item).name.to_string()).unwrap_or_default();
            let file = match code.as_str() {
                "ButtonL1" => "ButtonLB",
                "ButtonR1" => "ButtonRB",
                "ButtonL2" => "ButtonLT",
                "ButtonR2" => "ButtonRT",
                "ButtonL3" => "ButtonLS",
                "ButtonR3" => "ButtonRS",
                other => other,
            };
            let known = file.starts_with("Button") || file.starts_with("DPad");
            let image = if connected && known { format!("rbxasset://textures/ui/Controls/XboxController/{file}@2x.png") } else { String::new() };
            Ok(values([LuaValue::String(lua.create_string(&image)?)]))
        }));
        behaviors.add_method(uis, n.GetGamepadConnected, method(move |_, engine, _, args| {
            Ok(values([LuaValue::Boolean(connected && enum_named(engine, args.get(1), "Gamepad1"))]))
        }));
        let i = input.clone();
        behaviors.add_method(uis, n.IsGamepadButtonDown, method(move |_, engine, _, args| {
            let down = enum_named(engine, args.get(1), "Gamepad1")
                && enum_arg(args.get(2)).is_some_and(|code| i.keys_down.borrow().iter().any(|(k, _)| *k == code));
            Ok(values([LuaValue::Boolean(down)]))
        }));
    }

    // GuiService:Select(parent): the shown Selectable descendant with the lowest SelectionOrder
    // (first in tree order on a tie); the parent itself is never chosen and no match selects nothing
    if let (Some(gui_service), true) = (c.GuiService, valid(n.Select)) {
        let i = input.clone();
        behaviors.add_method(gui_service, n.Select, method(move |lua, engine, id, args| {
            let Some(parent) = super::lua::instance_id(args.get(1)) else { return Ok(none()) };
            let cx = Cx { lua, engine, i: &i };
            if let Some(target) = lowest_selectable(&cx, parent) {
                set_selected(&cx, id, Some(target));
            }
            Ok(none())
        }));
    }

    // GuiService.SelectedObject (measured, select-press.mjs): selecting fires SelectionGained on
    // the new object, SelectionLost on the old one; the selected object shows GuiState Hover.
    if let (Some(gui_service), true) = (c.GuiService, valid(n.SelectedObject)) {
        let i = input.clone();
        let setter: SetterFn = Rc::new(move |lua, engine, id, value| {
            let target = super::lua::instance_id(&value);
            let is_gui = target.is_some_and(|t| {
                let state = engine.state.borrow();
                i.class_is(state.dom.at(t).class, i.c.GuiObject)
            });
            if !matches!(value, LuaValue::Nil) && !is_gui {
                return raise("Unable to assign property SelectedObject. GuiObject expected");
            }
            let cx = Cx { lua, engine, i: &i };
            set_selected(&cx, id, target);
            Ok(())
        });
        behaviors.add_setter(gui_service, n.SelectedObject, setter);
    }

    // GuiState: stored (RawSet by the state machine); measured: an Interactable = false object and
    // its descendants report NonInteractable
    if let Some(gui_object) = c.GuiObject {
        let i = input.clone();
        let getter: GetterFn = Rc::new(move |lua, engine, id| {
            let (interactable, stored) = {
                let state = engine.state.borrow();
                (i.interactable(&state, id), state.dom.raw_get(id, i.n.GuiState).cloned())
            };
            if !interactable {
                return Ok(match engine.reflection.enum_item("GuiState", "NonInteractable") {
                    Some(item) => enum_item_value(lua, engine, item)?,
                    None => LuaValue::Nil,
                });
            }
            Ok(match stored {
                Some(value) => convert::to_lua(lua, engine, &value)?,
                None => LuaValue::Nil,
            })
        });
        behaviors.add_getter(gui_object, n.GuiState, getter);
    }

    // BasePlayerGui / LayerCollector:GetGuiObjectsAtPosition: every GuiObject at the point,
    // topmost first (measured on Studio's CoreGui)
    let objects_at = {
        let i = input.clone();
        method(move |lua, engine, id, args| {
            super::layout::update_all(engine);
            // The point is in the space of InputObject.Position and Mouse.Y: below the topbar inset
            let (x, y) = (args.number(1).unwrap_or(f64::NAN), args.number(2).unwrap_or(f64::NAN) + engine.options.topbar_inset);
            let order = i.draw_order(engine);
            let found: Vec<InstanceId> = {
                let state = engine.state.borrow();
                order
                    .iter()
                    .rev()
                    .filter(|entry| hit::contains(entry, x, y) && state.dom.is_descendant_of(entry.id, id))
                    .map(|entry| entry.id)
                    .collect()
            };
            let table = lua.create_table()?;
            for found in found {
                table.raw_push(instance_value(lua, engine, found)?)?;
            }
            Ok(values([LuaValue::Table(table)]))
        })
    };
    for class in [c.BasePlayerGui, c.LayerCollector].into_iter().flatten() {
        behaviors.add_method(class, n.GetGuiObjectsAtPosition, objects_at.clone());
    }

    // ScrollingFrame
    if let Some(scrolling_frame) = c.ScrollingFrame {
        let i = input.clone();
        behaviors.add_method(scrolling_frame, n.ScrollToTop, method(move |lua, engine, id, _| {
            let cx = Cx { lua, engine, i: &i };
            scrolling::stop(&cx, id);
            let (x, _) = cx.vector2(id, i.n.CanvasPosition);
            let value = cx.to_lua(&vector2(x, 0.0))?;
            cx.assign(id, "CanvasPosition", value)?;
            Ok(none())
        }));
        // Studio reports zero throughout wheel interpolation, including reversals and resets. The
        // spring's internal derivative is not this public inertial velocity.
        for name in [n.GetScrollVelocity, n.GetSampledInertialVelocity] {
            behaviors.add_method(scrolling_frame, name, method(|lua, _, _, _| Ok(values([vector2_lua(lua, 0.0, 0.0)?]))));
        }
        for name in [n.ResetScrollVelocity, n.ClearInertialScrolling] {
            let i = input.clone();
            behaviors.add_method(scrolling_frame, name, method(move |lua, engine, id, _| {
                scrolling::stop(&Cx { lua, engine, i: &i }, id);
                Ok(none())
            }));
        }
    }

    // TextBox
    if let Some(text_box) = c.TextBox {
        textbox::install(&mut behaviors, text_box, input);
    }
    drop(behaviors);
    engine.clear_member_cache();
}
