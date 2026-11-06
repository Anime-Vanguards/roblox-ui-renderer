-- Slice probe: centre mapping when the element is smaller than the texture's natural size (no border
-- overflow), overflow on one or both axes and SliceScale variants. Coordinates are physical pixels (UIScale
-- 2/3 cancels the 1.5 DPI), so SliceScale 1 draws one texel per pixel.
return function(Target)
	local Root = Instance.new("Frame")
	Root.Name = "Root"
	Root.Size = UDim2.fromOffset(1800, 1200)
	Root.BackgroundTransparency = 1
	Root.BorderSizePixel = 0
	local Scale = Instance.new("UIScale")
	Scale.Scale = 2 / 3
	Scale.Parent = Root
	Root.Parent = Target

	local function Slice(Uri, X, Y, W, H, Center, Props)
		local Label = Instance.new("ImageLabel")
		Label.Name = `{X}_{Y}`
		Label.Position = UDim2.fromOffset(X, Y)
		Label.Size = UDim2.fromOffset(W, H)
		Label.BackgroundColor3 = Color3.fromRGB(60, 60, 72)
		Label.BorderSizePixel = 0
		Label.Image = Uri
		Label.ScaleType = Enum.ScaleType.Slice
		Label.SliceCenter = Center
		for Key, Value in Props or {} do
			(Label :: any)[Key] = Value
		end
		Label.Parent = Root
		return Label
	end

	local Stone = "rbxasset://textures/ui/LegacyRbxGui/StoneBlockSide.png" -- 32x32
	local Motor = "rbxasset://textures/ui/Motor.png" -- 16x16
	local Glow = "rbxasset://textures/ui/Lobby/Buttons/glow_nine_slice.png" -- 156x156
	local Center = Rect.new(8, 8, 24, 24)
	local Double = { SliceScale = 2 }

	-- SliceScale 2: natural size 64, borders 32; smaller elements without border overflow
	local X = 10
	for _, Size in { 64, 56, 48, 40, 36, 33 } do
		Slice(Stone, X, 10, Size, Size, Center, Double)
		X += Size + 12
	end
	-- One axis overflowing, the other not; both overflowing
	X = 10
	for _, Dims in { { 24, 60 }, { 60, 24 }, { 28, 44 }, { 44, 28 }, { 26, 22 }, { 20, 29 } } do
		Slice(Stone, X, 90, Dims[1], Dims[2], Center, Double)
		X += Dims[1] + 12
	end
	-- SliceScale 1 (one texel per pixel): natural size 32
	X = 10
	for _, Size in { 32, 28, 24, 20, 17, 12 } do
		Slice(Stone, X, 170, Size, Size, Center)
		X += Size + 12
	end
	-- Motor 16x16 with a wide centre and magnified borders
	Slice(Motor, 10, 220, 60, 30, Rect.new(2, 2, 14, 14), { SliceScale = 3 })
	Slice(Motor, 82, 220, 30, 30, Rect.new(2, 2, 14, 14), { SliceScale = 3 })
	Slice(Motor, 124, 220, 20, 40, Rect.new(2, 2, 14, 14), { SliceScale = 3 })
	Slice(Motor, 156, 220, 14, 14, Rect.new(2, 2, 14, 14), { SliceScale = 3 })
	-- Glow (156, centre 36) at one texel per pixel
	local GlowCenter = Rect.new(60, 60, 96, 96)
	X = 10
	for _, Dims in { { 150, 150 }, { 140, 150 }, { 130, 100 }, { 125, 90 }, { 90, 60 }, { 60, 90 } } do
		Slice(Glow, X, 300, Dims[1], Dims[2], GlowCenter)
		X += Dims[1] + 12
	end
end
