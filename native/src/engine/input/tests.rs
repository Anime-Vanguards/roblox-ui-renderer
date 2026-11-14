//! Input tests: ports of runtime/tests/hit.luau, scroll_physics.luau (Studio's measured wheel
//! traces), presentation.luau and native_clipboard.luau, plus TextBox and drag checks of the
//! measured rules in runtime/input/textbox.luau and drag.luau. The scripts run in the Rust engine
//! with test hooks as globals (the Luau tests' direct module calls):
//!   Dispatch(event)  Input.Dispatch        Flush()  Scheduler.FlushDeferred
//!   UpdateAll()      Layout.UpdateAll      Step(dt) Scrolling.Step
//!   StepDisplay(dt)  Scheduler.StepDisplayDuration
//!   Order()          Hit.DrawOrder names   Contains(index, x, y)  Hit.Contains
//!   HitAt(x, y)      Hit.At                Interactable(object)   Hit.Interactable
//!   Enable(on)       Input.EnablePresentation   Apply(packet, phase)  Input.ApplyPresentation
//!   Snapshot()       Input.PresentationSnapshot  Route(list, notches, taker?)  Scrolling.Route
//!   Stop(object)     Scrolling.Stop        Serial(object)  the creation id

use std::rc::Rc;

use mlua::prelude::*;
use serde_json::{Value as Json, json};

use super::{Cx, hit, scrolling};
use crate::engine::{
    Engine, InstanceId, Options,
    lua::{self as engine_lua, globals::CallOutcome, instance_value},
    scheduler,
    session::Session,
};

fn start(width: f64, height: f64) -> (Lua, Rc<Engine>) {
    let engine = crate::engine::new_engine(Options { viewport_width: width, viewport_height: height, ..crate::engine::options() }).expect("engine");
    let lua = Lua::new();
    engine_lua::start(&lua, &engine).expect("start");
    engine.state.borrow_mut().sched.set_main_thread(&lua.current_thread());
    (lua, engine)
}

fn id_of(value: &LuaValue) -> LuaResult<InstanceId> {
    engine_lua::instance_id(value).ok_or_else(|| LuaError::runtime("instance expected"))
}

fn json_of(lua: &Lua, value: LuaValue) -> LuaResult<Json> {
    lua.from_value(value)
}

fn to_lua(lua: &Lua, value: &Json) -> LuaResult<LuaValue> {
    let options = LuaSerializeOptions::new().serialize_none_to_null(false).serialize_unit_to_null(false).set_array_metatable(false);
    lua.to_value_with(value, options)
}

/// The test hooks as globals of a script environment.
fn hooks(lua: &Lua, engine: &Rc<Engine>, env: &LuaTable) -> LuaResult<()> {
    let input = super::get(engine).unwrap();
    macro_rules! hook {
        ($e:ident, $i:ident, $name:expr, |$lua:ident, $args:tt: $ty:ty| $body:expr) => {{
            let $e = $e.clone();
            let $i = $i.clone();
            let f = lua.create_function(move |$lua, $args: $ty| {
                let ($e, $i) = (&$e, &$i);
                let _ = ($e, $i);
                $body
            })?;
            env.raw_set($name, f)?;
        }};
    }
    hook!(engine, input, "Dispatch", |lua, event: LuaValue| input.dispatch(lua, engine, &json_of(lua, event)?));
    hook!(engine, input, "Flush", |lua, _a: ()| scheduler::flush_deferred(lua, engine));
    hook!(engine, input, "UpdateAll", |_lua, _a: ()| {
        crate::engine::layout::update_all(engine);
        Ok(())
    });
    hook!(engine, input, "Step", |lua, dt: f64| scrolling::step(&Cx { lua, engine, i: input }, dt));
    hook!(engine, input, "StepDisplay", |lua, dt: f64| scheduler::step_display_duration(lua, engine, dt).map(|_| ()));
    hook!(engine, input, "Order", |lua, _a: ()| {
        let order = input.draw_order(engine);
        let names = lua.create_table()?;
        for entry in order.iter() {
            names.raw_push(engine.state.borrow().dom.at(entry.id).name.to_string())?;
        }
        Ok(names)
    });
    hook!(engine, input, "Contains", |_lua, (index, x, y): (usize, f64, f64)| {
        let order = input.draw_order(engine);
        Ok(hit::contains(&order[index - 1], x, y))
    });
    hook!(engine, input, "HitAt", |lua, (x, y): (f64, f64)| {
        let list = lua.create_table()?;
        for id in input.hit_at(engine, x, y) {
            list.raw_push(instance_value(lua, engine, id)?)?;
        }
        Ok(list)
    });
    hook!(engine, input, "Interactable", |_lua, object: LuaValue| Ok(input.interactable(&engine.state.borrow(), id_of(&object)?)));
    hook!(engine, input, "Enable", |_lua, on: bool| {
        input.enable_presentation(engine, on);
        Ok(())
    });
    hook!(engine, input, "Apply", |lua, (packet, phase): (LuaValue, Option<String>)| {
        input.apply_presentation(lua, engine, &json_of(lua, packet)?, phase.as_deref().unwrap_or(""))
    });
    hook!(engine, input, "Snapshot", |lua, _a: ()| match input.presentation_snapshot(lua, engine) {
        Some(snapshot) => to_lua(lua, &snapshot),
        None => Ok(LuaValue::Nil),
    });
    hook!(engine, input, "Route", |lua, (list, notches, taker): (LuaTable, f64, Option<f64>)| {
        let mut ids = Vec::new();
        for value in list.sequence_values::<LuaValue>() {
            ids.push(id_of(&value?)?);
        }
        let taker = taker.map(|t| json!(t));
        let (taken, _) = scrolling::route(&Cx { lua, engine, i: input }, &ids, notches, taker.as_ref());
        match taken {
            Some(id) => instance_value(lua, engine, id),
            None => Ok(LuaValue::Nil),
        }
    });
    hook!(engine, input, "Stop", |lua, object: LuaValue| {
        scrolling::stop(&Cx { lua, engine, i: input }, id_of(&object)?);
        Ok(())
    });
    hook!(engine, input, "Serial", |_lua, object: LuaValue| Ok(engine.state.borrow().dom.at(id_of(&object)?).serial));
    Ok(())
}

