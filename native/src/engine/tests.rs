//! Engine tests that need the GUI layer (the emulator has the rest of the script-API tests, ports
//! of runtime/tests/{datatypes,modules,scheduler,smoke}.luau). Each test starts a fresh engine (with
//! the GUI layer) without a project.

use std::rc::Rc;

use mlua::prelude::*;

use crate::engine::GuiEngine;
use super::{
    Engine,
    lua::{self as engine_lua, globals::CallOutcome},
    scheduler,
};

fn start() -> (Lua, Rc<Engine>) {
    let engine = crate::engine::new_engine(crate::engine::options()).expect("engine");
    let lua = Lua::new();
    engine_lua::start(&lua, &engine).expect("start");
    // ModuleScript.Source is not scriptable: tests set sources through the engine
    let e = engine.clone();
    let set_source = lua
        .create_function(move |_, (module, source): (LuaAnyUserData, LuaString)| {
            let id = engine_lua::raw::instance_of_ud(&module).ok_or_else(|| LuaError::runtime("Instance expected"))?;
            e.state.borrow_mut().dom.at_mut(id).set_source(Some(Rc::from(&*source.as_bytes())));
            Ok(())
        })
        .unwrap();
    engine.lua.borrow().shared.as_ref().unwrap().raw_set("__setSource", set_source).unwrap();
    (lua, engine)
}

/// Runs `source` on a scheduler thread with the shared globals; returns the output lines
/// ("kind: message") after flushing deferred work.
fn run(lua: &Lua, engine: &Rc<Engine>, source: &str) -> Vec<String> {
    // A script environment (script = a detached ModuleScript named Test, require)
    let script = {
        let mut state = engine.state.borrow_mut();
        let class = engine.reflection.class_id("ModuleScript").unwrap();
        super::instance::create_named(&mut state, &engine.reflection, class, "Test")
    };
    let env = engine_lua::modules::environment(lua, engine, script).expect("environment");
    let function = lua.load(source).set_name("=Test").set_environment(env).into_function().expect("compile");
    let call = engine_lua::globals::call(lua, engine, function, LuaMultiValue::new()).expect("call");
    if let CallOutcome::Failed(message, _) = call.outcome().unwrap() {
        panic!("test script failed: {message}");
    }
    lines(engine)
}

fn lines(engine: &Engine) -> Vec<String> {
    engine.state.borrow().output.entries.iter().map(|e| format!("{}: {}", e.kind, e.message)).collect()
}

/// Everything the engine holds per instance, signal and connection, and the Luau heap.
#[derive(Debug, PartialEq, Clone, Copy)]
struct Held {
    instances: usize,
    userdata: usize,
    values: usize,
    signals: usize,
    connections: usize,
    layout: (usize, usize, usize),
}

fn held(lua: &Lua, engine: &Rc<Engine>) -> (Held, usize) {
    scheduler::flush_deferred(lua, engine).unwrap();
    for _ in 0..3 {
        lua.gc_collect().unwrap();
        engine_lua::raw::collect(lua, engine).unwrap();
    }
    let state = engine.state.borrow();
    let (signals, connections) = state.signals.counts();
    let held = Held {
        instances: state.dom.live_count(),
        userdata: engine_lua::raw::stats(engine).0,
        values: engine_lua::raw::value_cache_len(engine),
        signals,
        connections,
        layout: engine.layout().borrow().as_ref().map_or((0, 0, 0), |layout| layout.counts()),
    };
    (held, lua.used_memory())
}

