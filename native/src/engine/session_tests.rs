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

/// A UI Labs generic story (`render`): its props carry the controls (with `--controls` applied) and
/// their converted control objects, and a live control change gives each subscribed listener a new
/// values table with `{ __old, __new }` infos, as the UI Labs plugin does.
#[test]
fn a_generic_story_follows_its_controls() {
    use super::story;
    let fixture = std::env::temp_dir().join(format!("rur-generic-story-test-{}.luau", std::process::id()));
    std::fs::write(
        &fixture,
        r#"
        return {
            controls = {
                Label = "One",
                Group = { EntryType = "ControlGroup", Controls = { Size = 10 } },
            },
            render = function(Props)
                local Label = Instance.new("TextLabel")
                Label.Text = Props.controls.Label
                Label.Size = UDim2.fromOffset(Props.controls.Group.Size, 20)
                Label.Parent = Props.target
                _G.Label, _G.Converted, _G.Calls = Label, Props.converted, 0
                Props.subscribe(function(Values, Infos)
                    _G.Calls += 1
                    _G.Fresh, _G.Infos = Values ~= Props.controls, Infos
                    Label.Text = Values.Label
                    Label.Size = UDim2.fromOffset(Values.Group.Size, 20)
                end)
                local Unsubscribe = Props.subscribe(function()
                    _G.Unsubscribed = true
                end)
                Unsubscribe()
            end,
        }
        "#,
    )
    .unwrap();
    let engine = crate::engine::new_engine(crate::engine::options()).expect("engine");
    let lua = Lua::new();
    let project = story::mount_project(&lua, &engine, &story::ProjectSpec::default(), std::time::Instant::now()).unwrap();
    let source = story::Source { kind: story::SourceKind::Fixture, path: fixture.to_string_lossy().into_owned() };
    let controls = json!({ "Group.Size": 30 });
    let mounted = story::mount(&lua, &engine, &project, Some(&source), 0.0, (800.0, 600.0), Some(&controls)).unwrap();
    assert_eq!(mounted.status, "ok", "{:?}", mounted.failure);
    let mut session = super::session::Session::new(&engine, project, mounted, [0.2, 0.2, 0.2], true);
    let ready = session.ready(&lua).unwrap();
    assert_eq!(ready["controls"].as_array().map(Vec::len), Some(2), "{ready}");
    run(
        &lua,
        &engine,
        r#"
        local Converted = _G.Converted
        assert(Converted.Label.EntryType == "Control" and Converted.Label.ControlValue == "One")
        assert(Converted.Group.EntryType == "ControlGroup" and Converted.Group.Controls.Size.ControlValue == 10)
        assert(_G.Label.Size.X.Offset == 30 and _G.Calls == 0)
        "#,
    );
    assert_eq!(session.control(&lua, &json!(["Label"]), &json!("Two"))["type"], "frame");
    run(
        &lua,
        &engine,
        r#"
        assert(_G.Calls == 1 and _G.Fresh and not _G.Unsubscribed)
        assert(_G.Label.Text == "Two")
        assert(_G.Infos.Label.__old == "One" and _G.Infos.Label.__new == "Two")
        assert(_G.Infos.Group.Size.__old == 30 and _G.Infos.Group.Size.__new == 30)
        "#,
    );
    assert_eq!(session.control(&lua, &json!(["Group", "Size"]), &json!(40))["type"], "frame");
    run(
        &lua,
        &engine,
        r#"
        assert(_G.Calls == 2 and _G.Label.Size.X.Offset == 40)
        assert(_G.Infos.Group.Size.__old == 30 and _G.Infos.Group.Size.__new == 40)
        assert(_G.Infos.Label.__old == "Two" and _G.Infos.Label.__new == "Two")
        "#,
    );
    for path in [json!(["Missing"]), json!(["Group"])] {
        assert_eq!(session.control(&lua, &path, &json!(1))["type"], "error", "{path}");
    }
    drop(session);
    drop(lua);
    let _ = std::fs::remove_file(fixture);
}