/// Runs `source` with the test hooks; panics with the script's error.
fn run(lua: &Lua, engine: &Rc<Engine>, source: &str) {
    let script = {
        let mut state = engine.state.borrow_mut();
        let class = engine.reflection.class_id("ModuleScript").unwrap();
        crate::engine::instance::create_named(&mut state, &engine.reflection, class, "Test")
    };
    let env = engine_lua::modules::environment(lua, engine, script).expect("environment");
    hooks(lua, engine, &env).expect("hooks");
    let function = lua.load(source).set_name("=Test").set_environment(env).into_function().expect("compile");
    let call = engine_lua::globals::call(lua, engine, function, LuaMultiValue::new()).expect("call");
    if let CallOutcome::Failed(message, _) = call.outcome().unwrap() {
        panic!("test script failed: {message}");
    }
}

/// runtime/tests/hit.luau (without the profile counters of the Luau same-point cache, and without
/// toggling rotation clipping, an engine option here)
#[test]
fn hit_order_geometry_and_interactability() {
    let (lua, engine) = start(800.0, 600.0);
    run(
        &lua,
        &engine,
        r#"
local Game = game
local function Make(Class, Name, Parent, Z)
	local Object = Instance.new(Class)
	Object.Name = Name
	if Object:IsA("GuiObject") then
		Object.Size = UDim2.fromOffset(100, 100)
		Object.ZIndex = Z or 1
	end
	Object.Parent = Parent
	return Object
end
local Root = Make("ScreenGui", "Root", Game:GetService("CoreGui"))
Root.ZIndexBehavior = Enum.ZIndexBehavior.Sibling
local A = Make("Frame", "A", Root, 2)
local B = Make("Frame", "B", Root, 1)
local C = Make("Frame", "C", A, 1)
local Folder = Make("Folder", "Folder", Root)
local E = Make("Frame", "E", Folder, 2)
local function Check(Expected)
	UpdateAll()
	local Names = table.concat(Order(), ",")
	assert(Names == Expected, Names .. " ~= " .. Expected)
end
Check("B,A,C,E")
A.Position = UDim2.fromOffset(40, 40)
Check("B,A,C,E")
assert(HitAt(45, 45)[1] == E)
B.ZIndex = 3
Check("A,C,E,B")
C.Parent = B
Check("A,E,B,C")
A.Visible = false
Check("E,B,C")
A.Visible = true
Check("A,E,B,C")
Root.Enabled = false
Check("")
Root.Enabled = true
Root.ZIndexBehavior = Enum.ZIndexBehavior.Global
Check("C,A,E,B")
local Group = Make("CanvasGroup", "Group", Root, 1)
local High = Make("Frame", "High", Group, 10)
Check("C,Group,High,A,E,B")
Root.ZIndexBehavior = Enum.ZIndexBehavior.Sibling
Check("Group,High,A,E,B,C")
Group:Destroy()
Check("A,E,B,C")
B.Interactable = false
assert(not Interactable(C))
B.Interactable = true
B.Active = false
assert(Interactable(C))
B.ClipsDescendants = true
C.Position = UDim2.fromOffset(80, 0)
Check("A,E,B,C")
assert(Contains(4, 100, 50) and not Contains(4, 101, 50))
B.Rotation = 90
Check("A,E,B,C")
assert(Contains(4, 50, 150), "rotation maps point and disables subtree clipping")
E.Parent = B
Check("A,B,C,E")
local Overlay = Make("ScreenGui", "Overlay", Game:GetService("CoreGui"))
Overlay.DisplayOrder = 10
local Top = Make("Frame", "Top", Overlay)
Check("A,B,C,E,Top")
Overlay.DisplayOrder = -10
Check("Top,A,B,C,E")
Overlay:Destroy()
local Scroll = Make("ScrollingFrame", "Scroll", Root, 4)
Scroll.CanvasSize = UDim2.fromOffset(100, 500)
local Cell = Make("Frame", "Cell", Scroll)
Cell.Position = UDim2.fromOffset(0, 150)
Check("A,B,C,E,Scroll,Cell")
assert(not Contains(6, 50, 175))
Scroll.CanvasPosition = Vector2.new(0, 150)
Check("A,B,C,E,Scroll,Cell")
assert(Contains(6, 50, 50))
Cell.Size = UDim2.fromOffset(20, 20)
Check("A,B,C,E,Scroll,Cell")
assert(not Contains(6, 50, 50))
B.Interactable = false
local Blocked = HitAt(50, 50)
assert(not table.find(Blocked, B) and not table.find(Blocked, E))
B.Interactable = true
assert(table.find(HitAt(50, 50), B))
assert(#HitAt(700, 500) == 0, "point changes must miss")
HitAt(50, 50)
B.Position = UDim2.fromOffset(600, 0)
UpdateAll()
assert(not table.find(HitAt(50, 50), B), "geometry changes must invalidate")
for _, Mutate in {
	function() A.Rotation = 45 end,
	function() C.Parent = A end,
	function() A.Visible = false end,
	function() A.Visible = true end,
	function() Root.Enabled = false end,
	function() Root.Enabled = true end,
} do
	local Before = HitAt(50, 50)
	Mutate()
	UpdateAll()
	local After = HitAt(50, 50)
	assert(typeof(Before) == "table" and typeof(After) == "table")
end
-- GetGuiObjectsAtPosition: every GuiObject at the point (also non-interactable), topmost first
B.Position = UDim2.fromOffset(0, 0)
B.Interactable = false
local Names = {}
for _, Object in game:GetService("CoreGui"):GetGuiObjectsAtPosition(50, 50) do
	table.insert(Names, Object.Name)
end
assert(table.find(Names, "B"), table.concat(Names, ","))
assert(B.GuiState == Enum.GuiState.NonInteractable and E.GuiState == Enum.GuiState.NonInteractable)
"#,
    );
}

/// runtime/tests/scroll_physics.luau: Studio's measured wheel traces (float32 spring), frame by frame.
#[test]
fn scroll_physics_matches_studio_traces() {
    let path = crate::engine::default_repo().join("tools/interaction/reference/scroll-physics.json");
    let text = std::fs::read_to_string(path).expect("reference");
    let reference = crate::engine::lune_json::decode(&text).expect("json");
    let (lua, engine) = start(1600.0, 1000.0);
    let cases = to_lua(&lua, &reference["cases"]).unwrap();
    lua.globals().set("Cases", cases).unwrap();
    let cases = lua.globals().get::<LuaValue>("Cases").unwrap();
    let script = {
        let mut state = engine.state.borrow_mut();
        let class = engine.reflection.class_id("ModuleScript").unwrap();
        crate::engine::instance::create_named(&mut state, &engine.reflection, class, "Test")
    };
    let env = engine_lua::modules::environment(&lua, &engine, script).unwrap();
    hooks(&lua, &engine, &env).unwrap();
    env.raw_set("Cases", cases).unwrap();
    let source = r#"
local Samples, Exact = 0, 0
for _, Case in Cases do
	local Root = Instance.new("ScreenGui", game:GetService("CoreGui"))
	Root.IgnoreGuiInset = true
	local Scroll = Instance.new("ScrollingFrame", Root)
	local Scale = Case.scale or 1
	Scroll.Size = UDim2.fromOffset(Case.window[1] / Scale, Case.window[2] / Scale)
	Scroll.CanvasSize = UDim2.fromOffset(Case.canvas[1] / Scale, Case.canvas[2] / Scale)
	Instance.new("UIScale", Scroll).Scale = Scale
	Scroll.BorderSizePixel = 0
	Scroll.ScrollBarThickness = 0
	Scroll.ScrollingDirection = Enum.ScrollingDirection.Y
	if Case.elastic then Scroll.ElasticBehavior = Enum.ElasticBehavior[Case.elastic] end
	if Case.listener then Scroll:GetPropertyChangedSignal("CanvasPosition"):Connect(function() end) end
	UpdateAll()
	assert(Scroll.AbsoluteWindowSize == Vector2.new(Case.window[1], Case.window[2])
		and Scroll.AbsoluteCanvasSize == Vector2.new(Case.canvas[1], Case.canvas[2]), Case.name .. ": scaled geometry differs")
	Scroll.CanvasPosition = Vector2.new(Case.samples[1].x, Case.samples[1].y)
	for Frame, Sample in Case.samples do
		Step(Sample.dt)
		UpdateAll()
		local Position, Velocity = Scroll.CanvasPosition, Scroll:GetScrollVelocity()
		local Expected = Vector2.new(Sample.x, Sample.y)
		assert(Position == Expected,
			string.format("%s frame %d: position %.9f, expected %.9f", Case.name, Frame, Position.Y, Expected.Y))
		assert(Velocity.X == Sample.vx and Velocity.Y == Sample.vy, Case.name .. ": public wheel velocity differs")
		Samples += 1
		if Position == Expected then Exact += 1 end
		for _, Action in Case.actions do
			if Action.frame ~= Frame then continue end
			if Action.wheel then Dispatch({ type = "wheel", x = 100, y = 100, notches = Action.wheel }) end
			if Action.reset then Scroll:ResetScrollVelocity() end
			if Action.position then Scroll.CanvasPosition = Vector2.new(0, Action.position) end
			if Action.samePosition then Scroll.CanvasPosition = Scroll.CanvasPosition end
			if Action.enabled ~= nil then Scroll.ScrollingEnabled = Action.enabled end
			if Action.canvas then Scroll.CanvasSize = UDim2.fromOffset(Case.canvas[1] / Scale, Action.canvas) end
		end
	end
	Root:Destroy()
end
_G.Samples = Samples
"#;
    let function = lua.load(source).set_name("=Test").set_environment(env).into_function().unwrap();
    let call = engine_lua::globals::call(&lua, &engine, function, LuaMultiValue::new()).unwrap();
    if let CallOutcome::Failed(message, _) = call.outcome().unwrap() {
        panic!("scroll physics: {message}");
    }
    let shared = engine.lua.borrow().shared.clone().unwrap();
    let samples: f64 = shared.get::<LuaTable>("_G").unwrap().get("Samples").unwrap();
    assert!(samples > 500.0, "only {samples} Studio frames replayed");
}

/// runtime/tests/presentation.luau
#[test]
fn presentation_ownership_and_handoff() {
    let (lua, engine) = start(800.0, 600.0);
    run(
        &lua,
        &engine,
        r#"
local Root = Instance.new("ScreenGui", game:GetService("CoreGui"))
Root.IgnoreGuiInset = true
local Scroll = Instance.new("ScrollingFrame", Root)
Scroll.Size = UDim2.fromOffset(400,400)
Scroll.CanvasSize = UDim2.fromOffset(400,2000)
local Button = Instance.new("TextButton", Scroll)
Button.Size = UDim2.fromOffset(100,40)
Button.Position = UDim2.fromOffset(20,700)
UpdateAll()
Flush()
assert(Snapshot() == nil)
Enable(true)
assert(Apply({version=1,seq=0,scrolls={}}, "input"))
local function State()
 for _, Row in Snapshot().scrolls do
  if Row.id == Serial(Scroll) then return Row end
 end
 error("missing scroll")
end
local function Packet(Seq, Y, Target, Epoch)
 local Row = State()
 return {version=1,seq=Seq,scrolls={{id=Row.id,epoch=Epoch or Row.epoch,position={0,Y},velocity={0,20},target={0,Target or Y}}}}
end
local Container = Instance.new("Frame", Root)
Container.Size = UDim2.fromOffset(400,400)
Scroll.Parent = Container
UpdateAll()
Scroll.Interactable = false
assert(not State().enabled, "noninteractable scroller cannot accept presentation ownership")
assert(not Apply(Packet(0,20),"input") and Scroll.CanvasPosition.Y==0)
Scroll.Interactable = true
Container.Interactable = false
assert(not State().enabled, "noninteractable ancestry cannot accept presentation ownership")
assert(not Apply(Packet(0,20),"input") and Scroll.CanvasPosition.Y==0)
Container.Interactable = true
local Initial = State()
assert(Initial.enabled, "simple scroller must support presentation")
local Changes = 0
Scroll:GetPropertyChangedSignal("CanvasPosition"):Connect(function()
 Changes += 1
 Button.Text = tostring(Scroll.CanvasPosition.Y)
 Button.Position = UDim2.fromOffset(20, Scroll.CanvasPosition.Y+20)
end)
assert(Apply(Packet(1,500,600), "input"))
assert(Scroll.CanvasPosition.Y==500 and Button.Text=="500", "input pose must settle derived callbacks")
assert(Button.AbsolutePosition.Y==20, "derived layout must settle before hit dispatch")
local Clicked = false
Button.MouseButton1Click:Connect(function() Clicked=true end)
Dispatch({type="down",x=40,y=30,button=1})
Dispatch({type="up",x=40,y=30,button=1})
assert(Clicked, "click must hit the presented virtual row")
local Before = Changes
Step(.1)
Flush()
assert(Changes==Before and Scroll.CanvasPosition.Y==500, "owned pose must not run a second spring")
Apply(Packet(2,550,650), "frame")
assert(Scroll.CanvasPosition.Y==500, "frame pose must await PreRender")
StepDisplay(1/60)
assert(Scroll.CanvasPosition.Y==550)
local Older = Packet(2,100)
assert(not Apply(Older,"input") and Scroll.CanvasPosition.Y==550)
local Epoch = State().epoch
Scroll.CanvasPosition = Vector2.new(0,200)
Flush()
assert(State().epoch>Epoch and not State().owned)
assert(not Apply(Packet(3,600,600,Epoch),"input") and Scroll.CanvasPosition.Y==200)
assert(Apply(Packet(4,600),"input"))
Epoch = State().epoch
Scroll.CanvasSize = UDim2.fromOffset(400,500)
UpdateAll()
Flush()
local Clamped=State()
assert(Clamped.epoch>Epoch and not Clamped.owned and Clamped.position[2]<=Clamped.range[2])
local Bad=Packet(5,50)
Bad.scrolls[1].position[2]=0/0
assert(not Apply(Bad,"input") and not State().owned)
Bad=Packet(6,50);Bad.scrolls[1].id=99999999
assert(not Apply(Bad,"input"))
Scroll.CanvasSize = UDim2.fromOffset(400,2000)
UpdateAll()
local Max=State().range[2]
assert(Apply(Packet(7,Max-20,Max),"input"))
local Taker=Route({Scroll},-1,State().id)
assert(Taker==Scroll, "a client notch reaching the boundary must retain its selected recipient")
Step(.1)
assert(Scroll.CanvasPosition.Y==Max-20)
assert(Apply({version=1,seq=7,scrolls={}},"input"))
assert(State().owned, "equal sequence must not relinquish ownership")
assert(Apply({version=1,seq=8,scrolls={}},"input"))
assert(not State().owned, "fresh complete ownership set must release omitted scrollers")
Scroll.CanvasPosition=Vector2.zero
Route({Scroll},-1)
Step(.1)
assert(Scroll.CanvasPosition.Y>0, "released ownership must restore runtime spring")
Stop(Scroll)
Scroll.CanvasPosition=Vector2.zero
Flush()
local WheelCount, WheelPositions = 0, {}
Scroll.MouseWheelBackward:Connect(function()
 WheelCount += 1
 table.insert(WheelPositions, Scroll.CanvasPosition.Y)
end)
local BeforeBurst = Changes
for N=1,8 do
 local Pose=Packet(10+N, N*10, N*100)
 assert(Apply(Pose,"wheel"))
 Dispatch({type="wheel",x=200,y=200,notches=-1,presentationTaker=State().id})
end
assert(Changes==BeforeBurst and Scroll.CanvasPosition.Y==0, "wheel burst must not publish intermediate poses")
assert(WheelCount==8, "every wheel callback must survive coalesced presentation")
for _, Y in WheelPositions do assert(Y==0, "wheel callbacks must observe current logical pose") end
StepDisplay(1/60)
assert(Changes==BeforeBurst+1 and Scroll.CanvasPosition.Y==80, "wheel burst commits latest pose once in PreRender")
assert(Apply(Packet(19,120,900),"wheel"))
assert(Apply(Packet(20,140,900),"input"))
Clicked=false
Dispatch({type="down",x=40,y=30,button=1})
Dispatch({type="up",x=40,y=30,button=1})
assert(Clicked and Button.Text=="140", "discrete barrier must settle virtualized content after queued wheels")
StepDisplay(1/60)
assert(Scroll.CanvasPosition.Y==140, "queued older wheel cannot rewind a discrete pose")
local Handoff=Packet(21,300,30)
Handoff.scrolls[1].velocity={0,-200}
assert(Apply(Handoff,"input"))
local CountBeforeHandoff=WheelCount
local ForwardCount=0
Scroll.MouseWheelForward:Connect(function() ForwardCount+=1 end)
assert(Apply({version=1,seq=22,scrolls={}},"input"))
assert(not State().owned and State().target[2]==30 and State().velocity[2]==-200,
 "voluntary release must preserve accepted target and velocity")
Dispatch({type="wheel",x=200,y=200,notches=1})
assert(State().target[2]==0, "native wheel after handoff must accumulate from retained target")
assert(WheelCount==CountBeforeHandoff and ForwardCount==1, "handoff must preserve exactly the actual wheel callback")
for _=1,120 do StepDisplay(1/120) end
assert(Scroll.CanvasPosition.Y==0, "native spring must finish the released target")
local Pausing = Packet(23,500,600)
assert(Apply(Pausing,"input"))
Scroll.ScrollingEnabled = false
local Paused = State()
assert(not Paused.owned and not Paused.enabled and Paused.epoch>Pausing.scrolls[1].epoch,
 "disabling must invalidate browser ownership")
assert(Paused.target[2]==600 and Paused.velocity[2]==20,
 "disabling must retain the accepted destination and derivative for runtime resume")
Step(.5)
assert(Scroll.CanvasPosition.Y==500, "browser-owned spring must stay paused while disabled")
assert(not Apply(Pausing,"input"), "stale browser pose must not overwrite the pause")
Scroll.ScrollingEnabled = true
Step(1/60)
assert(Scroll.CanvasPosition.Y>500 and Scroll.CanvasPosition.Y<600,
 "enabling must resume the accepted browser spring")
for _=1,60 do Step(1/60) end
assert(Scroll.CanvasPosition.Y==600, "resumed browser spring must reach its original destination")
assert(Apply(Packet(24,500,600),"input"))
Scroll.ScrollingEnabled = false
Scroll.CanvasPosition = Vector2.new(0,450)
Scroll.ScrollingEnabled = true
Step(.5)
assert(Scroll.CanvasPosition.Y==450, "script position while paused must cancel the retained spring")
Enable(false)
assert(Snapshot()==nil)
Scroll.CanvasPosition=Vector2.zero
Route({Scroll},-1)
Step(.1)
assert(Scroll.CanvasPosition.Y>0, "disabled mode must preserve the original server spring")
"#,
    );
}

/// A story that rebuilds its UI leaves the old ScrollingFrames outside the DataModel, alive while a
/// script references them: the snapshot lists only frames in the DataModel (a frame that comes
/// back is listed again, with its state), and a freed frame's presentation state is dropped.
#[test]
fn presentation_lists_only_frames_in_the_data_model() {
    let (lua, engine) = start(800.0, 600.0);
    run(
        &lua,
        &engine,
        r#"
local Root = Instance.new("ScreenGui", game:GetService("CoreGui"))
Root.IgnoreGuiInset = true
local function Make(Name)
 local Holder = Instance.new("Frame", Root)
 Holder.Name = Name
 Holder.Size = UDim2.fromOffset(400,400)
 local Scroll = Instance.new("ScrollingFrame", Holder)
 Scroll.Size = UDim2.fromOffset(400,400)
 Scroll.CanvasSize = UDim2.fromOffset(400,2000)
 return Holder, Scroll
end
local Kept, KeptScroll = Make("Kept")
local Removed, RemovedScroll = Make("Removed")
UpdateAll()
Flush()
Enable(true)
local function Listed()
 local Ids = {}
 for _, Row in Snapshot().scrolls do Ids[Row.id] = Row end
 return Ids
end
assert(Listed()[Serial(KeptScroll)] and Listed()[Serial(RemovedScroll)])
Removed.Parent = nil
UpdateAll()
assert(Listed()[Serial(KeptScroll)] and not Listed()[Serial(RemovedScroll)], "a frame outside the DataModel is not listed")
Removed.Parent = Root
UpdateAll()
local Row = Listed()[Serial(RemovedScroll)]
assert(Row and Row.enabled and Row.range[2] > 1000, "a frame that comes back is listed again")

for Index = 1, 20 do
 local Holder, Scroll = Make("Old" .. Index)
 UpdateAll()
 assert(Listed()[Serial(Scroll)])
 Holder.Parent = nil
end
UpdateAll()
Snapshot()
"#,
    );
    let presented = || super::get(&engine).unwrap().scroll.presented_count();
    assert_eq!(presented(), 22);
    lua.gc_collect().unwrap();
    lua.gc_collect().unwrap();
    // (the 20 removed holders with their frames, and the test script)
    assert_eq!(engine_lua::raw::collect(&lua, &engine).unwrap(), 41);
    assert_eq!(presented(), 2, "freed frames keep no presentation state");
}

/// TextBox focus, caret placement, multi-click selection, editing keys and the clipboard rules of
/// runtime/input/textbox.luau (measured: caret.mjs, caret2.mjs, textbox-focus.mjs).
#[test]
fn textbox_focus_caret_and_editing() {
    let (lua, engine) = start(900.0, 400.0);
    run(
        &lua,
        &engine,
        r#"
local UIS = game:GetService("UserInputService")
local Root = Instance.new("ScreenGui", game:GetService("CoreGui"))
Root.IgnoreGuiInset = true
local T = Instance.new("TextBox")
T.Position = UDim2.fromOffset(100, 100)
T.Size = UDim2.fromOffset(600, 80)
T.Font = Enum.Font.SourceSans
T.TextSize = 48
T.TextXAlignment = Enum.TextXAlignment.Left
T.ClearTextOnFocus = false
T.Text = "hello world"
T.Parent = Root
local Log = {}
T.Focused:Connect(function() table.insert(Log, "Focused") end)
T.FocusLost:Connect(function(Enter, Input) table.insert(Log, `FocusLost {Enter} {Input and Input.UserInputType.Name}`) end)
UIS.TextBoxFocused:Connect(function() table.insert(Log, "UIS.TextBoxFocused") end)
UIS.TextBoxFocusReleased:Connect(function() table.insert(Log, "UIS.TextBoxFocusReleased") end)
UpdateAll()
-- CaptureFocus: caret at the end, no selection
T:CaptureFocus()
assert(T.CursorPosition == 12 and T.SelectionStart == -1 and T:IsFocused() and UIS:GetFocusedTextBox() == T)
-- Script writes clamp to #Text + 1; a Text write keeps the caret unless past the end
T.CursorPosition = 100
assert(T.CursorPosition == 12)
T.Text = "hello"
assert(T.CursorPosition == 6)
T.Text = "hello world"
-- A click places the caret at the boundary 2/3 into a character; a double click selects the word;
-- a triple click the whole text
Dispatch({ type = "down", x = 105, y = 140, button = 1 })
Dispatch({ type = "up", x = 105, y = 140, button = 1 })
assert(T.CursorPosition == 1 and T.SelectionStart == -1, `click: {T.CursorPosition} {T.SelectionStart}`)
Dispatch({ type = "down", x = 150, y = 140, button = 1 })
Dispatch({ type = "up", x = 150, y = 140, button = 1 })
Dispatch({ type = "down", x = 150, y = 140, button = 1 })
Dispatch({ type = "up", x = 150, y = 140, button = 1 })
assert(T.SelectionStart == 1 and T.CursorPosition == 6, `double click: {T.SelectionStart} {T.CursorPosition}`)
Dispatch({ type = "down", x = 150, y = 140, button = 1 })
Dispatch({ type = "up", x = 150, y = 140, button = 1 })
assert(T.SelectionStart == 1 and T.CursorPosition == 12, "triple click selects everything")
-- Typing replaces the selection; keys edit
Dispatch({ type = "text", text = "abc def" })
assert(T.Text == "abc def" and T.CursorPosition == 8)
Dispatch({ type = "key", key = "Backspace", down = true, ctrl = true })
Dispatch({ type = "key", key = "Backspace", down = false })
assert(T.Text == "abc ", T.Text)
Dispatch({ type = "key", key = "Home", down = true })
Dispatch({ type = "key", key = "Home", down = false })
Dispatch({ type = "key", key = "Right", down = true, shift = true })
Dispatch({ type = "key", key = "Right", down = false })
assert(T.CursorPosition == 2 and T.SelectionStart == 1)
Dispatch({ type = "key", key = "C", down = true, ctrl = true })
Dispatch({ type = "key", key = "C", down = false, ctrl = true })
Dispatch({ type = "key", key = "End", down = true })
Dispatch({ type = "key", key = "End", down = false })
Dispatch({ type = "key", key = "V", down = true, ctrl = true })
Dispatch({ type = "key", key = "V", down = false, ctrl = true })
assert(T.Text == "abc a", T.Text)
-- Single-line boxes drop newlines; Return releases focus (FocusLost(true, key input))
Dispatch({ type = "text", text = "\n" })
assert(T.Text == "abc a")
Dispatch({ type = "key", key = "Return", down = true })
Dispatch({ type = "key", key = "Return", down = false })
assert(not T:IsFocused() and T.CursorPosition == -1 and T.SelectionStart == -1)
Flush()
local Expected = "Focused,UIS.TextBoxFocused,FocusLost true Keyboard,UIS.TextBoxFocusReleased"
assert(table.concat(Log, ",") == Expected, table.concat(Log, ","))
-- A click elsewhere releases a clicked box (FocusLost(false, input)) before UIS.InputBegan
table.clear(Log)
UIS.InputBegan:Connect(function(Input) if Input.UserInputType == Enum.UserInputType.MouseButton1 then table.insert(Log, "UIS.InputBegan") end end)
Dispatch({ type = "down", x = 150, y = 140, button = 1 })
Dispatch({ type = "up", x = 150, y = 140, button = 1 })
Dispatch({ type = "down", x = 800, y = 300, button = 1 })
Dispatch({ type = "up", x = 800, y = 300, button = 1 })
local Joined = table.concat(Log, ",")
assert(Joined == "Focused,UIS.TextBoxFocused,UIS.InputBegan,FocusLost false MouseButton1,UIS.TextBoxFocusReleased,UIS.InputBegan", Joined)
-- Drag selection from the press point
Dispatch({ type = "down", x = 105, y = 140, button = 1 })
Dispatch({ type = "move", x = 300, y = 140 })
Dispatch({ type = "up", x = 300, y = 140, button = 1 })
assert(T.SelectionStart == 1 and T.CursorPosition > 1, `drag: {T.SelectionStart} {T.CursorPosition}`)
"#,
    );
}

/// Draggable (measured, drag.mjs): DragBegin before InputBegan, Position follows the mouse in
/// offset, DragStopped(x, y) before InputEnded; UIDragDetector is inert in edit mode.
#[test]
fn draggable_and_drag_detector() {
    let (lua, engine) = start(800.0, 600.0);
    run(
        &lua,
        &engine,
        r#"
local Root = Instance.new("ScreenGui", game:GetService("CoreGui"))
Root.IgnoreGuiInset = true
local D = Instance.new("Frame")
D.Position = UDim2.new(0.1, 50, 0, 50)
D.Size = UDim2.fromOffset(100, 100)
D.Active = true
D.Draggable = true
D.Parent = Root
local Log = {}
D.DragBegin:Connect(function(Start) table.insert(Log, `DragBegin {Start.X.Offset}`) end)
D.InputBegan:Connect(function(Input) if Input.UserInputType ~= Enum.UserInputType.MouseMovement then table.insert(Log, `InputBegan {Input.UserInputType.Name}`) end end)
D.DragStopped:Connect(function(X, Y) table.insert(Log, `DragStopped {X} {Y}`) end)
D.InputEnded:Connect(function(Input) if Input.UserInputType ~= Enum.UserInputType.MouseMovement then table.insert(Log, `InputEnded {Input.UserInputType.Name}`) end end)
local P = Instance.new("Frame")
P.Position = UDim2.fromOffset(400, 300)
P.Size = UDim2.fromOffset(100, 100)
P.Parent = Root
local Detector = Instance.new("UIDragDetector")
Detector.Parent = P
local Started = false
Detector.DragStart:Connect(function() Started = true end)
UpdateAll()
Dispatch({ type = "down", x = 150, y = 100, button = 1 })
Dispatch({ type = "move", x = 170, y = 130 })
StepDisplay(1/60)
assert(D.Position == UDim2.new(0.1, 70, 0, 80), tostring(D.Position))
Dispatch({ type = "up", x = 175, y = 135, button = 1 })
Flush()
local Joined = table.concat(Log, ",")
assert(Joined == "DragBegin 50,InputBegan MouseButton1,DragStopped 175 135,InputEnded MouseButton1", Joined)
Dispatch({ type = "down", x = 450, y = 350, button = 1 })
Dispatch({ type = "move", x = 480, y = 350 })
StepDisplay(1/60)
Dispatch({ type = "up", x = 480, y = 350, button = 1 })
Flush()
assert(not Started and P.Position == UDim2.fromOffset(400, 300), "UIDragDetector is inert in edit mode")
"#,
    );
}

/// runtime/tests/native_clipboard.luau: a cut in one input batch precedes the host clipboard
/// echo, an external paste is used as is, and each copy reaches exactly one reply.
#[test]
fn session_clipboard_bridge() {
    let (lua, engine) = start(640.0, 480.0);
    run(
        &lua,
        &engine,
        r#"
local Root = Instance.new("ScreenGui", game:GetService("CoreGui"))
Root.IgnoreGuiInset = true
local Box = Instance.new("TextBox", Root)
Box.Size = UDim2.fromOffset(300, 40)
Box.Text = "copy me"
Box.ClearTextOnFocus = false
_G.Box = Box
UpdateAll()
"#,
    );
    let project_host = lua.create_table().unwrap();
    let project = crate::engine::story::Project {
        host: project_host,
        hierarchy: None,
        mount_time: 0.0,
        started: std::time::Instant::now(),
        plugin_widgets: None,
        sources: Vec::new(),
    };
    let story = crate::engine::story::Story::empty(&lua).unwrap();
    let mut session = Session::new(&engine, project, story, [0.18, 0.18, 0.18], true);
    let shortcut = |key: &str| vec![json!({ "type": "key", "key": key, "down": true, "ctrl": true }), json!({ "type": "key", "key": key, "down": false, "ctrl": true })];
    let text = || -> String {
        let shared = engine.lua.borrow().shared.clone().unwrap();
        let g: LuaTable = shared.get("_G").unwrap();
        let LuaValue::UserData(ud) = g.get::<LuaValue>("Box").unwrap() else { panic!() };
        let id = engine_lua::raw::instance_of_ud(&ud).unwrap();
        let name = engine.reflection.name_id("Text").unwrap();
        engine.state.borrow().dom.raw_get(id, name).unwrap().as_str().unwrap().to_string()
    };
    run(&lua, &engine, "_G.Box:CaptureFocus()");
    let mut events = shortcut("A");
    events.extend(shortcut("X"));
    events.push(json!({ "type": "paste", "text": "stale OS" }));
    let reply = session.frame(&lua, 0.0, None, &events);
    assert_eq!(reply["type"], "frame", "{reply}");
    assert_eq!(text(), "copy me", "same-batch cut/paste used stale host clipboard");
    assert_eq!(reply["clipboard"], "copy me", "cut did not echo the selected text");
    let mut events = shortcut("A");
    events.push(json!({ "type": "paste", "text": "external" }));
    let reply = session.frame(&lua, 0.0, None, &events);
    assert_eq!(text(), "external", "external host clipboard paste was discarded");
    assert!(reply.get("clipboard").is_none(), "paste should not emit a copy notification");
    let mut events = shortcut("A");
    events.extend(shortcut("C"));
    let reply = session.frame(&lua, 0.0, None, &events);
    assert_eq!(reply["clipboard"], "external", "copy did not echo the selected text");
    let reply = session.frame(&lua, 0.0, None, &[]);
    assert!(reply.get("clipboard").is_none(), "clipboard notification was repeated");
}

/// --actions click / drag (input/actions.rs): nodes located by path when the timeline reaches the
/// action (a button that moved is still hit), a visible node preferred over a hidden namesake, a
/// drag's moves spread over its duration; unknown and ambiguous paths fail the run by name after
/// the rest of the timeline ran.
#[test]
fn click_and_drag_actions_target_nodes() {
    let (lua, engine) = start(800.0, 600.0);
    run(
        &lua,
        &engine,
        r#"
local Root = Instance.new("ScreenGui")
Root.Name = "RURStory"
Root.IgnoreGuiInset = true
Root.Parent = game:GetService("CoreGui")
local Target = Instance.new("Frame")
Target.Name = "StoryTarget"
Target.Size = UDim2.fromScale(1, 1)
Target.Parent = Root
local Stack = Instance.new("Frame")
Stack.Name = "Stack"
Stack.Position = UDim2.fromOffset(100, 100)
Stack.Size = UDim2.fromOffset(300, 200)
Stack.Parent = Target
local Button = Instance.new("TextButton")
Button.Name = "Button"
Button.Text = "0"
Button.Size = UDim2.fromOffset(100, 40)
Button.Parent = Stack
Button.Activated:Connect(function() Button.Text = tostring(tonumber(Button.Text) + 1) end)
-- Moves between the clicks: the second click follows it
task.delay(0.3, function() Button.Position = UDim2.fromOffset(150, 120) end)
local Hidden = Instance.new("TextButton")
Hidden.Name = "Button"
Hidden.Visible = false
Hidden.Parent = Target
local Handle = Instance.new("Frame")
Handle.Name = "Handle"
Handle.Position = UDim2.fromOffset(500, 100)
Handle.Size = UDim2.fromOffset(50, 50)
Handle.Active = true
Handle.Draggable = true
Handle.Parent = Target
Handle.DragStopped:Connect(function(X, Y) Handle:SetAttribute("Stopped", `{X} {Y}`) end)
for _, Name in { "TwinA", "TwinB" } do
	local Holder = Instance.new("Frame")
	Holder.Name = Name
	Holder.Position = UDim2.fromOffset(0, 400)
	Holder.Parent = Target
	local Twin = Instance.new("Frame")
	Twin.Name = "Twin"
	Twin.Parent = Holder
end
"#,
    );
    let actions = crate::engine::story::load_actions(
        &lua,
        r#"[
            {"t": 0.1, "type": "click", "node": "StoryTarget.Stack.Button"},
            {"t": 0.5, "type": "click", "target": "Button", "anchor": {"x": 0.25, "y": 0.5}, "offset": {"x": 2, "y": 0}},
            {"t": 0.6, "type": "drag", "from": "Handle", "to": {"x": 600, "y": 200}, "steps": 5, "duration": 0.1},
            {"t": 0.8, "type": "click", "node": "Missing.Button"},
            {"t": 0.8, "type": "click", "node": "Twin"}
        ]"#,
    )
    .unwrap();
    let error = crate::engine::story::run_timeline(&lua, &engine, &actions, 1.0, &mut crate::engine::story::input_dispatch).unwrap_err().to_string();
    assert!(error.contains("--actions[3] (click at t=0.8): no GUI node matches \"Missing.Button\"; nodes named \"Button\": CoreGui.RURStory.StoryTarget.Button, CoreGui.RURStory.StoryTarget.Stack.Button"), "{error}");
    assert!(error.contains("--actions[4] (click at t=0.8): \"Twin\" matches 2 visible nodes (CoreGui.RURStory.StoryTarget.TwinA.Twin, CoreGui.RURStory.StoryTarget.TwinB.Twin); give a longer path"), "{error}");
    assert!(engine.state.borrow().sched.now >= 1.0 - 1e-9, "the timeline ran to the end");
    run(
        &lua,
        &engine,
        r#"
local Target = game:GetService("CoreGui").RURStory.StoryTarget
assert(Target.Stack.Button.Text == "2", `clicks: {Target.Stack.Button.Text}`)
assert(Target.Handle:GetAttribute("Stopped") == "600 200", `drag stopped at {Target.Handle:GetAttribute("Stopped")}`)
assert(Target.Handle.Position.X.Offset > 540 and Target.Handle.Position.Y.Offset > 140, `dragged to {Target.Handle.Position}`)
"#,
    );
}

/// A bad list is refused before it runs, naming the action
#[test]
fn bad_action_lists_are_refused() {
    let error = crate::engine::story::read_actions(r#"[{"t": 0.1, "type": "move", "x": 1, "y": 2}, {"type": "click", "node": ""}]"#).unwrap_err();
    assert_eq!(error, "--actions[1] (click): click: `node` is empty");
    let error = crate::engine::story::read_actions(r#"[{"type": "move""#).unwrap_err();
    assert!(error.starts_with("--actions: not valid JSON: "), "{error}");
}
