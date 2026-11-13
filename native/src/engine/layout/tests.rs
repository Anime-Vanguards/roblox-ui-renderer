//! Layout tests through the script API. Expected values were recorded from the Luau runtime (the
//! reference) running the same script as a fixture (`node cli/rur.mjs render --fixture ...`); the
//! golden corpus (`tools/engine/golden.mjs`) covers the stories.

use std::rc::Rc;

use mlua::prelude::*;

use crate::engine::{
    Engine, Options,
    lua::{self as engine_lua, globals::CallOutcome},
    scheduler,
};

fn start(pixel_scale: f64) -> (Lua, Rc<Engine>) {
    let engine = crate::engine::new_engine(Options { pixel_scale, ..crate::engine::options() }).expect("engine");
    let lua = Lua::new();
    engine_lua::start(&lua, &engine).expect("start");
    (lua, engine)
}

/// Runs `source` (a chunk returning `function(Target)`) against a full-viewport Target frame in a
/// CoreGui ScreenGui, runs virtual time to `until`, and returns the output lines.
fn run(source: &str, pixel_scale: f64, until: f64) -> Vec<String> {
    let (lua, engine) = start(pixel_scale);
    let script = {
        let mut state = engine.state.borrow_mut();
        let class = engine.reflection.class_id("ModuleScript").unwrap();
        crate::engine::instance::create_named(&mut state, &engine.reflection, class, "Test")
    };
    let env = engine_lua::modules::environment(&lua, &engine, script).expect("environment");
    let wrapper = format!(
        "local Story = (function() {source} end)()\n\
         local Gui = Instance.new('ScreenGui') Gui.Name = 'Story' Gui.Parent = game:GetService('CoreGui')\n\
         local Target = Instance.new('Frame') Target.Name = 'StoryTarget' Target.Size = UDim2.fromScale(1, 1)\n\
         Target.BackgroundTransparency = 1 Target.Parent = Gui\n\
         Story(Target)"
    );
    let function = lua.load(wrapper).set_name("=Test").set_environment(env).into_function().expect("compile");
    let call = engine_lua::globals::call(&lua, &engine, function, LuaMultiValue::new()).expect("call");
    if let CallOutcome::Failed(message, _) = call.outcome().unwrap() {
        panic!("test script failed: {message}");
    }
    scheduler::run_until(&lua, &engine, until).expect("frames");
    let lines = engine.state.borrow().output.entries.iter().map(|e| format!("{}: {}", e.kind, e.message)).collect();
    drop(lua);
    lines
}

/// AbsolutePosition is measured from below the top-bar inset, as in a playing Roblox client (Studio 0.744, a 58 px
/// inset: an IgnoreGuiInset ScreenGui reads (0, -58) and its frame at offset (10, 20) reads (10, -38); a ScreenGui
/// below the inset reads (0, 0) and the frame (10, 20)); sizes are as drawn.
#[test]
fn absolute_positions_are_measured_from_below_the_top_inset() {
    let engine = crate::engine::new_engine(Options { topbar_inset: 58.0, ..crate::engine::options() }).expect("engine");
    let lua = Lua::new();
    engine_lua::start(&lua, &engine).expect("start");
    let script = {
        let mut state = engine.state.borrow_mut();
        let class = engine.reflection.class_id("ModuleScript").unwrap();
        crate::engine::instance::create_named(&mut state, &engine.reflection, class, "Test")
    };
    let env = engine_lua::modules::environment(&lua, &engine, script).expect("environment");
    let source = r#"
        local function V(Value) return `{Value.X},{Value.Y}` end
        for _, Ignore in { true, false } do
            local Gui = Instance.new("ScreenGui")
            Gui.IgnoreGuiInset = Ignore
            local Box = Instance.new("Frame")
            Box.Position = UDim2.fromOffset(10, 20)
            Box.Size = UDim2.fromScale(0.5, 0.5)
            Box.Parent = Gui
            Gui.Parent = game:GetService("CoreGui")
            print(`{tostring(Ignore)} {V(Gui.AbsolutePosition)} {V(Gui.AbsoluteSize)} {V(Box.AbsolutePosition)} {V(Box.AbsoluteSize)}`)
        end
    "#;
    let function = lua.load(source).set_name("=Test").set_environment(env).into_function().expect("compile");
    let call = engine_lua::globals::call(&lua, &engine, function, LuaMultiValue::new()).expect("call");
    if let CallOutcome::Failed(message, _) = call.outcome().unwrap() {
        panic!("test script failed: {message}");
    }
    let lines: Vec<String> = engine.state.borrow().output.entries.iter().map(|e| e.message.clone()).collect();
    let (width, height) = (engine.options.viewport_width, engine.options.viewport_height);
    let half = |value: f64| (value / 2.0).floor();
    assert_eq!(lines[0], format!("true 0,-58 {width},{height} 10,-38 {},{}", half(width), half(height)), "{lines:?}");
    assert_eq!(lines[1], format!("false 0,0 {width},{} 10,20 {},{}", height - 58.0, half(width), half(height - 58.0)), "{lines:?}");
}

