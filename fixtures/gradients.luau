-- UIGradient: rotations on square/wide frames, offsets, transparency, multi-key, types, tiling.
return function(Target)
	local function Frame(X, Y, W, H, GradientProps, Radius)
		local F = Instance.new("Frame")
		F.Position = UDim2.fromOffset(X, Y)
		F.Size = UDim2.fromOffset(W, H)
		F.BackgroundColor3 = Color3.new(1, 1, 1)
		F.BorderSizePixel = 0
		local Gradient = Instance.new("UIGradient")
		Gradient.Color = ColorSequence.new(Color3.fromRGB(255, 40, 40), Color3.fromRGB(40, 80, 255))
		for Key, Value in GradientProps do
			(Gradient :: any)[Key] = Value
		end
		Gradient.Parent = F
		if Radius then
			local Corner = Instance.new("UICorner")
			Corner.CornerRadius = UDim.new(0, Radius)
			Corner.Parent = F
		end
		F.Parent = Target
		return F
	end

	for Index, Rotation in { 0, 30, 45, 90, 135, 180, -60 } do
		Frame(20 + (Index - 1) * 95, 20, 80, 80, { Rotation = Rotation })
		Frame(20 + (Index - 1) * 95, 115, 85, 40, { Rotation = Rotation })
	end

	for Index, Offset in { Vector2.new(0.25, 0), Vector2.new(-0.3, 0), Vector2.new(0, 0.4), Vector2.new(0.5, 0.5) } do
		Frame(20 + (Index - 1) * 95, 175, 80, 60, { Offset = Offset, Rotation = if Index > 2 then 90 else 0 })
	end

	Frame(420, 175, 120, 60, { Transparency = NumberSequence.new({ NumberSequenceKeypoint.new(0, 0), NumberSequenceKeypoint.new(0.5, 0.8), NumberSequenceKeypoint.new(1, 0) }) })
	Frame(560, 175, 120, 60, {
		Color = ColorSequence.new({
			ColorSequenceKeypoint.new(0, Color3.fromRGB(255, 0, 0)),
			ColorSequenceKeypoint.new(0.3, Color3.fromRGB(0, 255, 0)),
			ColorSequenceKeypoint.new(0.31, Color3.fromRGB(255, 255, 0)),
			ColorSequenceKeypoint.new(1, Color3.fromRGB(0, 0, 255)),
		}),
	})

	Frame(20, 260, 120, 80, { Type = Enum.GradientType.Radial }, 12)
	Frame(160, 260, 120, 80, { Type = Enum.GradientType.Conical })
	Frame(300, 260, 120, 80, { Scale = 0.5 })
	Frame(440, 260, 120, 80, { Scale = 0.5, TileMode = Enum.GradientTileMode.Repeat })
	Frame(580, 260, 120, 80, { Scale = 0.5, TileMode = Enum.GradientTileMode.Mirror })

	local Colored = Frame(20, 360, 120, 60, { Rotation = 0 })
	Colored.BackgroundColor3 = Color3.fromRGB(128, 255, 128)
	local Faded = Frame(160, 360, 120, 60, { Rotation = 0 })
	Faded.BackgroundTransparency = 0.5
end
