-- Sharp border UIStroke outer-edge geometry: the outer boundary sits at D = n + 0.5 - e physical
-- pixels outside the frame edge, so the pixel centre at n + 0.5 is e outside it. Rows 1-6: Outer,
-- rows 7-12: Center (D = t / 2); columns n = 0..11. Frames at integer physical positions.
-- compare: --viewport 800x760 --time 0.2 --background 0,0,0
return function(Target)
	local Offsets = { -0.0625, 0, 0.0625, 0.125, 0.1875, 0.25 }
	local Positions = { Enum.BorderStrokePosition.Outer, Enum.BorderStrokePosition.Center }

	for PositionIndex, Position in Positions do
		for OffsetIndex, Offset in Offsets do
			local RowIndex = (PositionIndex - 1) * #Offsets + OffsetIndex
			for Column = 0, 11 do
				local Distance = Column + 0.5 - Offset
				local Physical = if Position == Enum.BorderStrokePosition.Center then Distance * 2 else Distance
				local Frame = Instance.new("Frame")
				Frame.Name = `M{RowIndex}_{Column}`
				Frame.Position = UDim2.fromOffset(30 + Column * 64, 30 + (RowIndex - 1) * 60)
				Frame.Size = UDim2.fromOffset(40, 30)
				Frame.BackgroundTransparency = 1
				Frame.BorderSizePixel = 0

				local Stroke = Instance.new("UIStroke")
				Stroke.ApplyStrokeMode = Enum.ApplyStrokeMode.Border
				Stroke.Color = Color3.new(1, 1, 1)
				Stroke.Thickness = Physical / 1.5
				Stroke.BorderStrokePosition = Position
				Stroke.Parent = Frame

				Frame.Parent = Target
			end
		end
	end
end