/// A generic story's props.converted and listener calls follow the UI Labs plugin: a literal becomes
/// the control its constructor makes, a table (a control, a `{ value, ... }` list) is passed as it is,
/// a group is copied with its controls converted, and a value UI Labs makes no control of is left out
/// with a warning; a listener that errors or yields is reported with a warning while the listeners
/// after it still get the change; a control whose value is a table can be set.
#[test]
fn a_generic_story_converts_and_calls_listeners_as_ui_labs_does() {
    use super::story;
    let fixture = std::env::temp_dir().join(format!("rur-generic-story-plugin-test-{}.luau", std::process::id()));
    std::fs::write(
        &fixture,
        r#"
        local Options = { { Name = "x" }, { Name = "y" } }
        local Controls = {
            Text = "Hi",
            Count = 4,
            On = true,
            Tint = Color3.new(2, 0.5, -1),
            Align = Enum.TextXAlignment.Left,
            List = { "a", "b" },
            Pick = { EntryType = "Control", Type = "Choose", ControlValue = Options[1], List = Options, DefIndex = 1 },
            Group = { EntryType = "ControlGroup", Order = 3, Controls = { Size = 10 } },
        }
        _G.Specs = Controls
        return {
            controls = Controls,
            render = function(Props)
                _G.Props, _G.Calls = Props, {}
                Props.subscribe(function()
                    error("listener failed")
                end)
                Props.subscribe(function()
                    task.wait()
                    _G.Resumed = true
                end)
                Props.subscribe(function(Values, Infos)
                    table.insert(_G.Calls, { Values = Values, Infos = Infos })
                end)
            end,
        }
        "#,
    )
    .unwrap();
    let engine = crate::engine::new_engine(crate::engine::options()).expect("engine");
    let lua = Lua::new();
    let project = story::mount_project(&lua, &engine, &story::ProjectSpec::default(), std::time::Instant::now()).unwrap();
    let source = story::Source { kind: story::SourceKind::Fixture, path: fixture.to_string_lossy().into_owned() };
    let mounted = story::mount(&lua, &engine, &project, Some(&source), 0.0, (800.0, 600.0), None).unwrap();
    assert_eq!(mounted.status, "ok", "{:?}", mounted.failure);
    let mut session = super::session::Session::new(&engine, project, mounted, [0.2, 0.2, 0.2], true);
    session.ready(&lua).unwrap();
    let warnings = || -> Vec<String> {
        engine.state.borrow().output.entries.iter().filter(|entry| entry.kind == "warn").map(|entry| entry.message.clone()).collect()
    };
    run(
        &lua,
        &engine,
        r#"
        local Converted, Specs = _G.Props.converted, _G.Specs
        local Text, Count, On, Tint = Converted.Text, Converted.Count, Converted.On, Converted.Tint
        assert(Text.EntryType == "Control" and Text.Type == "String" and Text.ControlValue == "Hi")
        assert(Count.Type == "Number" and Count.ControlValue == 4 and Count.Dragger == true and Count.Sensibility == 40)
        assert(On.Type == "Boolean" and On.ControlValue == true)
        assert(Tint.Type == "Color3" and Tint.ControlValue == Color3.new(1, 0.5, 0))
        assert(Converted.Align == nil and _G.Props.controls.Align == Enum.TextXAlignment.Left)
        assert(Converted.List == Specs.List and Converted.Pick == Specs.Pick)
        assert(Converted.Group ~= Specs.Group and Converted.Group.Order == 3 and Converted.Group.Controls.Size.Type == "Number")
        "#,
    );
    assert!(warnings().iter().any(|warning| warning.contains("control Align") && warning.contains("EnumItem")), "{:?}", warnings());

    assert_eq!(session.control(&lua, &json!(["Count"]), &json!(5))["type"], "frame");
    run(
        &lua,
        &engine,
        r#"
        assert(#_G.Calls == 1 and _G.Calls[1].Values.Count == 5 and not _G.Resumed)
        local Infos = _G.Calls[1].Infos
        assert(Infos.Count.__old == 4 and Infos.Count.__new == 5 and Infos.Group.Size.__new == 10)
        assert(Infos.Align == nil)
        "#,
    );
    let seen = warnings();
    assert!(seen.iter().any(|warning| warning.contains("errored") && warning.contains("listener failed")), "{seen:?}");
    assert!(seen.iter().any(|warning| warning.contains("yielded")), "{seen:?}");
    session.frame(&lua, 1.0 / 60.0, None, &[]);
    run(&lua, &engine, "assert(_G.Resumed)");

    assert_eq!(session.control(&lua, &json!(["Pick"]), &json!(2))["type"], "frame");
    assert_eq!(session.control(&lua, &json!(["List"]), &json!("b"))["type"], "frame");
    run(
        &lua,
        &engine,
        r#"
        assert(#_G.Calls == 3)
        assert(_G.Calls[2].Values.Pick.Name == "y" and _G.Calls[2].Infos.Pick.__old.Name == "x")
        assert(_G.Calls[3].Values.List == "b" and _G.Calls[3].Infos.List.__old == "a")
        "#,
    );
    assert_eq!(session.control(&lua, &json!(["Pick", "Name"]), &json!("z"))["type"], "error");
    drop(session);
    drop(lua);
    let _ = std::fs::remove_file(fixture);
}

/// A stand-in for React and ReactRoblox (createElement, createRoot, function components rendered synchronously) and
/// for Fusion 0.2 (Value objects), enough to see which renderer the host picks and what it passes
const FAKE_PACKAGES: &str = r#"
local Fake = {}
function Fake.createElement(Type, Props)
    return { ["$$typeof"] = "element", type = Type, props = Props or {} }
end
function Fake.createRoot(Container)
    local Root = {}
    function Root:render(Element)
        _G.Renders = (_G.Renders or 0) + 1
        Container:ClearAllChildren()
        local function Build(Element)
            if type(Element.type) == "function" then
                return Build(Element.type(Element.props))
            end
            local Made = Instance.new(Element.type)
            for Key, Value in Element.props do
                Made[Key] = Value
            end
            Made.Parent = Container
            return Made
        end
        _G.Built = Build(Element)
    end
    return Root
end
function Fake.Value(Initial)
    local Value = { Current = Initial }
    function Value:get()
        return self.Current
    end
    function Value:set(New)
        self.Current = New
        if self.Changed then
            self.Changed(New)
        end
    end
    return Value
end
return Fake
"#;

/// A Flipbook project laid out as a root: the stand-in packages, a storybook giving its stories React and ReactRoblox,
/// and the stories under its storyRoots
fn flipbook_root(name: &str, stories: &[(&str, &str)]) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("rur-flipbook-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let storage = dir.join("ReplicatedStorage");
    std::fs::create_dir_all(storage.join("Stories")).unwrap();
    std::fs::write(storage.join("Fake.luau"), FAKE_PACKAGES).unwrap();
    std::fs::write(
        storage.join("Stories.storybook.luau"),
        r#"local Fake = require(script.Parent.Fake)
        return { name = "Test", storyRoots = { script.Parent.Stories }, packages = { React = Fake, ReactRoblox = Fake } }"#,
    )
    .unwrap();
    for (file, source) in stories {
        std::fs::write(storage.join("Stories").join(file), source).unwrap();
    }
    dir
}

fn mount_flipbook(dir: &std::path::Path, file: &str, controls: Option<&serde_json::Value>) -> (Lua, Rc<Engine>, super::session::Session) {
    use super::story;
    let engine = crate::engine::new_engine(crate::engine::options()).expect("engine");
    let lua = Lua::new();
    let spec = story::ProjectSpec { roots: vec![dir.to_string_lossy().into_owned()], ..Default::default() };
    let project = story::mount_project(&lua, &engine, &spec, std::time::Instant::now()).unwrap();
    let path = dir.join("ReplicatedStorage").join("Stories").join(file);
    let source = story::Source { kind: story::SourceKind::Story, path: path.to_string_lossy().into_owned() };
    let mounted = story::mount(&lua, &engine, &project, Some(&source), 0.0, (800.0, 600.0), controls).unwrap();
    assert_eq!(mounted.status, "ok", "{file}: {:?}", mounted.failure);
    let session = super::session::Session::new(&engine, project, mounted, [0.2, 0.2, 0.2], true);
    (lua, engine, session)
}

/// A Flipbook story whose storybook names React: its function is rendered as a component through createRoot with
/// Flipbook's props, typed and raw controls (with `--controls`) reach it, the storybook lists them, and a live change
/// renders it again
#[test]
fn a_flipbook_story_renders_with_its_storybooks_react_and_controls() {
    let dir = flipbook_root(
        "react",
        &[(
            "Counter.story.luau",
            r#"local Fake = require(game:GetService("ReplicatedStorage").Fake)
            return {
                controls = { label = "Hi", size = { type = "Select", items = { "Small", "Large" }, default = "Large" } },
                story = function(props)
                    _G.Props = props
                    return Fake.createElement("TextLabel", {
                        Text = props.controls.label .. " " .. props.controls.size,
                        Size = UDim2.fromOffset(if props.controls.size == "Large" then 300 else 100, 50),
                    })
                end,
            }"#,
        )],
    );
    let (lua, engine, mut session) = mount_flipbook(&dir, "Counter.story.luau", Some(&json!({ "label": "Yo" })));
    let ready = session.ready(&lua).unwrap();
    let controls = ready["controls"].as_array().cloned().unwrap_or_default();
    let size = controls.iter().find(|control| control["path"] == json!(["size"])).expect("size control");
    assert_eq!((size["kind"].as_str(), size["value"].as_str(), size["options"].clone()), (Some("choose"), Some("Large"), json!(["Small", "Large"])), "{ready}");
    run(
        &lua,
        &engine,
        r#"
        assert(_G.Built.Text == "Yo Large" and _G.Built.Size.X.Offset == 300, _G.Built.Text)
        assert(_G.Props.theme == "Dark" and _G.Props.locale == "en-us" and _G.Props.container == _G.Props.target)
        assert(_G.Props.container.Name == "StoryTarget" and type(_G.Props.story) == "table" and _G.Renders == 1)
        "#,
    );
    assert_eq!(session.control(&lua, &json!(["size"]), &json!("Small"))["type"], "frame");
    run(&lua, &engine, r#"assert(_G.Built.Text == "Yo Small" and _G.Built.Size.X.Offset == 100 and _G.Renders == 2, _G.Built.Text)"#);
    assert_eq!(session.control(&lua, &json!(["missing"]), &json!(1))["type"], "error");
    drop(session);
    drop(lua);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The other story shapes Flipbook takes: an element as the story, a story with its own packages (Fusion: a Value per
/// control, set on a change), one with none (`packages = {}`: story(container, props) for a two-argument function)
/// and a Hoarcekat function
#[test]
fn flipbook_elements_fusion_manual_and_hoarcekat_stories_mount() {
    let dir = flipbook_root(
        "shapes",
        &[
            (
                "Element.story.luau",
                r#"local Fake = require(game:GetService("ReplicatedStorage").Fake)
                return { story = Fake.createElement("Frame", { Name = "FromElement", Size = UDim2.fromOffset(40, 40) }) }"#,
            ),
            (
                "Fusion.story.luau",
                r#"local Fake = require(game:GetService("ReplicatedStorage").Fake)
                return {
                    packages = { Fusion = Fake },
                    controls = { title = "One" },
                    story = function(props)
                        local Label = Instance.new("TextLabel")
                        Label.Name = "FromFusion"
                        Label.Text = props.controls.title:get()
                        props.controls.title.Changed = function(New) Label.Text = New end
                        _G.Fusion = Label
                        return Label
                    end,
                }"#,
            ),
            (
                "Manual.story.luau",
                r#"return {
                    packages = {},
                    controls = { width = 70 },
                    story = function(container, props)
                        local Frame = Instance.new("Frame")
                        Frame.Name = "FromManual"
                        Frame.Size = UDim2.fromOffset(props.controls.width, 10)
                        Frame.Parent = container
                        _G.Mounts = (_G.Mounts or 0) + 1
                        return function() _G.Cleanups = (_G.Cleanups or 0) + 1 end
                    end,
                }"#,
            ),
            (
                "Hoarcekat.story.luau",
                r#"return function(target)
                    local Frame = Instance.new("Frame")
                    Frame.Name = "FromHoarcekat"
                    Frame.Parent = target
                    return function() end
                end"#,
            ),
        ],
    );
    let (lua, engine, session) = mount_flipbook(&dir, "Element.story.luau", None);
    run(&lua, &engine, r#"assert(_G.Built.Name == "FromElement" and _G.Built.Parent.Name == "StoryTarget")"#);
    drop((session, lua, engine));
    let (lua, engine, mut session) = mount_flipbook(&dir, "Fusion.story.luau", None);
    run(&lua, &engine, r#"assert(_G.Fusion.Text == "One" and _G.Fusion.Parent.Name == "StoryTarget" and _G.Renders == nil)"#);
    let _ = session.ready(&lua).unwrap();
    assert_eq!(session.control(&lua, &json!(["title"]), &json!("Two"))["type"], "frame");
    run(&lua, &engine, r#"assert(_G.Fusion.Text == "Two" and _G.Fusion.Parent ~= nil)"#);
    drop((session, lua, engine));
    let (lua, engine, mut session) = mount_flipbook(&dir, "Manual.story.luau", Some(&json!({ "width": 90 })));
    let target = r#"game:GetService("CoreGui").RURStory.StoryTarget"#;
    run(&lua, &engine, &format!("assert({target}.FromManual.Size.X.Offset == 90 and _G.Mounts == 1 and _G.Cleanups == nil)"));
    let _ = session.ready(&lua).unwrap();
    assert_eq!(session.control(&lua, &json!(["width"]), &json!(120))["type"], "frame");
    run(&lua, &engine, &format!("assert(_G.Mounts == 2 and _G.Cleanups == 1 and {target}.FromManual.Size.X.Offset == 120 and #{target}:GetChildren() == 1)"));
    drop((session, lua, engine));
    let (lua, engine, session) = mount_flipbook(&dir, "Hoarcekat.story.luau", None);
    run(&lua, &engine, &format!("assert({target}:FindFirstChild(\"FromHoarcekat\") ~= nil)"));
    drop((session, lua, engine));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Members a game script can't call raise Roblox's own error, which Roblox's packages catch (Foundation's resolution
/// scale, SafeFlags): the emulator raises it, as rur's own list of them did before
#[test]
fn members_a_game_script_cannot_call_raise_robloxs_error() {
    let (lua, engine) = start();
    run(
        &lua,
        &engine,
        r#"
        local GuiService = game:GetService("GuiService")
        local Ok, Message = pcall(GuiService.GetResolutionScale, GuiService)
        assert(not Ok and Message == "The current thread cannot call 'GetResolutionScale' (lacking capability RobloxScript)", tostring(Message))
        -- SafeFlags (Roblox's packages): a define that fails this way means every flag but Debug ones is on
        local Defined, Problem = pcall(function() game:DefineFastFlag("FoundationThing", false) end)
        assert(not Defined and string.match(Problem, "The current thread cannot call"), tostring(Problem))
        local Read, Error = pcall(function() return game:GetFastInt("Thing") end)
        assert(not Read and Error == "The current thread cannot call 'GetFastInt' (lacking capability LocalUser)", tostring(Error))
        assert(GuiService:IsTenFootInterface() == false)
        "#,
    );
}
