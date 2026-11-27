//! ContextActionService: BindAction / BindActionAtPriority / UnbindAction / UnbindAllActions,
//! SetTitle / SetPosition / SetImage / SetDescription / GetBoundActionInfo / GetAllBoundActionInfo.
//!
//! Priority/sink rules follow the documented contract: the highest priority runs first (the newest
//! bind first within a priority), a handler returning ContextActionResult.Pass lets the next one
//! run, anything else sinks the input, Begin and End walk the chain separately, and a sunk input
//! reaches UserInputService with gameProcessedEvent true and no longer activates the selected
//! button. A handler that yields counts as sinking.
//!
//! Touch button setters and info queries are measured in a playing client (Studio 0.747.0.7471481,
//! desktop, touch and gamepad, identities 2 / 4 / 8):
//!   - SetTitle / SetImage / SetDescription cast both arguments as Roblox string parameters
//!     (second first: missing/nil raises "Argument 2 missing or nil", then "Argument 1 missing or
//!     nil"; numbers/booleans/tables stringify), store the field on the bound action even when
//!     createTouchButton is false, and succeed silently when the action is not bound.
//!   - SetPosition checks arg 2 first (missing/nil -> "Argument 2 missing or nil"; non-UDim2 ->
//!     "Unable to cast <type> to UDim2"), then arg 1 as a string parameter; it stores the position
//!     on the bound action, does not expose a `position` key in GetBoundActionInfo, and succeeds
//!     silently when the action is not bound.
//!   - GetBoundActionInfo returns {} when unbound, else { inputTypes, priorityLevel, stackOrder,
//!     createTouchButton? (only when true), title?, image?, description? }. Rebinding the same
//!     name clears title / image / description / position and increments stackOrder.
//!
//! Measured in 0.747 clients, desktop and touch alike (each case in two sessions; notes/interaction.md "Touch
//! buttons"):
//!   - UnbindAction calls the action's handler once with Cancel and a new InputObject (UserInputType None,
//!     UserInputState Cancel, KeyCode None, Position and Delta 0), inside the call (a handler that yields lets it
//!     return); binding a bound name again calls the old handler that way twice. UnbindAllActions calls none.
//!   - SetTitle / SetImage / SetDescription / SetPosition / GetButton on a name that is not bound warn
//!     "ContextActionService could not find the function passed in, doing nothing." (UnbindAction of one does not).
//!     GetButton returns nil then, and on a device without a touch screen.
//! On a touch device a bind with createTouchButton shows a button (touch_buttons.rs), and GetButton returns it (a
//! touch-screen client's ContextActionTouch answers it; touch_buttons.rs).

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use mlua::prelude::*;
use rbx_dom_weak::types as dom;

use super::{Cx, Input, enum_arg, method, touch_buttons};
use super::super::{
    ClassId, Engine, InstanceId,
    instance as core,
    lua::{convert, enum_item_value, instance_value, none, raise, values},
    reflection::EnumItemId,
    scheduler,
    services::{self, Behaviors, StringParam},
    value::Value,
};

const FIRST: &[StringParam] = &[StringParam::at(1)];
const FIRST_TWO: &[StringParam] = &[StringParam::at(1), StringParam::at(2)];

struct Bound {
    name: String,
    handler: LuaFunction,
    create_touch_button: bool,
    priority: f64,
    inputs: Vec<EnumItemId>,
    order: u64,
    title: Option<String>,
    image: Option<String>,
    description: Option<String>,
    position: Option<dom::UDim2>,
}

#[derive(Default)]
pub struct Actions {
    bound: RefCell<Vec<Bound>>,
    next_order: Cell<u64>,
    /// The touch buttons' Luau half (touch_buttons.rs), made by the first bind that asks for a button on a touch device
    pub(super) touch: RefCell<Option<LuaTable>>,
    /// The ContextActionGui it made
    pub(super) touch_gui: Cell<Option<InstanceId>>,
}

