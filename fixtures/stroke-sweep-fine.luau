-- Border UIStroke thickness between the quarter steps of stroke-sweep.luau, and the same thicknesses
-- reached through a UIScale (a stage list: 1.65 under UIScale 0.8392 = 1.3846 px at 1x).
-- White Outer strokes on transparent 40x30 frames at integer positions. Row 1: thickness t directly;
-- row 2: t / 0.8392 under UIScale 0.8392; row 3: Center; row 4: Inner; row 5: UICorner(10) Outer;
-- row 6: UICorner(10) Outer at 1.30 .. 1.49 in 0.01 steps (the outer edge 0 .. 0.2 px short of the
-- next pixel centre: where the rounded stroke's quad margin ends).
-- compare: --viewport 1360x400 --time 0.2 --background 0,0,0
return function(Target)
	local Thicknesses = {}
	for Step = 0, 19 do
		table.insert(Thicknesses, 1 + Step * 0.05)
	end
	local Scale = 0.8391608595848083
	local Rows = {
		{ Scaled = false, Position = Enum.BorderStrokePosition.Outer },
		{ Scaled = true, Position = Enum.BorderStrokePosition.Outer },
		{ Scaled = false, Position = Enum.BorderStrokePosition.Center },
		{ Scaled = false, Position = Enum.BorderStrokePosition.Inner },
		{ Scaled = false, Position = Enum.BorderStrokePosition.Outer, Radius = 10 },
		{ Scaled = false, Position = Enum.BorderStrokePosition.Outer, Radius = 10, Fine = true },
	}

	for RowIndex, Row in Rows do
		local Holder = Instance.new("Frame")
		Holder.Name = `Row{RowIndex}`
		Holder.BackgroundTransparency = 1
		Holder.Position = UDim2.fromOffset(0, (RowIndex - 1) * 80)
		Holder.Size = UDim2.fromOffset(1360, 80)
		Holder.Parent = Target
		local Unit = 1
		if Row.Scaled then
			local UIScale = Instance.new("UIScale")
			UIScale.Scale = Scale
			UIScale.Parent = Holder
			Unit = 1 / Scale
		end
		for Column, Thickness in Thicknesses do
			if Row.Fine then
				Thickness = 1.3 + (Column - 1) * 0.01
			end
			local Frame = Instance.new("Frame")
			Frame.Name = `R{RowIndex}C{Column}`
			Frame.Position = UDim2.fromOffset((30 + (Column - 1) * 64) * Unit, 30 * Unit)
			Frame.Size = UDim2.fromOffset(40 * Unit, 30 * Unit)
			Frame.BackgroundTransparency = 1
			Frame.BorderSizePixel = 0
			if Row.Radius then
				local Corner = Instance.new("UICorner")
				Corner.CornerRadius = UDim.new(0, Row.Radius)
				Corner.Parent = Frame
			end
			local Stroke = Instance.new("UIStroke")
			Stroke.Color = Color3.new(1, 1, 1)
			Stroke.Thickness = Thickness * Unit
			Stroke.ApplyStrokeMode = Enum.ApplyStrokeMode.Border
			Stroke.BorderStrokePosition = Row.Position
			Stroke.Parent = Frame
			Frame.Parent = Holder
		end
	end
end
