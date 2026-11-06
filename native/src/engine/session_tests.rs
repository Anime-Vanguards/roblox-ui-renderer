//! Session tests (the port of runtime/tests/session_stepper.luau, including its scroll assertion
//! through engine::input, and in-process sessions); the scheduler's own tests are the emulator's.

use std::{cell::RefCell, rc::Rc};

use mlua::prelude::*;
use serde_json::json;

use crate::engine::GuiEngine;
use super::{
    Engine,
    lua::{self as engine_lua, globals::CallOutcome},
    scheduler::{self, Phase, f32r},
    session::Stepper,
};

fn start() -> (Lua, Rc<Engine>) {
    let engine = crate::engine::new_engine(crate::engine::options()).expect("engine");
    let lua = Lua::new();
    engine_lua::start(&lua, &engine).expect("start");
    engine.state.borrow_mut().sched.set_main_thread(&lua.current_thread());
    (lua, engine)
}

fn run(lua: &Lua, engine: &Rc<Engine>, source: &str) {
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
}

fn global(engine: &Engine, name: &str) -> LuaValue {
    let shared = engine.lua.borrow().shared.clone().unwrap();
    shared.get::<LuaTable>("_G").unwrap().get(name).unwrap()
}

fn number(engine: &Engine, name: &str) -> f64 {
    match global(engine, name) {
        LuaValue::Number(n) => n,
        LuaValue::Integer(n) => n as f64,
        other => panic!("{name}: {other:?}"),
    }
}

fn now(engine: &Engine) -> f64 {
    engine.state.borrow().sched.now
}

type Log = Rc<RefCell<Vec<f64>>>;

fn record(engine: &Engine, phase: Phase) -> Log {
    let log: Log = Rc::default();
    let sink = log.clone();
    engine.state.borrow_mut().sched.add_hook(
        phase,
        Rc::new(move |_, _, dt| {
            sink.borrow_mut().push(dt);
            Ok(())
        }),
    );
    log
}

