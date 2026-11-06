-- Border UIStroke render modes: small corner radii (ring vs clip mode switch at inner radius 0),
-- Miter/Bevel joins on rounded and sharp frames, Inner strokes thicker than the radius and
-- per-corner radii. White strokes on transparent 60x40 frames at integer physical positions.
-- compare: --viewport 860x760 --time 0.2 --background 0,0,0
return function(Target)
	local Outer = Enum.BorderStrokePosition.Outer
	local Center = Enum.BorderStrokePosition.Center
	local Inner = Enum.BorderStrokePosition.Inner
	local Round = Enum.LineJoinMode.Round
	local Miter = Enum.LineJoinMode.Miter
	local Bevel = Enum.LineJoinMode.Bevel

	local Rows = {
		{ Radius = 1, Join = Round, Cells = { { Outer, 0.25 }, { Outer, 0.5 }, { Outer, 1 }, { Outer, 1.5 }, { Outer, 2 }, { Outer, 2.25 }, { Outer, 3 }, { Outer, 5 } } },
		{ Radius = 1, Join = Round, Cells = { { Center, 1 }, { Center, 2 }, { Center, 3 }, { Center, 3.25 }, { Center, 4.5 }, { Center, 5 }, { Center, 6 }, { Center, 7 } } },
		{ Radius = 3, Join = Round, Cells = { { Center, 1 }, { Center, 2 }, { Center, 3 }, { Center, 3.25 }, { Center, 4.5 }, { Center, 5 }, { Center, 6 }, { Center, 7 } } },
		{ Radius = 3, Join = Round, Cells = { { Inner, 1 }, { Inner, 2 }, { Inner, 2.5 }, { Inner, 3 }, { Inner, 3.5 }, { Inner, 4 }, { Inner, 5 }, { Inner, 6 } } },
		{ Radius = 4, Join = Miter, Cells = { { Outer, 1 }, { Outer, 3.5 }, { Center, 2 }, { Center, 6 }, { Center, 10 }, { Inner, 2 }, { Inner, 4 }, { Inner, 6 } } },
		{ Radius = 4, Join = Bevel, Cells = { { Outer, 1 }, { Outer, 3.5 }, { Center, 2 }, { Center, 6 }, { Center, 10 }, { Inner, 2 }, { Inner, 4 }, { Inner, 6 } } },
		{ Radius = nil, Join = Miter, Cells = { { Outer, 3 }, { Center, 2 }, { Center, 3 }, { Center, 6 }, { Inner, 1 }, { Inner, 2 }, { Inner, 3 }, { Inner, 6 } } },
		{ Radius = nil, Join = Bevel, Cells = { { Outer, 3 }, { Center, 2 }, { Center, 3 }, { Center, 6 }, { Inner, 1 }, { Inner, 2 }, { Inner, 3 }, { Inner, 6 } } },
		{ Radius = 4, Join = Round, Cells = { { Inner, 1 }, { Inner, 2 }, { Inner, 3 }, { Inner, 4 }, { Inner, 4.5 }, { Inner, 5 }, { Inner, 6 }, { Inner, 7 } } },
		{ Radius = "Mixed", Join = Round, Cells = { { Outer, 1 }, { Outer, 3 }, { Center, 2 }, { Center, 6 }, { Inner, 2 }, { Inner, 6 }, { Center, 12 }, { Inner, 9 } } },
	}

	for RowIndex, Row in Rows do
		for Column, Cell in Row.Cells do
			local Frame = Instance.new("Frame")
			Frame.Name = `S{RowIndex}_{Column}`
			Frame.Position = UDim2.fromOffset(40 + (Column - 1) * 100, 40 + (RowIndex - 1) * 70)
			Frame.Size = UDim2.fromOffset(60, 40)
			Frame.BackgroundTransparency = 1
			Frame.BorderSizePixel = 0

			if Row.Radius == "Mixed" then
				local Corner = Instance.new("UICorner")
				Corner.TopLeftRadius = UDim.new(0, 8)
				Corner.TopRightRadius = UDim.new(0, 0)
				Corner.BottomRightRadius = UDim.new(0, 3)
				Corner.BottomLeftRadius = UDim.new(0, 12)
				Corner.Parent = Frame
			elseif Row.Radius then
				local Corner = Instance.new("UICorner")
				Corner.CornerRadius = UDim.new(0, Row.Radius)
				Corner.Parent = Frame
			end

			local Stroke = Instance.new("UIStroke")
			Stroke.ApplyStrokeMode = Enum.ApplyStrokeMode.Border
			Stroke.Color = Color3.new(1, 1, 1)
			Stroke.Thickness = Cell[2]
			Stroke.BorderStrokePosition = Cell[1]
			Stroke.LineJoinMode = Row.Join
			Stroke.Parent = Frame

			Frame.Parent = Target
		end
	end
end
