-- Border UIStroke on turned frames, where a rounded corner's arc meets a straight side (continues
-- image-probe-squarecorner-turned.luau's stroke rows): how far inside the inner edge a pixel near the join keeps the
-- arc's ramp. White strokes on clear frames over black; each row turns frames by a sweep of angles, rows vary the
-- radius, the stroke's position and thickness, whether the corners differ, and the frame's place by fractions of a px.
-- compare: --viewport 1140x700 --time 0.2 --background 0,0,0
return function(Target)
	local Center = Enum.BorderStrokePosition.Center
	local Inner = Enum.BorderStrokePosition.Inner
	local Outer = Enum.BorderStrokePosition.Outer
	local Angles = { 3, 11, 17, 23, 30, 37, 45, 53, 61, 68, 75, 83 }
	-- rows of { radii (top left, top right, bottom right, bottom left), position, thickness, offset }
	local Rows = {
		{ { 0, 10, 10, 10 }, Outer, 2, 0 },
		{ { 0, 10, 10, 10 }, Outer, 2, 0.37 },
		{ { 10, 10, 10, 10 }, Outer, 2, 0 },
		{ { 0, 6, 6, 6 }, Outer, 2, 0.21 },
		{ { 0, 10, 10, 10 }, Center, 4, 0.13 },
		{ { 0, 14, 14, 14 }, Inner, 2, 0.29 },
		{ { 10, 10, 10, 10 }, Center, 4, 0.13 },
	}
	for RowIndex, Row in Rows do
		local Radii, Position, Thickness, Offset = table.unpack(Row)
		for Column, Angle in Angles do
			local Frame = Instance.new("Frame")
			Frame.Name = `J{RowIndex}_{Column}`
			Frame.AnchorPoint = Vector2.new(0.5, 0.5)
			Frame.Position = UDim2.fromOffset(45 + (Column - 1) * 92 + Offset, 48 + (RowIndex - 1) * 96 + Offset * 0.7)
			Frame.Size = UDim2.fromOffset(52 + Column % 2, 36 + (Column // 2) % 2)
			Frame.Rotation = Angle
			Frame.BackgroundTransparency = 1
			Frame.BorderSizePixel = 0
			local Corner = Instance.new("UICorner")
			Corner.TopLeftRadius = UDim.new(0, Radii[1])
			Corner.TopRightRadius = UDim.new(0, Radii[2])
			Corner.BottomRightRadius = UDim.new(0, Radii[3])
			Corner.BottomLeftRadius = UDim.new(0, Radii[4])
			Corner.Parent = Frame
			local Stroke = Instance.new("UIStroke")
			Stroke.Color = Color3.new(1, 1, 1)
			Stroke.Thickness = Thickness
			Stroke.ApplyStrokeMode = Enum.ApplyStrokeMode.Border
			Stroke.BorderStrokePosition = Position
			Stroke.Parent = Frame
			Frame.Parent = Target
		end
	end
end
