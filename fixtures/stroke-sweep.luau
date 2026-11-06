-- Border UIStroke thickness sweep: white strokes on transparent 40x30 frames at integer physical
-- positions. Rows: sharp Outer/Center/Inner, then UICorner(10) Outer/Center/Inner.
-- compare: --viewport 1440x400 --time 0.2 --background 0,0,0
return function(Target)
	local Thicknesses = { 0.25, 0.5, 0.75, 1, 1.25, 1.5, 1.75, 2, 2.25, 2.5, 2.75, 3, 3.25, 3.5, 3.75, 4, 4.5, 5, 6, 7, 8, 10 }
	local Rows = {
		{ Radius = nil, Position = Enum.BorderStrokePosition.Outer },
		{ Radius = nil, Position = Enum.BorderStrokePosition.Center },
		{ Radius = nil, Position = Enum.BorderStrokePosition.Inner },
		{ Radius = 10, Position = Enum.BorderStrokePosition.Outer },
		{ Radius = 10, Position = Enum.BorderStrokePosition.Center },
		{ Radius = 10, Position = Enum.BorderStrokePosition.Inner },
	}

	for RowIndex, Row in Rows do
		for Column, Thickness in Thicknesses do
			local Frame = Instance.new("Frame")
			Frame.Name = `R{RowIndex}C{Column}`
			Frame.Position = UDim2.fromOffset(30 + (Column - 1) * 64, 30 + (RowIndex - 1) * 60)
			Frame.Size = UDim2.fromOffset(40, 30)
			Frame.BackgroundTransparency = 1
			Frame.BorderSizePixel = 0

			if Row.Radius then
				local Corner = Instance.new("UICorner")
				Corner.CornerRadius = UDim.new(0, Row.Radius)
				Corner.Parent = Frame
			end

			local Stroke = Instance.new("UIStroke")
			Stroke.ApplyStrokeMode = Enum.ApplyStrokeMode.Border
			Stroke.Color = Color3.new(1, 1, 1)
			Stroke.Thickness = Thickness
			Stroke.BorderStrokePosition = Row.Position
			Stroke.Parent = Frame

			Frame.Parent = Target
		end
	end
end
