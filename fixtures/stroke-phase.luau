-- Border UIStroke at fractional physical positions: each column moves the frame by a sixteenth
-- pixel phase and gives its size another fraction, so edges land on different sub-pixel phases
-- (never exactly on .5, where Studio's float snapping is ambiguous).
-- UDim offsets are integers, so the fractions go through Scale (Target is 1000x760).
-- Rows: thickness × (sharp, UICorner 8) × (Outer, Center).
-- compare: --viewport 1000x760 --time 0.2 --background 0,0,0
return function(Target)
	local Width, Height = 1000, 760
	local Rows = {}
	for _, Thickness in { 1, 1.5, 2.25, 3.5 } do
		for _, Radius in { 0, 8 } do
			for _, Position in { Enum.BorderStrokePosition.Outer, Enum.BorderStrokePosition.Center } do
				table.insert(Rows, { Thickness = Thickness, Radius = Radius, Position = Position })
			end
		end
	end

	for RowIndex, Row in Rows do
		for Column = 0, 7 do
			-- Physical fractions (2k + 1) / 16 for x, then permuted for y, width and height
			local PhaseX = (2 * Column + 1) / 16 / 1.5
			local PhaseY = (2 * ((Column * 5) % 8) + 1) / 16 / 1.5
			local SizeX = ((Column * 3) % 8) / 8 / 1.5
			local SizeY = ((Column * 7) % 8) / 8 / 1.5
			local Frame = Instance.new("Frame")
			Frame.Name = `P{RowIndex}_{Column}`
			Frame.Position = UDim2.new(PhaseX / Width, 30 + Column * 120, PhaseY / Height, 24 + (RowIndex - 1) * 46)
			Frame.Size = UDim2.new(SizeX / Width, 60, SizeY / Height, 24)
			Frame.BackgroundTransparency = 1
			Frame.BorderSizePixel = 0

			if Row.Radius > 0 then
				local Corner = Instance.new("UICorner")
				Corner.CornerRadius = UDim.new(0, Row.Radius)
				Corner.Parent = Frame
			end

			local Stroke = Instance.new("UIStroke")
			Stroke.ApplyStrokeMode = Enum.ApplyStrokeMode.Border
			Stroke.Color = Color3.new(1, 1, 1)
			Stroke.Thickness = Row.Thickness
			Stroke.BorderStrokePosition = Row.Position
			Stroke.Parent = Frame

			Frame.Parent = Target
		end
	end
end
