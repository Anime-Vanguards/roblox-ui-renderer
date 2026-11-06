-- UIStroke (Border mode): thickness, joins, positions, corners, transparency, gradient, ZIndex.
return function(Target)
	local function Frame(X, Y, W, H, Radius, StrokeProps, Color)
		local F = Instance.new("Frame")
		F.Position = UDim2.fromOffset(X, Y)
		F.Size = UDim2.fromOffset(W, H)
		F.BackgroundColor3 = Color or Color3.fromRGB(70, 70, 90)
		F.BorderSizePixel = 0
		if Radius then
			local Corner = Instance.new("UICorner")
			Corner.CornerRadius = UDim.new(0, Radius)
			Corner.Parent = F
		end
		local Stroke = Instance.new("UIStroke")
		Stroke.ApplyStrokeMode = Enum.ApplyStrokeMode.Border
		Stroke.Color = Color3.fromRGB(255, 190, 40)
		for Key, Value in StrokeProps do
			(Stroke :: any)[Key] = Value
		end
		Stroke.Parent = F
		F.Parent = Target
		return F, Stroke
	end

	for Index, Thickness in { 1, 1.5, 2, 3, 4.5, 8 } do
		Frame(30 + (Index - 1) * 105, 30, 70, 50, nil, { Thickness = Thickness })
		Frame(30 + (Index - 1) * 105, 110, 70, 50, 12, { Thickness = Thickness })
	end

	for Index, Join in { Enum.LineJoinMode.Round, Enum.LineJoinMode.Bevel, Enum.LineJoinMode.Miter } do
		Frame(30 + (Index - 1) * 110, 200, 80, 50, nil, { Thickness = 8, LineJoinMode = Join })
	end

	for Index, Position in { Enum.BorderStrokePosition.Outer, Enum.BorderStrokePosition.Center, Enum.BorderStrokePosition.Inner } do
		Frame(370 + (Index - 1) * 110, 200, 80, 50, 10, { Thickness = 6, BorderStrokePosition = Position })
	end

	Frame(30, 290, 90, 60, 14, { Thickness = 4, Transparency = 0.5 })

	local _, GradientStroke = Frame(150, 290, 90, 60, 14, { Thickness = 5, Color = Color3.new(1, 1, 1) })
	local Gradient = Instance.new("UIGradient")
	Gradient.Color = ColorSequence.new(Color3.fromRGB(255, 0, 0), Color3.fromRGB(0, 0, 255))
	Gradient.Parent = GradientStroke

	local Hidden = Frame(270, 290, 90, 60, 14, { Thickness = 3 })
	Hidden.BackgroundTransparency = 1

	local Front, FrontStroke = Frame(390, 290, 90, 60, 14, { Thickness = 6, ZIndex = 2 })
	FrontStroke.Color = Color3.fromRGB(40, 220, 255)
	local Inner = Instance.new("Frame")
	Inner.Size = UDim2.new(1, 20, 0, 20)
	Inner.Position = UDim2.fromOffset(-10, -5)
	Inner.BackgroundColor3 = Color3.fromRGB(255, 80, 120)
	Inner.BorderSizePixel = 0
	Inner.Parent = Front

	Frame(510, 290, 90, 60, 14, { Thickness = 4, ZIndex = -5 })
end