const API_SCRIPT: &str = r#"
return function(Target)
	local function V(Value) return `{Value.X},{Value.Y}` end
	local Holder = Instance.new("Frame")
	Holder.Size = UDim2.fromOffset(400, 300)
	Holder.Position = UDim2.fromOffset(10, 20)
	Holder.Parent = Target
	local F = Instance.new("Frame")
	F.Size = UDim2.new(0.5, 10, 0, 40)
	F.Position = UDim2.fromScale(0.25, 0.5)
	F.AnchorPoint = Vector2.new(0.5, 0.5)
	F.Parent = Holder
	print("frame", V(F.AbsoluteSize), V(F.AbsolutePosition), F.AbsoluteRotation)
	F.Size = UDim2.fromOffset(100, 50)
	print("resized", V(F.AbsoluteSize), V(F.AbsolutePosition))
	local Changes = 0
	F:GetPropertyChangedSignal("AbsoluteSize"):Connect(function() Changes += 1 end)
	F.Size = UDim2.fromOffset(120, 50)
	print("lazy", V(F.AbsoluteSize))
	task.wait()
	print("changes", Changes)
	local List = Instance.new("UIListLayout")
	List.Padding = UDim.new(0, 5)
	List.Parent = F
	for Index = 1, 3 do
		local Child = Instance.new("Frame")
		Child.Name = `C{4 - Index}`
		Child.Size = UDim2.fromOffset(20 * Index, 10)
		Child.LayoutOrder = -Index
		Child.Parent = F
	end
	print("list", V(List.AbsoluteContentSize))
	List.SortOrder = Enum.SortOrder.Name
	print("byname", V(F.C1.AbsolutePosition), V(F.C3.AbsolutePosition))
	List.FillDirection = Enum.FillDirection.Horizontal
	List.HorizontalAlignment = Enum.HorizontalAlignment.Center
	print("horizontal", V(List.AbsoluteContentSize), V(F.C2.AbsolutePosition))
	local Auto = Instance.new("Frame")
	Auto.AutomaticSize = Enum.AutomaticSize.XY
	Auto.Size = UDim2.fromOffset(10, 10)
	Auto.Parent = Holder
	local Pad = Instance.new("UIPadding")
	Pad.PaddingLeft = UDim.new(0, 4)
	Pad.PaddingTop = UDim.new(0, 6)
	Pad.Parent = Auto
	local Inner = Instance.new("Frame")
	Inner.Size = UDim2.fromOffset(70, 33)
	Inner.Position = UDim2.fromOffset(5, 7)
	Inner.Parent = Auto
	print("auto", V(Auto.AbsoluteSize), V(Inner.AbsolutePosition))
	local Scale = Instance.new("UIScale")
	Scale.Scale = 1.3
	Scale.Parent = Auto
	print("uiscale", V(Auto.AbsoluteSize), V(Inner.AbsoluteSize))
	local Scroll = Instance.new("ScrollingFrame")
	Scroll.Size = UDim2.fromOffset(200, 200)
	Scroll.CanvasSize = UDim2.fromOffset(0, 600)
	Scroll.Parent = Holder
	Scroll.CanvasPosition = Vector2.new(0, 5000)
	print("scroll", V(Scroll.CanvasPosition), V(Scroll.AbsoluteCanvasSize), V(Scroll.AbsoluteWindowSize))
	Scroll.CanvasPosition = Vector2.new(-5, 100.7)
	local Item = Instance.new("Frame")
	Item.Size = UDim2.fromOffset(50, 50)
	Item.Parent = Scroll
	print("scrolled", V(Scroll.CanvasPosition), V(Item.AbsolutePosition))
	local Rotated = Instance.new("Frame")
	Rotated.Size = UDim2.fromOffset(100, 40)
	Rotated.Position = UDim2.fromOffset(50, 50)
	Rotated.Rotation = 30
	Rotated.Parent = Holder
	local Child = Instance.new("Frame")
	Child.Size = UDim2.fromOffset(10, 10)
	Child.Parent = Rotated
	print("rotated", V(Child.AbsolutePosition), Child.AbsoluteRotation)
	local Pages = Instance.new("Frame")
	Pages.Size = UDim2.fromOffset(100, 100)
	Pages.Parent = Holder
	local PageLayout = Instance.new("UIPageLayout")
	PageLayout.Parent = Pages
	for Index = 1, 3 do
		local Page = Instance.new("Frame")
		Page.Name = `P{Index}`
		Page.Size = UDim2.fromScale(1, 1)
		Page.LayoutOrder = Index
		Page.Parent = Pages
	end
	print("page", V(Pages.P1.AbsolutePosition), V(PageLayout.AbsoluteContentSize))
	PageLayout:Next()
	print("next", PageLayout.CurrentPage and PageLayout.CurrentPage.Name, V(Pages.P2.AbsolutePosition))
	PageLayout:JumpToIndex(2)
	print("jump", PageLayout.CurrentPage and PageLayout.CurrentPage.Name, V(Pages.P3.AbsolutePosition))
	local Grid = Instance.new("UIGridLayout")
	local GridHolder = Instance.new("Frame")
	GridHolder.Size = UDim2.fromOffset(210, 100)
	GridHolder.Parent = Holder
	Grid.CellSize = UDim2.fromOffset(50, 20)
	Grid.Parent = GridHolder
	for _ = 1, 5 do Instance.new("Frame").Parent = GridHolder end
	print("grid", V(Grid.AbsoluteContentSize), V(Grid.AbsoluteCellSize), V(Grid.AbsoluteCellCount))
