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
    let mut options = crate::engine::options();
    options.profile.device.topbar_inset = Some(58.0);
    let engine = crate::engine::new_engine(options).expect("engine");
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
    let (width, height) = engine.options.profile.device.viewport();
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
            local Unfit = 0
            for _, Case in Cases do
                local Value, Lines, Inner, Holder, Reference = table.unpack(Case)
                local Kept = math.abs(Inner.TextBounds.Y - Reference.TextBounds.Y) < 0.01
                local Filled = math.abs(Inner.AbsoluteSize.Y - Holder.AbsoluteSize.Y) < 0.01
                if not (Kept and Filled) then
                    Short += 1
                    print(`short {Value} x{Lines}: bounds {Inner.TextBounds.Y} of {Reference.TextBounds.Y}, label {Inner.AbsoluteSize.Y} in {Holder.AbsoluteSize.Y}`)
                end
                if not Inner.TextFits then
                    Unfit += 1
                    print(`unfit {Value} x{Lines}: bounds {Inner.TextBounds.Y}, size {Inner.AbsoluteSize.Y}`)
                end
            end
            print(`checked {#Cases}, short {Short}, unfit {Unfit}`)
        end
    "#;
    for pixel_scale in [1.0, 1.25, 1.3333, 1.5, 2.0] {
        let lines = run(script, pixel_scale, 0.1);
        assert_eq!(lines.last().map(String::as_str), Some("print: checked 248, short 0, unfit 0"), "DPI {pixel_scale}: {lines:#?}");
    }
}