#[test]
fn session_stepper_display_cadence() {
    let (lua, engine) = start();
    let render_dts = record(&engine, Phase::Render);
    let heartbeat_dts = record(&engine, Phase::Heartbeat);
    run(
        &lua,
        &engine,
        r#"
        local Root = Instance.new("ScreenGui")
        Root.IgnoreGuiInset = true
        Root.Parent = game:GetService("CoreGui")
        local Scroll = Instance.new("ScrollingFrame")
        Scroll.Size = UDim2.fromOffset(400, 400)
        Scroll.CanvasSize = UDim2.fromOffset(400, 2000)
        Scroll.Parent = Root
        _G.Scroll = Scroll
        _G.Changes = 0
        Scroll:GetPropertyChangedSignal("CanvasPosition"):Connect(function() _G.Changes += 1 end)
        _G.Value = Instance.new("NumberValue")
        local Tween = game:GetService("TweenService"):Create(_G.Value, TweenInfo.new(1, Enum.EasingStyle.Linear), { Value = 1 })
        Tween:Play()
        task.delay(0.1, function() _G.WokeAt = os.clock() - 10000 end)
        "#,
    );
    let value = || match global(&engine, "Value") {
        LuaValue::UserData(ud) => {
            let id = engine_lua::raw::instance_of_ud(&ud).unwrap();
            let name = engine.reflection.name_id("Value").unwrap();
            engine.state.borrow().dom.number(id, name).unwrap()
        }
        other => panic!("{other:?}"),
    };

    super::layout::update_all(&engine);
    scheduler::flush_deferred(&lua, &engine).unwrap();
    let input = super::input::get(&engine).unwrap();
    input.dispatch(&lua, &engine, &json!({ "type": "wheel", "x": 200, "y": 200, "notches": -1 })).unwrap();
    let mut advance = Stepper::new(&engine);
    let start = now(&engine);
    let (frames, intervals) = advance.advance(&lua, &engine, 0.25, Some("display")).unwrap();
    assert!(frames == 1 && intervals == 15.0);
    assert!((now(&engine) - start - 0.25).abs() < 1e-9);
    assert!(render_dts.borrow().len() == 1 && (render_dts.borrow()[0] - 0.25).abs() < 1e-7);
    assert_eq!(heartbeat_dts.borrow().len(), 60, "display cadence must preserve heartbeat intervals");
    let woke = number(&engine, "WokeAt");
    assert!(woke >= start + 0.1 && woke < start + 0.11, "timer resumed at display end instead of heartbeat deadline");
    assert!((value() - 0.25).abs() < 1e-6, "tween lost elapsed time");
    let scroll_y = {
        let LuaValue::UserData(ud) = global(&engine, "Scroll") else { panic!("Scroll") };
        let id = engine_lua::raw::instance_of_ud(&ud).unwrap();
        let name = engine.reflection.name_id("CanvasPosition").unwrap();
        engine.state.borrow().dom.raw_get(id, name).and_then(|v| v.as_vector2()).unwrap().y
    };
    assert!(number(&engine, "Changes") == 1.0 && scroll_y > 0.0, "scroll must advance once per presented display");

    render_dts.borrow_mut().clear();
    heartbeat_dts.borrow_mut().clear();
    let fixed_start = now(&engine);
    let (frames, intervals) = advance.advance(&lua, &engine, 0.25, None).unwrap();
    assert!(frames == 15 && intervals == 15.0 && render_dts.borrow().len() == 15 && heartbeat_dts.borrow().len() == 60);
    assert!((now(&engine) - fixed_start - 0.25).abs() < 1e-9);
    assert!((value() - 0.5).abs() < 1e-6);

    let mut fractional = Stepper::new(&engine);
    let before = now(&engine);
    assert_eq!(fractional.advance(&lua, &engine, 1.0 / 120.0, Some("display")).unwrap().0, 1);
    assert!((now(&engine) - before - 1.0 / 120.0).abs() < 1e-9);
    assert_eq!(fractional.advance(&lua, &engine, 1.0 / 120.0, Some("display")).unwrap().0, 1);
    assert!((now(&engine) - before - 1.0 / 60.0).abs() < 1e-9);
    assert_eq!(fractional.advance(&lua, &engine, 0.0, Some("display")).unwrap().0, 0);
    let frame = engine.state.borrow().sched.frame_duration;
    let next_interval = (now(&engine) / frame + 1e-6).floor() as usize + 2;
    assert_eq!(fractional.advance(&lua, &engine, 2.0 / 60.0, Some("display")).unwrap().0, 1);
    heartbeat_dts.borrow_mut().clear();
    assert_eq!(fractional.advance(&lua, &engine, 1.0 / 60.0, None).unwrap().0, 1);
    let pattern = engine.state.borrow().sched.heartbeat_pattern.clone().unwrap();
    let expected = f32r(pattern[next_interval % 2][1] / 1000.0);
    assert_eq!(heartbeat_dts.borrow()[1], expected, "mixed display/fixed cadence lost heartbeat pattern phase");

    // 120 / 144 Hz displays keep the independent 240 Hz Heartbeat and its measured pattern
    let beat_times: Log = Rc::default();
    let sink = beat_times.clone();
    engine.state.borrow_mut().sched.add_hook(
        Phase::Heartbeat,
        Rc::new(move |_, engine, _| {
            sink.borrow_mut().push(engine.state.borrow().sched.now);
            Ok(())
        }),
    );
    let mut previous: Option<(Vec<f64>, Vec<f64>)> = None;
    for rate in [120u32, 144] {
        render_dts.borrow_mut().clear();
        heartbeat_dts.borrow_mut().clear();
        beat_times.borrow_mut().clear();
        let from = now(&engine);
        for _ in 0..rate {
            let (presented, elapsed) = fractional.advance(&lua, &engine, 1.0 / rate as f64, Some("display")).unwrap();
            assert!(presented == 1 && (elapsed - 60.0 / rate as f64).abs() < 1e-9);
        }
        assert_eq!(render_dts.borrow().len(), rate as usize, "display was capped below {rate} Hz");
        assert_eq!(heartbeat_dts.borrow().len(), 240, "high-refresh display changed the independent Heartbeat rate");
        assert!((now(&engine) - from - 1.0).abs() < 1e-9, "display elapsed time drifted");
        let dts = heartbeat_dts.borrow().clone();
        let times: Vec<f64> = beat_times.borrow().iter().map(|t| t - from).collect();
        if let Some((previous_dts, previous_times)) = &previous {
            for index in 0..dts.len() {
                assert_eq!(dts[index], previous_dts[index], "display cadence changed the measured Heartbeat pattern");
                assert!((times[index] - previous_times[index]).abs() < 1e-9);
            }
        }
        previous = Some((dts, times));
    }

    // Repeated short/long display steps interleaved with fixed steps must not restart a partial beat
    heartbeat_dts.borrow_mut().clear();
    let mixed_start = now(&engine);
    for _ in 0..60 {
        assert_eq!(fractional.advance(&lua, &engine, 0.001, Some("display")).unwrap().0, 1);
        assert_eq!(fractional.advance(&lua, &engine, 1.0 / 60.0, None).unwrap().0, 1);
        assert_eq!(fractional.advance(&lua, &engine, 1.0 / 60.0 - 0.001, Some("display")).unwrap().0, 1);
    }
    assert!((now(&engine) - mixed_start - 2.0).abs() < 1e-9);
    assert_eq!(heartbeat_dts.borrow().len(), 480, "mixed cadence duplicated or skipped a pending Heartbeat");
    let previous_dts = previous.unwrap().0;
    for (index, dt) in heartbeat_dts.borrow().iter().enumerate() {
        assert_eq!(*dt, previous_dts[index % 240], "mixed cadence reset the beat cursor");
    }

    // A fixed-step remainder is consumed once when returning to actual-duration display stepping
    let mut remainder = Stepper::new(&engine);
    let before = now(&engine);
    assert_eq!(remainder.advance(&lua, &engine, 0.001, None).unwrap().0, 0);
    assert_eq!(remainder.advance(&lua, &engine, 0.002, Some("display")).unwrap().0, 1);
    assert!((now(&engine) - before - 0.003).abs() < 1e-9);
    assert_eq!(remainder.advance(&lua, &engine, 0.0, Some("display")).unwrap().0, 0);

    // Timers keep their order at their Heartbeat deadlines; cancelled ones never resume
    run(
        &lua,
        &engine,
        r#"
        _G.Order, _G.Times = {}, {}
        _G.Due = os.clock() - 10000 + 0.01
        task.delay(0.01, function()
            table.insert(_G.Order, "first")
            table.insert(_G.Times, os.clock() - 10000)
            task.delay(0, function()
                table.insert(_G.Order, "nested")
                table.insert(_G.Times, os.clock() - 10000)
            end)
        end)
        task.delay(0.01, function()
            table.insert(_G.Order, "second")
            table.insert(_G.Times, os.clock() - 10000)
        end)
        local Cancelled = task.delay(0.01, function() error("cancelled timer resumed") end)
        task.cancel(Cancelled)
        "#,
    );
    for _ in 0..6 {
        remainder.advance(&lua, &engine, 1.0 / 144.0, Some("display")).unwrap();
    }
    let order: Vec<String> = match global(&engine, "Order") {
        LuaValue::Table(t) => t.sequence_values::<String>().map(Result::unwrap).collect(),
        other => panic!("{other:?}"),
    };
    assert_eq!(order.join(","), "first,second,nested");
    let times: Vec<f64> = match global(&engine, "Times") {
        LuaValue::Table(t) => t.sequence_values::<f64>().map(Result::unwrap).collect(),
        other => panic!("{other:?}"),
    };
    let due = number(&engine, "Due");
    assert!(times[0] >= due && times[0] < due + 0.005);
    // (the second reads on by the script work charged in between: ree's work clock)
    assert!(times[1] >= times[0] && times[1] - times[0] < 1e-4 && times[2] > times[1] + 1e-4, "{times:?}");
    assert!(!engine.state.borrow().output.entries.iter().any(|e| e.kind == "error"), "{:?}", engine.state.borrow().output.entries);

    // Display hints must not alter recorded Studio render durations or its fixed-step remainder
    scheduler::use_recorded_clock(&engine, Some(&json!({ "events": [["h", 0.003], ["r", 0.01], ["h", 0.007], ["r", 0.02]] }))).unwrap();
    render_dts.borrow_mut().clear();
    heartbeat_dts.borrow_mut().clear();
    let mut replay = Stepper::new(&engine);
    let before = now(&engine);
    assert!(replay.advance(&lua, &engine, 1.0 / 120.0, Some("display")).unwrap().0 == 0 && now(&engine) == before);
    assert_eq!(replay.advance(&lua, &engine, 1.0 / 120.0, Some("display")).unwrap().0, 1);
    assert!(render_dts.borrow()[0] == f32r(0.01) && heartbeat_dts.borrow()[0] == f32r(0.003));
    assert_eq!(replay.advance(&lua, &engine, 1.0 / 60.0, Some("display")).unwrap().0, 1);
    assert!(render_dts.borrow()[1] == f32r(0.02) && heartbeat_dts.borrow()[1] == f32r(0.007));
    scheduler::use_recorded_clock(&engine, None).unwrap();
    heartbeat_dts.borrow_mut().clear();
    assert!(
        replay.advance(&lua, &engine, 0.001, Some("display")).unwrap().0 == 1 && heartbeat_dts.borrow().is_empty(),
        "recorded clock left a stale display beat"
    );
}