impl Actions {
    /// Binds `name`; the handler it replaced, if the name was bound
    fn bind(&self, name: String, handler: LuaFunction, create_touch_button: bool, priority: f64, inputs: Vec<EnumItemId>) -> Option<LuaFunction> {
        let order = self.next_order.get() + 1;
        self.next_order.set(order);
        let mut bound = self.bound.borrow_mut();
        let replaced = bound.iter().position(|action| action.name == name).map(|index| bound.remove(index).handler);
        bound.push(Bound {
            name,
            handler,
            create_touch_button,
            priority,
            inputs,
            order,
            title: None,
            image: None,
            description: None,
            position: None,
        });
        replaced
    }

    /// Unbinds `name`; its handler, if it was bound
    fn unbind(&self, name: &str) -> Option<LuaFunction> {
        let mut bound = self.bound.borrow_mut();
        let index = bound.iter().position(|action| action.name == name)?;
        Some(bound.remove(index).handler)
    }

    /// The handler bound to `name`
    pub(super) fn handler(&self, name: &str) -> Option<LuaFunction> {
        self.bound.borrow().iter().find(|a| a.name == name).map(|a| a.handler.clone())
    }

    fn is_bound(&self, name: &str) -> bool {
        self.bound.borrow().iter().any(|a| a.name == name)
    }

    fn clear(&self) {
        self.bound.borrow_mut().clear();
    }

    fn set_title(&self, name: &str, title: String) -> bool {
        let mut bound = self.bound.borrow_mut();
        let Some(action) = bound.iter_mut().find(|a| a.name == name) else { return false };
        action.title = Some(title);
        true
    }

    fn set_image(&self, name: &str, image: String) -> bool {
        let mut bound = self.bound.borrow_mut();
        let Some(action) = bound.iter_mut().find(|a| a.name == name) else { return false };
        action.image = Some(image);
        true
    }

    fn set_description(&self, name: &str, description: String) -> bool {
        let mut bound = self.bound.borrow_mut();
        let Some(action) = bound.iter_mut().find(|a| a.name == name) else { return false };
        action.description = Some(description);
        true
    }

    fn set_position(&self, name: &str, position: dom::UDim2) -> bool {
        let mut bound = self.bound.borrow_mut();
        let Some(action) = bound.iter_mut().find(|a| a.name == name) else { return false };
        action.position = Some(position);
        true
    }

    fn info_table(&self, lua: &Lua, engine: &Rc<Engine>, name: &str) -> LuaResult<LuaTable> {
        let bound = self.bound.borrow();
        let Some(action) = bound.iter().find(|a| a.name == name) else {
            return lua.create_table();
        };
        action_info_table(lua, engine, action)
    }

    fn all_info_table(&self, lua: &Lua, engine: &Rc<Engine>) -> LuaResult<LuaTable> {
        let out = lua.create_table()?;
        let bound = self.bound.borrow();
        for action in bound.iter() {
            out.raw_set(action.name.as_str(), action_info_table(lua, engine, action)?)?;
        }
        Ok(out)
    }

    /// (name, handler) of the actions bound to `code` or `input_type`, in the order they run.
    fn chain(&self, code: EnumItemId, input_type: Option<EnumItemId>) -> Vec<(String, LuaFunction)> {
        let bound = self.bound.borrow();
        let mut matching: Vec<&Bound> = bound
            .iter()
            .filter(|action| action.inputs.iter().any(|&item| item == code || Some(item) == input_type))
            .collect();
        matching.sort_by(|a, b| b.priority.total_cmp(&a.priority).then(b.order.cmp(&a.order)));
        matching.into_iter().map(|action| (action.name.clone(), action.handler.clone())).collect()
    }
}

fn action_info_table(lua: &Lua, engine: &Rc<Engine>, action: &Bound) -> LuaResult<LuaTable> {
    let table = lua.create_table()?;
    let inputs = lua.create_table()?;
    for (index, &item) in action.inputs.iter().enumerate() {
        inputs.raw_set(index + 1, enum_item_value(lua, engine, item)?)?;
    }
    table.raw_set("inputTypes", inputs)?;
    table.raw_set("priorityLevel", action.priority)?;
    table.raw_set("stackOrder", action.order)?;
    if action.create_touch_button {
        table.raw_set("createTouchButton", true)?;
    }
    if let Some(title) = &action.title {
        table.raw_set("title", title.as_str())?;
    }
    if let Some(image) = &action.image {
        table.raw_set("image", image.as_str())?;
    }
    if let Some(description) = &action.description {
        table.raw_set("description", description.as_str())?;
    }
    Ok(table)
}

