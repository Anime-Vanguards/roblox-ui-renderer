-- Border UIStroke LineJoinMode on sharp frames (and one rounded row, where joins should not apply).
-- Columns: thickness; rows: join × position.
-- compare: --viewport 1000x860 --time 0.2 --background 0,0,0
return function(Target)
	local Joins = { Enum.LineJoinMode.Round, Enum.LineJoinMode.Bevel, Enum.LineJoinMode.Miter }
	local Positions = { Enum.BorderStrokePosition.Outer, Enum.BorderStrokePosition.Center, Enum.BorderStrokePosition.Inner }
	local Thicknesses = { 1, 2, 3.5, 5, 8, 12 }

	local RowIndex = 0
	local function AddRow(Join, Position, Radius)
		RowIndex += 1
		for Column, Thickness in Thicknesses do
			local Frame = Instance.new("Frame")
			Frame.Name = `J{RowIndex}_{Column}`
			Frame.Position = UDim2.fromOffset(40 + (Column - 1) * 150, 40 + (RowIndex - 1) * 76)
			Frame.Size = UDim2.fromOffset(80, 36)
			Frame.BackgroundTransparency = 1
			Frame.BorderSizePixel = 0

			if Radius then
				local Corner = Instance.new("UICorner")
				Corner.CornerRadius = UDim.new(0, Radius)
				Corner.Parent = Frame
			end

			local Stroke = Instance.new("UIStroke")
			Stroke.ApplyStrokeMode = Enum.ApplyStrokeMode.Border
			Stroke.Color = Color3.new(1, 1, 1)
			Stroke.Thickness = Thickness
			Stroke.LineJoinMode = Join
			Stroke.BorderStrokePosition = Position
			Stroke.Parent = Frame

			Frame.Parent = Target
		end
	end

	for _, Join in Joins do
		for _, Position in Positions do
			AddRow(Join, Position, nil)
		end
	end
	AddRow(Enum.LineJoinMode.Miter, Enum.BorderStrokePosition.Outer, 6)
	AddRow(Enum.LineJoinMode.Bevel, Enum.BorderStrokePosition.Outer, 6)
end
