-- AutomaticSize frames with a UIAspectRatioConstraint (the choice prompts' holders, layout-autosize-prompt.luau): how
-- the ratio, AspectType, DominantAxis, padding, a UISizeConstraint and the automatic axes combine with what the frame
-- holds. Each cell holds one automatic frame H with the constraint, around a box B, a scale leaf S, a scale container
-- M holding a box, a (1, 1) frame F or a wrapped label T.
--   node cli/rur.mjs compare --fixture fixtures/layout-autosize-aspect.luau --viewport 1800x1260 --time 0.2 --background 20,20,26
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
	-- Opts: auto ("X" | "Y" | "XY"), size (UDim2), ratio, type, dominant, pad ({ left, right, top, bottom }), min, max
	-- (Vector2), centre (anchored at the cell's centre), build (function(H) adding the content)
	local function Case(Name, Opts)
		local Cell = Instance.new("Frame")
		Cell.Name = Name
		Cell.Position = UDim2.fromOffset((Count % 6) * 300, (Count // 6) * 180)
		Cell.Size = UDim2.fromOffset(296, 176)
		Cell.BackgroundColor3 = Color3.fromRGB(30, 30, 38)
		Cell.BorderSizePixel = 0
		Cell.Parent = Target
		Count += 1
		local H = Frame("H", Cell, Opts.size, Color3.fromRGB(200, 40, 50))
		H.AutomaticSize = Enum.AutomaticSize[Opts.auto or "Y"]
		if Opts.centre then
			H.AnchorPoint = Vector2.new(0.5, 0.5)
			H.Position = UDim2.fromScale(0.5, 0.5)
		else
			H.Position = UDim2.fromOffset(6, 6)
		end
		local Aspect = Instance.new("UIAspectRatioConstraint")
		Aspect.AspectRatio = Opts.ratio
		Aspect.AspectType = Enum.AspectType[Opts.type or "FitWithinMaxSize"]
		Aspect.DominantAxis = Enum.DominantAxis[Opts.dominant or "Width"]
		Aspect.Parent = H
		if Opts.pad then
			local Pad = Instance.new("UIPadding")
			Pad.PaddingLeft, Pad.PaddingRight = UDim.new(0, Opts.pad[1]), UDim.new(0, Opts.pad[2])
			Pad.PaddingTop, Pad.PaddingBottom = UDim.new(0, Opts.pad[3]), UDim.new(0, Opts.pad[4])
			Pad.Parent = H
		end
		if Opts.min or Opts.max then
			local Constraint = Instance.new("UISizeConstraint")
			Constraint.MinSize = Opts.min or Vector2.zero
			Constraint.MaxSize = Opts.max or Vector2.new(math.huge, math.huge)
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
	-- a scale-sized AutomaticSize Y container holding a W x Ht box (a prompt's panel)
	local function Panel(Size, W, Ht)
		return function(H)
			local M = Frame("M", H, Size, Color3.fromRGB(40, 90, 200))
			M.AutomaticSize = Enum.AutomaticSize.Y
			Frame("B", M, UDim2.fromOffset(W, Ht), Color3.fromRGB(40, 160, 60))
		end
	end
	local function Full(W, Ht)
		return function(H)
			Frame("F", H, UDim2.fromScale(1, 1), Color3.fromRGB(230, 200, 40))
			Frame("B", H, UDim2.fromOffset(W, Ht), Color3.fromRGB(40, 160, 60))
		end
	end
	local function Text(Size)
		return function(H)
			local T = Instance.new("TextLabel")
			T.Name = "T"
			T.AutomaticSize = Enum.AutomaticSize.Y
			T.BackgroundTransparency = 1
			T.FontFace = Face
			T.Size = Size
			T.Text = "Which units would you like to unequip from the team right now?"
			T.TextColor3 = Color3.new(1, 1, 1)
			T.TextSize = 18
			T.TextWrapped = true
			T.Parent = H
		end
	end
	local function List(Gap, Pad)
		return function(H)
			local M = Frame("M", H, UDim2.fromScale(1, 0.8), Color3.fromRGB(40, 90, 200))
			M.AutomaticSize = Enum.AutomaticSize.Y
			local L = Instance.new("UIListLayout")
			L.Padding = Gap
			L.HorizontalAlignment = Enum.HorizontalAlignment.Center
			L.SortOrder = Enum.SortOrder.LayoutOrder
			L.Parent = M
			local P = Instance.new("UIPadding")
			P.PaddingTop, P.PaddingBottom = UDim.new(0, Pad), UDim.new(0, Pad)
			P.PaddingLeft, P.PaddingRight = UDim.new(0, Pad), UDim.new(0, Pad)
			P.Parent = M
			for Index = 1, 2 do
				local B = Frame(`B{Index}`, M, UDim2.fromOffset(120, 24), Color3.fromRGB(40, 160, 60))
				B.LayoutOrder = Index
			end
		end
	end

	-- AutomaticSize Y, FitWithinMaxSize: the base Size fitted to the ratio, then the height grown to the content?
	Case("y240x60_r3_b100x40", { size = UDim2.fromOffset(240, 60), ratio = 3, build = Box(100, 40) })
	Case("y240x60_r3_b100x100", { size = UDim2.fromOffset(240, 60), ratio = 3, build = Box(100, 100) })
	Case("y240x60_r3_b230x40", { size = UDim2.fromOffset(240, 60), ratio = 3, build = Box(230, 40) })
	Case("y150x60_r3_b100x40", { size = UDim2.fromOffset(150, 60), ratio = 3, build = Box(100, 40) })
	Case("y150x60_r3_b100x80", { size = UDim2.fromOffset(150, 60), ratio = 3, build = Box(100, 80) })
	Case("y240x0_r3_b100x40", { size = UDim2.fromOffset(240, 0), ratio = 3, build = Box(100, 40) })
	Case("y240x0_r3_s1x40", { size = UDim2.fromOffset(240, 0), ratio = 3, build = Leaf(UDim2.new(1, 0, 0, 40)) })
	Case("y240x0_r3_m100x40", { size = UDim2.fromOffset(240, 0), ratio = 3, build = Panel(UDim2.fromScale(1, 0), 100, 40) })
	Case("y240x60_r3_m100x80", { size = UDim2.fromOffset(240, 60), ratio = 3, build = Panel(UDim2.fromScale(1, 0), 100, 80) })
	Case("y240x60_r3_m09_100x30", { size = UDim2.fromOffset(240, 60), ratio = 3, build = Panel(UDim2.fromScale(1, 0.9), 100, 30) })
	Case("y240x60_r3_full_b100x100", { size = UDim2.fromOffset(240, 60), ratio = 3, build = Full(100, 100) })
	Case("y240x60_r3_full_b100x30", { size = UDim2.fromOffset(240, 60), ratio = 3, build = Full(100, 30) })
	Case("y240x60_r3_domH_b100x100", { size = UDim2.fromOffset(240, 60), ratio = 3, dominant = "Height", build = Box(100, 100) })
	Case("y240x60_r3_swpW_b100x120", { size = UDim2.fromOffset(240, 60), ratio = 3, type = "ScaleWithParentSize", build = Box(100, 120) })
	Case("y240x60_r3_swpW_b100x40", { size = UDim2.fromOffset(240, 60), ratio = 3, type = "ScaleWithParentSize", build = Box(100, 40) })
	Case("y240x60_r3_swpH_b100x100", { size = UDim2.fromOffset(240, 60), ratio = 3, type = "ScaleWithParentSize", dominant = "Height", build = Box(100, 100) })
	Case("y240x60_r3_swpH_b100x40", { size = UDim2.fromOffset(240, 60), ratio = 3, type = "ScaleWithParentSize", dominant = "Height", build = Box(100, 40) })
	Case("y240x60_r3_pad10_b100x60", { size = UDim2.fromOffset(240, 60), ratio = 3, pad = { 10, 10, 10, 10 }, build = Box(100, 60) })
	Case("y240x60_r3_pad10_b100x20", { size = UDim2.fromOffset(240, 60), ratio = 3, pad = { 10, 10, 10, 10 }, build = Box(100, 20) })
	Case("y240x120_r2_max200_b50x50", { size = UDim2.fromOffset(240, 120), ratio = 2, max = Vector2.new(200, math.huge), build = Box(50, 50) })
	Case("y240x60_r3_min220_b50x50", { size = UDim2.fromOffset(240, 60), ratio = 3, min = Vector2.new(220, 0), build = Box(50, 50) })
	Case("y240x60_r3_maxh50_b50x40", { size = UDim2.fromOffset(240, 60), ratio = 3, max = Vector2.new(math.huge, 50), build = Box(50, 40) })
	Case("y240x60_r3_maxh50_b50x90", { size = UDim2.fromOffset(240, 60), ratio = 3, max = Vector2.new(math.huge, 50), build = Box(50, 90) })
	Case("yscale_r3_b100x40", { size = UDim2.fromScale(0.8, 0.3), ratio = 3, build = Box(100, 40) })
	Case("ycentre_r3_b100x100", { size = UDim2.fromOffset(240, 60), ratio = 3, centre = true, build = Box(100, 100) })
	Case("y240x60_r3_leaf05", { size = UDim2.fromOffset(240, 60), ratio = 3, build = Leaf(UDim2.fromScale(0.5, 0.5)) })
	Case("y240x60_r3_text", { size = UDim2.fromOffset(240, 60), ratio = 3, build = Text(UDim2.new(1, -20, 0, 0)) })
	Case("y240x60_r3_list10", { size = UDim2.fromOffset(240, 60), ratio = 3, build = List(UDim.new(0, 10), 8) })
	Case("y240x60_r3_list01", { size = UDim2.fromOffset(240, 60), ratio = 3, build = List(UDim.new(0.1, 0), 8) })
	Case("y240x60_r3_list01_tall", { size = UDim2.fromOffset(240, 60), ratio = 3, build = List(UDim.new(0.1, 0), 20) })

	-- AutomaticSize X and XY
	Case("x60x60_r05_b40x40", { auto = "X", size = UDim2.fromOffset(60, 60), ratio = 0.5, build = Box(40, 40) })
	Case("x60x60_r05_b20x40", { auto = "X", size = UDim2.fromOffset(60, 60), ratio = 0.5, build = Box(20, 40) })
	Case("x200x60_r2_b150x40", { auto = "X", size = UDim2.fromOffset(200, 60), ratio = 2, build = Box(150, 40) })
	Case("x0x60_r2_b50x40", { auto = "X", size = UDim2.fromOffset(0, 60), ratio = 2, build = Box(50, 40) })
	Case("x100x100_r2_b50x40", { auto = "X", size = UDim2.fromOffset(100, 100), ratio = 2, build = Box(50, 40) })
	Case("x100x100_r2_domH_b50x40", { auto = "X", size = UDim2.fromOffset(100, 100), ratio = 2, dominant = "Height", build = Box(50, 40) })
	Case("xy100x60_r2_b150x30", { auto = "XY", size = UDim2.fromOffset(100, 60), ratio = 2, build = Box(150, 30) })
	Case("xy100x60_r2_b40x80", { auto = "XY", size = UDim2.fromOffset(100, 60), ratio = 2, build = Box(40, 80) })
	Case("xy0x0_r2_b100x40", { auto = "XY", size = UDim2.fromOffset(0, 0), ratio = 2, build = Box(100, 40) })
	Case("xy0x0_r3_text", { auto = "XY", size = UDim2.fromOffset(0, 0), ratio = 3, build = Text(UDim2.new(1, -20, 0, 0)) })
	Case("xy100x60_r2_m09_100x30", { auto = "XY", size = UDim2.fromOffset(100, 60), ratio = 2, build = Panel(UDim2.fromScale(1, 0.9), 100, 30) })
	Case("xy200x60_r2_full_b60x80", { auto = "XY", size = UDim2.fromOffset(200, 60), ratio = 2, build = Full(60, 80) })
end