/// Roblox's cast-error type name for a non-UDim2 value passed to SetPosition (measured, Studio 0.747:
/// "Unable to cast string to UDim2", "Unable to cast Vector2 to UDim2").
fn cast_type_name(engine: &Engine, value: &LuaValue) -> String {
    match value {
        LuaValue::Nil => "nil".into(),
        LuaValue::Boolean(_) => "bool".into(),
        LuaValue::Integer(_) | LuaValue::Number(_) => "double".into(),
        LuaValue::String(_) => "string".into(),
        LuaValue::Function(_) => "Function".into(),
        LuaValue::Table(table) => {
            if table.raw_len() > 0 || table.is_empty() {
                "Array".into()
            } else {
                "Dictionary".into()
            }
        }
        LuaValue::Vector(_) => "Vector3".into(),
        LuaValue::UserData(_) => match convert::from_lua(value) {
            Value::Instance(_) => "Object".into(),
            Value::EnumItem(item) => engine.reflection.enum_item_type_name(item).to_string(),
            Value::Vector2(_) => "Vector2".into(),
            Value::Vector3(_) => "Vector3".into(),
            Value::UDim(_) => "UDim".into(),
            Value::UDim2(_) => "UDim2".into(),
            Value::Color3(_) => "Color3".into(),
            Value::Rect(_) => "Rect".into(),
            Value::NumberRange(_) => "NumberRange".into(),
            Value::NumberSequence(_) => "NumberSequence".into(),
            Value::ColorSequence(_) => "ColorSequence".into(),
            Value::Font(_) => "Font".into(),
            Value::Content(_) => "Content".into(),
            _ => "value".into(),
        },
        _ => "value".into(),
    }
}

fn collect_inputs(args: impl Iterator<Item = LuaValue>) -> Vec<EnumItemId> {
    let mut inputs = Vec::new();
    for value in args {
        if let LuaValue::Table(table) = &value {
            inputs.extend(table.clone().sequence_values::<LuaValue>().filter_map(|item| item.ok()).filter_map(|item| enum_arg(&item)));
        } else if let Some(item) = enum_arg(&value) {
            inputs.push(item);
        }
    }
    inputs
}

fn default_priority(engine: &Engine) -> f64 {
    engine.reflection.enum_item("ContextActionPriority", "Default").map_or(2000.0, |item| engine.reflection.enum_item_info(item).value as f64)
}

fn string_arg(args: &super::Args, index: usize) -> String {
    args.string(index).map(|s| s.to_string_lossy()).unwrap_or_default()
}

/// Measured (0.747, desktop and touch): what a setter or GetButton warns for a name that is not bound
const NOT_BOUND: &str = "ContextActionService could not find the function passed in, doing nothing.";

/// Runs `handler` for action `name` in a thread of its own (resumed at once, as the client calls it inside the call or
/// event): (name, UserInputState `state`, `object`)
pub(super) fn call(lua: &Lua, engine: &Rc<Engine>, name: &str, handler: &LuaFunction, state: &str, object: LuaValue) -> LuaResult<()> {
    let Some(state_item) = engine.reflection.enum_item("UserInputState", state) else { return Ok(()) };
    let thread = lua.create_thread(handler.clone())?;
    let args = (name, enum_item_value(lua, engine, state_item)?, object).into_lua_multi(lua)?;
    scheduler::resume(lua, engine, &thread, args);
    Ok(())
}

/// The Cancel an unbind or rebind hands the action's handler: a new InputObject with UserInputType None, state Cancel,
/// KeyCode None and a zero Position and Delta (measured, 0.747)
fn cancel(lua: &Lua, engine: &Rc<Engine>, input: &Input, name: &str, handler: &LuaFunction) -> LuaResult<()> {
    let Some(class) = input.c.InputObject else { return Ok(()) };
    let n = &input.n;
    let object = {
        let mut state = engine.state.borrow_mut();
        let id = core::create(&mut state, &engine.reflection, class);
        let zero = super::vector3(0.0, 0.0, 0.0);
        for (property, value) in [
            (n.Name, Value::String(super::super::value::Str::from("InputObject"))),
            (n.UserInputType, item_value(engine, "UserInputType", "None")),
            (n.UserInputState, item_value(engine, "UserInputState", "Cancel")),
            (n.KeyCode, item_value(engine, "KeyCode", "None")),
            (n.Position, zero.clone()),
            (n.Delta, zero),
        ] {
            core::raw_set_silent(&mut state, &engine.reflection, id, property, value);
        }
        id
    };
    call(lua, engine, name, handler, "Cancel", instance_value(lua, engine, object)?)
}

