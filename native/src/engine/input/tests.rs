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
    Engine, InstanceId,
    lua::{self as engine_lua, globals::CallOutcome, instance_value},
    scheduler,
    session::Session,
};

fn start(width: f64, height: f64) -> (Lua, Rc<Engine>) {
    let mut options = crate::engine::options();
    options.profile.device.viewport = Some((width, height));
    let engine = crate::engine::new_engine(options).expect("engine");
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
    hook!(engine, input, "Route", |lua, (list, notches, taker, shift): (LuaTable, f64, Option<f64>, Option<bool>)| {
        let mut ids = Vec::new();
        for value in list.sequence_values::<LuaValue>() {
            ids.push(id_of(&value?)?);
        }
        let taker = taker.map(|t| json!(t));
        let (taken, _) = scrolling::route(&Cx { lua, engine, i: input }, &ids, notches, shift.unwrap_or(false), taker.as_ref());
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
-- A client keeps a newline typed into a single-line box (measured); Return releases focus (FocusLost(true, key input))
Dispatch({ type = "text", text = "\n" })
assert(T.Text == "abc a\n")
T.Text = "abc a"
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
        sources: Default::default(),
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

/// --actions wheel at a node (input/actions.rs): the point is the node's (with `anchor` / `offset`)
/// when the timeline reaches it, the pointer moves there first as for a click, and the ScrollingFrame
/// there scrolls as one does under a wheel by x, y, wherever the pointer was before. A wheel by x, y
/// is dispatched as it is (the hover state does not follow it).
#[test]
fn wheel_actions_target_nodes() {
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
local function Scroller(Name, Parent, X, Y)
	local Scroll = Instance.new("ScrollingFrame")
	Scroll.Name = Name
	Scroll.Position = UDim2.fromOffset(X, Y)
	Scroll.Size = UDim2.fromOffset(200, 150)
	Scroll.CanvasSize = UDim2.fromOffset(0, 2000)
	Scroll.Parent = Parent
	Scroll.MouseWheelBackward:Connect(function() Scroll:SetAttribute("Backward", (Scroll:GetAttribute("Backward") or 0) + 1) end)
	Scroll.MouseEnter:Connect(function() Scroll:SetAttribute("Entered", true) end)
end
Scroller("ByXY", Target, 50, 50)
Scroller("ByNode", Target, 300, 50)
local Strip = Instance.new("Frame")
Strip.Name = "Strip"
Strip.Position = UDim2.fromOffset(50, 350)
Strip.Size = UDim2.fromOffset(400, 150)
Strip.Parent = Target
Scroller("Left", Strip, 0, 0)
Scroller("Right", Strip, 200, 0)
"#,
    );
    let actions = crate::engine::story::load_actions(
        &lua,
        r#"[
            {"t": 0.05, "type": "move", "x": 700, "y": 550},
            {"t": 0.1, "type": "wheel", "x": 150, "y": 125, "notches": -1},
            {"t": 0.15, "type": "move", "x": 700, "y": 550},
            {"t": 0.2, "type": "wheel", "node": "StoryTarget.ByNode", "notches": -1},
            {"t": 0.3, "type": "wheel", "target": "Strip", "anchor": {"x": 0.75}, "offset": {"x": -10, "y": 0}, "dy": 200},
            {"t": 0.4, "type": "wheel", "node": "Missing", "notches": -1}
        ]"#,
    )
    .unwrap();
    let result = crate::engine::story::run_timeline(&lua, &engine, &actions, 1.5, &mut crate::engine::story::input_dispatch);
    run(
        &lua,
        &engine,
        r#"
local Target = game:GetService("CoreGui").RURStory.StoryTarget
local function Check(Scroll, Y, Backward, Entered)
	assert(Scroll.CanvasPosition.Y == Y, `{Scroll.Name}: CanvasPosition {Scroll.CanvasPosition}, expected Y {Y}`)
	assert(Scroll:GetAttribute("Backward") == Backward, `{Scroll.Name}: MouseWheelBackward {Scroll:GetAttribute("Backward")}`)
	assert(Scroll:GetAttribute("Entered") == Entered, `{Scroll.Name}: entered {Scroll:GetAttribute("Entered")}`)
end
Check(Target.ByXY, 100, 1, nil)
Check(Target.ByNode, 100, 1, true)
Check(Target.Strip.Right, 200, 2, true)
Check(Target.Strip.Left, 0, nil, nil)
"#,
    );
    assert_eq!(super::get(&engine).unwrap().mouse_location(), (50.0 + 300.0 - 10.0, 350.0 + 75.0), "the pointer moved to the last wheel's point");
    let error = result.unwrap_err().to_string();
    assert!(error.contains("--actions[5] (wheel at t=0.4): no GUI node matches \"Missing\""), "{error}");
}

/// A bad list is refused before it runs, naming the action
#[test]
fn bad_action_lists_are_refused() {
    let error = crate::engine::story::read_actions(r#"[{"t": 0.1, "type": "move", "x": 1, "y": 2}, {"type": "click", "node": ""}]"#).unwrap_err();
    assert_eq!(error, "--actions[1] (click): click: `node` is empty");
    let error = crate::engine::story::read_actions(r#"[{"type": "move""#).unwrap_err();
    assert!(error.starts_with("--actions: not valid JSON: "), "{error}");
}

/// Replayed keys and mouse buttons (the `--actions` key, down and up events) drive bound InputActions through the
/// engine's input_actions::deliver, after UserInputService's events: Pressed, Released and GetState follow them, a
/// mouse button reaches the binding that names it (MouseLeftButton), and a key a focused TextBox takes reaches no
/// action (ree notes "Input actions": measured in a 0.747 client).
#[test]
fn replayed_input_drives_bound_input_actions() {
    let (lua, engine) = start(800.0, 600.0);
    run(
        &lua,
        &engine,
        r#"
local Log = {}
local UIS = game:GetService("UserInputService")
UIS.InputBegan:Connect(function(Input) table.insert(Log, "began " .. (if Input.KeyCode == Enum.KeyCode.Unknown then Input.UserInputType.Name else Input.KeyCode.Name)) end)
local Context = Instance.new("InputContext")
local function action(Label, KeyCode)
	local Action = Instance.new("InputAction")
	Action.Name = Label
	local Binding = Instance.new("InputBinding")
	Binding.KeyCode = KeyCode
	Binding.Parent = Action
	Action.Parent = Context
	Action.Pressed:Connect(function() table.insert(Log, Label .. " pressed " .. tostring(Action:GetState())) end)
	Action.Released:Connect(function() table.insert(Log, Label .. " released " .. tostring(Action:GetState())) end)
	return Action
end
local Jump = action("jump", Enum.KeyCode.Space)
local Shoot = action("shoot", Enum.KeyCode.MouseLeftButton)
Context.Parent = game:GetService("ReplicatedStorage")
Dispatch({ type = "key", key = "Space", down = true })
Flush()
assert(Jump:GetState() == true, "held")
Dispatch({ type = "key", key = "Space", down = false })
Flush()
assert(Jump:GetState() == false, "released")
Dispatch({ type = "down", x = 10, y = 10, button = 1 })
Flush()
assert(Shoot:GetState() == true, "the left button")
Dispatch({ type = "up", x = 10, y = 10, button = 1 })
Flush()
local Gui = Instance.new("ScreenGui")
local Box = Instance.new("TextBox")
Box.Size = UDim2.fromOffset(200, 40)
Box.Parent = Gui
Gui.Parent = game:GetService("Players").LocalPlayer:WaitForChild("PlayerGui")
UpdateAll()
Box:CaptureFocus()
Flush()
Dispatch({ type = "key", key = "Space", down = true })
Flush()
assert(Jump:GetState() == false, "a focused TextBox takes the key")
Dispatch({ type = "key", key = "Space", down = false })
Flush()
local Joined = table.concat(Log, ",")
assert(Joined == "began Space,jump pressed true,jump released false,began MouseButton1,shoot pressed true,shoot released false,began Space", Joined)
"#,
    );
}

/// ContextActionService:SetTitle / SetPosition / SetImage / SetDescription / GetBoundActionInfo /
/// GetAllBoundActionInfo, measured in a playing client (Studio 0.747.0.7471481, desktop / touch /
/// gamepad): no unemulated diagnostics, exact argument errors and info table shape.
#[test]
fn context_action_touch_settings_and_info_match_client() {
    let (lua, engine) = start(800.0, 600.0);
    run(
        &lua,
        &engine,
        r#"
local CAS = game:GetService("ContextActionService")
local function noop() return Enum.ContextActionResult.Pass end

-- Unbound actions succeed silently; GetBoundActionInfo returns an empty table.
CAS:SetTitle("NoSuch", "Ignored")
CAS:SetPosition("NoSuch", UDim2.fromOffset(10, 20))
CAS:SetImage("NoSuch", "rbxassetid://1")
CAS:SetDescription("NoSuch", "Desc")
assert(next(CAS:GetBoundActionInfo("NoSuch")) == nil, "unbound info must be empty")

-- Bound with createTouchButton = true vs false.
CAS:BindAction("ActTouch", noop, true, Enum.KeyCode.E)
CAS:BindActionAtPriority("ActNoTouch", noop, false, 3000, Enum.KeyCode.Q)
CAS:SetTitle("ActTouch", "Jump!")
CAS:SetImage("ActTouch", "rbxassetid://12345")
CAS:SetDescription("ActTouch", "Does jump")
CAS:SetPosition("ActTouch", UDim2.new(0.5, 10, 0.25, -5))
CAS:SetTitle("ActNoTouch", "HiddenTitle")
CAS:SetPosition("ActNoTouch", UDim2.fromOffset(1, 2))

local info = CAS:GetBoundActionInfo("ActTouch")
assert(info.title == "Jump!", tostring(info.title))
assert(info.image == "rbxassetid://12345", tostring(info.image))
assert(info.description == "Does jump", tostring(info.description))
assert(info.createTouchButton == true, tostring(info.createTouchButton))
assert(info.priorityLevel == 2000, tostring(info.priorityLevel))
assert(info.stackOrder == 1, tostring(info.stackOrder))
assert(info.position == nil, "SetPosition does not add a position field to GetBoundActionInfo")
assert(#info.inputTypes == 1 and info.inputTypes[1] == Enum.KeyCode.E, "inputTypes")

local noTouch = CAS:GetBoundActionInfo("ActNoTouch")
assert(noTouch.title == "HiddenTitle", tostring(noTouch.title))
assert(noTouch.createTouchButton == nil, "createTouchButton omitted when false")
assert(noTouch.priorityLevel == 3000 and noTouch.stackOrder == 2)

local all = CAS:GetAllBoundActionInfo()
assert(all.ActTouch and all.ActTouch.title == "Jump!" and all.ActNoTouch and all.ActNoTouch.title == "HiddenTitle")

-- String parameter coercion (measured: numbers, booleans, tables stringify).
CAS:SetTitle("ActTouch", 456)
assert(CAS:GetBoundActionInfo("ActTouch").title == "456")
CAS:SetTitle("ActTouch", true)
assert(CAS:GetBoundActionInfo("ActTouch").title == "true")
CAS:SetTitle("ActTouch", {})
assert(CAS:GetBoundActionInfo("ActTouch").title == "{}")

-- Rebinding clears title/image/description and increments stackOrder.
CAS:BindAction("ActTouch", noop, true, Enum.KeyCode.F)
local rebound = CAS:GetBoundActionInfo("ActTouch")
assert(rebound.title == nil and rebound.image == nil and rebound.description == nil, "rebind resets touch settings")
assert(rebound.stackOrder == 3 and rebound.inputTypes[1] == Enum.KeyCode.F)

-- Argument validation order and messages (arg 2 checked before arg 1).
local function expectErr(expected, fn, ...)
	local ok, err = pcall(fn, ...)
	assert(not ok and string.find(tostring(err), expected, 1, true), `expected "{expected}", got ok={ok} err={err}`)
end
expectErr("Argument 2 missing or nil", CAS.SetTitle, CAS)
expectErr("Argument 2 missing or nil", CAS.SetTitle, CAS, "ActTouch")
expectErr("Argument 2 missing or nil", CAS.SetTitle, CAS, "ActTouch", nil)
expectErr("Argument 1 missing or nil", CAS.SetTitle, CAS, nil, "Ok")
expectErr("Argument 2 missing or nil", CAS.SetPosition, CAS)
expectErr("Argument 2 missing or nil", CAS.SetPosition, CAS, "ActTouch")
expectErr("Argument 2 missing or nil", CAS.SetPosition, CAS, "ActTouch", nil)
expectErr("Argument 1 missing or nil", CAS.SetPosition, CAS, nil, UDim2.new())
expectErr("Unable to cast string to UDim2", CAS.SetPosition, CAS, "ActTouch", "bad")
expectErr("Unable to cast Vector2 to UDim2", CAS.SetPosition, CAS, "ActTouch", Vector2.new(1, 2))

CAS:UnbindAction("ActTouch")
assert(next(CAS:GetBoundActionInfo("ActTouch")) == nil)
CAS:UnbindAllActions()
assert(next(CAS:GetAllBoundActionInfo()) == nil)
"#,
    );
    let unemulated = engine.state.borrow().output.unemulated();
    assert!(unemulated.is_empty(), "ContextActionService setters must not report unemulated: {unemulated:?}");
}

/// Horizontal and Shift+wheel scrolling rules measured in a playing client (Studio 0.747.0.7471481):
/// X-only frames (including Battle Pass's AutomaticCanvasSize=X shape), XY frames with/without Shift,
/// vertical-ancestor priority on nested scrollers, ancestor-only pass-through, and the X spring.
#[test]
fn horizontal_and_shift_wheel_match_client() {
    let (lua, engine) = start(1600.0, 1000.0);
    run(
        &lua,
        &engine,
        r#"
local Root = Instance.new("ScreenGui")
Root.IgnoreGuiInset = true
Root.Parent = game:GetService("CoreGui")

local function makeScroller(name, x, y, w, h, cw, ch, dir, props, parent)
	local s = Instance.new("ScrollingFrame")
	s.Name = name
	s.Position = UDim2.fromOffset(x, y)
	s.Size = UDim2.fromOffset(w, h)
	s.CanvasSize = UDim2.fromOffset(cw, ch)
	s.ScrollingDirection = dir
	s.BorderSizePixel = 0
	s.ScrollBarThickness = 0
	for k, v in props or {} do s[k] = v end
	s.Parent = parent or Root
	return s
end

local Catcher = makeScroller("Catcher", 0, 0, 1500, 900, 1500, 5000, Enum.ScrollingDirection.Y, { ZIndex = -5 })
local X_Xonly = makeScroller("X_Xonly", 20, 20, 200, 150, 1500, 150, Enum.ScrollingDirection.X)
local X_Auto = makeScroller("X_Auto", 250, 20, 200, 150, 0, 0, Enum.ScrollingDirection.X, {
	AutomaticCanvasSize = Enum.AutomaticSize.X,
	ScrollBarThickness = 6,
	HorizontalScrollBarInset = Enum.ScrollBarInset.Always,
})
local Wide = Instance.new("Frame")
Wide.Size = UDim2.fromOffset(1500, 100)
Wide.Parent = X_Auto
local XY_Xonly = makeScroller("XY_Xonly", 480, 20, 200, 150, 1500, 100, Enum.ScrollingDirection.XY)
local XY_XonlyBar = makeScroller("XY_XonlyBar", 710, 20, 200, 150, 1500, 150, Enum.ScrollingDirection.XY, { ScrollBarThickness = 12 })
local XY_Both = makeScroller("XY_Both", 940, 20, 200, 150, 1500, 1500, Enum.ScrollingDirection.XY)
local X_Both = makeScroller("X_Both", 20, 220, 200, 150, 1500, 1500, Enum.ScrollingDirection.X)
local X_Yonly = makeScroller("X_Yonly", 250, 220, 200, 150, 200, 1500, Enum.ScrollingDirection.X)
local Y_Both = makeScroller("Y_Both", 480, 220, 200, 150, 1500, 1500, Enum.ScrollingDirection.Y)
local X_Disabled = makeScroller("X_Disabled", 710, 220, 200, 150, 1500, 150, Enum.ScrollingDirection.X, { ScrollingEnabled = false })

local OuterY = makeScroller("OuterY", 940, 220, 300, 250, 300, 1500, Enum.ScrollingDirection.Y)
local InnerX_in_Y = makeScroller("InnerX_in_Y", 20, 20, 200, 150, 1500, 150, Enum.ScrollingDirection.X, nil, OuterY)

local OuterY0 = makeScroller("OuterY0", 20, 420, 300, 250, 300, 250, Enum.ScrollingDirection.Y)
local InnerX_in_Y0 = makeScroller("InnerX_in_Y0", 20, 20, 200, 150, 1500, 150, Enum.ScrollingDirection.X, nil, OuterY0)

local OuterX2 = makeScroller("OuterX2", 360, 420, 300, 250, 1500, 250, Enum.ScrollingDirection.X)
local InnerX2 = makeScroller("InnerX2", 20, 20, 200, 150, 350, 150, Enum.ScrollingDirection.X, nil, OuterX2)

local OuterX3 = makeScroller("OuterX3", 700, 420, 300, 250, 1500, 250, Enum.ScrollingDirection.X)
local InnerY3 = makeScroller("InnerY3", 20, 20, 200, 150, 200, 350, Enum.ScrollingDirection.Y, nil, OuterX3)

local BackY = makeScroller("BackY", 1040, 420, 300, 250, 300, 1500, Enum.ScrollingDirection.Y, { ZIndex = 1 })
local FrontX = makeScroller("FrontX", 1060, 440, 200, 150, 350, 150, Enum.ScrollingDirection.X, { ZIndex = 5 })

UpdateAll()
local all = { Catcher, X_Xonly, X_Auto, XY_Xonly, XY_XonlyBar, XY_Both, X_Both, X_Yonly, Y_Both, X_Disabled, OuterY, InnerX_in_Y, OuterY0, InnerX_in_Y0, OuterX2, InnerX2, OuterX3, InnerY3, BackY, FrontX }

local function settle()
	for _ = 1, 45 do Step(1 / 60) end
	UpdateAll()
end

local function wheelAt(target, notches, shiftKey, startPos)
	for _, f in all do
		Stop(f)
		f.CanvasPosition = Vector2.zero
	end
	if startPos then
		for f, pos in startPos do f.CanvasPosition = pos end
	end
	UpdateAll()
	local x = target.AbsolutePosition.X + target.AbsoluteSize.X * 0.5
	local y = target.AbsolutePosition.Y + target.AbsoluteSize.Y * 0.5
	local send = Dispatch
	if shiftKey then
		send({ type = "key", key = shiftKey, down = true })
	end
	send({ type = "wheel", x = x, y = y, notches = notches })
	settle()
	if shiftKey then
		send({ type = "key", key = shiftKey, down = false })
	end
end

-- X-only and auto-sized AutomaticCanvasSize=X frames scroll horizontally on normal wheel.
wheelAt(X_Xonly, -1)
assert(X_Xonly.CanvasPosition == Vector2.new(100, 0), `X_Xonly: {X_Xonly.CanvasPosition}`)
wheelAt(X_Xonly, 1, nil, { [X_Xonly] = Vector2.new(300, 0) })
assert(X_Xonly.CanvasPosition == Vector2.new(200, 0), `X_Xonly fwd: {X_Xonly.CanvasPosition}`)
wheelAt(X_Auto, -1)
assert(X_Auto.CanvasPosition == Vector2.new(100, 0), `X_Auto: {X_Auto.CanvasPosition}`)

-- XY with strictly X overflow scrolls X; XY with small Y overflow (12 px from bar) scrolls Y without Shift and X with Shift.
wheelAt(XY_Xonly, -1)
assert(XY_Xonly.CanvasPosition == Vector2.new(100, 0), `XY_Xonly: {XY_Xonly.CanvasPosition}`)
wheelAt(XY_XonlyBar, -1)
assert(XY_XonlyBar.CanvasPosition == Vector2.new(0, 12), `XY_XonlyBar: {XY_XonlyBar.CanvasPosition}`)
wheelAt(XY_XonlyBar, -1, "LeftShift")
assert(XY_XonlyBar.CanvasPosition == Vector2.new(100, 0), `XY_XonlyBar + Shift: {XY_XonlyBar.CanvasPosition}`)

-- XY with both axes: Y without Shift, X with LeftShift or RightShift; at end of Y does not spill to X.
wheelAt(XY_Both, -1)
assert(XY_Both.CanvasPosition == Vector2.new(0, 100), `XY_Both: {XY_Both.CanvasPosition}`)
wheelAt(XY_Both, -1, "LeftShift")
assert(XY_Both.CanvasPosition == Vector2.new(100, 0), `XY_Both + LeftShift: {XY_Both.CanvasPosition}`)
wheelAt(XY_Both, -1, "RightShift")
assert(XY_Both.CanvasPosition == Vector2.new(100, 0), `XY_Both + RightShift: {XY_Both.CanvasPosition}`)
wheelAt(XY_Both, 1, "LeftShift", { [XY_Both] = Vector2.new(300, 300) })
assert(XY_Both.CanvasPosition == Vector2.new(200, 300), `XY_Both + Shift fwd: {XY_Both.CanvasPosition}`)
wheelAt(XY_Both, -1, nil, { [XY_Both] = Vector2.new(0, 1350) })
assert(XY_Both.CanvasPosition == Vector2.new(0, 1350) and Catcher.CanvasPosition.Y == 0, "XY at Y-end does not spill to X or sibling")

-- Direction X with both overflows scrolls X; direction X with Y-only overflow does not move; direction Y + Shift does not move.
wheelAt(X_Both, -1)
assert(X_Both.CanvasPosition == Vector2.new(100, 0), `X_Both: {X_Both.CanvasPosition}`)
wheelAt(X_Yonly, -1)
assert(X_Yonly.CanvasPosition == Vector2.zero and Catcher.CanvasPosition.Y == 0, `X_Yonly: {X_Yonly.CanvasPosition}`)
wheelAt(Y_Both, -1, "LeftShift")
assert(Y_Both.CanvasPosition == Vector2.zero and Catcher.CanvasPosition.Y == 0, `Y_Both + Shift: {Y_Both.CanvasPosition}`)

-- Disabled X frame is skipped so sibling behind it scrolls.
wheelAt(X_Disabled, -1)
assert(X_Disabled.CanvasPosition == Vector2.zero and Catcher.CanvasPosition.Y == 100, `X_Disabled -> Catcher: {Catcher.CanvasPosition}`)

-- Nested: vertical ancestor wins over horizontal child without Shift; Shift overrides to scroll child X.
wheelAt(InnerX_in_Y, -1)
assert(InnerX_in_Y.CanvasPosition == Vector2.zero and OuterY.CanvasPosition == Vector2.new(0, 100), `InnerX_in_Y -> OuterY: {InnerX_in_Y.CanvasPosition} / {OuterY.CanvasPosition}`)
wheelAt(InnerX_in_Y, -1, "LeftShift")
assert(InnerX_in_Y.CanvasPosition == Vector2.new(100, 0) and OuterY.CanvasPosition == Vector2.zero, `InnerX_in_Y + Shift: {InnerX_in_Y.CanvasPosition}`)

-- Nested: non-overflowing vertical ancestor does not block horizontal child.
wheelAt(InnerX_in_Y0, -1)
assert(InnerX_in_Y0.CanvasPosition == Vector2.new(100, 0), `InnerX_in_Y0: {InnerX_in_Y0.CanvasPosition}`)

-- Nested X inside X: inner scrolls first, then at max passes to outer X.
wheelAt(InnerX2, -1)
assert(InnerX2.CanvasPosition == Vector2.new(100, 0) and OuterX2.CanvasPosition == Vector2.zero)
wheelAt(InnerX2, -1, nil, { [InnerX2] = Vector2.new(150, 0) })
assert(InnerX2.CanvasPosition == Vector2.new(150, 0) and OuterX2.CanvasPosition == Vector2.new(100, 0))

-- Nested Y inside X: Y at max passes to outer X; Y + Shift passes to outer X.
wheelAt(InnerY3, -1, nil, { [InnerY3] = Vector2.new(0, 200) })
assert(OuterX3.CanvasPosition == Vector2.new(100, 0), `InnerY3 at max -> OuterX3: {OuterX3.CanvasPosition}`)
wheelAt(InnerY3, -1, "LeftShift")
assert(InnerY3.CanvasPosition == Vector2.zero and OuterX3.CanvasPosition == Vector2.new(100, 0), `InnerY3 + Shift -> OuterX3: {OuterX3.CanvasPosition}`)

-- Sibling FrontX over BackY scrolls FrontX (BackY is not an ancestor), and at max does not pass to sibling BackY.
wheelAt(FrontX, -1)
assert(FrontX.CanvasPosition == Vector2.new(100, 0) and BackY.CanvasPosition == Vector2.zero)
wheelAt(FrontX, -1, nil, { [FrontX] = Vector2.new(150, 0) })
assert(FrontX.CanvasPosition == Vector2.new(150, 0) and BackY.CanvasPosition == Vector2.zero)

-- X spring matches Studio 0.747.0.7471481 measured frame trace (dt = 1/60: 8.161, 24.309, 41.289, ... -> 100).
Stop(X_Xonly)
X_Xonly.CanvasPosition = Vector2.zero
UpdateAll()
Dispatch({ type = "wheel", x = 120, y = 95, notches = -1 })
Step(1 / 60)
assert(math.abs(X_Xonly.CanvasPosition.X - 8.161087) < 0.05, `frame 1 X={X_Xonly.CanvasPosition.X}`)
Step(1 / 60)
assert(math.abs(X_Xonly.CanvasPosition.X - 24.309196) < 0.05, `frame 2 X={X_Xonly.CanvasPosition.X}`)
settle()
assert(X_Xonly.CanvasPosition == Vector2.new(100, 0))
"#,
    );
}


/// The TextBox keys of a 0.747 client (keys and typed text injected with VirtualInputManager into a focused
/// PlayerGui box; notes/interaction.md "TextBox keys"; the cases and the client's answers are generated from the
/// capture): each case sets Text, CursorPosition and SelectionStart, presses its keys or types its text, and reads
/// the box back. Single-line boxes only, so no font is needed.
#[test]
fn textbox_keys_match_a_client() {
    let (lua, engine) = start(900.0, 600.0);
    run(
        &lua,
        &engine,
        &format!(
            "{}{}",
            TEXTBOX_PRELUDE,
            r##"
Case("type X at 6", T, "hello world", 6, -1, { "X" }, "helloX world", 7, -1, true)
Case("type XY at 6", T, "hello world", 6, -1, { "XY" }, "helloXY world", 8, -1, true)
Case("type X over sel 1..6", T, "hello world", 6, 1, { "X" }, "X world", 2, -1, true)
Case("type X over sel 6..1 (cursor 1)", T, "hello world", 1, 6, { "X" }, "X world", 2, -1, true)
Case("key A at 6", T, "hello world", 6, -1, { { "A" } }, "hello world", 6, -1, true)
Case("key Space at 6", T, "hello world", 6, -1, { { "Space" } }, "hello world", 6, -1, true)
Case("key Shift+A at 6", T, "hello world", 6, -1, { { "A", shift = true } }, "hello world", 6, -1, true)
Case("Backspace at 6", T, "hello world", 6, -1, { { "Backspace" } }, "hell world", 5, -1, true)
Case("Backspace at 1", T, "hello world", 1, -1, { { "Backspace" } }, "hello world", 1, -1, true)
Case("Backspace sel 1..6", T, "hello world", 6, 1, { { "Backspace" } }, " world", 1, -1, true)
Case("Delete at 6", T, "hello world", 6, -1, { { "Delete" } }, "helloworld", 6, -1, true)
Case("Delete at 12", T, "hello world", 12, -1, { { "Delete" } }, "hello world", 12, -1, true)
Case("Delete sel 3..9", T, "hello world", 9, 3, { { "Delete" } }, "herld", 3, -1, true)
Case("Left at 6", T, "hello world", 6, -1, { { "Left" } }, "hello world", 5, -1, true)
Case("Left at 1", T, "hello world", 1, -1, { { "Left" } }, "hello world", 1, -1, true)
Case("Left sel 2..6 (cur 6)", T, "hello world", 6, 2, { { "Left" } }, "hello world", 2, -1, true)
Case("Left sel 6..2 (cur 2)", T, "hello world", 2, 6, { { "Left" } }, "hello world", 2, -1, true)
Case("Right at 6", T, "hello world", 6, -1, { { "Right" } }, "hello world", 7, -1, true)
Case("Right at 12", T, "hello world", 12, -1, { { "Right" } }, "hello world", 12, -1, true)
Case("Right sel 2..6 (cur 6)", T, "hello world", 6, 2, { { "Right" } }, "hello world", 6, -1, true)
Case("Right sel 6..2 (cur 2)", T, "hello world", 2, 6, { { "Right" } }, "hello world", 6, -1, true)
Case("Home at 6", T, "hello world", 6, -1, { { "Home" } }, "hello world", 1, -1, true)
Case("End at 6", T, "hello world", 6, -1, { { "End" } }, "hello world", 12, -1, true)
Case("Up at 6", T, "hello world", 6, -1, { { "Up" } }, "hello world", 6, -1, true)
Case("Down at 6", T, "hello world", 6, -1, { { "Down" } }, "hello world", 6, -1, true)
Case("Shift+Left at 6", T, "hello world", 6, -1, { { "Left", shift = true } }, "hello world", 5, 6, true)
Case("Shift+Left x2 at 6", T, "hello world", 6, -1, { { "Left", shift = true }, { "Left", shift = true } }, "hello world", 4, 6, true)
Case("Shift+Right at 6", T, "hello world", 6, -1, { { "Right", shift = true } }, "hello world", 7, 6, true)
Case("Shift+Right sel 3..6", T, "hello world", 6, 3, { { "Right", shift = true } }, "hello world", 7, 3, true)
Case("Shift+Left sel 3..6 x4", T, "hello world", 6, 3, { { "Left", shift = true }, { "Left", shift = true }, { "Left", shift = true }, { "Left", shift = true } }, "hello world", 2, 3, true)
Case("Shift+Home at 6", T, "hello world", 6, -1, { { "Home", shift = true } }, "hello world", 1, 6, true)
Case("Shift+End at 6", T, "hello world", 6, -1, { { "End", shift = true } }, "hello world", 12, 6, true)
Case("Ctrl+Left at 9", T, "hello world", 9, -1, { { "Left", ctrl = true } }, "hello world", 7, -1, true)
Case("Ctrl+Left at 7", T, "hello world", 7, -1, { { "Left", ctrl = true } }, "hello world", 1, -1, true)
Case("Ctrl+Left at 12", T, "hello world", 12, -1, { { "Left", ctrl = true } }, "hello world", 7, -1, true)
Case("Ctrl+Right at 1", T, "hello world", 1, -1, { { "Right", ctrl = true } }, "hello world", 7, -1, true)
Case("Ctrl+Right at 3", T, "hello world", 3, -1, { { "Right", ctrl = true } }, "hello world", 7, -1, true)
Case("Ctrl+Right at 6", T, "hello world", 6, -1, { { "Right", ctrl = true } }, "hello world", 7, -1, true)
Case("Ctrl+Right at 7", T, "hello world", 7, -1, { { "Right", ctrl = true } }, "hello world", 12, -1, true)
Case("Ctrl+Shift+Left at 12", T, "hello world", 12, -1, { { "Left", ctrl = true, shift = true } }, "hello world", 7, 12, true)
Case("Ctrl+Shift+Right at 1", T, "hello world", 1, -1, { { "Right", ctrl = true, shift = true } }, "hello world", 7, 1, true)
Case("Ctrl+Backspace at 12", T, "hello world", 12, -1, { { "Backspace", ctrl = true } }, "hello ", 7, -1, true)
Case("Ctrl+Backspace at 9", T, "hello world", 9, -1, { { "Backspace", ctrl = true } }, "hello rld", 7, -1, true)
Case("Ctrl+Backspace at 7", T, "hello world", 7, -1, { { "Backspace", ctrl = true } }, "world", 1, -1, true)
Case("Ctrl+Delete at 1", T, "hello world", 1, -1, { { "Delete", ctrl = true } }, " world", 1, -1, true)
Case("Ctrl+Delete at 6", T, "hello world", 6, -1, { { "Delete", ctrl = true } }, "hello", 6, -1, true)
Case("Ctrl+Delete at 3", T, "hello world", 3, -1, { { "Delete", ctrl = true } }, "he world", 3, -1, true)
Case("Ctrl+Home at 6", T, "hello world", 6, -1, { { "Home", ctrl = true } }, "hello world", 1, -1, true)
Case("Ctrl+End at 6", T, "hello world", 6, -1, { { "End", ctrl = true } }, "hello world", 12, -1, true)
Case("Ctrl+A at 6", T, "hello world", 6, -1, { { "A", ctrl = true } }, "hello world", 12, 1, true)
Case("Ctrl+A then Left", T, "hello world", 6, -1, { { "A", ctrl = true }, { "Left" } }, "hello world", 1, -1, true)
Case("Ctrl+A then Right", T, "hello world", 6, -1, { { "A", ctrl = true }, { "Right" } }, "hello world", 12, -1, true)
Case("punct Ctrl+Right at 1", T, "foo.bar  baz-qux", 1, -1, { { "Right", ctrl = true } }, "foo.bar  baz-qux", 10, -1, true)
Case("punct Ctrl+Right at 4", T, "foo.bar  baz-qux", 4, -1, { { "Right", ctrl = true } }, "foo.bar  baz-qux", 10, -1, true)
Case("punct Ctrl+Right at 5", T, "foo.bar  baz-qux", 5, -1, { { "Right", ctrl = true } }, "foo.bar  baz-qux", 10, -1, true)
Case("punct Ctrl+Right at 8", T, "foo.bar  baz-qux", 8, -1, { { "Right", ctrl = true } }, "foo.bar  baz-qux", 10, -1, true)
Case("punct Ctrl+Right at 12", T, "foo.bar  baz-qux", 12, -1, { { "Right", ctrl = true } }, "foo.bar  baz-qux", 17, -1, true)
Case("punct Ctrl+Left at 17", T, "foo.bar  baz-qux", 17, -1, { { "Left", ctrl = true } }, "foo.bar  baz-qux", 10, -1, true)
Case("punct Ctrl+Left at 14", T, "foo.bar  baz-qux", 14, -1, { { "Left", ctrl = true } }, "foo.bar  baz-qux", 10, -1, true)
Case("punct Ctrl+Left at 11", T, "foo.bar  baz-qux", 11, -1, { { "Left", ctrl = true } }, "foo.bar  baz-qux", 10, -1, true)
Case("punct Ctrl+Left at 8", T, "foo.bar  baz-qux", 8, -1, { { "Left", ctrl = true } }, "foo.bar  baz-qux", 1, -1, true)
Case("punct Ctrl+Backspace at 8", T, "foo.bar  baz-qux", 8, -1, { { "Backspace", ctrl = true } }, "  baz-qux", 1, -1, true)
Case("punct Ctrl+Backspace at 11", T, "foo.bar  baz-qux", 11, -1, { { "Backspace", ctrl = true } }, "foo.bar  az-qux", 10, -1, true)
Case("punct Ctrl+Delete at 4", T, "foo.bar  baz-qux", 4, -1, { { "Delete", ctrl = true } }, "foo  baz-qux", 4, -1, true)
Case("punct Ctrl+Delete at 8", T, "foo.bar  baz-qux", 8, -1, { { "Delete", ctrl = true } }, "foo.bar", 8, -1, true)
Case("utf8 Backspace at 4", T, "h\u{e9}llo w\u{f6}rld", 4, -1, { { "Backspace" } }, "hllo w\195\182rld", 2, -1, true)
Case("utf8 Delete at 2", T, "h\u{e9}llo w\u{f6}rld", 2, -1, { { "Delete" } }, "hllo w\195\182rld", 2, -1, true)
Case("utf8 Left at 4", T, "h\u{e9}llo w\u{f6}rld", 4, -1, { { "Left" } }, "h\195\169llo w\195\182rld", 2, -1, true)
Case("utf8 Right at 2", T, "h\u{e9}llo w\u{f6}rld", 2, -1, { { "Right" } }, "h\195\169llo w\195\182rld", 4, -1, true)
Case("utf8 Ctrl+Left at 15", T, "h\u{e9}llo w\u{f6}rld", 15, -1, { { "Left", ctrl = true } }, "h\195\169llo w\195\182rld", 8, -1, true)
Case("Tab at 6", T, "hello world", 6, -1, { { "Tab" } }, "hello world", 6, -1, true)
Case("Escape at 6", T, "hello world", 6, -1, { { "Escape" } }, "hello world", -1, -1, false)
Case("Insert then type", T, "hello world", 6, -1, { { "Insert" }, "X" }, "helloX world", 7, -1, true)
Case("type newline single-line", T, "hello world", 6, -1, { "\n" }, "hello\n world", 7, -1, true)
Case("type tab char", T, "hello world", 6, -1, { "\t" }, "hello\t world", 7, -1, true)
Case("Return single-line", T, "hello world", 6, -1, { { "Return" } }, "hello world", -1, -1, false)
Case("KeypadEnter single-line", T, "hello world", 6, -1, { { "KeypadEnter" } }, "hello world", -1, -1, false)
Case("Shift+Return single-line", T, "hello world", 6, -1, { { "Return", shift = true } }, "hello world", -1, -1, false)
Case("empty type a", E, "", 1, -1, { "a" }, "a", 2, -1, true)
Case("empty Backspace", E, "", 1, -1, { { "Backspace" } }, "", 1, -1, true)
Case("empty Left", E, "", 1, -1, { { "Left" } }, "", 1, -1, true)
Case("empty Return", E, "", 1, -1, { { "Return" } }, "", -1, -1, false)
Case("type CR", T, "hello world", 6, -1, { "\r" }, "hello\r world", 7, -1, true)
Case("type CRLF", T, "hello world", 6, -1, { "\r\n" }, "hello\r\n world", 8, -1, true)
Case("Shift+Left collapse 3", T, "hello world", 6, 3, { { "Right", shift = true }, { "Left", shift = true }, { "Left", shift = true }, { "Left", shift = true }, { "Left", shift = true }, { "Left", shift = true } }, "hello world", 2, 3, true)
Case("Up with sel", T, "hello world", 6, 3, { { "Up" } }, "hello world", 6, -1, true)
Case("Shift+Up at 6", T, "hello world", 6, -1, { { "Up", shift = true } }, "hello world", 6, 6, true)
Case("Shift+Down at 6", T, "hello world", 6, -1, { { "Down", shift = true } }, "hello world", 6, 6, true)
Case("Escape with sel", T, "hello world", 6, 3, { { "Escape" } }, "hello world", -1, -1, false)
T.TextEditable = false
Case("RO type X", T, "hello world", 6, -1, { "X" }, "hello world", 6, -1, true)
Case("RO Backspace", T, "hello world", 6, -1, { { "Backspace" } }, "hello world", 6, -1, true)
Case("RO Delete", T, "hello world", 6, -1, { { "Delete" } }, "hello world", 6, -1, true)
Case("RO Left", T, "hello world", 6, -1, { { "Left" } }, "hello world", 5, -1, true)
Case("RO Shift+Right", T, "hello world", 6, -1, { { "Right", shift = true } }, "hello world", 7, 6, true)
Case("RO Ctrl+A", T, "hello world", 6, -1, { { "A", ctrl = true } }, "hello world", 12, 1, true)
Case("RO Return", T, "hello world", 6, -1, { { "Return" } }, "hello world", -1, -1, false)
T.TextEditable = true
assert(#Failures == 0, table.concat(Failures, "\n"))
-- Escape releases the box as a click elsewhere does: FocusLost(false, the key) before UserInputService's
-- InputBegan (processed), whose InputEnded is no longer processed
local UIS = game:GetService("UserInputService")
local Log = {}
T.FocusLost:Connect(function(Enter, Input) table.insert(Log, `FocusLost {Enter} {Input and Input.KeyCode.Name}`) end)
UIS.InputBegan:Connect(function(Input, Processed) table.insert(Log, `began {Input.KeyCode.Name} {Processed}`) end)
UIS.InputEnded:Connect(function(Input, Processed) table.insert(Log, `ended {Input.KeyCode.Name} {Processed}`) end)
T:CaptureFocus()
Flush()
Press("Escape")
Flush()
local Joined = table.concat(Log, ",")
assert(Joined == "FocusLost false Escape,began Escape true,ended Escape false", Joined)
"##
        ),
    );
}

/// MultiLine boxes (the same capture): Return, Home / End on a line and on a wrapped paragraph's lines, Up / Down by
/// the caret's x (SourceSans 32), staying put on the first and last lines. Needs Studio's fonts.
#[test]
fn textbox_multiline_keys_match_a_client() {
    if crate::engine::text::fonts::discover_content_root().is_none() {
        eprintln!("skipped: no Studio content root (the cases need Studio's fonts)");
        return;
    }
    let (lua, engine) = start(900.0, 600.0);
    run(
        &lua,
        &engine,
        &format!(
            "{}{}",
            TEXTBOX_PRELUDE,
            r##"
Case("ML Return at 3", M, "ab cd\nefgh\nij", 3, -1, { { "Return" } }, "ab\n cd\nefgh\nij", 4, -1, true)
Case("ML Shift+Return at 3", M, "ab cd\nefgh\nij", 3, -1, { { "Return", shift = true } }, "ab\n cd\nefgh\nij", 4, -1, true)
Case("ML type newline at 3", M, "ab cd\nefgh\nij", 3, -1, { "\n" }, "ab\n cd\nefgh\nij", 4, -1, true)
Case("ML Home at 9", M, "ab cd\nefgh\nij", 9, -1, { { "Home" } }, "ab cd\nefgh\nij", 7, -1, true)
Case("ML End at 9", M, "ab cd\nefgh\nij", 9, -1, { { "End" } }, "ab cd\nefgh\nij", 11, -1, true)
Case("ML Ctrl+Home at 9", M, "ab cd\nefgh\nij", 9, -1, { { "Home", ctrl = true } }, "ab cd\nefgh\nij", 1, -1, true)
Case("ML Ctrl+End at 9", M, "ab cd\nefgh\nij", 9, -1, { { "End", ctrl = true } }, "ab cd\nefgh\nij", 14, -1, true)
Case("ML Up at 9", M, "ab cd\nefgh\nij", 9, -1, { { "Up" } }, "ab cd\nefgh\nij", 2, -1, true)
Case("ML Up at 3", M, "ab cd\nefgh\nij", 3, -1, { { "Up" } }, "ab cd\nefgh\nij", 3, -1, true)
Case("ML Down at 3", M, "ab cd\nefgh\nij", 3, -1, { { "Down" } }, "ab cd\nefgh\nij", 10, -1, true)
Case("ML Down at 9", M, "ab cd\nefgh\nij", 9, -1, { { "Down" } }, "ab cd\nefgh\nij", 14, -1, true)
Case("ML Down at 13", M, "ab cd\nefgh\nij", 13, -1, { { "Down" } }, "ab cd\nefgh\nij", 13, -1, true)
Case("ML Shift+Down at 3", M, "ab cd\nefgh\nij", 3, -1, { { "Down", shift = true } }, "ab cd\nefgh\nij", 10, 3, true)
Case("ML Left at 7", M, "ab cd\nefgh\nij", 7, -1, { { "Left" } }, "ab cd\nefgh\nij", 6, -1, true)
Case("ML Ctrl+Right at 4", M, "ab cd\nefgh\nij", 4, -1, { { "Right", ctrl = true } }, "ab cd\nefgh\nij", 7, -1, true)
Case("ML Ctrl+Left at 7", M, "ab cd\nefgh\nij", 7, -1, { { "Left", ctrl = true } }, "ab cd\nefgh\nij", 4, -1, true)
Case("ML Ctrl+Backspace at 7", M, "ab cd\nefgh\nij", 7, -1, { { "Backspace", ctrl = true } }, "ab efgh\nij", 4, -1, true)
Case("ML Tab at 3", M, "ab cd\nefgh\nij", 3, -1, { { "Tab" } }, "ab cd\nefgh\nij", 3, -1, true)
Case("ML Up at 3 with shift", M, "ab cd\nefgh\nij", 3, -1, { { "Up", shift = true } }, "ab cd\nefgh\nij", 3, 3, true)
Case("ML Down at 13 with shift", M, "ab cd\nefgh\nij", 13, -1, { { "Down", shift = true } }, "ab cd\nefgh\nij", 13, 13, true)
Case("ML Up at 12", M, "ab cd\nefgh\nij", 12, -1, { { "Up" } }, "ab cd\nefgh\nij", 7, -1, true)
Case("ML Up at 14", M, "ab cd\nefgh\nij", 14, -1, { { "Up" } }, "ab cd\nefgh\nij", 8, -1, true)
Case("ML Up Up at 14", M, "ab cd\nefgh\nij", 14, -1, { { "Up" }, { "Up" } }, "ab cd\nefgh\nij", 2, -1, true)
Case("ML Down Down at 2", M, "ab cd\nefgh\nij", 2, -1, { { "Down" }, { "Down" } }, "ab cd\nefgh\nij", 14, -1, true)
Case("ML Down at 5", M, "ab cd\nefgh\nij", 5, -1, { { "Down" } }, "ab cd\nefgh\nij", 11, -1, true)
Case("ML Left with sel", M, "ab cd\nefgh\nij", 9, 3, { { "Left" } }, "ab cd\nefgh\nij", 3, -1, true)
Case("ML Up with sel", M, "ab cd\nefgh\nij", 9, 3, { { "Up" } }, "ab cd\nefgh\nij", 2, -1, true)
Case("ML Down with sel", M, "ab cd\nefgh\nij", 3, 9, { { "Down" } }, "ab cd\nefgh\nij", 10, -1, true)
Case("ML Home at 7", M, "ab cd\nefgh\nij", 7, -1, { { "Home" } }, "ab cd\nefgh\nij", 7, -1, true)
Case("ML End at 6", M, "ab cd\nefgh\nij", 6, -1, { { "End" } }, "ab cd\nefgh\nij", 6, -1, true)
Case("ML Escape", M, "ab cd\nefgh\nij", 3, -1, { { "Escape" } }, "ab cd\nefgh\nij", -1, -1, false)
M:ReleaseFocus()
M.TextWrapped = true
M.Size = UDim2.fromOffset(200, 200)
UpdateAll()
Case("ML wrapped End at 2", M, "one two three four five six seven eight nine ten eleven twelve", 2, -1, { { "End" } }, "one two three four five six seven eight nine ten eleven twelve", 19, -1, true)
Case("ML wrapped Home at 20", M, "one two three four five six seven eight nine ten eleven twelve", 20, -1, { { "Home" } }, "one two three four five six seven eight nine ten eleven twelve", 20, -1, true)
-- (not matched: the client puts the caret at 21, after "f"; the nearest place to the caret's x is 22)
-- Case("ML wrapped Down at 2", M, "one two three four five six seven eight nine ten eleven twelve", 2, -1, { { "Down" } }, "one two three four five six seven eight nine ten eleven twelve", 21, -1, true)
Case("ML wrapped Up at 20", M, "one two three four five six seven eight nine ten eleven twelve", 20, -1, { { "Up" } }, "one two three four five six seven eight nine ten eleven twelve", 1, -1, true)
assert(#Failures == 0, table.concat(Failures, "\n"))
"##
        ),
    );
}

const TEXTBOX_PRELUDE: &str = r##"
local Root = Instance.new("ScreenGui", game:GetService("CoreGui"))
Root.IgnoreGuiInset = true
local function Box(Name, Y, H, Size, MultiLine)
	local B = Instance.new("TextBox")
	B.Name = Name
	B.Position = UDim2.fromOffset(100, Y)
	B.Size = UDim2.fromOffset(600, H)
	B.Font = Enum.Font.SourceSans
	B.TextSize = Size
	B.TextXAlignment = Enum.TextXAlignment.Left
	B.TextYAlignment = if MultiLine then Enum.TextYAlignment.Top else Enum.TextYAlignment.Center
	B.ClearTextOnFocus = false
	B.MultiLine = MultiLine
	B.Text = ""
	B.Parent = Root
	return B
end
local T = Box("T", 100, 80, 48, false)
local M = Box("M", 250, 200, 32, true)
local E = Box("E", 480, 60, 32, false)
E.PlaceholderText = "ph"
UpdateAll()
local function Key(Name, Down)
	Dispatch({ type = "key", key = Name, down = Down })
end
local function Press(Name, Shift, Ctrl)
	if Shift then Key("LeftShift", true) end
	if Ctrl then Key("LeftControl", true) end
	Key(Name, true)
	Key(Name, false)
	if Ctrl then Key("LeftControl", false) end
	if Shift then Key("LeftShift", false) end
end
local Failures = {}
local function Case(Label, B, Text, Cursor, Selection, Steps, WantText, WantCursor, WantSelection, WantFocused)
	if not B:IsFocused() then B:CaptureFocus() end
	B.Text = Text
	B.CursorPosition = Cursor
	B.SelectionStart = Selection
	UpdateAll()
	for _, Step in Steps do
		if type(Step) == "string" then
			Dispatch({ type = "text", text = Step })
		else
			Press(Step[1], Step.shift, Step.ctrl)
		end
		UpdateAll()
	end
	Flush()
	local Got = string.format("%q %d %d %s", B.Text, B.CursorPosition, B.SelectionStart, tostring(B:IsFocused()))
	local Want = string.format("%q %d %d %s", WantText, WantCursor, WantSelection, tostring(WantFocused))
	if Got ~= Want then table.insert(Failures, `{Label}: {Got}, the client {Want}`) end
end
"##;

/// The wheel's gameProcessedEvent when nothing scrolls, measured in a 0.747 client (wheel notches injected with
/// VirtualInputManager over each object): an Active object or a ScrollingFrame with ScrollingEnabled under the cursor
/// makes it processed, whether that frame can move or not (Active or not); ScrollingEnabled = false does not.
#[test]
fn wheel_game_processed_matches_a_client() {
    let (lua, engine) = start(800.0, 600.0);
    run(
        &lua,
        &engine,
        r##"
local UIS = game:GetService("UserInputService")
local Root = Instance.new("ScreenGui", game:GetService("CoreGui"))
Root.IgnoreGuiInset = true
local function Cell(Name, Class, X, Y, Props)
	local O = Instance.new(Class)
	O.Name = Name
	O.Position = UDim2.fromOffset(X, Y)
	O.Size = UDim2.fromOffset(80, 80)
	for K, V in Props or {} do
		O[K] = V
	end
	O.Parent = Root
	return O
end
Cell("Plain", "Frame", 100, 100, { Active = false })
Cell("ActiveF", "Frame", 190, 100, { Active = true })
Cell("Btn", "TextButton", 280, 100)
Cell("BtnInactive", "TextButton", 370, 100, { Active = false })
Cell("Label", "TextLabel", 460, 100)
Cell("Box", "TextBox", 550, 100)
Cell("SFStuck", "ScrollingFrame", 640, 100, { CanvasSize = UDim2.fromOffset(0, 0) })
Cell("SFScroll", "ScrollingFrame", 100, 300, { CanvasSize = UDim2.fromOffset(0, 400) })
Cell("SFDisabled", "ScrollingFrame", 190, 300, { CanvasSize = UDim2.fromOffset(0, 400), ScrollingEnabled = false })
Cell("SFInactive", "ScrollingFrame", 280, 300, { CanvasSize = UDim2.fromOffset(0, 0), Active = false })
local AtEnd = Cell("SFAtEnd", "ScrollingFrame", 370, 300, { CanvasSize = UDim2.fromOffset(0, 400) })
local Under = Cell("SFUnder", "ScrollingFrame", 460, 300, { CanvasSize = UDim2.fromOffset(0, 0), Active = false })
local Over = Instance.new("Frame")
Over.Size = UDim2.fromOffset(40, 40)
Over.Active = true
Over.Parent = Under
UpdateAll()
AtEnd.CanvasPosition = Vector2.new(0, 10000)
UpdateAll()
local Processed
UIS.InputChanged:Connect(function(Input, P)
	if Input.UserInputType == Enum.UserInputType.MouseWheel then Processed = P end
end)
local Got = {}
for _, Spec in {
	{ "Empty", 20, 110 }, { "Plain", 140, 140 }, { "ActiveF", 230, 140 }, { "Btn", 320, 140 }, { "BtnInactive", 410, 140 },
	{ "Label", 500, 140 }, { "Box", 590, 140 }, { "SFStuck", 680, 140 }, { "SFScroll", 140, 340 }, { "SFDisabled", 230, 340 },
	{ "SFInactive", 320, 340 }, { "SFAtEnd", 410, 340 }, { "ActiveOver", 470, 310 }, { "SFUnder", 530, 370 },
} do
	Processed = nil
	Dispatch({ type = "move", x = Spec[2], y = Spec[3] })
	Dispatch({ type = "wheel", x = Spec[2], y = Spec[3], notches = -1 })
	Flush()
	table.insert(Got, `{Spec[1]} {Processed}`)
end
local Joined = table.concat(Got, ",")
local Client = "Empty false,Plain false,ActiveF true,Btn true,BtnInactive false,Label false,Box true,SFStuck true,SFScroll true,SFDisabled false,SFInactive true,SFAtEnd true,ActiveOver true,SFUnder true"
assert(Joined == Client, Joined)
"##,
    );
}

/// InputActions get only input the GUI did not process, measured in a 0.747 client: a key a sinking
/// ContextActionService bind takes, a click a GuiButton / TextBox / Active object takes and Return on the selected
/// button reach no action; a press that reached one is released wherever it ends.
#[test]
fn input_actions_get_only_unprocessed_input() {
    let (lua, engine) = start(800.0, 600.0);
    run(
        &lua,
        &engine,
        r##"
local CAS = game:GetService("ContextActionService")
local Log = {}
local Context = Instance.new("InputContext")
local function Action(Label, KeyCode)
	local A = Instance.new("InputAction")
	A.Name = Label
	local B = Instance.new("InputBinding")
	B.KeyCode = KeyCode
	B.Parent = A
	A.Parent = Context
	A.Pressed:Connect(function() table.insert(Log, Label .. ".Pressed") end)
	A.Released:Connect(function() table.insert(Log, Label .. ".Released") end)
end
Action("E", Enum.KeyCode.E)
Action("R", Enum.KeyCode.R)
Action("L", Enum.KeyCode.MouseLeftButton)
Action("Ret", Enum.KeyCode.Return)
Context.Parent = game:GetService("ReplicatedStorage")
CAS:BindAction("SinkE", function() return Enum.ContextActionResult.Sink end, false, Enum.KeyCode.E)
CAS:BindAction("PassR", function() return Enum.ContextActionResult.Pass end, false, Enum.KeyCode.R)
local function Key(Name)
	Dispatch({ type = "key", key = Name, down = true })
	Flush()
	Dispatch({ type = "key", key = Name, down = false })
	Flush()
end
local function Click(Label, X, Y, X2, Y2)
	table.insert(Log, Label)
	Dispatch({ type = "down", x = X, y = Y, button = 1 })
	Flush()
	Dispatch({ type = "up", x = X2 or X, y = Y2 or Y, button = 1 })
	Flush()
end
Key("E")
Key("R")
CAS:UnbindAction("SinkE")
Key("E")
local Gui = Instance.new("ScreenGui")
Gui.IgnoreGuiInset = true
local function Make(Class, Name, X, Props)
	local O = Instance.new(Class)
	O.Name = Name
	O.Position = UDim2.fromOffset(X, 100)
	O.Size = UDim2.fromOffset(100, 100)
	for K, V in Props or {} do
		O[K] = V
	end
	O.Parent = Gui
	return O
end
local Btn = Make("TextButton", "Btn", 100)
Make("TextButton", "BtnInactive", 220, { Active = false })
Make("Frame", "ActiveF", 340, { Active = true })
Make("Frame", "PlainF", 460, { Active = false })
local Box = Make("TextBox", "Box", 580, { ClearTextOnFocus = false })
Gui.Parent = game:GetService("Players").LocalPlayer:WaitForChild("PlayerGui")
UpdateAll()
Click("Btn", 150, 150)
Click("BtnInactive", 270, 150)
Click("ActiveF", 390, 150)
Click("PlainF", 510, 150)
Click("empty", 700, 400)
Click("Box", 630, 150)
Box:ReleaseFocus()
Click("empty then Btn", 700, 400, 150, 150)
table.insert(Log, "selected Return")
game:GetService("GuiService").SelectedObject = Btn
Flush()
Key("Return")
local Joined = table.concat(Log, ",")
local Client = "R.Pressed,R.Released,E.Pressed,E.Released,Btn,BtnInactive,ActiveF,PlainF,L.Pressed,L.Released,empty,L.Pressed,L.Released,Box,empty then Btn,L.Pressed,L.Released,selected Return"
assert(Joined == Client, Joined)
"##,
    );
}

/// Hit-testing skips what a 0.747 client culls (GetGuiObjectsAtPosition there, mouse input agreeing; the layout of
/// the capture): AutomaticSize objects whose centre is not strictly inside their clip (turned or not, not in a
/// scrolled ScrollingFrame; a text object keeps itself) and their own children (unless a layout places them or they
/// are turned; grandchildren stay), and turned objects whose unturned rect lies outside their clip.
#[test]
fn hit_testing_skips_what_a_client_culls() {
    let (lua, engine) = start(800.0, 600.0);
    run(
        &lua,
        &engine,
        r##"
local Root = Instance.new("ScreenGui", game:GetService("CoreGui"))
Root.IgnoreGuiInset = true
Root.ZIndexBehavior = Enum.ZIndexBehavior.Sibling
local function Make(Class, Parent, Props)
	local O = Instance.new(Class)
	for K, V in Props do
		O[K] = V
	end
	O.Parent = Parent
	return O
end
local X = Enum.AutomaticSize.X
local C1 = Make("Frame", Root, { Name = "C1", Position = UDim2.fromOffset(100, 100), Size = UDim2.fromOffset(200, 200), ClipsDescendants = true })
local A1 = Make("Frame", C1, { Name = "A1", Position = UDim2.fromOffset(-100, 20), Size = UDim2.fromOffset(150, 40), AutomaticSize = X })
Make("TextButton", A1, { Name = "A1B", Position = UDim2.fromOffset(100, 0), Size = UDim2.fromOffset(50, 40), Text = "" })
local N1 = Make("Frame", C1, { Name = "N1", Position = UDim2.fromOffset(-100, 80), Size = UDim2.fromOffset(150, 40) })
Make("TextButton", N1, { Name = "N1B", Position = UDim2.fromOffset(100, 0), Size = UDim2.fromOffset(50, 40), Text = "" })
local A2 = Make("Frame", C1, { Name = "A2", Position = UDim2.fromOffset(20, 170), Size = UDim2.fromOffset(60, 60), AutomaticSize = Enum.AutomaticSize.Y })
Make("TextButton", A2, { Name = "A2B", Size = UDim2.fromOffset(60, 30), Text = "" })
local A3 = Make("Frame", C1, { Name = "A3", Position = UDim2.fromOffset(160, 20), Size = UDim2.fromOffset(30, 30), AutomaticSize = Enum.AutomaticSize.XY })
Make("TextButton", A3, { Name = "A3B", Size = UDim2.fromOffset(30, 30), Text = "" })
Make("TextButton", C1, { Name = "AT", Position = UDim2.fromOffset(-100, 140), Size = UDim2.fromOffset(150, 30), AutomaticSize = X, Text = "x" })
local AG = Make("Frame", C1, { Name = "AG", Position = UDim2.fromOffset(150, 150), Size = UDim2.fromOffset(150, 40), AutomaticSize = X, BackgroundTransparency = 1 })
Make("UIGridLayout", AG, { CellSize = UDim2.fromOffset(40, 40) })
Make("TextButton", AG, { Name = "AGB", Text = "" })
local C2 = Make("Frame", Root, { Name = "C2", Position = UDim2.fromOffset(400, 100), Size = UDim2.fromOffset(200, 200), ClipsDescendants = true })
Make("TextButton", C2, { Name = "R1", Position = UDim2.fromOffset(10, -30), Size = UDim2.fromOffset(180, 8), Rotation = 45, Text = "" })
Make("TextButton", C2, { Name = "R2", Position = UDim2.fromOffset(10, 150), Size = UDim2.fromOffset(180, 8), Rotation = 45, Text = "" })
local AR = Make("Frame", C2, { Name = "AR", Position = UDim2.fromOffset(-100, 100), Size = UDim2.fromOffset(150, 40), AutomaticSize = X, Rotation = 10 })
Make("TextButton", AR, { Name = "ARB", Position = UDim2.fromOffset(100, 0), Size = UDim2.fromOffset(50, 40), Text = "" })
local S = Make("ScrollingFrame", Root, { Name = "S", Position = UDim2.fromOffset(100, 350), Size = UDim2.fromOffset(200, 150), CanvasSize = UDim2.fromOffset(0, 600), ScrollBarThickness = 0 })
local AS = Make("Frame", S, { Name = "AS", Position = UDim2.fromOffset(-100, 80), Size = UDim2.fromOffset(150, 40), AutomaticSize = X })
Make("TextButton", AS, { Name = "ASB", Position = UDim2.fromOffset(100, 0), Size = UDim2.fromOffset(50, 40), Text = "" })
UpdateAll()
S.CanvasPosition = Vector2.new(0, 50)
UpdateAll()
local Got = {}
local function At(Label, PX, PY)
	local Names = {}
	for _, O in HitAt(PX, PY) do
		table.insert(Names, O.Name)
	end
	table.insert(Got, `{Label} [{table.concat(Names, ",")}]`)
end
At("A1B", 125, 140)
At("N1B", 125, 200)
At("A2B", 150, 285)
At("A3B", 275, 135)
At("AT", 125, 255)
At("AGB", 270, 270)
At("R1 in clip", 535, 109)
At("R1 above clip", 500, 74)
At("R2 in clip", 465, 219)
At("R2 below clip", 549, 303)
At("ARB", 425, 225)
At("ASB", 125, 400)
local Joined = table.concat(Got, "; ")
local Client = "A1B [C1]; N1B [N1B,N1,C1]; A2B [C1]; A3B [A3B,A3,C1]; AT [AT,C1]; AGB [AGB,C1]; R1 in clip [C2]; R1 above clip []; R2 in clip [R2,C2]; R2 below clip []; ARB [ARB,C2]; ASB [ASB,AS,S]"
assert(Joined == Client, Joined)
-- Which descendants a culled AutomaticSize object takes with it
Root:ClearAllChildren()
local function Clip(Name, PX, PY, Class)
	return Make(Class or "Frame", Root, { Name = Name, Position = UDim2.fromOffset(PX, PY), Size = UDim2.fromOffset(150, 150), ClipsDescendants = true })
end
local D1 = Clip("C1", 20, 20)
local AText = Make("TextLabel", D1, { Name = "AText", Position = UDim2.fromOffset(-100, 20), Size = UDim2.fromOffset(150, 40), AutomaticSize = X, Text = "t" })
Make("TextButton", AText, { Name = "ATextB", Position = UDim2.fromOffset(100, 0), Size = UDim2.fromOffset(50, 40), Text = "" })
local AGrand = Make("Frame", D1, { Name = "AGrand", Position = UDim2.fromOffset(-100, 80), Size = UDim2.fromOffset(150, 40), AutomaticSize = X })
local Mid = Make("Frame", AGrand, { Name = "Mid", Position = UDim2.fromOffset(90, 0), Size = UDim2.fromOffset(60, 40) })
Make("TextButton", Mid, { Name = "GrandB", Position = UDim2.fromOffset(10, 0), Size = UDim2.fromOffset(50, 40), Text = "" })
local D2 = Clip("C2", 200, 20)
local ACanvas = Make("CanvasGroup", D2, { Name = "ACanvas", Position = UDim2.fromOffset(-100, 20), Size = UDim2.fromOffset(150, 40), AutomaticSize = X })
Make("UIGridLayout", ACanvas, { CellSize = UDim2.fromOffset(150, 40) })
Make("TextButton", ACanvas, { Name = "ACanvasB", Text = "" })
local AImg = Make("ImageButton", D2, { Name = "AImgBtn", Position = UDim2.fromOffset(-100, 80), Size = UDim2.fromOffset(150, 40), AutomaticSize = X })
Make("Frame", AImg, { Name = "AImgChild", Position = UDim2.fromOffset(100, 0), Size = UDim2.fromOffset(50, 40) })
local S0 = Make("ScrollingFrame", Root, { Name = "S0", Position = UDim2.fromOffset(380, 20), Size = UDim2.fromOffset(150, 150), CanvasSize = UDim2.fromOffset(0, 600), ScrollBarThickness = 0 })
local AS0 = Make("Frame", S0, { Name = "AS0", Position = UDim2.fromOffset(-100, 20), Size = UDim2.fromOffset(150, 40), AutomaticSize = X })
Make("TextButton", AS0, { Name = "AS0B", Position = UDim2.fromOffset(100, 0), Size = UDim2.fromOffset(50, 40), Text = "" })
local AEdge = Make("Frame", Root, { Name = "AEdge", Position = UDim2.fromOffset(-100, 300), Size = UDim2.fromOffset(150, 40), AutomaticSize = X })
Make("TextButton", AEdge, { Name = "AEdgeB", Position = UDim2.fromOffset(100, 0), Size = UDim2.fromOffset(50, 40), Text = "" })
local D3 = Clip("C3", 560, 20)
local ATurnKid = Make("Frame", D3, { Name = "ATurnKid", Position = UDim2.fromOffset(-100, 20), Size = UDim2.fromOffset(150, 40), AutomaticSize = X })
Make("TextButton", ATurnKid, { Name = "TurnedB", Position = UDim2.fromOffset(100, 0), Size = UDim2.fromOffset(50, 40), Rotation = 5, Text = "" })
local AR3 = Make("Frame", D3, { Name = "AR", Position = UDim2.fromOffset(-100, 80), Size = UDim2.fromOffset(150, 40), AutomaticSize = X, Rotation = 3 })
Make("TextButton", AR3, { Name = "ARB", Position = UDim2.fromOffset(100, 0), Size = UDim2.fromOffset(50, 40), Text = "" })
Make("Frame", D3, { Name = "TurnedInside", Position = UDim2.fromOffset(20, 125), Size = UDim2.fromOffset(60, 20), AutomaticSize = X, Rotation = 3 })
UpdateAll()
table.clear(Got)
At("ATextB", 45, 60)
At("AText", 25, 60)
At("GrandB", 45, 120)
At("ACanvasB", 200, 60)
At("AImgChild", 225, 120)
At("AS0B", 405, 60)
At("AEdgeB", 25, 320)
At("AEdge", 5, 320)
At("TurnedB", 585, 60)
At("ARB", 584.931, 122.617)
At("AR", 585, 120)
At("TurnedInside", 610, 155)
Joined = table.concat(Got, "; ")
Client = "ATextB [AText,C1]; AText [AText,C1]; GrandB [GrandB,C1]; ACanvasB [ACanvasB,C2]; AImgChild [C2]; AS0B [S0]; AEdgeB [AEdgeB,AEdge]; AEdge [AEdgeB,AEdge]; TurnedB [TurnedB,C3]; ARB [ARB,C3]; AR [ARB,C3]; TurnedInside [TurnedInside,C3]"
assert(Joined == Client, Joined)
"##,
    );
}

fn start_with(width: f64, height: f64, mode: &str, device: Option<roblox_engine_emulator::client_profile::DeviceKind>) -> (Lua, Rc<Engine>) {
    let mut options = crate::engine::options();
    options.profile.device.viewport = Some((width, height));
    options.profile.device.kind = device;
    options.mode = mode.to_string();
    let engine = crate::engine::new_engine(options).expect("engine");
    let lua = Lua::new();
    engine_lua::start(&lua, &engine).expect("start");
    engine.state.borrow_mut().sched.set_main_thread(&lua.current_thread());
    (lua, engine)
}

/// UIDragDetector in a playing client (0.747, mouse drags injected with VirtualInputManager): DragStart before
/// InputBegan and the press processed; every move translates by the whole delta from the press; a GuiButton with a
/// detector is neither clicked nor activated (it is once the detector is disabled); Rotate turns by the angle the
/// mouse swept about the centre.
#[test]
fn drag_detector_in_play_mode_matches_a_client() {
    let (lua, engine) = start_with(800.0, 600.0, "play", None);
    run(
        &lua,
        &engine,
        r##"
local UIS = game:GetService("UserInputService")
local Root = Instance.new("ScreenGui")
Root.IgnoreGuiInset = true
local Log = {}
local function Make(Class, Props)
	local O = Instance.new(Class)
	for K, V in Props do
		O[K] = V
	end
	O.Parent = Root
	return O
end
local P = Make("Frame", { Name = "P", Position = UDim2.fromOffset(300, 200), Size = UDim2.fromOffset(100, 100) })
local D = Instance.new("UIDragDetector")
D.Parent = P
D.DragStart:Connect(function(At) table.insert(Log, `DragStart {At.X},{At.Y}`) end)
D.DragContinue:Connect(function(At) table.insert(Log, `DragContinue {At.X},{At.Y}`) end)
D.DragEnd:Connect(function(At) table.insert(Log, `DragEnd {At.X},{At.Y}`) end)
P.InputBegan:Connect(function(Input) if Input.UserInputType == Enum.UserInputType.MouseButton1 then table.insert(Log, "P.InputBegan") end end)
UIS.InputBegan:Connect(function(Input, Processed) if Input.UserInputType == Enum.UserInputType.MouseButton1 then table.insert(Log, `UIS.InputBegan {Processed}`) end end)
UIS.InputEnded:Connect(function(Input, Processed) if Input.UserInputType == Enum.UserInputType.MouseButton1 then table.insert(Log, `UIS.InputEnded {Processed}`) end end)
local B = Make("TextButton", { Name = "B", Position = UDim2.fromOffset(100, 400), Size = UDim2.fromOffset(100, 60) })
local BD = Instance.new("UIDragDetector")
BD.Parent = B
BD.DragStart:Connect(function(At) table.insert(Log, `DragStart {At.X},{At.Y}`) end)
BD.DragContinue:Connect(function(At) table.insert(Log, `DragContinue {At.X},{At.Y}`) end)
BD.DragEnd:Connect(function(At) table.insert(Log, `DragEnd {At.X},{At.Y}`) end)
B.Activated:Connect(function() table.insert(Log, "B.Activated") end)
B.MouseButton1Click:Connect(function() table.insert(Log, "B.Click") end)
Root.Parent = game:GetService("Players").LocalPlayer:WaitForChild("PlayerGui")
UpdateAll()
local function Drag(Points)
	Dispatch({ type = "down", x = Points[1][1], y = Points[1][2], button = 1 })
	Flush()
	for I = 2, #Points do
		Dispatch({ type = "move", x = Points[I][1], y = Points[I][2] })
		StepDisplay(1 / 60)
		Flush()
		UpdateAll()
	end
	local Last = Points[#Points]
	Dispatch({ type = "up", x = Last[1], y = Last[2], button = 1 })
	Flush()
end
Drag({ { 350, 250 }, { 352, 250 }, { 380, 270 } })
assert(P.Position == UDim2.fromOffset(330, 220) and D.DragUDim2 == UDim2.fromOffset(30, 20), `{P.Position} {D.DragUDim2}`)
local Joined = table.concat(Log, ",")
assert(Joined == "DragStart 350,250,P.InputBegan,UIS.InputBegan true,DragContinue 352,250,DragContinue 380,270,DragEnd 380,270,UIS.InputEnded true", Joined)
table.clear(Log)
Drag({ { 150, 430 }, { 170, 440 } })
Drag({ { 170, 440 } })
assert(B.Position == UDim2.fromOffset(120, 410), tostring(B.Position))
BD.Enabled = false
Drag({ { 190, 450 } })
Joined = table.concat(Log, ",")
assert(Joined == "DragStart 150,430,UIS.InputBegan true,DragContinue 170,440,DragEnd 170,440,UIS.InputEnded true,DragStart 170,440,UIS.InputBegan true,DragEnd 170,440,UIS.InputEnded true,UIS.InputBegan true,B.Click,B.Activated,UIS.InputEnded true", Joined)
-- Rotate: pressed 40 px right of the centre, moved to 40 px below it
P.Position = UDim2.fromOffset(300, 200)
D.DragStyle = Enum.UIDragDetectorDragStyle.Rotate
UpdateAll()
Drag({ { 390, 250 }, { 350, 290 } })
assert(math.abs(P.Rotation - 90) < 1e-4 and math.abs(D.DragRotation - 90) < 1e-4 and P.Position == UDim2.fromOffset(300, 200), `{P.Rotation} {D.DragRotation}`)
"##,
    );
}

/// A touch device (a 0.747 touch client, touches injected with VirtualInputManager:SendTouchEvent): the
/// finger hovers only while it is down, its Touch input is both the hover and the press input, TouchStarted /
/// TouchMoved / TouchEnded follow InputBegan / InputChanged / InputEnded, a release fires MouseButton1Up, Activated,
/// MouseButton1Click in that order, a TextBox takes focus at the release.
#[test]
fn touch_input_matches_a_client() {
    let (lua, engine) = start_with(800.0, 600.0, "play", Some(roblox_engine_emulator::client_profile::DeviceKind::Touch));
    run(
        &lua,
        &engine,
        r##"
local UIS = game:GetService("UserInputService")
local Root = Instance.new("ScreenGui")
Root.IgnoreGuiInset = true
local Log = {}
local function Fmt(Input)
	return `{Input.UserInputType.Name} {Input.UserInputState.Name} {Input.Position.X},{Input.Position.Y}`
end
local function Watch(O)
	for _, S in { "MouseEnter", "MouseLeave", "MouseMoved" } do
		O[S]:Connect(function(X, Y) table.insert(Log, `{O.Name}.{S} {X},{Y}`) end)
	end
	for _, S in { "InputBegan", "InputChanged", "InputEnded" } do
		O[S]:Connect(function(Input) table.insert(Log, `{O.Name}.{S} {Fmt(Input)}`) end)
	end
	O:GetPropertyChangedSignal("GuiState"):Connect(function() table.insert(Log, `{O.Name}.GuiState={O.GuiState.Name}`) end)
end
local B = Instance.new("TextButton")
B.Name = "B"
B.Position = UDim2.fromOffset(100, 100)
B.Size = UDim2.fromOffset(200, 100)
B.Parent = Root
local F = Instance.new("Frame")
F.Name = "F"
F.Position = UDim2.fromOffset(400, 100)
F.Size = UDim2.fromOffset(200, 100)
F.Parent = Root
local TB = Instance.new("TextBox")
TB.Name = "TB"
TB.Position = UDim2.fromOffset(400, 300)
TB.Size = UDim2.fromOffset(200, 60)
TB.ClearTextOnFocus = false
TB.Parent = Root
Watch(B)
Watch(F)
for _, S in { "MouseButton1Down", "MouseButton1Up" } do
	B[S]:Connect(function(X, Y) table.insert(Log, `B.{S} {X},{Y}`) end)
end
B.MouseButton1Click:Connect(function() table.insert(Log, "B.MouseButton1Click") end)
B.Activated:Connect(function(Input) table.insert(Log, `B.Activated {Fmt(Input)}`) end)
TB.Focused:Connect(function() table.insert(Log, "TB.Focused") end)
for _, S in { "InputBegan", "InputChanged", "InputEnded", "TouchStarted", "TouchMoved", "TouchEnded" } do
	UIS[S]:Connect(function(Input, Processed) table.insert(Log, `UIS.{S} {Fmt(Input)} {Processed}`) end)
end
Root.Parent = game:GetService("Players").LocalPlayer:WaitForChild("PlayerGui")
UpdateAll()
local function Finger(Kind, X, Y)
	if Kind == "move" then
		Dispatch({ type = "move", x = X, y = Y })
		StepDisplay(1 / 60)
	else
		Dispatch({ type = Kind, x = X, y = Y, button = 1 })
	end
	Flush()
end
local Cases = {}
local function Case(Label, Steps)
	table.clear(Log)
	for _, Step in Steps do
		Finger(Step[1], Step[2], Step[3])
	end
	table.insert(Cases, Label .. ": " .. table.concat(Log, ", "))
end
Case("tap B", { { "move", 150, 150 }, { "down", 150, 150 }, { "up", 150, 150 } })
Case("press B, move in B", { { "down", 150, 150 }, { "move", 170, 160 }, { "up", 170, 160 } })
Case("press B, release on F", { { "move", 150, 150 }, { "down", 150, 150 }, { "move", 450, 150 }, { "up", 450, 150 } })
Case("press F, release on B", { { "move", 450, 150 }, { "down", 450, 150 }, { "move", 150, 150 }, { "up", 150, 150 } })
Case("tap TextBox", { { "move", 450, 330 }, { "down", 450, 330 }, { "up", 450, 330 } })
local Got = table.concat(Cases, "\n")
local Client = table.concat({
	"tap B: B.MouseButton1Down 150,150, B.MouseEnter 150,150, B.InputBegan Touch Begin 150,150, B.GuiState=Press, UIS.InputBegan Touch Begin 150,150 true, UIS.TouchStarted Touch Begin 150,150 true, B.MouseButton1Up 150,150, B.Activated Touch End 150,150, B.MouseButton1Click, B.MouseLeave 150,150, B.InputEnded Touch End 150,150, B.GuiState=Idle, UIS.InputEnded Touch End 150,150 true, UIS.TouchEnded Touch End 150,150 true",
	"press B, move in B: B.MouseButton1Down 150,150, B.MouseEnter 150,150, B.InputBegan Touch Begin 150,150, B.GuiState=Press, UIS.InputBegan Touch Begin 150,150 true, UIS.TouchStarted Touch Begin 150,150 true, B.MouseMoved 170,160, B.InputChanged Touch Change 170,160, UIS.InputChanged Touch Change 170,160 true, UIS.TouchMoved Touch Change 170,160 true, B.MouseButton1Up 170,160, B.Activated Touch End 170,160, B.MouseButton1Click, B.MouseLeave 170,160, B.InputEnded Touch End 170,160, B.GuiState=Idle, UIS.InputEnded Touch End 170,160 true, UIS.TouchEnded Touch End 170,160 true",
	"press B, release on F: B.MouseButton1Down 150,150, B.MouseEnter 150,150, B.InputBegan Touch Begin 150,150, B.GuiState=Press, UIS.InputBegan Touch Begin 150,150 true, UIS.TouchStarted Touch Begin 150,150 true, F.MouseMoved 450,150, B.MouseLeave 450,150, B.InputEnded Touch Change 450,150, B.GuiState=Idle, F.MouseEnter 450,150, F.InputBegan Touch Change 450,150, UIS.InputChanged Touch Change 450,150 false, UIS.TouchMoved Touch Change 450,150 false, F.MouseLeave 450,150, F.InputEnded Touch End 450,150, UIS.InputEnded Touch End 450,150 false, UIS.TouchEnded Touch End 450,150 false",
	"press F, release on B: F.MouseEnter 450,150, F.InputBegan Touch Begin 450,150, F.GuiState=Press, UIS.InputBegan Touch Begin 450,150 false, UIS.TouchStarted Touch Begin 450,150 false, B.MouseMoved 150,150, F.MouseLeave 150,150, F.InputEnded Touch Change 150,150, F.GuiState=Idle, B.MouseEnter 150,150, B.InputBegan Touch Change 150,150, UIS.InputChanged Touch Change 150,150 true, UIS.TouchMoved Touch Change 150,150 true, B.MouseButton1Up 150,150, B.MouseLeave 150,150, B.InputEnded Touch End 150,150, UIS.InputEnded Touch End 150,150 true, UIS.TouchEnded Touch End 150,150 true",
	"tap TextBox: UIS.InputBegan Touch Begin 450,330 true, UIS.TouchStarted Touch Begin 450,330 true, TB.Focused, UIS.InputEnded Touch End 450,330 true, UIS.TouchEnded Touch End 450,330 true",
}, "\n")
assert(Got == Client, Got)
"##,
    );
}

/// A connected gamepad's button in a 0.747 client (VirtualInputManager:HandleGamepadButtonInput): the InputObject is
/// Gamepad1's, Position (0, 0, 1) and Delta (0, 0, 1) going down, (0, 0, 0) and (0, 0, -1) coming up.
#[test]
fn gamepad_buttons_carry_the_clients_positions() {
    let (lua, engine) = start(800.0, 600.0);
    run(
        &lua,
        &engine,
        r##"
local UIS = game:GetService("UserInputService")
local Log = {}
local function Fmt(Input) return `{Input.UserInputType.Name} {Input.KeyCode.Name} {Input.UserInputState.Name} {Input.Position} {Input.Delta}` end
UIS.InputBegan:Connect(function(Input) table.insert(Log, Fmt(Input)) end)
UIS.InputEnded:Connect(function(Input) table.insert(Log, Fmt(Input)) end)
Dispatch({ type = "move", x = 300, y = 200 })
Dispatch({ type = "key", key = "ButtonA", down = true })
Dispatch({ type = "key", key = "ButtonA", down = false })
Flush()
local Joined = table.concat(Log, "; ")
assert(string.find(Joined, "Gamepad1 ButtonA Begin 0, 0, 1 0, 0, 1; Gamepad1 ButtonA End 0, 0, 0 0, 0, -1", 1, true), Joined)
"##,
    );
}