end
"#;

/// Lazy getters, change events of computed properties, lists (sort orders, alignment),
/// AutomaticSize with padding and UIScale, CanvasPosition clamping and physical-pixel canvas
/// offsets, rotation, UIPageLayout navigation and grids: the Luau runtime's values at DPI 1.5.
/// A rect read pays the lazy layout a client runs for it (install.rs `ensure`): a canvas moved and read in the same Heartbeat
/// handler pays the scroll's pass (62 us and 0.312 a record moved); moved in Heartbeat and read in the next frame's PreRender,
/// after the frame's layout point, where a client has laid it out already, it pays nothing.
#[test]
fn a_rect_read_after_a_change_pays_the_lazy_layout_a_client_runs() {
    let output = run(
        r#"
        return function(Target)
            local Scroller = Instance.new("ScrollingFrame")
            Scroller.Size = UDim2.fromOffset(200, 200)
            Scroller.CanvasSize = UDim2.fromOffset(0, 2200)
            Scroller.Parent = Target
            local Cards = {}
            for I = 1, 20 do
                local Card = Instance.new("Frame")
                Card.Size = UDim2.fromOffset(100, 100)
                Card.Position = UDim2.fromOffset(0, I * 100)
                Card.Parent = Scroller
                Cards[I] = Card
            end
            local RunService = game:GetService("RunService")
            local function Twice()
                local T0 = os.clock()
                local _ = Cards[1].AbsolutePosition
                local T1 = os.clock()
                local _ = Cards[1].AbsolutePosition
                local T2 = os.clock()
                return ((T1 - T0) - (T2 - T1)) * 1e6
            end
            task.spawn(function()
                RunService.Heartbeat:Wait()
                RunService.Heartbeat:Wait()
                local _ = Cards[1].AbsolutePosition
                Scroller.CanvasPosition = Vector2.new(0, 50)
                print(string.format("same %.3f", Twice()))
                RunService.Heartbeat:Wait()
                Scroller.CanvasPosition = Vector2.new(0, 100)
                RunService.PreRender:Wait()
                local _ = Cards[2].AbsolutePosition
                local _ = Cards[1].Name
                print(string.format("prerender %.3f", Twice()))
            end)
        end
        "#,
        1.0,
        1.0,
    );
    let value = |name: &str| -> f64 {
        let line = output.iter().find(|line| line.contains(&format!(": {name} "))).unwrap_or_else(|| panic!("{name} missing: {output:?}"));
        line.rsplit(' ').next().unwrap().parse().unwrap()
    };
    let same = value("same");
    assert!((64.0..75.0).contains(&same), "a read after a scroll in its handler pays {same:.2} us, a client 62 + 0.31 a card");
    let after = value("prerender");
    assert!(after.abs() < 0.05, "a read in PreRender of a scroll made before the frame's layout point pays {after:.3} us");
}

