-- UIGradient keypoints closer than 1 / 256 under Scale, on Radial and Conical gradients: black at 0, white at 0.3,
-- grey 128 at 0.3001 and black at 1, so the colour where the cut list ends holds beyond it. Scale above 1 (one radial
-- tile with its centre beyond the right edge, a square at Scale 1.1 whose corners lie at t 1.29), Scale 1, a linear
-- tile under Scale 2 and Rotation 30, and Scale 0.5 and 0.75 on each type.
-- compare: --viewport 1000x380 --time 0.2 --background 0,0,0
return function(Target)
	local Close = ColorSequence.new({
		ColorSequenceKeypoint.new(0, Color3.new(0, 0, 0)),
		ColorSequenceKeypoint.new(0.3, Color3.new(1, 1, 1)),
		ColorSequenceKeypoint.new(0.3001, Color3.fromRGB(128, 128, 128)),
		ColorSequenceKeypoint.new(1, Color3.new(0, 0, 0)),
	})
	local Count = 0
	local function Tile(W, H, Props)
		local Column, Row = Count % 6, Count // 6
		Count += 1
		local Frame = Instance.new("Frame")
		Frame.Name = `C{Count}`
		Frame.Position = UDim2.fromOffset(10 + Column * 165, 10 + Row * 120)
		Frame.Size = UDim2.fromOffset(W, H)
		Frame.BackgroundColor3 = Color3.new(1, 1, 1)
		Frame.BorderSizePixel = 0
		local G = Instance.new("UIGradient")
		G.Color = Close
		for Key, Value in Props do
			(G :: any)[Key] = Value
		end
		G.Parent = Frame
		Frame.Parent = Target
	end
	local Radial, Conical = Enum.GradientType.Radial, Enum.GradientType.Conical
	-- row 1: radial
	Tile(150, 80, { Type = Radial, Scale = 2 })
	Tile(150, 80, { Type = Radial, Scale = 2, Offset = Vector2.new(0.25, 0) })
	Tile(150, 80, { Type = Radial, Scale = 1.25 })
	Tile(150, 80, { Type = Radial, Scale = 4, Offset = Vector2.new(0.6, 0) })
	Tile(100, 100, { Type = Radial, Scale = 1.1 })
	Tile(150, 80, { Type = Radial, Scale = 0.5 })
	-- row 2: conical
	Tile(150, 80, { Type = Conical, Scale = 2 })
	Tile(150, 80, { Type = Conical, Scale = 1.25 })
	Tile(150, 80, { Type = Conical, Scale = 4, Rotation = 30 })
	Tile(150, 80, { Type = Conical, Scale = 2, Offset = Vector2.new(0.25, 0.25) })
	Tile(150, 80, { Type = Conical, Scale = 0.5 })
	Tile(150, 80, { Type = Conical })
	-- row 3: radial at Scale 1, linear, and Scale below 1 on each type
	Tile(150, 80, { Type = Radial })
	Tile(150, 80, { Scale = 2, Rotation = 30 })
	Tile(150, 80, { Type = Enum.GradientType.Elliptical, Scale = 0.5 })
	Tile(150, 80, { Type = Radial, Scale = 0.75 })
	Tile(150, 80, { Type = Conical, Scale = 0.75 })
end
