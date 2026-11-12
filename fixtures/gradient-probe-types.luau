-- UIGradient types and tiling: Radial and Conical on tiles of several shapes, with Offset, Rotation and Scale,
-- TileMode Repeat and Mirror on each type, and Transparency keypoints between the Color keypoints.
-- compare: --viewport 1000x560 --time 0.2 --background 0,0,0
return function(Target)
	local RedBlue = ColorSequence.new(Color3.fromRGB(255, 40, 40), Color3.fromRGB(40, 80, 255))
	local Count = 0
	local function Tile(W, H, Props)
		local Column, Row = Count % 7, Count // 7
		Count += 1
		local Frame = Instance.new("Frame")
		Frame.Name = `G{Count}`
		Frame.Position = UDim2.fromOffset(10 + Column * 140, 10 + Row * 110)
		Frame.Size = UDim2.fromOffset(W, H)
		Frame.BackgroundColor3 = Color3.new(1, 1, 1)
		Frame.BorderSizePixel = 0
		local Gradient = Instance.new("UIGradient")
		Gradient.Color = RedBlue
		for Key, Value in Props do
			(Gradient :: any)[Key] = Value
		end
		Gradient.Parent = Frame
		Frame.Parent = Target
	end
	local Radial, Conical = Enum.GradientType.Radial, Enum.GradientType.Conical
	local Repeat, Mirror = Enum.GradientTileMode.Repeat, Enum.GradientTileMode.Mirror
	-- row 1: radial shapes
	Tile(100, 100, { Type = Radial })
	Tile(130, 50, { Type = Radial })
	Tile(40, 100, { Type = Radial })
	Tile(120, 80, { Type = Radial, Offset = Vector2.new(0.25, 0) })
	Tile(120, 80, { Type = Radial, Offset = Vector2.new(0, -0.5) })
	Tile(120, 80, { Type = Radial, Rotation = 30 })
	Tile(120, 80, { Type = Radial, Scale = 0.5 })
	-- row 2: radial tiling and scale, conical shapes
	Tile(120, 80, { Type = Radial, Scale = 0.5, TileMode = Repeat })
	Tile(120, 80, { Type = Radial, Scale = 0.5, TileMode = Mirror })
	Tile(120, 80, { Type = Radial, Scale = 2 })
	Tile(100, 100, { Type = Conical })
	Tile(130, 50, { Type = Conical })
	Tile(40, 100, { Type = Conical })
	Tile(120, 80, { Type = Conical, Rotation = 30 })
	-- row 3: conical offset, scale and tiling
	Tile(120, 80, { Type = Conical, Rotation = 90 })
	Tile(120, 80, { Type = Conical, Offset = Vector2.new(0.25, 0.25) })
	Tile(120, 80, { Type = Conical, Scale = 0.5 })
	Tile(120, 80, { Type = Conical, Scale = 0.5, TileMode = Repeat })
	Tile(120, 80, { Type = Conical, Scale = 0.5, TileMode = Mirror })
	Tile(120, 80, { Type = Conical, Scale = 2 })
	Tile(120, 80, { Scale = 2 })
	-- row 4: linear tiling with rotation and offset, scale above 1 tiled, transparency
	Tile(120, 80, { Scale = 0.25, TileMode = Repeat, Rotation = 30 })
	Tile(120, 80, { Scale = 0.25, TileMode = Mirror, Rotation = 90, Offset = Vector2.new(0, 0.1) })
	Tile(120, 80, { Scale = 0.5, TileMode = Repeat, Offset = Vector2.new(0.3, 0) })
	Tile(120, 80, { Scale = 2, TileMode = Repeat })
	Tile(120, 80, { Transparency = NumberSequence.new({ NumberSequenceKeypoint.new(0, 0), NumberSequenceKeypoint.new(0.25, 1), NumberSequenceKeypoint.new(0.6, 0.2), NumberSequenceKeypoint.new(1, 0.5) }) })
	Tile(120, 80, { Transparency = NumberSequence.new(0.5) })
	Tile(120, 80, { Type = Radial, Transparency = NumberSequence.new(0, 1) })
	-- row 5: many colour keys, a step, keys with tiling
	local Keys = ColorSequence.new({
		ColorSequenceKeypoint.new(0, Color3.fromRGB(255, 0, 0)),
		ColorSequenceKeypoint.new(0.2, Color3.fromRGB(0, 255, 0)),
		ColorSequenceKeypoint.new(0.5, Color3.fromRGB(0, 0, 255)),
		ColorSequenceKeypoint.new(0.5001, Color3.fromRGB(255, 255, 0)),
		ColorSequenceKeypoint.new(1, Color3.fromRGB(20, 20, 20)),
	})
	Tile(120, 80, { Color = Keys })
	Tile(120, 80, { Color = Keys, Type = Radial })
	Tile(120, 80, { Color = Keys, Type = Conical })
	Tile(120, 80, { Color = Keys, Scale = 0.5, TileMode = Mirror })
	Tile(120, 80, { Color = Keys, Rotation = 45, Transparency = NumberSequence.new({ NumberSequenceKeypoint.new(0, 0.5), NumberSequenceKeypoint.new(0.35, 0), NumberSequenceKeypoint.new(1, 0.9) }) })
	Tile(120, 80, { Color = ColorSequence.new(Color3.fromRGB(10, 10, 10), Color3.fromRGB(250, 250, 250)) })
	Tile(120, 80, { Color = ColorSequence.new(Color3.fromRGB(0, 0, 0), Color3.fromRGB(128, 128, 128)), Type = Radial })
end