/// A ScrollingFrame's own UISizeConstraint and UIAspectRatioConstraint constrain its
/// AutomaticCanvasSize extent before UIPadding offsets and inset scrollbar thickness are added
/// (measured in Roblox Studio 0.747.0.7471481 across MaxSize, MinSize, UIScale, FitWithinMaxSize,
/// ScaleWithParentSize, inset scrollbars and combined constraints).
#[test]
fn automatic_canvas_size_applies_size_and_aspect_constraints() {
    let script = r#"
        return function(Target)
            local function Make(Opts)
                local H = Instance.new("Frame")
                H.Size = UDim2.fromOffset(Opts.hw or 200, Opts.hh or 140)
                H.Parent = Target
                if Opts.uiscale then
                    local U = Instance.new("UIScale")
                    U.Scale = Opts.uiscale
                    U.Parent = H
                end
                local S = Instance.new("ScrollingFrame")
                S.Size = Opts.size or UDim2.fromOffset(180, 100)
                S.AutomaticCanvasSize = Enum.AutomaticSize[Opts.axis or "Y"]
                S.CanvasSize = Opts.canvas or UDim2.new()
                S.ScrollBarThickness = Opts.thickness or 0
                S.BorderSizePixel = 0
                if Opts.hinset then S.HorizontalScrollBarInset = Opts.hinset end
                S.Parent = H
                if Opts.minsize or Opts.maxsize then
                    local C = Instance.new("UISizeConstraint")
                    if Opts.minsize then C.MinSize = Opts.minsize end
                    if Opts.maxsize then C.MaxSize = Opts.maxsize end
                    C.Parent = S
                end
                if Opts.aspect then
                    local A = Instance.new("UIAspectRatioConstraint")
                    A.AspectRatio = Opts.aspect
                    if Opts.atype then A.AspectType = Opts.atype end
                    if Opts.dom then A.DominantAxis = Opts.dom end
                    A.Parent = S
                end
                if Opts.pad then
                    local P = Instance.new("UIPadding")
                    P.PaddingTop = UDim.new(0, Opts.pad[1])
                    P.PaddingBottom = UDim.new(0, Opts.pad[2])
                    P.PaddingLeft = UDim.new(0, Opts.pad[3])
                    P.PaddingRight = UDim.new(0, Opts.pad[4])
                    P.Parent = S
                end
                local F = Instance.new("Frame")
                F.Size = Opts.child or UDim2.fromOffset(50, 300)
                F.Parent = S
                return S
            end
            local Cases = {
                { "maxY", Make({ hw = 110, size = UDim2.new(0.9, 0, 0.7, 4), maxsize = Vector2.new(1000, 70), child = UDim2.fromOffset(200, 220) }), 99, 70 },
                { "maxY_pad", Make({ hw = 110, size = UDim2.new(0.9, 0, 0.7, 4), maxsize = Vector2.new(1000, 70), pad = { 2, 1, 2, 1 }, child = UDim2.fromOffset(200, 220) }), 99, 73 },
                { "maxY_uiscale_pad", Make({ hw = 110, size = UDim2.new(0.9, 0, 0.7, 4), maxsize = Vector2.new(1000, 70), uiscale = 1.25, pad = { 2, 1, 2, 1 }, child = UDim2.fromOffset(200, 220) }), 123.75, 91.25 },
                { "maxY_inset_pad", Make({ size = UDim2.fromOffset(100, 100), maxsize = Vector2.new(1000, 80), thickness = 10, canvas = UDim2.fromOffset(300, 0), hinset = Enum.ScrollBarInset.ScrollBar, pad = { 2, 1, 0, 0 }, child = UDim2.fromOffset(50, 200) }), 300, 93 },
                { "minY_pad", Make({ size = UDim2.fromOffset(100, 80), minsize = Vector2.new(0, 150), pad = { 2, 1, 0, 0 }, child = UDim2.fromOffset(20, 60) }), 100, 153 },
                { "aspectY_w100", Make({ aspect = 0.5, child = UDim2.fromOffset(100, 400) }), 50, 200 },
                { "aspectY_padY", Make({ aspect = 0.5, pad = { 10, 5, 0, 0 }, child = UDim2.fromOffset(50, 300) }), 50, 115 },
                { "aspectXY_pad", Make({ axis = "XY", aspect = 0.5, pad = { 10, 5, 8, 4 }, child = UDim2.fromOffset(120, 400) }), 132, 255 },
                { "aspectXY_scaleH", Make({ hh = 200, axis = "XY", aspect = 0.5, atype = Enum.AspectType.ScaleWithParentSize, dom = Enum.DominantAxis.Height, child = UDim2.fromOffset(60, 180) }), 50, 100 },
                { "both_XY", Make({ axis = "XY", aspect = 2.0, maxsize = Vector2.new(160, 150), child = UDim2.fromOffset(300, 300) }), 160, 80 },
            }
            local Bad = 0
            for _, Case in Cases do
                local Name, S, WantX, WantY = table.unpack(Case)
                local Got = S.AbsoluteCanvasSize
                if math.abs(Got.X - WantX) > 0.01 or math.abs(Got.Y - WantY) > 0.01 then
                    Bad += 1
                    print(`bad {Name}: got ({Got.X}, {Got.Y}), want ({WantX}, {WantY})`)
                end
            end
            print(`checked {#Cases}, bad {Bad}`)
        end
    "#;
    let lines = run(script, 1.0, 0.1);
    assert_eq!(lines.last().map(String::as_str), Some("print: checked 10, bad 0"), "{lines:#?}");
}

