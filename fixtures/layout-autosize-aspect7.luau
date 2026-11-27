-- AutomaticSize frames with a UIAspectRatioConstraint, continued from layout-autosize-aspect6.luau: frames with a
-- UIListLayout or UIGridLayout of their own, a UISizeConstraint the ratio's box passes, and automatic X / XY frames
-- with padding around a (1, 1) frame F. Each cell holds one automatic frame H (240 x 60 of ratio 3, automatic Y, unless
-- named).
--   node cli/rur.mjs compare --fixture fixtures/layout-autosize-aspect7.luau --viewport 2400x880 --time 0.2 --background 20,20,26
return function(Target)
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
	local function Pad(Parent, P)
		local Padding = Instance.new("UIPadding")
		Padding.PaddingLeft, Padding.PaddingRight = UDim.new(0, P[1]), UDim.new(0, P[2])
		Padding.PaddingTop, Padding.PaddingBottom = UDim.new(0, P[3]), UDim.new(0, P[4])
		Padding.Parent = Parent
	end
	-- Opts: auto, size, ratio, pad, list ({ fill, count, item size }), grid ({ cell, count }), max, min, full, box
	local function Case(Name, Opts)
		local Cell = Instance.new("Frame")
		Cell.Name = Name
		Cell.Position = UDim2.fromOffset((Count % 8) * 300, (Count // 8) * 220)
		Cell.Size = UDim2.fromOffset(296, 216)
		Cell.BackgroundColor3 = Color3.fromRGB(30, 30, 38)
		Cell.BorderSizePixel = 0
		Cell.Parent = Target
		Count += 1
		local H = Frame("H", Cell, Opts.size or UDim2.fromOffset(240, 60), Color3.fromRGB(200, 40, 50))
		H.AutomaticSize = Enum.AutomaticSize[Opts.auto or "Y"]
		H.Position = UDim2.fromOffset(6, 6)
		local Aspect = Instance.new("UIAspectRatioConstraint")
		Aspect.AspectRatio = Opts.ratio or 3
		Aspect.Parent = H
		if Opts.pad then
			Pad(H, Opts.pad)
		end
		if Opts.max or Opts.min then
			local Constraint = Instance.new("UISizeConstraint")
			if Opts.max then
				Constraint.MaxSize = Opts.max
			end
			if Opts.min then
				Constraint.MinSize = Opts.min
			end
			Constraint.Parent = H
		end
		if Opts.list then
			local List = Instance.new("UIListLayout")
			List.FillDirection = Enum.FillDirection[Opts.list[1]]
			List.Padding = UDim.new(0, 5)
			List.SortOrder = Enum.SortOrder.LayoutOrder
			List.Parent = H
			for K = 1, Opts.list[2] do
				local Item = Frame(`I{K}`, H, Opts.list[3], Color3.fromRGB(40, 160, 60))
				Item.LayoutOrder = K
			end
		end
		if Opts.grid then
			local Grid = Instance.new("UIGridLayout")
			Grid.CellSize = Opts.grid[1]
			Grid.CellPadding = UDim2.fromOffset(5, 5)
			Grid.SortOrder = Enum.SortOrder.LayoutOrder
			Grid.Parent = H
			for K = 1, Opts.grid[2] do
				local Item = Frame(`I{K}`, H, UDim2.fromOffset(10, 10), Color3.fromRGB(40, 160, 60))
				Item.LayoutOrder = K
			end
		end
		if Opts.full then
			Frame("F", H, UDim2.fromScale(1, 1), Color3.fromRGB(230, 200, 40))
		end
		if Opts.box then
			Frame("B", H, UDim2.fromOffset(Opts.box[1], Opts.box[2]), Color3.fromRGB(40, 120, 160))
		end
	end
	local Item = UDim2.fromOffset(100, 30)
	-- a vertical list of 100 x 30 items, 5 px apart
	for N = 1, 4 do
		Case(`list_n{N}`, { list = { "Vertical", N, Item } })
	end
	Case("list_pad10_n1", { pad = { 10, 10, 10, 10 }, list = { "Vertical", 1, Item } })
	Case("list_pad10_n3", { pad = { 10, 10, 10, 10 }, list = { "Vertical", 3, Item } })
	Case("list_wide_n2", { list = { "Vertical", 2, UDim2.fromOffset(260, 30) } })
	Case("list_scale_n2", { list = { "Vertical", 2, UDim2.new(1, 0, 0, 30) } })
	-- a horizontal list of 50 x 40 items
	for _, N in { 1, 3, 5 } do
		Case(`hlist_n{N}`, { list = { "Horizontal", N, UDim2.fromOffset(50, 40) } })
	end
	-- a grid of 50 x 40 cells, 5 px apart (4 to a row at 240)
	for _, N in { 2, 4, 8, 12 } do
		Case(`grid_n{N}`, { grid = { UDim2.fromOffset(50, 40), N } })
	end
	Case("grid_scale_n6", { grid = { UDim2.fromScale(0.3, 0.5), 6 } })
	-- a UISizeConstraint the ratio's box passes
	Case("max_w200_b260x100", { max = Vector2.new(200, math.huge), box = { 260, 100 } })
	Case("max_h50_b260x100", { max = Vector2.new(math.huge, 50), box = { 260, 100 } })
	Case("max_h70_pady30_b100x40", { max = Vector2.new(math.huge, 70), pad = { 0, 0, 30, 30 }, box = { 100, 40 } })
	Case("max_w200_full_b260x100", { max = Vector2.new(200, math.huge), full = true, box = { 260, 100 } })
	Case("min_h90_b100x40", { min = Vector2.new(0, 90), box = { 100, 40 } })
	Case("max_x_w40_b100x40", { auto = "X", size = UDim2.fromOffset(60, 120), ratio = 0.5, max = Vector2.new(40, math.huge), box = { 100, 40 } })
	-- automatic X and XY with padding around a (1, 1) frame
	local X = UDim2.fromOffset(60, 120)
	Case("x_pad10_full_b20x100", { auto = "X", size = X, ratio = 0.5, pad = { 10, 10, 10, 10 }, full = true, box = { 20, 100 } })
	Case("x_padx20_full_b20x40", { auto = "X", size = X, ratio = 0.5, pad = { 20, 20, 0, 0 }, full = true, box = { 20, 40 } })
	Case("x_pady20_full_b100x40", { auto = "X", size = X, ratio = 0.5, pad = { 0, 0, 20, 20 }, full = true, box = { 100, 40 } })
	Case("xy_pad10_full_b60x80", { auto = "XY", size = UDim2.fromOffset(200, 60), ratio = 2, pad = { 10, 10, 10, 10 }, full = true, box = { 60, 80 } })
	Case("xy_pady20_full_b60x20", { auto = "XY", size = UDim2.fromOffset(200, 60), ratio = 2, pad = { 0, 0, 20, 20 }, full = true, box = { 60, 20 } })
end
