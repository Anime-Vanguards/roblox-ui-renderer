//! ContextActionService: BindAction / BindActionAtPriority / UnbindAction / UnbindAllActions, run for
//! key and gamepad input. Not measured against Studio beyond the documented contract: the highest
//! priority runs first (the newest bind first within a priority), a handler returning
//! ContextActionResult.Pass lets the next one run, anything else sinks the input, Begin and End walk
//! the chain separately, and a sunk input reaches UserInputService with gameProcessedEvent true and no
//! longer activates the selected button. A handler that yields counts as sinking.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use mlua::prelude::*;

use super::{Cx, Input, enum_arg, method};
use super::super::{
    ClassId, Engine, InstanceId,
    lua::{enum_item_value, instance_value, none},
    reflection::EnumItemId,
    scheduler,
    services::Behaviors,
};

struct Bound {
    name: String,
    handler: LuaFunction,
    priority: f64,
    inputs: Vec<EnumItemId>,
    order: u64,
}

#[derive(Default)]
pub struct Actions {
    bound: RefCell<Vec<Bound>>,
    next_order: Cell<u64>,
}

impl Actions {
    fn bind(&self, name: String, handler: LuaFunction, priority: f64, inputs: Vec<EnumItemId>) {
        let order = self.next_order.get();
        self.next_order.set(order + 1);
        let mut bound = self.bound.borrow_mut();
        bound.retain(|action| action.name != name);
        bound.push(Bound { name, handler, priority, inputs, order });
    }

    fn unbind(&self, name: &str) {
        self.bound.borrow_mut().retain(|action| action.name != name);
    }

    fn clear(&self) {
        self.bound.borrow_mut().clear();
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

pub(super) fn install(behaviors: &mut Behaviors, input: &Rc<Input>, class: ClassId) {
    let n = &input.n;
    let i = input.clone();
    behaviors.add_method(class, n.BindAction, method(move |_, engine, _, args| {
        let name = args.get(1).to_string().unwrap_or_default();
        let LuaValue::Function(handler) = args.get(2).clone() else { return Ok(none()) };
        let inputs = collect_inputs(args.0.iter().skip(4).cloned());
        i.actions.bind(name, handler, default_priority(engine), inputs);
        Ok(none())
    }));
    let i = input.clone();
    behaviors.add_method(class, n.BindActionAtPriority, method(move |_, engine, _, args| {
        let name = args.get(1).to_string().unwrap_or_default();
        let LuaValue::Function(handler) = args.get(2).clone() else { return Ok(none()) };
        let priority = match args.get(4) {
            LuaValue::Integer(value) => *value as f64,
            LuaValue::Number(value) => *value,
            _ => default_priority(engine),
        };
        let inputs = collect_inputs(args.0.iter().skip(5).cloned());
        i.actions.bind(name, handler, priority, inputs);
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