fn item_value(engine: &Engine, enum_name: &str, item: &str) -> Value {
    engine.reflection.enum_item(enum_name, item).map_or(Value::Nil, Value::EnumItem)
}

pub(super) fn install(behaviors: &mut Behaviors, input: &Rc<Input>, class: ClassId) {
    let n = &input.n;
    let i = input.clone();
    behaviors.add_method(class, n.BindAction, method(move |lua, engine, _, args| {
        let name = args.get(1).to_string().unwrap_or_default();
        let LuaValue::Function(handler) = args.get(2).clone() else { return Ok(none()) };
        let create_touch_button = !matches!(args.get(3), LuaValue::Nil | LuaValue::Boolean(false));
        let inputs = collect_inputs(args.0.iter().skip(4).cloned());
        let replaced = i.actions.bind(name.clone(), handler, create_touch_button, default_priority(engine), inputs);
        bound(lua, engine, &i, &name, replaced, create_touch_button)?;
        Ok(none())
    }));
    let i = input.clone();
    behaviors.add_method(class, n.BindActionAtPriority, method(move |lua, engine, _, args| {
        let name = args.get(1).to_string().unwrap_or_default();
        let LuaValue::Function(handler) = args.get(2).clone() else { return Ok(none()) };
        let create_touch_button = !matches!(args.get(3), LuaValue::Nil | LuaValue::Boolean(false));
        let priority = match args.get(4) {
            LuaValue::Integer(value) => *value as f64,
            LuaValue::Number(value) => *value,
            _ => default_priority(engine),
        };
        let inputs = collect_inputs(args.0.iter().skip(5).cloned());
        let replaced = i.actions.bind(name.clone(), handler, create_touch_button, priority, inputs);
        bound(lua, engine, &i, &name, replaced, create_touch_button)?;
        Ok(none())
    }));
    let i = input.clone();
    behaviors.add_method(class, n.UnbindAction, method(move |lua, engine, _, args| {
        let name = args.get(1).to_string().unwrap_or_default();
        if let Some(handler) = i.actions.unbind(&name) {
            cancel(lua, engine, &i, &name, &handler)?;
            touch_buttons::unbind(&i, &name)?;
        }
        Ok(none())
    }));
    let i = input.clone();
    behaviors.add_method(class, n.UnbindAllActions, method(move |_, _, _, _| {
        i.actions.clear();
        touch_buttons::clear(&i)?;
        Ok(none())
    }));
    let i = input.clone();
    behaviors.add_method(class, n.SetTitle, services::strings(FIRST_TWO, method(move |lua, engine, _, args| {
        let (name, title) = (string_arg(args, 1), string_arg(args, 2));
        if !i.actions.set_title(&name, title.clone()) {
            engine.warn(NOT_BOUND);
            return Ok(none());
        }
        touch_buttons::update(&i, "Title", &name, LuaValue::String(lua.create_string(&title)?))?;
        Ok(none())
    })));
    let i = input.clone();
    behaviors.add_method(class, n.SetImage, services::strings(FIRST_TWO, method(move |lua, engine, _, args| {
        let (name, image) = (string_arg(args, 1), string_arg(args, 2));
        if !i.actions.set_image(&name, image.clone()) {
            engine.warn(NOT_BOUND);
            return Ok(none());
        }
        touch_buttons::update(&i, "Image", &name, LuaValue::String(lua.create_string(&image)?))?;
        Ok(none())
    })));
    let i = input.clone();
    behaviors.add_method(class, n.SetDescription, services::strings(FIRST_TWO, method(move |_, engine, _, args| {
        if !i.actions.set_description(&string_arg(args, 1), string_arg(args, 2)) {
            engine.warn(NOT_BOUND);
        }
        Ok(none())
    })));
    let i = input.clone();
    behaviors.add_method(class, n.SetPosition, method(move |lua, engine, _, args| {
        // Measured (Studio 0.747): argument 2 (UDim2) is checked before argument 1 (actionName).
        let pos_arg = args.get(2);
        if pos_arg.is_nil() {
            return raise("Argument 2 missing or nil");
        }
        let Value::UDim2(position) = convert::from_lua(pos_arg) else {
            return raise(format!("Unable to cast {} to UDim2", cast_type_name(engine, pos_arg)));
        };
        let cast = services::cast_strings(lua, engine, args, FIRST)?;
        let args = cast.as_ref().unwrap_or(args);
        let name = string_arg(args, 1);
        if !i.actions.set_position(&name, position) {
            engine.warn(NOT_BOUND);
            return Ok(none());
        }
        touch_buttons::update(&i, "Position", &name, pos_arg.clone())?;
        Ok(none())
    }));
    let i = input.clone();
    behaviors.add_method(class, n.GetBoundActionInfo, services::strings(FIRST, method(move |lua, engine, _, args| {
        Ok(values([LuaValue::Table(i.actions.info_table(lua, engine, &string_arg(args, 1))?)]))
    })));
    let i = input.clone();
    behaviors.add_method(class, n.GetAllBoundActionInfo, method(move |lua, engine, _, _| {
        Ok(values([LuaValue::Table(i.actions.all_info_table(lua, engine)?)]))
    }));
    // Measured (0.747): a touch-screen client returns the action's touch button at once (touch_buttons.rs); nil for an
    // action without one and on other devices; a name not bound warns
    let i = input.clone();
    behaviors.add_method(class, n.GetButton, services::strings(FIRST, method(move |_, engine, _, args| {
        let name = string_arg(args, 1);
        if !i.actions.is_bound(&name) {
            engine.warn(NOT_BOUND);
            return Ok(values([LuaValue::Nil]));
        }
        Ok(values([touch_buttons::button(&i, &name)?]))
    })));
}