/// AutomaticSize with UIPadding and UISizeConstraint against a Roblox 0.747 client's rects
/// (data/studio/whole/autosize-constraint.json): on an automatic axis the constraint bounds the content inside the
/// padding offsets (MinSize 22 and 6 + 6 px of padding make a label holding 8 px of text 34 wide), MaxSize then caps
/// the object but its children still push it past the cap, an automatic parent measures a capped child at the size
/// its content asked for, and a MinSize above MaxSize switches the constraint off. Cells whose text measures
/// differently from Studio's are counted apart: their sizes follow the text, not these rules.
#[test]
fn automatic_size_constraints_bound_the_content_inside_the_padding() {
    let studio: serde_json::Value = serde_json::from_str(include_str!("../../../../data/studio/whole/autosize-constraint.json")).expect("measurements");
    let fixtures = [
        ("layout-autosize-constraint", include_str!("../../../../fixtures/layout-autosize-constraint.luau")),
        ("layout-autosize-constraint2", include_str!("../../../../fixtures/layout-autosize-constraint2.luau")),
    ];
    let (mut compared, mut text_cells, mut misses) = (0, 0, Vec::new());
    for (name, source) in fixtures {
        let script = format!(
            r#"
            local Fixture = (function() {source} end)()
            return function(Target)
                Fixture(Target)
                task.spawn(function()
                    task.wait(0.1)
                    local Rects = {{}}
                    for _, Cell in Target:GetChildren() do
                        local function Visit(Node, Path)
                            for _, Child in Node:GetChildren() do
                                if Child:IsA("GuiObject") then
                                    local At = Path .. "/" .. Child.Name
                                    local Rect = {{ Child.AbsolutePosition.X - Cell.AbsolutePosition.X, Child.AbsolutePosition.Y - Cell.AbsolutePosition.Y, Child.AbsoluteSize.X, Child.AbsoluteSize.Y }}
                                    if Child:IsA("TextLabel") or Child:IsA("TextButton") or Child:IsA("TextBox") then
                                        table.insert(Rect, Child.TextBounds.X)
                                        table.insert(Rect, Child.TextBounds.Y)
                                    end
                                    Rects[At] = Rect
                                    Visit(Child, At)
                                end
                            end
                        end
                        Visit(Cell, "/" .. Cell.Name)
                    end
                    print(game:GetService("HttpService"):JSONEncode(Rects))
                end)
            end
            "#
        );
        let lines = run(&script, 1.5, 0.5);
        let json = lines.iter().find_map(|line| line.strip_prefix("print: {").map(|rest| format!("{{{rest}"))).unwrap_or_else(|| panic!("{name}: no rects: {lines:#?}"));
        let ours: serde_json::Value = serde_json::from_str(&json).unwrap();
        let expected = studio["fixtures"][name].as_object().unwrap();
        let values = |value: &serde_json::Value| -> Vec<f64> { value.as_array().map(|items| items.iter().filter_map(|v| v.as_f64()).collect()).unwrap_or_default() };
        let cell_of = |path: &str| path.split('/').nth(1).unwrap_or("").to_string();
        // Cells holding text that measures differently from Studio's
        let mut text_differs = std::collections::HashSet::new();
        for (path, want) in expected {
            let (want, got) = (values(want), values(&ours[path]));
            if want.len() == 6 && (got.len() != 6 || (want[4] - got[4]).abs() > 0.01) {
                text_differs.insert(cell_of(path));
            }
        }
        for (path, want) in expected {
            let (want, got) = (values(want), values(&ours[path]));
            compared += 1;
            let same = got.len() >= 4 && (0..4).all(|k| (want[k] - got[k]).abs() < 0.01);
            if same {
                continue;
            }
            if text_differs.contains(&cell_of(path)) {
                text_cells += 1;
                continue;
            }
            misses.push(format!("{name} {path}: Studio {:?}, ours {got:?}", &want[..4]));
        }
    }
    eprintln!("compared {compared}, {text_cells} differing only through their text's width");
    assert!(misses.is_empty(), "{} of {compared} rects differ from Studio's:\n{}", misses.len(), misses.join("\n"));
    assert!(compared > 260, "only {compared} rects compared");
}

