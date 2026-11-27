-- UIGridLayout in float32 (continues layout-f32-lists.luau): grids in containers of fractional widths built from scale
-- sizes, under UIScale 1 / 0.885 / 1.3, centred or top-left, with and without CellPadding. Each cell holds a frame H
-- (its own UIScale), the grid's container G with seven items, and a hidden label T whose Text reads the grid's
-- AbsoluteCellSize, AbsoluteContentSize and AbsoluteCellCount (float32, %.9g). Compare bit for bit (float32).
--   node cli/rur.mjs compare --fixture fixtures/layout-f32-grids.luau --viewport 2160x1270 --time 0.2 --background 20,20,26
return function(Target)
	local Count = 0
	local Labels = {}
	local function Case(Name, Scale, J, Opts)
		local Cell = Instance.new("Frame")
		Cell.Name = Name
		Cell.Position = UDim2.fromOffset((Count % 8) * 270, (Count // 8) * 140)
		Cell.Size = UDim2.fromOffset(266, 136)
		Cell.BackgroundColor3 = Color3.fromRGB(30, 30, 38)
		Cell.BorderSizePixel = 0
		Cell.Parent = Target
		Count += 1
		local H = Instance.new("Frame")
		H.Name = "H"
		H.Position = UDim2.fromOffset(2, 2)
		H.Size = UDim2.fromOffset(200, 100)
		H.BackgroundTransparency = 1
		H.Parent = Cell
		if Scale ~= 1 then
			local UIScale = Instance.new("UIScale")
			UIScale.Scale = Scale
			UIScale.Parent = H
		end
		local G = Instance.new("Frame")
		G.Name = "G"
		G.Position = UDim2.new(0.01 + 0.0311 * J, 0, 0.02 + 0.0217 * J, 1)
		G.Size = UDim2.new(0.5 + 0.0173 * J, 1, 0.6 + 0.0119 * J, 2)
		G.BackgroundColor3 = Color3.fromRGB(60, 60, 80)
		G.BorderSizePixel = 0
		G.Parent = H
		local Grid = Instance.new("UIGridLayout")
		Grid.CellSize = Opts.cell or UDim2.new(0.137, 3, 0.211, 1)
		Grid.CellPadding = Opts.pad or UDim2.new(0.019, 1, 0.023, 2)
		Grid.HorizontalAlignment = Enum.HorizontalAlignment[Opts.h or "Center"]
		Grid.VerticalAlignment = Enum.VerticalAlignment[Opts.v or "Center"]
		Grid.SortOrder = Enum.SortOrder.LayoutOrder
		Grid.Parent = G
		for K = 1, 7 do
			local Item = Instance.new("Frame")
			Item.Name = `I{K}`
			Item.LayoutOrder = K
			Item.BackgroundColor3 = Color3.fromRGB(200, 40 + 20 * K, 50)
			Item.BorderSizePixel = 0
			Item.Parent = G
		end
		local T = Instance.new("TextLabel")
		T.Name = "T"
		T.Position = UDim2.fromOffset(2, 120)
		T.Size = UDim2.fromOffset(260, 14)
		T.BackgroundTransparency = 1
		T.TextTransparency = 1
		T.Text = ""
		T.Parent = Cell
		table.insert(Labels, { T, Grid })
	end
	for _, Variant in {
		{ "c", {} },
		{ "l", { h = "Left", v = "Top" } },
		{ "o", { h = "Left", v = "Top", pad = UDim2.new(0, 0, 0, 0) } },
	} do
		for Index, Scale in { 1, 0.885, 1.3 } do
			for J = 0, 7 do
				Case(`{Variant[1]}{Index}_{J}`, Scale, J, Variant[2])
			end
		end
	end
	task.wait()
	for _, Pair in Labels do
		local Grid = Pair[2]
		local Cell, Content, Cells = Grid.AbsoluteCellSize, Grid.AbsoluteContentSize, Grid.AbsoluteCellCount
		Pair[1].Text = string.format("%.9g %.9g %.9g %.9g %d %d", Cell.X, Cell.Y, Content.X, Content.Y, Cells.X, Cells.Y)
	end
end
