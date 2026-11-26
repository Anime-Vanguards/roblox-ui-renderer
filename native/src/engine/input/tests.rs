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