/// A UI in the DataModel, laid out, every button's handlers capturing the whole tree (the cycle
/// Roblox never collects), tagged and with attributes: built `count` times by `source`.
const BUILD_UI: &str = r#"
    local Gui = Instance.new("ScreenGui")
    Gui.Name = "LifetimeGui"
    local Root = Instance.new("Frame")
    Root.Name = "Root"
    Root.Size = UDim2.fromOffset(400, 300)
    Root.Parent = Gui
    local Layout = Instance.new("UIListLayout")
    Layout.Parent = Root
    local State = { Root = Root, Buttons = {} }
    for Index = 1, 40 do
        local Button = Instance.new("TextButton")
        Button.Name = "Button" .. Index
        Button.Size = UDim2.new(1, 0, 0, 20)
        Button.Text = "Row " .. Index
        Button:AddTag("LifetimeRow")
        Button:SetAttribute("Index", Index)
        Instance.new("UICorner").Parent = Button
        local Stroke = Instance.new("UIStroke")
        Stroke.Parent = Button
        Button.Parent = Root
        State.Buttons[Index] = Button
        Button.Activated:Connect(function()
            State.Root.Visible = not State.Root.Visible
        end)
        Button:GetPropertyChangedSignal("Text"):Connect(function()
            Stroke.Thickness = #Button.Text
        end)
        Button.AncestryChanged:Connect(function()
            return Button, Root
        end)
        Button:GetAttributeChangedSignal("Index"):Connect(function()
            return Gui
        end)
        Button.MouseEnter:Once(function()
            return State
        end)
    end
    Root.Changed:Connect(function()
        return State
    end)
    Gui.Parent = game:GetService("StarterGui")
    -- (laid out, and datatype values read: the converted-value cache)
    assert(State.Buttons[40].AbsolutePosition.Y > State.Buttons[1].AbsolutePosition.Y)
    assert(Root.Size == UDim2.fromOffset(400, 300) and State.Buttons[3].AbsoluteSize.X == 400)
    State.Buttons[1].Text = "changed"
"#;

/// Destroy disconnects every connection of the instance and its descendants, and a destroyed
/// tree nothing references is freed whole: the instances, their userdata and converted values,
/// their signals and connections with the handlers, their layout state, and the Luau heap. Built
/// and destroyed again and again, nothing accumulates (slots are reused).
#[test]
fn a_destroyed_tree_is_freed_whole() {
    let (lua, engine) = start();
    let cycle = |destroy: &str| {
        let output = run(&lua, &engine, &format!("{BUILD_UI}\n{destroy}"));
        assert!(output.iter().all(|l| !l.starts_with("error")), "{output:?}");
    };
    let destroy = "Gui:Destroy()";
    // (a first cycle makes what is made once: member dispatch entries, signal userdata of services)
    cycle(destroy);
    let (baseline, baseline_heap) = held(&lua, &engine);
    let slots = engine.state.borrow().dom.slot_count();
    let capacity = engine.state.borrow().signals.capacity();
    for round in 0..25 {
        cycle(destroy);
        let (now, heap) = held(&lua, &engine);
        assert_eq!(now, baseline, "round {round}");
        assert!(heap < baseline_heap + 64 * 1024, "round {round}: Luau heap {heap} from {baseline_heap}");
    }
    assert_eq!(engine.state.borrow().dom.slot_count(), slots, "instance slots are reused");
    assert_eq!(engine.state.borrow().signals.capacity(), capacity, "signal and connection slots are reused");

    // Removed and not destroyed, the tree stays: its handlers are connected and capture it (what
    // Roblox does; lua/leaks.rs reports it). 1 ScreenGui, Root, 1 layout, 40 buttons with 2 children;
    // 5 connections a button and Root.Changed's, and the script the handlers' environment references
    cycle("Gui.Parent = nil");
    let (now, _) = held(&lua, &engine);
    assert_eq!(now.instances, baseline.instances + 3 + 40 * 3 + 1);
    assert_eq!(now.connections, baseline.connections + 40 * 5 + 1);
    let report = engine_lua::leaks::report(&lua, &engine).unwrap();
    assert_eq!(report["outsideDataModel"]["heldByConnectedHandlers"], 123, "{report:#}");
    let handlers = report["handlers"].as_array().unwrap();
    assert_eq!(handlers.len(), 6, "{report:#}");
    assert!(handlers.iter().all(|handler| handler["trees"] == 1 && handler["handler"].as_str().unwrap().starts_with("Test:")), "{report:#}");
    let activated = handlers.iter().find(|handler| handler["signal"] == "Activated").unwrap();
    assert_eq!((&activated["connections"], &activated["class"]), (&serde_json::json!(40), &serde_json::json!("TextButton")));
    assert!(activated["captures"].as_str().unwrap().contains("field"), "{activated:#}");
}