#[test]
fn layout_api_matches_luau() {
    let lines = run(API_SCRIPT, 1.5, 0.1);
    let expected = [
        "print: frame 210,40 5,150 0",
        "print: resized 100,50 60,145",
        "print: lazy 120,50",
        "print: changes 1",
        "print: list 60,40",
        "print: byname 50,145 50,175",
        "print: horizontal 130,10 110,145",
        "print: auto 79,46 19,33",
        "print: uiscale 102.70000457763672,59.79999542236328 91,42.89999771118164",
        "print: scroll 0,400 200,600 187,200",
        "print: scrolled 0,100.69999694824219 10,-80.66666412353516",
        "print: rotated 73.52885437011719,49.509620666503906 30",
        "print: page 10,20 300,100",
        "print: next P2 10,20",
        "print: jump P3 10,20",
        "print: grid 160,45 50,20 3,2",
    ];
    assert_eq!(lines, expected);
}

/// A wrapped AutomaticSize Y label in an AutomaticSize Y frame keeps every line it was measured with. The frame's
/// height comes back from the label's bounds through float32 local units and the UIScale; the final pass lays the
/// label out again in that height, a few float32 roundings below the line it was measured to hold (87 px of text
/// against an 86.9999974 px box at DPI 1.3333 under UIScale 810 / 806), and the 1e-6 px slack of the line test cut
/// the last line: the label was sized for two of its three lines while its frame kept three. Each label is
/// checked against the same label in a fixed frame.
#[test]
fn an_automatic_label_keeps_the_lines_its_automatic_frame_was_sized_for() {
    let script = r#"
        return function(Target)
            local function Label(Parent, Lines)
                local Label = Instance.new("TextLabel")
                Label.AutomaticSize = Enum.AutomaticSize.Y
                Label.FontFace = Font.new("rbxasset://fonts/families/FredokaOne.json")
                Label.Size = UDim2.fromOffset(179, 0)
                Label.Text = string.rep("Line\n", Lines - 1) .. "Line"
                Label.TextSize = 22
                Label.TextWrapped = true
                Label.Parent = Parent
                return Label
            end
            local Cases = {}
            local Scales = { 810 / 806 }
            for Percent = 70, 130 do
                table.insert(Scales, Percent / 100)
            end
            for Index, Value in Scales do
                for Lines = 1, 4 do
                    local Card = Instance.new("Frame")
                    Card.Size = UDim2.fromOffset(400, 300)
                    Card.Position = UDim2.fromOffset(Lines * 10, Index * 2)
                    local Scale = Instance.new("UIScale")
                    Scale.Scale = Value
                    Scale.Parent = Card
                    Card.Parent = Target
                    local Holder = Instance.new("Frame")
                    Holder.AutomaticSize = Enum.AutomaticSize.Y
                    Holder.Size = UDim2.fromOffset(190, 0)
                    Holder.Parent = Card
                    local Fixed = Instance.new("Frame")
                    Fixed.Position = UDim2.fromOffset(200, 0)
                    Fixed.Size = UDim2.fromOffset(190, 300)
                    Fixed.Parent = Card
                    table.insert(Cases, { Value, Lines, Label(Holder, Lines), Holder, Label(Fixed, Lines) })
                end
            end
            local Short = 0
            for _, Case in Cases do
                local Value, Lines, Inner, Holder, Reference = table.unpack(Case)
                local Kept = math.abs(Inner.TextBounds.Y - Reference.TextBounds.Y) < 0.01
                local Filled = math.abs(Inner.AbsoluteSize.Y - Holder.AbsoluteSize.Y) < 0.01
                if not (Kept and Filled) then
                    Short += 1
                    print(`short {Value} x{Lines}: bounds {Inner.TextBounds.Y} of {Reference.TextBounds.Y}, label {Inner.AbsoluteSize.Y} in {Holder.AbsoluteSize.Y}`)
                end
            end
            print(`checked {#Cases}, short {Short}`)
        end
    "#;
    for pixel_scale in [1.0, 1.25, 1.3333, 1.5, 2.0] {
        let lines = run(script, pixel_scale, 0.1);
        assert_eq!(lines.last().map(String::as_str), Some("print: checked 248, short 0"), "DPI {pixel_scale}: {lines:#?}");
    }
}