/// The in-process session API (the native app's path): every reply applies to a SceneStore
/// (changed nodes as patches), and the store then holds exactly the session's full export.
#[test]
fn in_process_session_patches_rebuild_the_scene() {
    use super::story;
    let fixture = std::env::temp_dir().join(format!("rur-session-test-{}.luau", std::process::id()));
    std::fs::write(
        &fixture,
        r#"
        return function(Target)
            local Frame = Instance.new("Frame")
            Frame.Size = UDim2.fromOffset(100, 50)
            Frame.Parent = Target
            local Label = Instance.new("TextLabel")
            Label.Text = "0"
            Label.Parent = Frame
            local Count = 0
            game:GetService("RunService").Heartbeat:Connect(function()
                Count += 1
                if Count % 8 == 0 then
                    Label.Text = tostring(Count)
                    Frame.BackgroundTransparency = (Count % 16) / 16
                end
                if Count == 40 then
                    Label:Destroy()
                end
            end)
            game:GetService("TweenService"):Create(Frame, TweenInfo.new(0.5), { BackgroundColor3 = Color3.new(1, 0, 0) }):Play()
        end
        "#,
    )
    .unwrap();
    let engine = crate::engine::new_engine(crate::engine::options()).expect("engine");
    let lua = Lua::new();
    let project = story::mount_project(&lua, &engine, &story::ProjectSpec::default(), std::time::Instant::now()).unwrap();
    let source = story::Source { kind: story::SourceKind::Fixture, path: fixture.to_string_lossy().into_owned() };
    let mounted = story::mount(&lua, &engine, &project, Some(&source), 0.0, (800.0, 600.0), None).unwrap();
    let mut session = super::session::Session::new(&engine, project, mounted, [0.2, 0.2, 0.2], true);
    let mut store = crate::scene::SceneStore::default();
    let mut ready = session.ready(&lua).unwrap();
    store.apply_owned(&mut ready).unwrap();
    let mut patches = 0;
    for frame in 0..40 {
        let mut reply = session.frame(&lua, 1.0 / 60.0, if frame % 3 == 0 { Some("display") } else { None }, &[]);
        assert_eq!(reply["type"], "frame", "{reply}");
        // The retained export sends only the nodes it rebuilt (whole nodes, marked for SceneStore)
        if let Some(nodes) = reply.pointer("/delta/nodes").and_then(|n| n.as_array()) {
            assert_eq!(reply["delta"]["retained"], true, "{reply}");
            if nodes.len() < reply["counters"]["nodes"].as_u64().unwrap_or(0) as usize {
                patches += 1;
            }
        }
        store.apply_owned(&mut reply).unwrap();
        let mut full = session.scene(&lua);
        // (frame replies carry the session cache's node shape: no `order`, the draw lists hold it)
        if let Some(nodes) = full.pointer_mut("/scene/nodes").and_then(|n| n.as_array_mut()) {
            for node in nodes {
                if let Some(map) = node.as_object_mut() {
                    map.remove("order");
                }
            }
        }
        let held = store.to_value();
        assert_eq!(held["nodes"], full["scene"]["nodes"], "frame {frame}");
        assert_eq!(held["layers"], full["scene"]["layers"], "frame {frame}");
        for key in ["viewport", "background", "status"] {
            assert_eq!(held[key], full["scene"][key], "frame {frame} {key}");
        }
    }
    assert!(patches > 0, "unchanged nodes are not sent again");
    drop(session);
    drop(lua);
    let _ = std::fs::remove_file(fixture);
}

