-- UIListLayout and ScrollingFrame canvases in float32 (continues layout-f32-lists.luau): lists in containers of
-- fractional sizes built from scale sizes, centred or at the far end, vertical and horizontal, under UIScale 1 / 0.885 /
-- 1.3, and ScrollingFrames whose CanvasSize scale makes fractional canvases with scale-sized children (CanvasPosition
-- set once the frame is parented). Each cell holds
-- a frame H (its own UIScale) with the list L or the ScrollingFrame S, and a hidden label T whose Text reads the list's
-- AbsoluteContentSize or the frame's AbsoluteCanvasSize and AbsoluteWindowSize (float32, %.9g). Compare bit for bit.
--   node cli/rur.mjs compare --fixture fixtures/layout-f32-lists2.luau --viewport 2160x1270 --time 0.2 --background 20,20,26
return function(Target)
	local Count = 0
	local Reads = {}
	local function Cell(Name, Scale)
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
		local T = Instance.new("TextLabel")
		T.Name = "T"
		T.Position = UDim2.fromOffset(2, 120)
		T.Size = UDim2.fromOffset(260, 14)
		T.BackgroundTransparency = 1
		T.TextTransparency = 1
		T.Text = ""
		T.Parent = Cell
		return H, T
	end
	local Scales = { 1, 0.885, 1.3 }
	-- lists: v / e vertical (centred / far end), h / k horizontal
	for _, Variant in { { "v", "Vertical", "Center", "Center" }, { "e", "Vertical", "Right", "Bottom" }, { "h", "Horizontal", "Center", "Center" }, { "k", "Horizontal", "Right", "Bottom" } } do
		for Index, Scale in Scales do
			for J = 0, 3 do
				local H, T = Cell(`{Variant[1]}{Index}_{J}`, Scale)
				local L = Instance.new("Frame")
				L.Name = "L"
				L.Position = UDim2.new(0.01 + 0.0311 * J, 0, 0.02 + 0.0217 * J, 1)
				L.Size = UDim2.new(0.6 + 0.0173 * J, 1, 0.8 + 0.0119 * J, 2)
				L.BackgroundColor3 = Color3.fromRGB(60, 60, 80)
				L.BorderSizePixel = 0
				L.Parent = H
				local Layout = Instance.new("UIListLayout")
				Layout.FillDirection = Enum.FillDirection[Variant[2]]
				Layout.Padding = UDim.new(0.0137, 1)
				Layout.HorizontalAlignment = Enum.HorizontalAlignment[Variant[3]]
				Layout.VerticalAlignment = Enum.VerticalAlignment[Variant[4]]
				Layout.SortOrder = Enum.SortOrder.LayoutOrder
				Layout.Parent = L
				for K = 1, 4 do
					local Item = Instance.new("Frame")
					Item.Name = `I{K}`
					Item.LayoutOrder = K
					Item.Size = if Variant[2] == "Vertical" then UDim2.new(0.71, K, 0.093 + 0.011 * K, 1) else UDim2.new(0.093 + 0.011 * K, 1, 0.71, K)
					Item.BackgroundColor3 = Color3.fromRGB(200, 40 + 30 * K, 50)
					Item.BorderSizePixel = 0
					Item.Parent = L
				end
				table.insert(Reads, function()
					local Size = Layout.AbsoluteContentSize
					T.Text = string.format("%.9g %.9g", Size.X, Size.Y)
				end)
			end
		end
	end
	-- scrolling frames
	for Index, Scale in Scales do
		for J = 0, 7 do
			local H, T = Cell(`s{Index}_{J}`, Scale)
			local S = Instance.new("ScrollingFrame")
			S.Name = "S"
			S.Position = UDim2.new(0.05 + 0.013 * J, 1, 0.1 + 0.011 * J, 0)
			S.Size = UDim2.new(0.37 + 0.021 * J, 3, 0.3 + 0.017 * J, 1)
			S.CanvasSize = UDim2.new(1.7 + 0.031 * J, 11, 2.3 + 0.023 * J, 7)
			S.ScrollBarThickness = 7
			S.BackgroundColor3 = Color3.fromRGB(60, 60, 80)
			S.BorderSizePixel = 0
			S.Parent = H
			S.CanvasPosition = Vector2.new(13.37, 21.71)
			for K = 1, 3 do
				local Child = Instance.new("Frame")
				Child.Name = `C{K}`
				Child.Position = UDim2.new(0.21 * K, K, 0.17 * K, 3)
				Child.Size = UDim2.new(0.19, 1, 0.13, 0)
				Child.AnchorPoint = Vector2.new(0.3, 0.2)
				Child.BackgroundColor3 = Color3.fromRGB(200, 40 + 30 * K, 50)
				Child.BorderSizePixel = 0
				Child.Parent = S
			end
			table.insert(Reads, function()
				local Canvas, Window = S.AbsoluteCanvasSize, S.AbsoluteWindowSize
				T.Text = string.format("%.9g %.9g %.9g %.9g", Canvas.X, Canvas.Y, Window.X, Window.Y)
			end)
		end
	end
	task.wait()
	for _, Read in Reads do
		Read()
	end
end
