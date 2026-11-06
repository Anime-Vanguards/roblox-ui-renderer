-- Border UIStroke × UICorner radius: radius sweep (incl. radius < thickness and a pill) for
-- Outer/Center/Inner at two thicknesses, plus per-corner radii.
-- compare: --viewport 1200x620 --time 0.2 --background 0,0,0
return function(Target)
	local Radii = { UDim.new(0, 1), UDim.new(0, 2), UDim.new(0, 4), UDim.new(0, 6), UDim.new(0, 9), UDim.new(0, 14), UDim.new(0, 22), UDim.new(0.5, 0) }
	local Positions = { Enum.BorderStrokePosition.Outer, Enum.BorderStrokePosition.Center, Enum.BorderStrokePosition.Inner }

	local RowIndex = 0
	for _, Thickness in { 2, 6 } do
		for _, Position in Positions do
			RowIndex += 1
			for Column, Radius in Radii do
				local Frame = Instance.new("Frame")
				Frame.Name = `K{RowIndex}_{Column}`
				Frame.Position = UDim2.fromOffset(30 + (Column - 1) * 140, 30 + (RowIndex - 1) * 90)
				Frame.Size = UDim2.fromOffset(90, 56)
				Frame.BackgroundTransparency = 1
				Frame.BorderSizePixel = 0

				local Corner = Instance.new("UICorner")
				Corner.CornerRadius = Radius
				Corner.Parent = Frame

				local Stroke = Instance.new("UIStroke")
				Stroke.ApplyStrokeMode = Enum.ApplyStrokeMode.Border
				Stroke.Color = Color3.new(1, 1, 1)
				Stroke.Thickness = Thickness
				Stroke.BorderStrokePosition = Position
				Stroke.Parent = Frame

				Frame.Parent = Target
			end
		end
	end
end