/// A session whose story rebuilds part of its UI: destroyed panels leave nothing behind (instances,
/// layout records, export entries); panels that are removed and not destroyed stay while their
/// handlers capture them (as in Roblox) but keep no export entries; and a panel that comes back is
/// exported again exactly (the store equals the session's full scene throughout).
#[test]
fn a_session_frees_what_a_story_destroys() {
    use super::story;
    let fixture = std::env::temp_dir().join(format!("rur-rebuild-test-{}.luau", std::process::id()));
    std::fs::write(
        &fixture,
        r#"
        return function(Target)
            local function Build(Name)
                local Panel = Instance.new("Frame")
                Panel.Name = Name
                Panel.Size = UDim2.fromOffset(300, 240)
                local State = { Panel = Panel }
                for Index = 1, 30 do
                    local Button = Instance.new("TextButton")
                    Button.Name = "Button" .. Index
                    Button.Size = UDim2.fromOffset(120, 20)
                    Button.Position = UDim2.fromOffset(Index * 4, Index * 7)
                    Button.Text = Name .. " " .. Index
                    Instance.new("UICorner").Parent = Button
                    Button.Parent = Panel
                    Button.Activated:Connect(function()
                        State.Panel.Visible = false
                    end)
                end
                return Panel
            end
            local Count = 0
            local Current = Build("Panel0")
            Current.Parent = Target
            -- Replaces the panel; the old one is destroyed, or only removed
            function _G.Flip(Destroy)
                Count += 1
                local Old = Current
                Current = Build("Panel" .. Count)
                Current.Parent = Target
                if Destroy then
                    Old:Destroy()
                else
                    Old.Parent = nil
                end
            end
            -- Two panels kept by the story and shown in turn (a cached `show`)
            local Kept
            function _G.Swap()
                Kept = Kept or { Build("KeptA"), Build("KeptB") }
                Current.Parent = nil
                Current = if Current == Kept[1] then Kept[2] else Kept[1]
                Current.Parent = Target
            end
        end
        "#,
    )
    .unwrap();
    let engine = crate::engine::new_engine(crate::engine::options()).expect("engine");
    let lua = Lua::new();
    let project = story::mount_project(&lua, &engine, &story::ProjectSpec::default(), std::time::Instant::now()).unwrap();
    let source = story::Source { kind: story::SourceKind::Fixture, path: fixture.to_string_lossy().into_owned() };
    let mounted = story::mount(&lua, &engine, &project, Some(&source), 0.0, (800.0, 600.0), None).unwrap();
    let mut session = super::session::Session::new(&engine, project, mounted, [0.2, 0.2, 0.2], true);
    let mut store = crate::scene::SceneStore::default();
    let mut ready = session.ready(&lua).unwrap();
    store.apply_owned(&mut ready).unwrap();
    // One step: the script, three frames applied to the store (which must equal the full scene),
    // then a collection; returns (instances, layout records, export entries)
    let mut step = |script: &str| {
        run(&lua, &engine, script);
        for _ in 0..3 {
            let mut reply = session.frame(&lua, 1.0 / 60.0, None, &[]);
            assert_eq!(reply["type"], "frame", "{reply}");
            store.apply_owned(&mut reply).unwrap();
        }
        let mut full = session.scene(&lua);
        if let Some(nodes) = full.pointer_mut("/scene/nodes").and_then(|n| n.as_array_mut()) {
            for node in nodes {
                if let Some(map) = node.as_object_mut() {
                    map.remove("order");
                }
            }
        }
        let held = store.to_value();
        assert_eq!(held["nodes"], full["scene"]["nodes"], "after {script}");
        assert_eq!(held["layers"], full["scene"]["layers"], "after {script}");
        scheduler::flush_deferred(&lua, &engine).unwrap();
        for _ in 0..3 {
            lua.gc_collect().unwrap();
            engine_lua::raw::collect(&lua, &engine).unwrap();
        }
        // (the collection's freed instances reach the export cache at its next build)
        session.frame(&lua, 1.0 / 60.0, None, &[]);
        let errors: Vec<String> = engine.state.borrow().output.entries.iter().filter(|e| e.kind == "error").map(|e| e.message.clone()).collect();
        assert!(errors.is_empty(), "{errors:?}");
        let layout = engine.layout().borrow().as_ref().unwrap().counts().0;
        (engine.state.borrow().dom.live_count(), layout, session.export_entries())
    };
    step("_G.Flip(true)");
    let baseline = step("_G.Flip(true)");
    for _ in 0..12 {
        assert_eq!(step("_G.Flip(true)"), baseline, "a destroyed panel left something behind");
    }
    // Removed, not destroyed: 1 + 30 * 2 instances stay per flip, with their layout records and no
    // export entries
    for round in 1..=5 {
        let (instances, layout, entries) = step("_G.Flip(false)");
        assert_eq!(instances, baseline.0 + round * 61, "round {round}");
        assert_eq!(layout, baseline.1 + round * 31, "round {round}");
        assert_eq!(entries, baseline.2, "round {round}: removed panels keep export entries");
    }
    // Panels shown in turn: the hidden one keeps no entries, and comes back as it was
    let first = step("_G.Swap()");
    for _ in 0..6 {
        let now = step("_G.Swap()");
        assert_eq!((now.0, now.2), (first.0, first.2), "swapping kept panels");
    }
    drop(step);
    drop(session);
    drop(lua);
    let _ = std::fs::remove_file(fixture);
}
