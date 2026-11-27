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
    start_in(pixel_scale, None)
}

/// [`start`] with the viewport `viewport` (logical px) instead of the default one.
fn start_in(pixel_scale: f64, viewport: Option<(f64, f64)>) -> (Lua, Rc<Engine>) {
    let mut options = Options { pixel_scale, ..crate::engine::options() };
    if viewport.is_some() {
        options.profile.device.viewport = viewport;
    }
    let engine = crate::engine::new_engine(options).expect("engine");
    let lua = Lua::new();
    engine_lua::start(&lua, &engine).expect("start");
    (lua, engine)
}

/// Runs `source` (a chunk returning `function(Target)`) against a full-viewport Target frame in a
/// CoreGui ScreenGui, runs virtual time to `until`, and returns the output lines.
fn run(source: &str, pixel_scale: f64, until: f64) -> Vec<String> {
    run_with_fonts(source, pixel_scale, until, None)
}

/// [`run`], its text engine reading `fonts` (a test's own font set, engine::cli `own_fonts`) instead of the machine's
fn run_with_fonts(source: &str, pixel_scale: f64, until: f64, fonts: Option<&crate::engine::text::FontPaths>) -> Vec<String> {
    run_in(source, pixel_scale, until, fonts, None)
}

/// [`run_with_fonts`] in the viewport `viewport` (logical px).
fn run_in(source: &str, pixel_scale: f64, until: f64, fonts: Option<&crate::engine::text::FontPaths>, viewport: Option<(f64, f64)>) -> Vec<String> {
    let (lua, engine) = start_in(pixel_scale, viewport);
    if let Some(fonts) = fonts {
        use crate::engine::GuiEngine;
        *engine.text().borrow_mut() = Some(crate::engine::text::TextEngine::new(fonts.clone()));
    }
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
/// handler pays the scroll's pass (123 us and 0.051 a record moved); moved in Heartbeat and read in the next frame's PreRender,
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
    assert!((120.0..130.0).contains(&same), "a read after a scroll in its handler pays {same:.2} us, a 0.747 client 123 + 0.05 a card");
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

/// TextBounds, an automatic label's size and TextService's sizes are the text's logical size rounded up to a whole
/// logical px (FFlagTextBoundsRoundUpToLogicalPixels, on in production clients): a Roblox 0.747 client at 1.5x
/// reports these (text/measure.rs text_bounds; unrounded, "Max Players" is 79.333 px, under UIScale 0.9 72.667).
/// The client's widths are Source Sans Pro's, so they are checked where Studio's fonts are; without them (CI) the
/// test font stands in for Source Sans, and the same lines must come out with its widths, each a whole px.
#[test]
fn text_bounds_round_up_to_whole_logical_pixels() {
    let script = r#"
        return function(Target)
            local Face = Font.new("rbxasset://fonts/families/SourceSansPro.json")
            local function Label(Parent, Rich, Automatic)
                local L = Instance.new("TextLabel")
                L.FontFace = Face
                L.TextSize = 20
                L.RichText = Rich
                L.Text = "Max Players"
                if Automatic then
                    L.AutomaticSize = Enum.AutomaticSize.XY
                    L.Size = UDim2.new()
                else
                    L.Size = UDim2.fromOffset(280, 100)
                end
                L.Parent = Parent
                return L
            end
            local Scaled = Instance.new("Frame")
            Scaled.Size = UDim2.fromOffset(300, 200)
            Scaled.Parent = Target
            local UIScale = Instance.new("UIScale")
            UIScale.Scale = 0.9
            UIScale.Parent = Scaled
            local Cases = { plain = Label(Target, false, false), rich = Label(Target, true, false), auto = Label(Target, false, true), scaled = Label(Scaled, true, false), scaledAuto = Label(Scaled, true, true) }
            task.spawn(function()
                task.wait(0.05)
                for _, Name in { "plain", "rich", "auto", "scaled", "scaledAuto" } do
                    local L = Cases[Name]
                    print(string.format("%s %.3f %.3f %.3f %.3f", Name, L.TextBounds.X, L.TextBounds.Y, L.AbsoluteSize.X, L.AbsoluteSize.Y))
                end
                local Size = game:GetService("TextService"):GetTextSize("Hello world", 17, Enum.Font.SourceSans, Vector2.new(1000, 1000))
                print(string.format("GetTextSize %.3f %.3f", Size.X, Size.Y))
            end)
        end
    "#;
    // (the widths: "Max Players" at 20, under UIScale 0.9, "Hello world" at 17)
    let expected = |plain: &str, scaled: &str, service: &str| {
        vec![
            format!("plain {plain} 20.000 280.000 100.000"),
            format!("rich {plain} 20.000 280.000 100.000"),
            format!("auto {plain} 20.000 {plain} 20.000"),
            format!("scaled {scaled} 18.000 252.000 90.000"),
            format!("scaledAuto {scaled} 18.000 {scaled} 18.000"),
            format!("GetTextSize {service} 17.000"),
        ]
    };
    if crate::engine::text::fonts::discover_content_root().is_some() {
        let lines = run(script, 1.5, 0.2);
        let printed: Vec<&str> = lines.iter().filter_map(|line| line.strip_prefix("print: ")).collect();
        assert_eq!(printed, expected("80.000", "73.000", "65.000"), "{lines:#?}");
        return;
    }
    if crate::assets::tests::test_font().is_none() {
        eprintln!("skipped: no Studio content root and no test font (set RUR_TEST_FONT)");
        return;
    }
    let directory = std::env::temp_dir().join(format!("rur-text-bounds-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    std::fs::create_dir_all(&directory).unwrap();
    let fonts = crate::engine::cli::own_fonts(&directory, &crate::engine::default_repo());
    let lines = run_with_fonts(script, 1.5, 0.2, Some(&fonts));
    std::fs::remove_dir_all(&directory).unwrap();
    let printed: Vec<&str> = lines.iter().filter_map(|line| line.strip_prefix("print: ")).collect();
    let width = |line: usize| printed.get(line).and_then(|line| line.split(' ').nth(1)).unwrap_or_default();
    let (plain, scaled, service) = (width(0), width(3), width(5));
    for value in [plain, scaled, service] {
        let whole = value.parse::<f64>().is_ok_and(|value| value > 0.0 && value.fract() == 0.0);
        assert!(whole, "the test font's widths are whole px: {lines:#?}");
    }
    assert_eq!(printed, expected(plain, scaled, service), "{lines:#?}");
}

/// AutomaticSize with UIPadding and UISizeConstraint against a Roblox 0.747 client's rects
/// (data/studio/whole/autosize-constraint.json): on an automatic axis the constraint bounds the content inside the
/// padding offsets (MinSize 22 and 6 + 6 px of padding make a label holding 8 px of text 34 wide), MaxSize then caps
/// the object but its children still push it past the cap, an automatic parent measures a capped child at the size
/// its content asked for, and a MinSize above MaxSize switches the constraint off. Automatic labels take their
/// TextBounds rounded up to whole logical px (text/measure.rs text_bounds), as the client does. The client's text
/// widths are its fonts': without Studio's fonts (CI) only the cells holding no text are compared.
#[test]
fn automatic_size_constraints_bound_the_content_inside_the_padding() {
    let fonts = crate::engine::text::fonts::discover_content_root().is_some();
    let studio: serde_json::Value = serde_json::from_str(include_str!("../../../../data/studio/whole/autosize-constraint.json")).expect("measurements");
    let fixtures = [
        ("layout-autosize-constraint", include_str!("../../../../fixtures/layout-autosize-constraint.luau")),
        ("layout-autosize-constraint2", include_str!("../../../../fixtures/layout-autosize-constraint2.luau")),
    ];
    let (mut compared, mut misses) = (0, Vec::new());
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
        // (cells with a text object: their rects follow the text's width)
        let text_cells: std::collections::HashSet<String> = expected.iter().filter(|(_, want)| values(want).len() == 6).map(|(path, _)| cell_of(path)).collect();
        for (path, want) in expected {
            if !fonts && text_cells.contains(&cell_of(path)) {
                continue;
            }
            let (want, got) = (values(want), values(&ours[path]));
            compared += 1;
            // (rect, then TextBounds for text objects)
            let same = got.len() == want.len() && (0..want.len()).all(|k| (want[k] - got[k]).abs() < 0.01);
            if same {
                continue;
            }
            misses.push(format!("{name} {path}: Studio {want:?}, ours {got:?}"));
        }
    }
    assert!(misses.is_empty(), "{} of {compared} rects differ from Studio's:\n{}", misses.len(), misses.join("\n"));
    assert!(compared > if fonts { 260 } else { 220 }, "only {compared} rects compared");
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

/// Every GuiObject's AbsolutePosition and AbsoluteSize below the target, from the target's origin (`%.9g`: each value
/// reads back to its float32), and the Text of the labels named T, as the fixture `source` leaves them in `viewport`.
fn float32_rects(source: &str, viewport: (f64, f64)) -> (std::collections::HashMap<String, [f32; 4]>, std::collections::HashMap<String, String>) {
    let script = format!(
        r#"
        local Fixture = (function() {source} end)()
        return function(Target)
            Fixture(Target)
            task.spawn(function()
                task.wait(0.1)
                local function Visit(Node, Path)
                    for _, Child in Node:GetChildren() do
                        local At = Path .. "/" .. Child.Name
                        if Child:IsA("GuiBase2d") then
                            local P, S = Child.AbsolutePosition - Target.AbsolutePosition, Child.AbsoluteSize
                            print(string.format("rect %s %.9g %.9g %.9g %.9g", At, P.X, P.Y, S.X, S.Y))
                            if (Child.Name == "T" or string.sub(Child.Name, 1, 2) == "U_") and Child:IsA("TextLabel") then
                                print(string.format("text %s %s", At, Child.Text))
                            end
                        end
                        if Child:IsA("GuiObject") or Child:IsA("ScreenGui") or Child:IsA("Folder") then
                            Visit(Child, At)
                        end
                    end
                end
                Visit(Target, "")
            end)
        end
        "#
    );
    let lines = run_in(&script, 1.5, 0.5, None, Some(viewport));
    let (mut rects, mut texts) = (std::collections::HashMap::new(), std::collections::HashMap::new());
    for line in &lines {
        if let Some(rest) = line.strip_prefix("print: rect ") {
            let parts: Vec<&str> = rest.split(' ').collect();
            let value = |k: usize| parts[k].parse::<f64>().unwrap() as f32;
            rects.insert(parts[0].to_string(), [value(1), value(2), value(3), value(4)]);
        } else if let Some(rest) = line.strip_prefix("print: text ") {
            let (path, text) = rest.split_once(' ').unwrap_or((rest, ""));
            texts.insert(path.to_string(), text.to_string());
        }
    }
    assert!(!rects.is_empty(), "no rects: {lines:#?}");
    (rects, texts)
}

/// The list, grid and canvas fixtures a client was measured on in float32 (data/studio/whole/layout-f32.json), with
/// their viewports.
const FLOAT32_FIXTURES: [(&str, &str, (f64, f64)); 5] = [
    ("layout-f32-lists", include_str!("../../../../fixtures/layout-f32-lists.luau"), (1200.0, 900.0)),
    ("layout-f32-grids", include_str!("../../../../fixtures/layout-f32-grids.luau"), (2160.0, 1270.0)),
    ("layout-f32-lists2", include_str!("../../../../fixtures/layout-f32-lists2.luau"), (2160.0, 1270.0)),
    ("layout-canvas-unparented", include_str!("../../../../fixtures/layout-canvas-unparented.luau"), (1600.0, 880.0)),
    ("layout-nested-screengui", include_str!("../../../../fixtures/layout-nested-screengui.luau"), (1600.0, 1000.0)),
];

/// Not reproduced: children of a ScrollingFrame's canvas in the list fixtures, an ulp off in about a fifth of their values
/// (their canvas basis is neither the canvas size nor its corner-read AbsoluteCanvasSize), and the AbsoluteWindowSize
/// layout-f32-lists2 reads (an ulp off in 9 of 48 values).
fn float32_known(name: &str, path: &str) -> bool {
    (name.starts_with("layout-f32-") && (path.contains("_S/") || path.contains("/S/") || (path.starts_with("/s") && path.ends_with("/T"))))
        || (name == "layout-canvas-unparented" && path.starts_with("/cell_scaled_over/S/C"))
}

/// UIListLayout, UIGridLayout and a ScrollingFrame's canvas against a Roblox 0.747 client bit for bit (float32;
/// data/studio/whole/layout-f32.json): lists walk their items in float32 local units (the next item at
/// f32(f32(cursor + size) + gap)), grids size each item f32(f32(cell + pad) - pad) and place it by its cell's centre
/// with an AnchorPoint of 0.5, AbsoluteCellSize is unscaled, AbsoluteContentSize is f32(content x UIScale), and
/// AbsoluteCanvasSize reads back through the canvas's far corner. P4d: a ScrollingFrame with no GUI above it takes its
/// CanvasSize's scale against its own size (so a CanvasPosition written before parenting clamps there), a negative
/// AbsoluteWindowSize reads as its size, the canvas scrolls by the float32 CanvasPosition rounded to device pixels
/// (layout-canvas-unparented); a ScreenGui inside a GuiObject is laid out over the whole screen, unclipped, unturned and
/// unscaled by what holds it (layout-nested-screengui).
#[test]
fn lists_grids_and_canvases_lay_out_in_a_clients_float32() {
    let studio: serde_json::Value = serde_json::from_str(include_str!("../../../../data/studio/whole/layout-f32.json")).expect("measurements");
    // (without Studio's fonts the automatic labels of layout-f32-lists, `H1_T`.., measure 0 px: CI checks the rest)
    let fonts = crate::engine::text::fonts::discover_content_root().is_some();
    let (mut compared, mut misses, mut texts_compared) = (0, Vec::new(), 0);
    for (name, source, viewport) in FLOAT32_FIXTURES {
        let (rects, texts) = float32_rects(source, viewport);
        let expected = &studio["fixtures"][name];
        for (path, want) in expected["rects"].as_object().unwrap() {
            if float32_known(name, path) || (!fonts && path.ends_with("_T")) {
                continue;
            }
            let want: Vec<f32> = rect_values(want).iter().map(|v| *v as f32).collect();
            let got = rects.get(path).unwrap_or_else(|| panic!("{name} {path} missing"));
            compared += 1;
            if (0..4).any(|k| want[k].to_bits() != got[k].to_bits()) {
                misses.push(format!("{name} {path}: client {want:?}, ours {got:?}"));
            }
        }
        for (path, want) in expected["texts"].as_object().unwrap() {
            if float32_known(name, path) {
                continue;
            }
            texts_compared += 1;
            let got = texts.get(path).map(String::as_str).unwrap_or("");
            if got != want.as_str().unwrap() {
                misses.push(format!("{name} {path}: client reads {want}, ours {got}"));
            }
        }
    }
    assert!(misses.is_empty(), "{} of {compared} rects and {texts_compared} reads differ from the client's:
{}", misses.len(), misses.join("
"));
    assert!(compared >= 1300 && texts_compared >= 120, "only {compared} rects and {texts_compared} reads compared");
}

/// The client's measurements of the AutomaticSize + UIAspectRatioConstraint fixtures
/// (data/studio/whole/autosize-aspect.json).
fn aspect_measured() -> serde_json::Value {
    serde_json::from_str(include_str!("../../../../data/studio/whole/autosize-aspect.json")).expect("measurements")
}

/// The AutomaticSize + UIAspectRatioConstraint fixtures the client was measured on.
const ASPECT_FIXTURES: [(&str, &str); 14] = [
    ("layout-autosize-aspect", include_str!("../../../../fixtures/layout-autosize-aspect.luau")),
    ("layout-autosize-aspect2", include_str!("../../../../fixtures/layout-autosize-aspect2.luau")),
    ("layout-autosize-aspect3", include_str!("../../../../fixtures/layout-autosize-aspect3.luau")),
    ("layout-autosize-aspect4", include_str!("../../../../fixtures/layout-autosize-aspect4.luau")),
    ("layout-autosize-aspect5", include_str!("../../../../fixtures/layout-autosize-aspect5.luau")),
    ("layout-autosize-aspect-probes", include_str!("../../../../fixtures/layout-autosize-aspect-probes.luau")),
    ("layout-autosize-aspect-panels", include_str!("../../../../fixtures/layout-autosize-aspect-panels.luau")),
    ("layout-autosize-prompt", include_str!("../../../../fixtures/layout-autosize-prompt.luau")),
    ("layout-autosize-aspect-view", include_str!("../../../../fixtures/layout-autosize-aspect-view.luau")),
    ("layout-autosize-aspect6", include_str!("../../../../fixtures/layout-autosize-aspect6.luau")),
    ("layout-autosize-aspect7", include_str!("../../../../fixtures/layout-autosize-aspect7.luau")),
    ("layout-autosize-aspect8", include_str!("../../../../fixtures/layout-autosize-aspect8.luau")),
    ("layout-autosize-aspect9", include_str!("../../../../fixtures/layout-autosize-aspect9.luau")),
    ("layout-autosize-aspect10", include_str!("../../../../fixtures/layout-autosize-aspect10.luau")),
];

/// Not reproduced (LA9d): the children of an XY frame of a 0-wide base whose ratio box is shorter than its top and bottom
/// padding (0 x 0 of ratio 2 with 20 + 20 px around a 60 x 80 box: the frame is 60 x 80 as in the client, its box 60 x 30)
/// start 10 px down in the client (the box less the bottom padding), 20 here, and a (1, 1) frame there is 50 tall (to the
/// frame's height less the bottom padding), 30 here.
const ASPECT_KNOWN: [&str; 5] = [
    "/b0x0_r2_t20b20_b60x80/H/B",
    "/b0x0_r2_t20b20_full_b60x80/H/B",
    "/b0x0_r2_t20b20_full_b60x80/H/F",
    "/b0x60_r2_t20b20_full_b60x80/H/B",
    "/b0x60_r2_t20b20_full_b60x80/H/F",
];

/// Automatic frames with a UIAspectRatioConstraint against a Roblox 0.747 client's rects
/// (data/studio/whole/autosize-aspect.json): the content's natural size inside the padding, fitted to the ratio, plus
/// the top and bottom padding, times the ratio is the width, at least the base Size fitted to the ratio and, on an
/// axis that is not automatic, at most the base Size; the height follows the width; the children lay out in that box
/// and push the automatic axes further. Automatic children grow into the box the ratio makes of the base width and
/// the room the frame has. On an automatic axis scale children size against the box less the padding or what the
/// content holds up to the box; a frame's own UIListLayout or UIGridLayout content counts as what it holds. Cells
/// without text (the text cells need Studio's fonts, see the next test).
#[test]
fn aspect_constrained_automatic_frames_fit_their_base_size_first() {
    let studio = aspect_measured();
    let (mut compared, mut misses, mut known) = (0, Vec::new(), 0);
    for (name, source) in ASPECT_FIXTURES {
        let ours = cell_rects(source, 1.5);
        let expected = studio["fixtures"][name].as_object().unwrap();
        let cell_of = |path: &str| path.split('/').nth(1).unwrap_or("").to_string();
        let text_cells: std::collections::HashSet<String> = expected.iter().filter(|(_, want)| rect_values(want).len() == 6).map(|(path, _)| cell_of(path)).collect();
        for (path, want) in expected {
            if text_cells.contains(&cell_of(path)) {
                continue;
            }
            let (want, got) = (rect_values(want), rect_values(&ours[path]));
            compared += 1;
            let same = got.len() >= 4 && (0..4).all(|k| (want[k] - got[k]).abs() < 0.01);
            if ASPECT_KNOWN.contains(&path.as_str()) {
                assert!(!same, "{name} {path} now matches the client: take it out of ASPECT_KNOWN");
                known += 1;
            } else if !same {
                misses.push(format!("{name} {path}: client {:?}, ours {got:?}", &want[..4]));
            }
        }
    }
    assert!(misses.is_empty(), "{} of {compared} rects differ from the client's:\n{}", misses.len(), misses.join("\n"));
    assert_eq!(known, ASPECT_KNOWN.len());
    assert!(compared >= 615, "only {compared} rects compared");
}

/// Wrapped labels in automatic frames with a UIAspectRatioConstraint against a Roblox 0.747 client
/// (data/studio/whole/autosize-aspect.json): on the axis that is not automatic a label measures against the frame's
/// base Size, so a (1, -20) label in a 240 x 60 frame of ratio 3 keeps the two lines it has at 220 px while the frame
/// is 180 wide. Rects and TextBounds within the client's whole-px TextBounds.
#[test]
fn aspect_constrained_automatic_frames_measure_text_against_their_base_size() {
    if crate::engine::text::fonts::discover_content_root().is_none() {
        eprintln!("skipped: no Studio content root (the cases need Studio's fonts)");
        return;
    }
    let studio = aspect_measured();
    let (mut compared, mut misses) = (0, Vec::new());
    for (name, source) in ASPECT_FIXTURES {
        let expected = studio["fixtures"][name].as_object().unwrap();
        let cell_of = |path: &str| path.split('/').nth(1).unwrap_or("").to_string();
        let text_cells: std::collections::HashSet<String> = expected.iter().filter(|(_, want)| rect_values(want).len() == 6).map(|(path, _)| cell_of(path)).collect();
        if text_cells.is_empty() {
            continue;
        }
        let ours = cell_rects(source, 1.5);
        for (path, want) in expected {
            if !text_cells.contains(&cell_of(path)) {
                continue;
            }
            let (want, got) = (rect_values(want), rect_values(&ours[path]));
            compared += 1;
            if !(got.len() == want.len() && (0..want.len()).all(|k| (want[k] - got[k]).abs() < 1.0)) {
                misses.push(format!("{name} {path}: client {want:?}, ours {got:?}"));
            }
        }
    }
    assert!(misses.is_empty(), "{} of {compared} rects differ from the client's:\n{}", misses.len(), misses.join("\n"));
    assert!(compared >= 79, "only {compared} rects compared");
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
/// a second (scroll) pass.
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
    let expected = crate::cost::CLIENT_LAYOUT_PASS_MS + delta.stored as f64 * crate::cost::CLIENT_LAYOUT_STORED_MS + 22.0 * crate::cost::CLIENT_LAYOUT_MOVED_MS;
    assert!(
        (ms - expected).abs() < 1e-12,
        "non-scroll parent move must price one pass, its stored records and 22 moved records ({expected} ms), not a second scroll pass: got {ms} ms ({delta:?})"
    );
    drop(lua);
}

/// The objects a client lays out again although they did not change (P11b, measured in a 0.747 client, notes/cost-calibration.md
/// "Re-laid objects"): after a geometry write its whole tree (the unchanged children of every object the pass lays out, and
/// what is under them; `relaid`, `relaidUnder`), the written object's own siblings for a script's read (`relaid_near`); a Text
/// write in a fixed box, LayoutOrder without a layout object and another ScreenGui's tree none; a ZIndex write sorts the tree's
/// records again (`resorted`) without a pass.
#[test]
fn a_geometry_write_relays_its_tree_and_a_text_write_does_not() {
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
        local C = Instance.new("Frame")
        C.Size = UDim2.fromScale(1, 1)
        C.Parent = Target
        local Kids = {}
        for I = 1, 50 do
            local F = Instance.new("Frame")
            F.Size = UDim2.fromOffset(10, 10)
            F.Position = UDim2.fromOffset((I % 10) * 12, (I // 10) * 12)
            F.Parent = C
            Kids[I] = F
        end
        for J = 1, 4 do
            local G = Instance.new("Frame")
            G.Size = UDim2.fromScale(0.5, 0.5)
            G.Parent = Kids[2]
        end
        local Label = Instance.new("TextLabel")
        Label.Size = UDim2.fromOffset(60, 20)
        Label.Text = "A"
        Label.Parent = C
        local Grid = Instance.new("Frame")
        Grid.Size = UDim2.fromOffset(300, 300)
        Grid.Parent = Target
        Instance.new("UIGridLayout").Parent = Grid
        local Cells = {}
        for I = 1, 6 do
            local F = Instance.new("Frame")
            F.LayoutOrder = I
            F.Parent = Grid
            Cells[I] = F
        end
        local Other = Instance.new("ScreenGui")
        Other.Parent = game:GetService("CoreGui")
        for I = 1, 30 do
            Instance.new("Frame").Parent = Other
        end
        local R = 0
        Exports.resize = function() R += 1 Kids[1].Size = UDim2.fromOffset(10 + R, 10) end
        Exports.text = function() R += 1 Label.Text = "A" .. R end
        Exports.zindex = function() R += 1 Kids[3].ZIndex = 1 + R end
        Exports.order = function() R += 1 Kids[1].LayoutOrder = R end
        Exports.cellorder = function() R += 1 Cells[1].LayoutOrder = Cells[1].LayoutOrder == 1 and 0 or 1 end
    "#;
    let setup = lua.load(source).set_name("=Test").set_environment(env).into_function().expect("compile");
    let call = engine_lua::globals::call(&lua, &engine, setup, LuaMultiValue::new()).expect("call");
    if let CallOutcome::Failed(message, _) = call.outcome().unwrap() {
        panic!("setup failed: {message}");
    }
    scheduler::run_until(&lua, &engine, 0.1).expect("initial layout");
    let step = |name: &str| {
        let f: LuaFunction = exports.get(name).expect("export");
        let before = super::stats_snapshot(&engine);
        let call = engine_lua::globals::call(&lua, &engine, f, LuaMultiValue::new()).expect("call");
        if let CallOutcome::Failed(message, _) = call.outcome().unwrap() {
            panic!("{name} failed: {message}");
        }
        super::update_all(&engine);
        super::stats_snapshot(&engine).delta_since(&before)
    };
    // A child resized among 49 frames and a label: they are re-laid, with the 4 frames under one of them, and the grid beside
    // the container (with its 6 cells); the other ScreenGui is not
    let resize = step("resize");
    assert_eq!(resize.passes, 1, "{resize:?}");
    assert_eq!(resize.relaid, 51, "the resized frame's 50 siblings and the grid beside its container: {resize:?}");
    assert_eq!(resize.relaid_under, 4 + 6, "the frames under a sibling and the grid's cells: {resize:?}");
    assert_eq!(resize.relaid_near, 50, "a read lays out the written frame's siblings: {resize:?}");
    let ms = crate::cost::desktop_layout_cpu_ms(resize);
    let without = crate::cost::desktop_layout_cpu_ms(super::Stats { relaid: 0, relaid_under: 0, ..resize });
    let priced = 51.0 * crate::cost::CLIENT_LAYOUT_RELAID_MS + 10.0 * crate::cost::CLIENT_LAYOUT_RELAID_UNDER_MS;
    assert!((ms - without - priced).abs() < 1e-12, "re-laid records are priced: {ms} vs {without} + {priced}");
    // A Text write in a fixed box: a pass, nothing re-laid
    let text = step("text");
    assert_eq!((text.passes, text.texts, text.relaid, text.relaid_under), (1, 1, 0, 0), "{text:?}");
    // LayoutOrder without a layout object: nothing re-laid; beside a UIGridLayout: the grid's other cells and the tree
    let order = step("order");
    assert_eq!(order.relaid, 0, "{order:?}");
    let cell = step("cellorder");
    assert!(cell.relaid >= 5, "a grid cell's LayoutOrder re-lays its siblings: {cell:?}");
    // A ZIndex write: no pass, the tree's records sorted again (the ScreenGui, Target, the container, 50 frames, 4 under one,
    // the label, the grid and its 6 cells)
    let z = step("zindex");
    assert_eq!(z.passes, 0, "{z:?}");
    assert_eq!(z.resorted, 1 + 1 + 1 + 50 + 4 + 1 + 1 + 6, "{z:?}");
    drop(lua);
}

/// Pins `shift_children`'s `stats.moved += 1` feeding `lazy_layout_price` (`install.rs`):
/// moving a non-scroll parent with 100 nested children and reading a child's `AbsolutePosition`
/// in the same Heartbeat charges `23 + (stored + 101) * 0.312` us (~55 us), not just the
/// pass (~24 us) and not a 123 us scroll pass.
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
        (50.0..62.0).contains(&same),
        "same-Heartbeat read after moving parent + 101 descendants should pay ~55 us (23 + ~102 * 0.312), got {same:.2} us"
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

/// A joiner (ZWJ, ZWNJ) is one grapheme cluster with the character before it, and the cluster takes a face that has
/// both: in SourceSans, which has no joiner, "A\u{200D}" measures as Arimo's "A", "A\u{200C}B" as "A\u{200D}B", and the B
/// after a joiner stays SourceSans' (a Roblox 0.747 client, fixtures/text-probe-zwj.luau). Needs Studio's fonts.
#[test]
fn a_joiner_takes_the_character_before_it_into_a_face_that_has_both() {
    if crate::engine::text::fonts::discover_content_root().is_none() {
        eprintln!("skipped: needs Studio's fonts (SourceSans without a joiner, Arimo with one)");
        return;
    }
    let script = r#"
        return function(Target)
            local TextService = game:GetService("TextService")
            local function Width(Text, Family)
                local L = Instance.new("TextLabel")
                L.FontFace = Font.new("rbxasset://fonts/families/" .. Family .. ".json")
                L.TextSize = 32
                L.Size = UDim2.fromOffset(400, 50)
                L.Text = Text
                L.Parent = Target
                return L.TextBounds.X
            end
            print(Width("A\u{200D}", "SourceSansPro"), Width("A", "Arimo"))
            print(Width("A\u{200C}B", "SourceSansPro"), Width("A\u{200D}B", "SourceSansPro"))
            print(Width("\u{200D}B", "SourceSansPro"), Width("B", "SourceSansPro"))
            print(Width("A", "SourceSansPro") ~= Width("A", "Arimo"))
        end
    "#;
    let lines = run(script, 1.5, 0.2);
    let printed: Vec<&str> = lines.iter().filter_map(|line| line.strip_prefix("print: ")).collect();
    assert_eq!(printed.len(), 4, "{lines:#?}");
    for pair in &printed[..3] {
        let values: Vec<&str> = pair.split(' ').collect();
        assert_eq!(values[0], values[1], "{lines:#?}");
    }
    assert_eq!(printed[3], "true", "the two faces' A differ, so the first line is not vacuous: {lines:#?}");
}
