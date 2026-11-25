-- UIGradient Type Elliptical: tiles of several shapes (one at a fractional place and size), with Rotation, Offset,
-- Scale and TileMode, colour and keyed transparency sequences, keypoints closer than 1 / 256 under Scale, a UICorner, a border, a turned frame, an image, text
-- a UIStroke and a turned image. Most tiles are white under a transparency of 0 to 1, so a pixel's brightness over the black
-- background reads the gradient's t.
-- compare: --viewport 1000x740 --time 0.2 --background 0,0,0
return function(Target)
	local Elliptical = Enum.GradientType.Elliptical
	local Repeat, Mirror = Enum.GradientTileMode.Repeat, Enum.GradientTileMode.Mirror
	local Fade = NumberSequence.new(0, 1)
	local RedBlue = ColorSequence.new(Color3.fromRGB(255, 40, 40), Color3.fromRGB(40, 80, 255))
	local Count = 0
	-- The next cell's place: six columns 165 px apart, rows 120 px apart
	local function Cell(Dx, Dy)
		local Column, Row = Count % 6, Count // 6
		Count += 1
		return Count, UDim2.fromOffset(10 + Column * 165 + (Dx or 0), 10 + Row * 120 + (Dy or 0))
	end
	local function Gradient(Parent, Props)
		local G = Instance.new("UIGradient")
		G.Type = Elliptical
		G.Transparency = Fade
		for Key, Value in Props do
			(G :: any)[Key] = Value
		end
		G.Parent = Parent
	end
	local function Tile(W, H, Props, FrameProps)
		local Index, Position = Cell(FrameProps and FrameProps.Dx, FrameProps and FrameProps.Dy)
		local Frame = Instance.new("Frame")
		Frame.Name = `E{Index}`
		Frame.Position = Position
		Frame.Size = UDim2.fromOffset(W, H)
		Frame.BackgroundColor3 = Color3.new(1, 1, 1)
		Frame.BorderSizePixel = 0
		for Key, Value in FrameProps or {} do
			if Key ~= "Dx" and Key ~= "Dy" then
				(Frame :: any)[Key] = Value
			end
		end
		Gradient(Frame, Props)
		Frame.Parent = Target
		return Frame
	end

	-- row 1: shapes
	Tile(100, 100, {})
	Tile(150, 60, {})
	Tile(60, 100, {})
	Tile(150, 30, {})
	-- (a UDim's offset is whole: the scale gives the tile a fractional place and size, 670.4, 10.3, 101.3 x 67.71)
	Tile(100, 67, {}, { Position = UDim2.new(0.0004, 670, 0.3 / 740, 10), Size = UDim2.new(0.0013, 100, 0.71 / 740, 67) })
	Tile(150, 100, { Offset = Vector2.new(0.25, 0) })
	-- row 2: rotation and offset
	Tile(150, 80, { Rotation = 30 })
	Tile(150, 80, { Rotation = 90 })
	Tile(150, 80, { Rotation = 45 })
	Tile(150, 80, { Rotation = 30, Offset = Vector2.new(0.25, 0) })
	Tile(150, 80, { Offset = Vector2.new(0, -0.3) })
	Tile(150, 80, { Offset = Vector2.new(0.6, 0.6) })
	-- row 3: scale and tiling
	Tile(150, 80, { Scale = 0.5 })
	Tile(150, 80, { Scale = 2 })
	Tile(150, 80, { Scale = 0.5, TileMode = Repeat })
	Tile(150, 80, { Scale = 0.5, TileMode = Mirror })
	Tile(150, 80, { Scale = 0.5, Offset = Vector2.new(0.2, 0.1) })
	Tile(150, 80, { Scale = 2, Rotation = 30 })
	-- row 4: colour and keyed transparency sequences
	local Opaque = NumberSequence.new(0)
	Tile(150, 80, { Transparency = Opaque, Color = ColorSequence.new(Color3.new(0, 0, 0), Color3.new(1, 1, 1)) })
	Tile(150, 80, {
		Transparency = Opaque,
		Color = ColorSequence.new({
			ColorSequenceKeypoint.new(0, Color3.fromRGB(255, 0, 0)),
			ColorSequenceKeypoint.new(0.2, Color3.fromRGB(0, 255, 0)),
			ColorSequenceKeypoint.new(0.5, Color3.fromRGB(0, 0, 255)),
			ColorSequenceKeypoint.new(0.5001, Color3.fromRGB(255, 255, 0)),
			ColorSequenceKeypoint.new(1, Color3.fromRGB(20, 20, 20)),
		}),
	})
	Tile(150, 80, {
		Color = RedBlue,
		Transparency = NumberSequence.new({
			NumberSequenceKeypoint.new(0, 0),
			NumberSequenceKeypoint.new(0.3, 0.8),
			NumberSequenceKeypoint.new(0.7, 0.2),
			NumberSequenceKeypoint.new(1, 1),
		}),
	})
	Tile(150, 80, {
		Transparency = Opaque,
		Scale = 2,
		Color = ColorSequence.new({
			ColorSequenceKeypoint.new(0, Color3.new(0, 0, 0)),
			ColorSequenceKeypoint.new(0.3, Color3.new(1, 1, 1)),
			ColorSequenceKeypoint.new(0.3001, Color3.fromRGB(128, 128, 128)),
			ColorSequenceKeypoint.new(1, Color3.new(0, 0, 0)),
		}),
	})
	Tile(150, 80, { Transparency = Opaque, Color = RedBlue, Rotation = 30, Scale = 0.5, TileMode = Repeat })
	Tile(150, 80, { Scale = 0.25, TileMode = Mirror })
	-- row 5: a UICorner, a border, a turned frame, an image, text and a stroke
	local Rounded = Tile(150, 80, {})
	local Corner = Instance.new("UICorner")
	Corner.CornerRadius = UDim.new(0, 24)
	Corner.Parent = Rounded
	Tile(150, 80, {}, { BorderSizePixel = 6, BorderColor3 = Color3.new(1, 1, 1) })
	Tile(130, 70, {}, { Rotation = 20, Dx = 10, Dy = 5 })
	do
		local Index, Position = Cell()
		local Image = Instance.new("ImageLabel")
		Image.Name = `E{Index}`
		Image.Position = Position
		Image.Size = UDim2.fromOffset(150, 80)
		Image.BackgroundTransparency = 1
		Image.BorderSizePixel = 0
		Image.Image = "rbxasset://textures/ui/InGameMenu/WhiteSquare.png"
		Gradient(Image, { Color = RedBlue })
		Image.Parent = Target
	end
	do
		local Index, Position = Cell()
		local Label = Instance.new("TextLabel")
		Label.Name = `E{Index}`
		Label.Position = Position
		Label.Size = UDim2.fromOffset(150, 80)
		Label.BackgroundTransparency = 1
		Label.TextColor3 = Color3.new(1, 1, 1)
		Label.FontFace = Font.new("rbxasset://fonts/families/FredokaOne.json")
		Label.TextSize = 64
		Label.Text = "OVAL"
		Gradient(Label, { Transparency = Opaque, Color = RedBlue })
		Label.Parent = Target
	end
	do
		local Index, Position = Cell()
		local Frame = Instance.new("Frame")
		Frame.Name = `E{Index}`
		Frame.Position = Position + UDim2.fromOffset(8, 8)
		Frame.Size = UDim2.fromOffset(134, 64)
		Frame.BackgroundTransparency = 1
		Frame.BorderSizePixel = 0
		local Stroke = Instance.new("UIStroke")
		Stroke.ApplyStrokeMode = Enum.ApplyStrokeMode.Border
		Stroke.Color = Color3.new(1, 1, 1)
		Stroke.Thickness = 8
		Stroke.Parent = Frame
		Gradient(Stroke, { Transparency = Opaque, Color = RedBlue })
		Frame.Parent = Target
	end
	-- row 6: a turned image
	do
		local Index, Position = Cell(10, 5)
		local Image = Instance.new("ImageLabel")
		Image.Name = `E{Index}`
		Image.Position = Position
		Image.Size = UDim2.fromOffset(130, 70)
		Image.Rotation = 20
		Image.BackgroundTransparency = 1
		Image.BorderSizePixel = 0
		Image.Image = "rbxasset://textures/ui/InGameMenu/WhiteSquare.png"
		Gradient(Image, {})
		Image.Parent = Target
	end
	-- and keypoints closer than 1 / 256 under Scale above 1, one with the centre beyond the right edge
	local Close = ColorSequence.new({
		ColorSequenceKeypoint.new(0, Color3.new(0, 0, 0)),
		ColorSequenceKeypoint.new(0.3, Color3.new(1, 1, 1)),
		ColorSequenceKeypoint.new(0.3001, Color3.fromRGB(128, 128, 128)),
		ColorSequenceKeypoint.new(1, Color3.new(0, 0, 0)),
	})
	Tile(150, 80, { Transparency = Opaque, Color = Close, Scale = 1.25 })
	Tile(150, 80, { Transparency = Opaque, Color = Close, Scale = 3, Rotation = 90 })
	Tile(150, 80, { Transparency = Opaque, Color = Close, Scale = 4, Offset = Vector2.new(0.6, 0) })
end
