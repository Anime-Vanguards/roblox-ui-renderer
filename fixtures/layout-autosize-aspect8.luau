-- AutomaticSize frames with a UIAspectRatioConstraint holding a (1, 1) frame F, continued from
-- layout-autosize-aspect6.luau and -aspect7.luau: automatic X (60 x 120 of ratio 0.5) and automatic XY (200 x 60 of
-- ratio 2) frames H swept over their padding and the box B's size, and frames with a UIListLayout or UIGridLayout and
-- padding.
--   node cli/rur.mjs compare --fixture fixtures/layout-autosize-aspect8.luau --viewport 2400x1320 --time 0.2 --background 20,20,26
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
	-- Opts: auto, size, ratio, pad ({ left, right, top, bottom }), box ({ w, h }), full, list, grid
	local function Case(Name, Opts)
		local Cell = Instance.new("Frame")
		Cell.Name = Name
		Cell.Position = UDim2.fromOffset((Count % 8) * 300, (Count // 8) * 220)
		Cell.Size = UDim2.fromOffset(296, 216)
		Cell.BackgroundColor3 = Color3.fromRGB(30, 30, 38)
		Cell.BorderSizePixel = 0
		Cell.Parent = Target
		Count += 1
		local H = Frame("H", Cell, Opts.size, Color3.fromRGB(200, 40, 50))
		H.AutomaticSize = Enum.AutomaticSize[Opts.auto]
		H.Position = UDim2.fromOffset(6, 6)
		local Aspect = Instance.new("UIAspectRatioConstraint")
		Aspect.AspectRatio = Opts.ratio
		Aspect.Parent = H
		local P = Opts.pad
		if P[1] ~= 0 or P[2] ~= 0 or P[3] ~= 0 or P[4] ~= 0 then
			local Pad = Instance.new("UIPadding")
			Pad.PaddingLeft, Pad.PaddingRight = UDim.new(0, P[1]), UDim.new(0, P[2])
			Pad.PaddingTop, Pad.PaddingBottom = UDim.new(0, P[3]), UDim.new(0, P[4])
			Pad.Parent = H
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
	-- automatic X: 60 x 120 of ratio 0.5
	for _, P in { { 0, 0, 0, 0 }, { 10, 10, 0, 0 }, { 20, 0, 0, 0 }, { 0, 0, 10, 10 }, { 10, 10, 10, 10 }, { 0, 0, 30, 0 } } do
		for _, B in { { 20, 40 }, { 50, 40 }, { 80, 40 }, { 20, 100 }, { 110, 40 } } do
			Case(`x_l{P[1]}r{P[2]}t{P[3]}b{P[4]}_b{B[1]}x{B[2]}`, { auto = "X", size = UDim2.fromOffset(60, 120), ratio = 0.5, pad = P, full = true, box = B })
		end
	end
	-- automatic XY: 200 x 60 of ratio 2
	for _, P in { { 0, 0, 0, 0 }, { 10, 10, 0, 0 }, { 0, 0, 10, 10 }, { 10, 10, 10, 10 }, { 20, 0, 0, 10 } } do
		for _, B in { { 60, 20 }, { 60, 80 }, { 220, 20 } } do
			Case(`xy_l{P[1]}r{P[2]}t{P[3]}b{P[4]}_b{B[1]}x{B[2]}`, { auto = "XY", size = UDim2.fromOffset(200, 60), ratio = 2, pad = P, full = true, box = B })
		end
	end
	-- frames with a layout and padding (240 x 60 of ratio 3, automatic Y), and an automatic X frame with a list
	Case("list_pad10_n2", { auto = "Y", size = UDim2.fromOffset(240, 60), ratio = 3, pad = { 10, 10, 10, 10 }, list = { "Vertical", 2, UDim2.fromOffset(100, 30) } })
	Case("grid_pad10_n8", { auto = "Y", size = UDim2.fromOffset(240, 60), ratio = 3, pad = { 10, 10, 10, 10 }, grid = { UDim2.fromOffset(50, 40), 8 } })
	Case("x_hlist_n3", { auto = "X", size = UDim2.fromOffset(60, 120), ratio = 0.5, pad = { 0, 0, 0, 0 }, list = { "Horizontal", 3, UDim2.fromOffset(40, 50) } })
end