/// After a BindAction: the replaced handler's two Cancels and the end of its button, then the new button
fn bound(lua: &Lua, engine: &Rc<Engine>, input: &Input, name: &str, replaced: Option<LuaFunction>, create_touch_button: bool) -> LuaResult<()> {
    if let Some(old) = replaced {
        cancel(lua, engine, input, name, &old)?;
        cancel(lua, engine, input, name, &old)?;
        touch_buttons::unbind(input, name)?;
    }
    if create_touch_button && touch_buttons::enabled(engine) {
        touch_buttons::bind(lua, engine, input, name)?;
    }
    Ok(())
}

/// Runs the chain for one input state. Returns whether a handler sank it.
pub(super) fn run(cx: &Cx, code: EnumItemId, input_type: &str, state: &str, object: InstanceId) -> LuaResult<bool> {
    let input_type = cx.engine.reflection.enum_item("UserInputType", input_type);
    let chain = cx.i.actions.chain(code, input_type);
    if chain.is_empty() {
        return Ok(false);
    }
    let state_item = cx.engine.reflection.enum_item("UserInputState", state).expect("UserInputState");
    let pass = cx.engine.reflection.enum_item("ContextActionResult", "Pass");
    let wrap: LuaFunction = cx
        .lua
        .load("return function(Slot, Handler) return function(...) Slot[1] = Handler(...); Slot.Finished = true end end")
        .set_name("=rur.contextaction")
        .call(())?;
    for (name, handler) in chain {
        let slot = cx.lua.create_table()?;
        let runner: LuaFunction = wrap.call((slot.clone(), handler))?;
        let thread = cx.lua.create_thread(runner)?;
        let args = (name, enum_item_value(cx.lua, cx.engine, state_item)?, instance_value(cx.lua, cx.engine, object)?).into_lua_multi(cx.lua)?;
        let ran = scheduler::resume(cx.lua, cx.engine, &thread, args);
        let finished = slot.raw_get::<bool>("Finished").unwrap_or(false);
        let result: LuaValue = slot.raw_get(1).unwrap_or(LuaValue::Nil);
        let passed = ran && finished && enum_arg(&result).is_some() && enum_arg(&result) == pass;
        if !passed {
            return Ok(true);
        }
    }
    Ok(false)
}