/// Per-instance storage (Slots): entries are guarded by the id's generation, and a slot far beyond
/// the ones in use costs one chunk, not everything below it.
#[test]
fn slots_are_guarded_by_generation_and_grow_by_chunks() {
    use crate::engine::InstanceId;
    let id = |index, generation| InstanceId { index, generation };
    let mut slots = super::Slots::<String>::default();
    assert_eq!(slots.get(id(0, 0)), None);
    assert_eq!(slots.take(id(70_000, 0)), None);
    slots.set(id(3, 1), "three".into());
    slots.set(id(70_000, 2), "far".into());
    slots.set(id(255, 0), "edge".into());
    slots.set(id(256, 0), "next chunk".into());
    assert_eq!(slots.get(id(3, 1)).map(String::as_str), Some("three"));
    assert_eq!(slots.get(id(3, 2)), None, "a reused slot does not answer for the old instance");
    assert_eq!(slots.get(id(4, 1)), None);
    assert_eq!(slots.get(id(1_000, 0)), None, "a chunk nothing was written to");
    assert_eq!(slots.get(id(255, 0)).map(String::as_str), Some("edge"));
    assert_eq!(slots.get(id(256, 0)).map(String::as_str), Some("next chunk"));
    slots.get_mut(id(70_000, 2)).unwrap().push_str(" away");
    assert_eq!(slots.take(id(70_000, 3)), None);
    assert_eq!(slots.take(id(70_000, 2)).as_deref(), Some("far away"));
    assert_eq!(slots.get(id(70_000, 2)), None);
    slots.set(id(3, 2), "again".into());
    assert_eq!(slots.get(id(3, 1)), None);
    assert_eq!(slots.get(id(3, 2)).map(String::as_str), Some("again"));
}
