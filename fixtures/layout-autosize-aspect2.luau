-- AutomaticSize frames with a UIAspectRatioConstraint, continued from layout-autosize-aspect.luau: the frame's padding,
-- scale-width content (a prompt's 0.95 header bar), the room automatic children get to grow into, content wider than
-- the base Size, MaxSize, and wrapped text. Each cell holds one automatic frame H with the constraint.
--   node cli/rur.mjs compare --fixture fixtures/layout-autosize-aspect2.luau --viewport 1800x1260 --time 0.2 --background 20,20,26
return function(Target)
	local Face = Font.new("rbxasset://fonts/families/SourceSansPro.json")
	local Count = 0
	local function Frame(Name, Parent, Size, Colour)
		local F = Instance.new("Frame")
		F.Name = Name
		F.Size = Size
		F.BackgroundColor3 = Colour or Color3.fromRGB(60, 60, 80)
		F.BackgroundTransparency = 0.4
		F.BorderSizePixel = 0
		F.Parent = Parent
		return F
	end
	-- Opts: auto, size, ratio, type, dominant, pad ({ left, right, top, bottom } offsets, or UDims), max (Vector2),
	-- at (H's offset in the cell, default 6, 6), build
	local function Case(Name, Opts)
		local Cell = Instance.new("Frame")
		Cell.Name = Name
		Cell.Position = UDim2.fromOffset((Count % 6) * 300, (Count // 6) * 180)
		Cell.Size = UDim2.fromOffset(296, 176)
		Cell.BackgroundColor3 = Color3.fromRGB(30, 30, 38)
		Cell.BorderSizePixel = 0
		Cell.Parent = Target
		Count += 1
		local H = Frame("H", Cell, Opts.size or UDim2.fromOffset(240, 60), Color3.fromRGB(200, 40, 50))
		H.AutomaticSize = Enum.AutomaticSize[Opts.auto or "Y"]
		H.Position = UDim2.fromOffset(Opts.at and Opts.at[1] or 6, Opts.at and Opts.at[2] or 6)
		local Aspect = Instance.new("UIAspectRatioConstraint")
		Aspect.AspectRatio = Opts.ratio or 3
		Aspect.AspectType = Enum.AspectType[Opts.type or "FitWithinMaxSize"]
		Aspect.DominantAxis = Enum.DominantAxis[Opts.dominant or "Width"]
		Aspect.Parent = H
		if Opts.pad then
			local Pad = Instance.new("UIPadding")
			local function Value(V)
				return if typeof(V) == "UDim" then V else UDim.new(0, V)
			end
			Pad.PaddingLeft, Pad.PaddingRight = Value(Opts.pad[1]), Value(Opts.pad[2])
			Pad.PaddingTop, Pad.PaddingBottom = Value(Opts.pad[3]), Value(Opts.pad[4])
			Pad.Parent = H
		end
		if Opts.max then
			local Constraint = Instance.new("UISizeConstraint")
			Constraint.MaxSize = Opts.max
			Constraint.Parent = H
		end
		Opts.build(H)
	end
	local function Box(W, Ht)
		return function(H)
			Frame("B", H, UDim2.fromOffset(W, Ht), Color3.fromRGB(40, 160, 60))
		end
	end
	local function Leaf(Size)
		return function(H)
			Frame("S", H, Size, Color3.fromRGB(40, 90, 200))
		end
	end
	-- a (1, 0) AutomaticSize Y panel holding an item (list or plain) of Size ItemSize, with an optional 100 x 20 child,
	-- and a W x Ht box
	local function Panel(UseList, ItemSize, ItemHolds, W, Ht)
		return function(H)
			local M = Frame("M", H, UDim2.fromScale(1, 0), Color3.fromRGB(40, 90, 200))
			M.AutomaticSize = Enum.AutomaticSize.Y
			if UseList then
				local L = Instance.new("UIListLayout")
				L.HorizontalAlignment = Enum.HorizontalAlignment.Center
				L.SortOrder = Enum.SortOrder.LayoutOrder
				L.Parent = M
			end
			local Item = Frame("I", M, ItemSize, Color3.fromRGB(230, 200, 40))
			Item.LayoutOrder = 1
			if ItemHolds then
				local C = Frame("C", Item, UDim2.fromOffset(100, 20), Color3.fromRGB(240, 240, 240))
				C.AnchorPoint = Vector2.new(0.5, 0.5)
				C.Position = UDim2.fromScale(0.5, 0.5)
			end
			if W then
				local B = Frame("B", M, UDim2.fromOffset(W, Ht), Color3.fromRGB(40, 160, 60))
				B.LayoutOrder = 2
			end
		end
	end
	-- a (1, 0.8) AutomaticSize Y panel with a list of scale gap Gap and Pad px of padding, holding two 120 x 24 items
	local function GapList(Gap, Pad, Horizontal)
		return function(H)
			local M = Frame("M", H, if Horizontal then UDim2.fromScale(0.8, 1) else UDim2.fromScale(1, 0.8), Color3.fromRGB(40, 90, 200))
			M.AutomaticSize = if Horizontal then Enum.AutomaticSize.X else Enum.AutomaticSize.Y
			local L = Instance.new("UIListLayout")
			L.Padding = UDim.new(Gap, 0)
			L.FillDirection = if Horizontal then Enum.FillDirection.Horizontal else Enum.FillDirection.Vertical
			L.SortOrder = Enum.SortOrder.LayoutOrder
			L.Parent = M
			local P = Instance.new("UIPadding")
			P.PaddingTop, P.PaddingBottom = UDim.new(0, Pad), UDim.new(0, Pad)
			P.PaddingLeft, P.PaddingRight = UDim.new(0, Pad), UDim.new(0, Pad)
			P.Parent = M
			for Index = 1, 2 do
				local B = Frame(`B{Index}`, M, if Horizontal then UDim2.fromOffset(24, 40) else UDim2.fromOffset(120, 24), Color3.fromRGB(40, 160, 60))
				B.LayoutOrder = Index
			end
		end
	end
	local function Text(Text, Size)
		return function(H)
			local T = Instance.new("TextLabel")
			T.Name = "T"
			T.AutomaticSize = Enum.AutomaticSize.Y
			T.BackgroundTransparency = 1
			T.FontFace = Face
			T.Size = UDim2.new(1, -20, 0, 0)
			T.Text = Text
			T.TextColor3 = Color3.new(1, 1, 1)
			T.TextSize = Size
			T.TextWrapped = true
			T.Parent = H
		end
	end

	-- the frame's padding
	Case("pad10_b100x100", { pad = { 10, 10, 10, 10 }, build = Box(100, 100) })
	Case("padtop10_b100x100", { pad = { 0, 0, 10, 0 }, build = Box(100, 100) })
	Case("padbottom10_b100x100", { pad = { 0, 0, 0, 10 }, build = Box(100, 100) })
	Case("padx30_b100x100", { pad = { 30, 30, 0, 0 }, build = Box(100, 100) })
	Case("padx60_b100x100", { pad = { 60, 60, 0, 0 }, build = Box(100, 100) })
	Case("pady30_b100x40", { pad = { 0, 0, 30, 30 }, build = Box(100, 40) })
	Case("h0_pad10_b100x40", { size = UDim2.fromOffset(240, 0), pad = { 10, 10, 10, 10 }, build = Box(100, 40) })
	Case("pady01_b100x100", { pad = { 0, 0, UDim.new(0.1, 0), UDim.new(0.1, 0) }, build = Box(100, 100) })
	-- scale widths in the content
	Case("leaf095x100", { build = Leaf(UDim2.new(0.95, 0, 0, 100)) })
	Case("list_item095_b100x60", { build = Panel(true, UDim2.new(0.95, 0, 0, 40), false, 100, 60) })
	Case("list_item095holds_b100x60", { build = Panel(true, UDim2.new(0.95, 0, 0, 40), true, 100, 60) })
	Case("plain_item095x100", { build = Panel(false, UDim2.new(0.95, 0, 0, 100), false) })
	Case("plain_item095x100holds", { build = Panel(false, UDim2.new(0.95, 0, 0, 100), true) })
	-- the room automatic children grow into
	Case("gap01_low", { at = { 6, 120 }, build = GapList(0.1, 8) })
	Case("gap01_r2", { ratio = 2, build = GapList(0.1, 8) })
	Case("gap01_pady5", { pad = { 0, 0, 5, 5 }, build = GapList(0.1, 8) })
	Case("gap01_swpW", { type = "ScaleWithParentSize", build = GapList(0.1, 8) })
	Case("gap01_swpH", { type = "ScaleWithParentSize", dominant = "Height", build = GapList(0.1, 8) })
	Case("gap01_x", { auto = "X", size = UDim2.fromOffset(60, 120), ratio = 0.5, build = GapList(0.1, 8, true) })
	Case("gap01_max", { max = Vector2.new(math.huge, 50), build = GapList(0.1, 8) })
	-- content wider than the fitted or the base width
	Case("b200x100", { build = Box(200, 100) })
	Case("b260x100", { build = Box(260, 100) })
	Case("b260x40", { build = Box(260, 40) })
	Case("x_b40x100", { auto = "X", size = UDim2.fromOffset(60, 60), ratio = 0.5, build = Box(40, 100) })
	Case("xy_b150x90", { auto = "XY", size = UDim2.fromOffset(100, 60), ratio = 2, build = Box(150, 90) })
	Case("max200_b220x100", { max = Vector2.new(200, math.huge), build = Box(220, 100) })
	-- wrapped text
	Case("text_short", { build = Text("Choose one of the two rooms", 18) })
	Case("text_long", { build = Text("Which units would you like to unequip from the team right now?", 18) })
	Case("text_long_h0", { size = UDim2.fromOffset(240, 0), build = Text("Which units would you like to unequip from the team right now?", 18) })
	Case("text_long_r2", { ratio = 2, build = Text("Which units would you like to unequip from the team right now?", 18) })
end