/// Every GuiObject below each cell (a child of the target) of the fixture `source` as path -> [x, y, w, h] relative
/// to its cell, text objects with [.., tbx, tby], read through the script API after the layout settled.
fn cell_rects(source: &str, pixel_scale: f64) -> serde_json::Value {
    let script = format!(
        r#"
        local Fixture = (function() {source} end)()
        return function(Target)
            Fixture(Target)
            task.spawn(function()
                task.wait(0.1)
                local Rects = {{}}
                for _, Cell in Target:GetChildren() do
                    local function Visit(Node, Path)
                        for _, Child in Node:GetChildren() do
                            if Child:IsA("GuiObject") then
                                local At = Path .. "/" .. Child.Name
                                local Rect = {{ Child.AbsolutePosition.X - Cell.AbsolutePosition.X, Child.AbsolutePosition.Y - Cell.AbsolutePosition.Y, Child.AbsoluteSize.X, Child.AbsoluteSize.Y }}
                                if Child:IsA("TextLabel") or Child:IsA("TextButton") or Child:IsA("TextBox") then
                                    table.insert(Rect, Child.TextBounds.X)
                                    table.insert(Rect, Child.TextBounds.Y)
                                end
                                Rects[At] = Rect
                                Visit(Child, At)
                            end
                        end
                    end
                    Visit(Cell, "/" .. Cell.Name)
                end
                print(game:GetService("HttpService"):JSONEncode(Rects))
            end)
        end
        "#
    );
    let lines = run(&script, pixel_scale, 0.5);
    let json = lines.iter().find_map(|line| line.strip_prefix("print: {").map(|rest| format!("{{{rest}"))).unwrap_or_else(|| panic!("no rects: {lines:#?}"));
    serde_json::from_str(&json).unwrap()
}

fn rect_values(value: &serde_json::Value) -> Vec<f64> {
    value.as_array().map(|items| items.iter().filter_map(|v| v.as_f64()).collect()).unwrap_or_default()
}

/// The client's measurements of the AutomaticSize measure fixtures (data/studio/whole/autosize-measure.json).
fn measured() -> serde_json::Value {
    serde_json::from_str(include_str!("../../../../data/studio/whole/autosize-measure.json")).expect("measurements")
}

/// AutomaticSize measures against a Roblox 0.747 client's rects (data/studio/whole/autosize-measure.json): while an
/// automatic frame is measured, its childless children size against its own Size (the scale part resolved against
/// the space it can grow into, capped by its MaxSize, less its padding), as do a list's scale gaps; the second pass
/// resolves the gaps against the box less the padding and lets them grow the frame; deeper down, a plain
/// container's childless children size against its Size resolved against its measured box; an object its
/// children pushed past its box resolves its padding's scale against that box.
#[test]
fn automatic_frames_measure_against_their_own_size() {
    let studio = measured();
    let fixtures = [
        ("layout-autosize-constraint3", include_str!("../../../../fixtures/layout-autosize-constraint3.luau")),
        ("layout-autolist-scalepad", include_str!("../../../../fixtures/layout-autolist-scalepad.luau")),
        ("layout-autonested-scalepos", include_str!("../../../../fixtures/layout-autonested-scalepos.luau")),
    ];
    let (mut compared, mut misses) = (0, Vec::new());
    for (name, source) in fixtures {
        let ours = cell_rects(source, 1.5);
        for (path, want) in studio["fixtures"][name].as_object().unwrap() {
            let (want, got) = (rect_values(want), rect_values(&ours[path]));
            compared += 1;
            if !(got.len() >= 4 && (0..4).all(|k| (want[k] - got[k]).abs() < 0.01)) {
                misses.push(format!("{name} {path}: client {:?}, ours {got:?}", &want[..4]));
            }
        }
    }
    assert!(misses.is_empty(), "{} of {compared} rects differ from the client's:\n{}", misses.len(), misses.join("\n"));
    assert!(compared > 560, "only {compared} rects compared");
}

