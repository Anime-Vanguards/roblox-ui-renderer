-- AutomaticSize frames with a UIAspectRatioConstraint, continued from layout-autosize-aspect8.luau: scale children F
-- smaller than the frame's box (alone or beside a box B), with and without padding, on automatic Y, X and XY frames;
-- centred lists and grids, a list with a scale Padding, and a ScaleWithParentSize frame with a list. Each cell holds
-- one automatic frame H.
--   node cli/rur.mjs compare --fixture fixtures/layout-autosize-aspect9.luau --viewport 2400x880 --time 0.2 --background 20,20,26
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
	-- Opts: auto, size, ratio, pad ({ left, right, top, bottom }), full (F's Size), box ({ w, h }), list, grid, parent
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
		if Opts.parent then
			Aspect.AspectType = Enum.AspectType.ScaleWithParentSize
		end
		Aspect.Parent = H
		local P = Opts.pad or { 0, 0, 0, 0 }
		if P[1] ~= 0 or P[2] ~= 0 or P[3] ~= 0 or P[4] ~= 0 then
			local Pad = Instance.new("UIPadding")
			Pad.PaddingLeft, Pad.PaddingRight = UDim.new(0, P[1]), UDim.new(0, P[2])
			Pad.PaddingTop, Pad.PaddingBottom = UDim.new(0, P[3]), UDim.new(0, P[4])
			Pad.Parent = H
		end
		if Opts.list then
			local List = Instance.new("UIListLayout")
			List.FillDirection = Enum.FillDirection[Opts.list[1]]
			List.Padding = Opts.list[4] or UDim.new(0, 5)
			List.HorizontalAlignment = Enum.HorizontalAlignment[Opts.list[5] or "Left"]
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
			Grid.HorizontalAlignment = Enum.HorizontalAlignment[Opts.grid[3] or "Left"]
			Grid.SortOrder = Enum.SortOrder.LayoutOrder
			Grid.Parent = H
			for K = 1, Opts.grid[2] do
				local Item = Frame(`I{K}`, H, UDim2.fromOffset(10, 10), Color3.fromRGB(40, 160, 60))
				Item.LayoutOrder = K
			end
		end
		if Opts.full then
			Frame("F", H, Opts.full, Color3.fromRGB(230, 200, 40))
		end
		if Opts.box then
			Frame("B", H, UDim2.fromOffset(Opts.box[1], Opts.box[2]), Color3.fromRGB(40, 120, 160))
		end
	end
	local Half, HalfH, HalfW = UDim2.fromScale(0.5, 0.5), UDim2.fromScale(1, 0.5), UDim2.fromScale(0.5, 1)
	-- automatic Y, 240 x 60 of ratio 3
	Case("y_half", { full = Half })
	Case("y_halfh", { full = HalfH })
	Case("y_half_b100x20", { full = Half, box = { 100, 20 } })
	Case("y_halfh_b100x100", { full = HalfH, box = { 100, 100 } })
	Case("y_t10b10_half", { pad = { 0, 0, 10, 10 }, full = Half })
	Case("y_t10b10_halfh", { pad = { 0, 0, 10, 10 }, full = HalfH })
	Case("y_t10b10_halfh_b100x20", { pad = { 0, 0, 10, 10 }, full = HalfH, box = { 100, 20 } })
	Case("y_t20b20_halfh_b100x70", { pad = { 0, 0, 20, 20 }, full = HalfH, box = { 100, 70 } })
	Case("y_l20r20_halfh_b100x70", { pad = { 20, 20, 0, 0 }, full = HalfH, box = { 100, 70 } })
	Case("y_full_b260x40", { full = UDim2.fromScale(1, 1), box = { 260, 40 } })
	-- automatic X, 60 x 120 of ratio 0.5
	local X = UDim2.fromOffset(60, 120)
	Case("x_half", { auto = "X", size = X, ratio = 0.5, full = Half })
	Case("x_halfw", { auto = "X", size = X, ratio = 0.5, full = HalfW })
	Case("x_l10r10_halfw", { auto = "X", size = X, ratio = 0.5, pad = { 10, 10, 0, 0 }, full = HalfW })
	Case("x_t20b20_halfw_b20x40", { auto = "X", size = X, ratio = 0.5, pad = { 0, 0, 20, 20 }, full = HalfW, box = { 20, 40 } })
	Case("x_t30b0_full_b20x100", { auto = "X", size = X, ratio = 0.5, pad = { 0, 0, 30, 0 }, full = UDim2.fromScale(1, 1), box = { 20, 100 } })
	-- automatic XY, 200 x 60 of ratio 2
	local XY = UDim2.fromOffset(200, 60)
	Case("xy_half", { auto = "XY", size = XY, ratio = 2, full = Half })
	Case("xy_pad10_half", { auto = "XY", size = XY, ratio = 2, pad = { 10, 10, 10, 10 }, full = Half })
	Case("xy_l30r0_full_b60x20", { auto = "XY", size = XY, ratio = 2, pad = { 30, 0, 0, 0 }, full = UDim2.fromScale(1, 1), box = { 60, 20 } })
	Case("xy_l30r0t0b5_full_b60x20", { auto = "XY", size = XY, ratio = 2, pad = { 30, 0, 0, 5 }, full = UDim2.fromScale(1, 1), box = { 60, 20 } })
	Case("xy_l10r10t20b20_full_b60x80", { auto = "XY", size = XY, ratio = 2, pad = { 10, 10, 20, 20 }, full = UDim2.fromScale(1, 1), box = { 60, 80 } })
	-- layouts
	local Item = UDim2.fromOffset(100, 30)
	Case("list_centre_n2", { list = { "Vertical", 2, Item, nil, "Center" } })
	Case("list_centre_n4", { list = { "Vertical", 4, Item, nil, "Center" } })
	Case("list_scalegap_n3", { list = { "Vertical", 3, Item, UDim.new(0.1, 0) } })
	Case("grid_centre_n8", { grid = { UDim2.fromOffset(50, 40), 8, "Center" } })
	Case("hlist_pad10_n5", { pad = { 10, 10, 10, 10 }, list = { "Horizontal", 5, UDim2.fromOffset(50, 40) } })
	Case("x_list_n3", { auto = "X", size = X, ratio = 0.5, list = { "Vertical", 3, UDim2.fromOffset(40, 50) } })
	Case("xy_list_n3", { auto = "XY", size = XY, ratio = 2, list = { "Vertical", 3, UDim2.fromOffset(100, 30) } })
	Case("parent_list_n3", { parent = true, list = { "Vertical", 3, Item } })
end
