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

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use mlua::prelude::*;
use rbx_dom_weak::types as dom;

use super::{Cx, Input, enum_arg, method};
use super::super::{
    ClassId, Engine, InstanceId,
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
}

impl Actions {
    fn bind(&self, name: String, handler: LuaFunction, create_touch_button: bool, priority: f64, inputs: Vec<EnumItemId>) {
        let order = self.next_order.get() + 1;
        self.next_order.set(order);
        let mut bound = self.bound.borrow_mut();
        bound.retain(|action| action.name != name);
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
    }

    fn unbind(&self, name: &str) {
        self.bound.borrow_mut().retain(|action| action.name != name);
    }

    fn clear(&self) {
        self.bound.borrow_mut().clear();
    }

    fn set_title(&self, name: &str, title: String) {
        if let Some(action) = self.bound.borrow_mut().iter_mut().find(|a| a.name == name) {
            action.title = Some(title);
        }
    }

    fn set_image(&self, name: &str, image: String) {
        if let Some(action) = self.bound.borrow_mut().iter_mut().find(|a| a.name == name) {
            action.image = Some(image);
        }
    }

    fn set_description(&self, name: &str, description: String) {
        if let Some(action) = self.bound.borrow_mut().iter_mut().find(|a| a.name == name) {
            action.description = Some(description);
        }
    }

    fn set_position(&self, name: &str, position: dom::UDim2) {
        if let Some(action) = self.bound.borrow_mut().iter_mut().find(|a| a.name == name) {
            action.position = Some(position);
        }
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

pub(super) fn install(behaviors: &mut Behaviors, input: &Rc<Input>, class: ClassId) {
    let n = &input.n;
    let i = input.clone();
    behaviors.add_method(class, n.BindAction, method(move |_, engine, _, args| {
        let name = args.get(1).to_string().unwrap_or_default();
        let LuaValue::Function(handler) = args.get(2).clone() else { return Ok(none()) };
        let create_touch_button = !matches!(args.get(3), LuaValue::Nil | LuaValue::Boolean(false));
        let inputs = collect_inputs(args.0.iter().skip(4).cloned());
        i.actions.bind(name, handler, create_touch_button, default_priority(engine), inputs);
        Ok(none())
    }));
    let i = input.clone();
    behaviors.add_method(class, n.BindActionAtPriority, method(move |_, engine, _, args| {
        let name = args.get(1).to_string().unwrap_or_default();
        let LuaValue::Function(handler) = args.get(2).clone() else { return Ok(none()) };
        let create_touch_button = !matches!(args.get(3), LuaValue::Nil | LuaValue::Boolean(false));
        let priority = match args.get(4) {
            LuaValue::Integer(value) => *value as f64,
            LuaValue::Number(value) => *value,
            _ => default_priority(engine),
        };
        let inputs = collect_inputs(args.0.iter().skip(5).cloned());
        i.actions.bind(name, handler, create_touch_button, priority, inputs);
        Ok(none())
    }));
    let i = input.clone();
    behaviors.add_method(class, n.UnbindAction, method(move |_, _, _, args| {
        i.actions.unbind(&args.get(1).to_string().unwrap_or_default());
        Ok(none())
    }));
    let i = input.clone();
    behaviors.add_method(class, n.UnbindAllActions, method(move |_, _, _, _| {
        i.actions.clear();
        Ok(none())
    }));
    let i = input.clone();
    behaviors.add_method(class, n.SetTitle, services::strings(FIRST_TWO, method(move |_, _, _, args| {
        i.actions.set_title(&string_arg(args, 1), string_arg(args, 2));
        Ok(none())
    })));
    let i = input.clone();
    behaviors.add_method(class, n.SetImage, services::strings(FIRST_TWO, method(move |_, _, _, args| {
        i.actions.set_image(&string_arg(args, 1), string_arg(args, 2));
        Ok(none())
    })));
    let i = input.clone();
    behaviors.add_method(class, n.SetDescription, services::strings(FIRST_TWO, method(move |_, _, _, args| {
        i.actions.set_description(&string_arg(args, 1), string_arg(args, 2));
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
        i.actions.set_position(&string_arg(args, 1), position);
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