/// Text measures against a Roblox 0.747 client (data/studio/whole/autosize-measure.json): wrapped AutomaticSize XY
/// text with less room than its box takes its height from the text laid out in that room (a (0.5, 30) label in a
/// 0 px wide frame is 100 tall, one glyph per line), unwrapped AutomaticSize X text drops its last space run only
/// where that fits its room, and wrapped AutomaticSize Y labels keep a line their box ends float32 roundings short
/// of (UIScale sweep). The client reports TextBounds in whole px, so every value is compared within 1 px.
#[test]
fn automatic_text_measures_in_the_room_it_has() {
    if crate::engine::text::fonts::discover_content_root().is_none() {
        eprintln!("skipped: no Studio content root (the cases need Studio's fonts)");
        return;
    }
    let studio = measured();
    let fixtures = [
        ("layout-autotext-matrix", include_str!("../../../../fixtures/layout-autotext-matrix.luau"), 1.5),
        ("layout-autotext-linefit", include_str!("../../../../fixtures/layout-autotext-linefit.luau"), 1.5),
        ("layout-autotext-linefit@1x", include_str!("../../../../fixtures/layout-autotext-linefit.luau"), 1.0),
    ];
    let (mut compared, mut misses) = (0, Vec::new());
    for (name, source, pixel_scale) in fixtures {
        let ours = cell_rects(source, pixel_scale);
        for (path, want) in studio["fixtures"][name].as_object().unwrap() {
            let (want, got) = (rect_values(want), rect_values(&ours[path]));
            compared += 1;
            // (within the client's whole-px TextBounds, which move automatic sizes and list rows by less than a px; a
            // line kept or cut moves them by a whole line)
            if !(got.len() == want.len() && (0..want.len()).all(|k| (want[k] - got[k]).abs() < 1.0)) {
                misses.push(format!("{name} {path}: client {want:?}, ours {got:?}"));
            }
        }
    }
    assert!(misses.is_empty(), "{} of {compared} rects differ from the client's:\n{}", misses.len(), misses.join("\n"));
    assert!(compared > 1500, "only {compared} rects compared");
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

/// Moving a non-scroll parent shifts its same-shape subtree via `store` -> `moved(id, false)` ->
/// `shift_children(id, false)`. Every shifted record must count under `stats.moved`, never
/// `stats.scroll_moved` or `stats.scroll_passes`, so `desktop_layout_cpu_ms` does not charge
/// the 62 us scroll base.
#[test]
fn non_scroll_parent_move_counts_moved_descendants_not_scroll_moved() {
    let (lua, engine) = start(1.0);
    let script = {
        let mut state = engine.state.borrow_mut();
        let class = engine.reflection.class_id("ModuleScript").unwrap();
        crate::engine::instance::create_named(&mut state, &engine.reflection, class, "Test")
    };
    let env = engine_lua::modules::environment(&lua, &engine, script).expect("environment");
    let exports = lua.create_table().expect("exports");
    env.set("Exports", exports.clone()).expect("set Exports");
    let source = r#"
        local Gui = Instance.new("ScreenGui")
        Gui.Name = "Story"
        Gui.Parent = game:GetService("CoreGui")
        local Target = Instance.new("Frame")
        Target.Size = UDim2.fromScale(1, 1)
        Target.Parent = Gui
        local Panel = Instance.new("Frame")
        Panel.Size = UDim2.fromOffset(200, 200)
        Panel.Position = UDim2.fromOffset(0, 0)
        Panel.Parent = Target
        local Wrapper = Instance.new("Frame")
        Wrapper.Size = UDim2.fromOffset(180, 180)
        Wrapper.Position = UDim2.fromOffset(10, 10)
        Wrapper.Parent = Panel
        for I = 1, 20 do
            local Child = Instance.new("Frame")
            Child.Size = UDim2.fromOffset(40, 20)
            Child.Position = UDim2.fromOffset(0, (I - 1) * 8)
            Child.Parent = Wrapper
        end
        Exports.move_panel = function(X, Y)
            Panel.Position = UDim2.fromOffset(X, Y)
        end
    "#;
    let setup = lua.load(source).set_name("=Test").set_environment(env).into_function().expect("compile");
    let call = engine_lua::globals::call(&lua, &engine, setup, LuaMultiValue::new()).expect("call");
    if let CallOutcome::Failed(message, _) = call.outcome().unwrap() {
        panic!("setup failed: {message}");
    }
    let move_panel: LuaFunction = exports.get("move_panel").expect("move_panel fn");
    scheduler::run_until(&lua, &engine, 0.1).expect("initial layout");

    let before = super::stats_snapshot(&engine);
    let call = engine_lua::globals::call(
        &lua,
        &engine,
        move_panel,
        LuaMultiValue::from_vec(vec![LuaValue::Number(40.0), LuaValue::Number(25.0)]),
    )
    .expect("move");
    if let CallOutcome::Failed(message, _) = call.outcome().unwrap() {
        panic!("move_panel failed: {message}");
    }
    super::update_all(&engine);
    let after = super::stats_snapshot(&engine);
    let delta = after.delta_since(&before);

    assert_eq!(delta.passes, 1, "expected 1 layout pass: {delta:?}");
    assert_eq!(delta.scroll_moved, 0, "non-scroll parent move must not increment scroll_moved: {delta:?}");
    assert_eq!(delta.scroll_passes, 0, "non-scroll parent move must not increment scroll_passes: {delta:?}");
    assert_eq!(delta.moved, 22, "expected Panel + Wrapper + 20 children moved via same_shape/shift_children: {delta:?}");
    let ms = crate::cost::desktop_layout_cpu_ms(delta);
    assert!(
        ms > 0.005 && ms < 0.020,
        "non-scroll parent move must price at ~9 us, not >= 62 us scroll base: got {ms} ms ({delta:?})"
    );
    drop(lua);
}

/// Pins `shift_children`'s `stats.moved += 1` feeding `lazy_layout_price` (`install.rs`):
/// moving a non-scroll parent with 100 nested children and reading a child's `AbsolutePosition`
/// in the same Heartbeat charges `2.3 + (stored + 101) * 0.312` us (~34 us), not just the
/// parent (~3 us) and not a 62 us scroll pass.
#[test]
fn lazy_layout_read_after_non_scroll_parent_move_charges_moved_descendants() {
    let output = run(
        r#"
        return function(Target)
            local Panel = Instance.new("Frame")
            Panel.Size = UDim2.fromOffset(200, 200)
            Panel.Position = UDim2.fromOffset(0, 0)
            Panel.Parent = Target
            local Wrapper = Instance.new("Frame")
            Wrapper.Size = UDim2.fromOffset(180, 180)
            Wrapper.Parent = Panel
            local Children = {}
            for I = 1, 100 do
                local Child = Instance.new("Frame")
                Child.Size = UDim2.fromOffset(40, 10)
                Child.Position = UDim2.fromOffset(0, I)
                Child.Parent = Wrapper
                Children[I] = Child
            end
            local RunService = game:GetService("RunService")
            local function Twice()
                local T0 = os.clock()
                local _ = Children[1].AbsolutePosition
                local T1 = os.clock()
                local _ = Children[1].AbsolutePosition
                local T2 = os.clock()
                return ((T1 - T0) - (T2 - T1)) * 1e6
            end
            task.spawn(function()
                RunService.Heartbeat:Wait()
                RunService.Heartbeat:Wait()
                local _ = Children[1].AbsolutePosition
                Panel.Position = UDim2.fromOffset(30, 15)
                print(string.format("same %.3f", Twice()))
                RunService.Heartbeat:Wait()
                Panel.Position = UDim2.fromOffset(60, 15)
                RunService.PreRender:Wait()
                local _ = Children[2].AbsolutePosition
                local _ = Children[1].Name
                print(string.format("prerender %.3f", Twice()))
            end)
        end
        "#,
        1.0,
        1.0,
    );
    let value = |name: &str| -> f64 {
        let line = output
            .iter()
            .find(|line| line.contains(&format!(": {name} ")))
            .unwrap_or_else(|| panic!("{name} missing: {output:?}"));
        line.rsplit(' ').next().unwrap().parse().unwrap()
    };
    let same = value("same");
    assert!(
        (30.0..45.0).contains(&same),
        "same-Heartbeat read after moving parent + 101 descendants should pay ~34 us (2.3 + ~102 * 0.312), got {same:.2} us"
    );
    let after = value("prerender");
    assert!(
        after.abs() < 0.05,
        "PreRender read after the frame's layout point should pay ~0 us, got {after:.3} us"
    );
}

/// Direct `engine::layout` stats check for ScrollingFrame coalescing (`clamp_canvas_position`
/// does not force `ensure` on write) and separation of `scroll_moved`/`scroll_passes` from `moved`.
#[test]
fn scroll_stats_coalesce_within_frame_and_separate_from_moved() {
    let (lua, engine) = start(1.0);
    let script = {
        let mut state = engine.state.borrow_mut();
        let class = engine.reflection.class_id("ModuleScript").unwrap();
        crate::engine::instance::create_named(&mut state, &engine.reflection, class, "Test")
    };
    let env = engine_lua::modules::environment(&lua, &engine, script).expect("environment");
    let exports = lua.create_table().expect("exports");
    env.set("Exports", exports.clone()).expect("set Exports");
    let source = r#"
        local Gui = Instance.new("ScreenGui")
        Gui.Name = "Story"
        Gui.Parent = game:GetService("CoreGui")
        local Target = Instance.new("Frame")
        Target.Size = UDim2.fromScale(1, 1)
        Target.Parent = Gui
        local Scroller = Instance.new("ScrollingFrame")
        Scroller.Size = UDim2.fromOffset(200, 200)
        Scroller.CanvasSize = UDim2.fromOffset(0, 2000)
        Scroller.Parent = Target
        local First
        for I = 1, 15 do
            local Card = Instance.new("Frame")
            Card.Size = UDim2.fromOffset(100, 40)
            Card.Position = UDim2.fromOffset(0, (I - 1) * 50)
            Card.Parent = Scroller
            if I == 1 then First = Card end
        end
        Exports.act = function(Mode)
            if Mode == "coalesce" then
                Scroller.CanvasPosition = Vector2.new(0, 100)
                Scroller.CanvasPosition = Vector2.new(0, 200)
            elseif Mode == "read_between" then
                Scroller.CanvasPosition = Vector2.new(0, 300)
                local _ = First.AbsolutePosition
                Scroller.CanvasPosition = Vector2.new(0, 400)
                local _ = First.AbsolutePosition
            end
        end
    "#;
    let setup = lua.load(source).set_name("=Test").set_environment(env).into_function().expect("compile");
    let call = engine_lua::globals::call(&lua, &engine, setup, LuaMultiValue::new()).expect("call");
    if let CallOutcome::Failed(message, _) = call.outcome().unwrap() {
        panic!("setup failed: {message}");
    }
    let act: LuaFunction = exports.get("act").expect("act fn");
    scheduler::run_until(&lua, &engine, 0.1).expect("initial layout");

    // Two writes with no read between them coalesce into 1 scroll pass moving 15 records.
    let before = super::stats_snapshot(&engine);
    let call = engine_lua::globals::call(&lua, &engine, act.clone(), LuaMultiValue::from_vec(vec![LuaValue::String(lua.create_string("coalesce").unwrap())])).expect("coalesce");
    if let CallOutcome::Failed(message, _) = call.outcome().unwrap() {
        panic!("coalesce failed: {message}");
    }
    super::update_all(&engine);
    let delta = super::stats_snapshot(&engine).delta_since(&before);
    assert_eq!(delta.scroll_passes, 1, "two writes without a read must coalesce to 1 scroll pass: {delta:?}");
    assert_eq!(delta.scroll_moved, 15, "expected 15 scrolled cards: {delta:?}");
    assert_eq!(delta.moved, 0, "scroll must not count under non-scroll moved: {delta:?}");

    // Write -> read -> write -> read forces 2 scroll passes moving 30 records total.
    let before2 = super::stats_snapshot(&engine);
    let call2 = engine_lua::globals::call(&lua, &engine, act, LuaMultiValue::from_vec(vec![LuaValue::String(lua.create_string("read_between").unwrap())])).expect("read_between");
    if let CallOutcome::Failed(message, _) = call2.outcome().unwrap() {
        panic!("read_between failed: {message}");
    }
    let delta2 = super::stats_snapshot(&engine).delta_since(&before2);
    assert_eq!(delta2.scroll_passes, 2, "read between writes must force 2 scroll passes: {delta2:?}");
    assert_eq!(delta2.scroll_moved, 30, "expected 30 scrolled card moves across 2 passes: {delta2:?}");
    assert_eq!(delta2.moved, 0, "scroll must not count under non-scroll moved: {delta2:?}");
    drop(lua);
}
