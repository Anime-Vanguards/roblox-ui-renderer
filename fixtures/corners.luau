-- UICorner: offsets, scale, clamping, per-corner radii, with transparency.
return function(Target)
	local function Frame(X, Y, W, H, Color, CornerProps)
		local F = Instance.new("Frame")
		F.Position = UDim2.fromOffset(X, Y)
		F.Size = UDim2.fromOffset(W, H)
		F.BackgroundColor3 = Color
		F.BorderSizePixel = 0
		local Corner = Instance.new("UICorner")
		for Key, Value in CornerProps do
			(Corner :: any)[Key] = Value
		end
		Corner.Parent = F
		F.Parent = Target
		return F
	end

	local Radii = { 0, 2, 4, 8, 13, 24, 40 }
	for Index, Radius in Radii do
		Frame(20 + (Index - 1) * 100, 20, 80, 80, Color3.fromRGB(80, 160, 255), { CornerRadius = UDim.new(0, Radius) })
		Frame(20 + (Index - 1) * 100, 120, 90, 45, Color3.fromRGB(255, 120, 80), { CornerRadius = UDim.new(0, Radius) })
	end

	for Index, Scale in { 0.1, 0.25, 0.5, 1 } do
		Frame(20 + (Index - 1) * 110, 190, 100, 50, Color3.fromRGB(120, 230, 120), { CornerRadius = UDim.new(Scale, 0) })
		Frame(470 + (Index - 1) * 70, 190, 50, 100, Color3.fromRGB(230, 200, 90), { CornerRadius = UDim.new(Scale, 0) })
	end

	Frame(20, 310, 120, 80, Color3.fromRGB(200, 100, 255), { TopLeftRadius = UDim.new(0, 30), TopRightRadius = UDim.new(0, 4), BottomRightRadius = UDim.new(0, 16), BottomLeftRadius = UDim.new(0, 0) })
	Frame(160, 310, 120, 80, Color3.fromRGB(200, 100, 255), { CornerRadius = UDim.new(0, 20), TopLeftRadius = UDim.new(0, 2) })

	local Faded = Frame(300, 310, 120, 80, Color3.fromRGB(255, 255, 255), { CornerRadius = UDim.new(0, 16) })
	Faded.BackgroundTransparency = 0.5

	local Bordered = Frame(440, 310, 120, 80, Color3.fromRGB(60, 200, 200), { CornerRadius = UDim.new(0, 16) })
	Bordered.BorderSizePixel = 3
	Bordered.BorderColor3 = Color3.new(1, 0, 0)

	Frame(581, 311, 97, 59, Color3.fromRGB(250, 250, 250), { CornerRadius = UDim.new(0, 11) })
end
