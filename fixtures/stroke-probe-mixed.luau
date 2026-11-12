-- Border UIStroke on frames whose corners have different radii: where the inner edge of a straight side is
-- cut by geometry and where it ramps. White strokes on clear 60 x 40 frames at whole device pixels; rows are
-- radii sets, columns thicknesses whose inner edge lies on, beside and between pixel centres.
-- compare: --viewport 900x480 --time 0.2 --background 0,0,0
return function(Target)
	local Center = Enum.BorderStrokePosition.Center
	local Inner = Enum.BorderStrokePosition.Inner
	local Outer = Enum.BorderStrokePosition.Outer
	-- (device px: logical x 1.5) inner extents 1.5, 0.75, 1.2, 2.25, 3, 4.5 and outer strokes
	local Cells = { { Center, 2 }, { Center, 1 }, { Inner, 0.8 }, { Inner, 1.5 }, { Inner, 2 }, { Center, 6 }, { Outer, 1 }, { Outer, 2.2 } }
	local Rows = {
		{ 8, 0, 3, 12 },
		{ 8, 6, 3, 12 },
		{ 8, 8, 8, 12 },
		{ 0, 8, 8, 8 },
		{ 8, 8, 8, 8 },
		{ 12, 12, 0, 0 },
	}
	for RowIndex, Radii in Rows do
		for Column, Cell in Cells do
			local Frame = Instance.new("Frame")
			Frame.Name = `X{RowIndex}_{Column}`
			Frame.Position = UDim2.fromOffset(40 + (Column - 1) * 100, 40 + (RowIndex - 1) * 70)
			Frame.Size = UDim2.fromOffset(60, 40)
			Frame.BackgroundTransparency = 1
			Frame.BorderSizePixel = 0
			local Corner = Instance.new("UICorner")
			Corner.TopLeftRadius = UDim.new(0, Radii[1])
			Corner.TopRightRadius = UDim.new(0, Radii[2])
			Corner.BottomRightRadius = UDim.new(0, Radii[3])
			Corner.BottomLeftRadius = UDim.new(0, Radii[4])
			Corner.Parent = Frame
			local Stroke = Instance.new("UIStroke")
			Stroke.ApplyStrokeMode = Enum.ApplyStrokeMode.Border
			Stroke.Color = Color3.new(1, 1, 1)
			Stroke.Thickness = Cell[2]
			Stroke.BorderStrokePosition = Cell[1]
			Stroke.Parent = Frame
			Frame.Parent = Target
		end
	end
end
