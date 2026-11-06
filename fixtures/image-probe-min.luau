-- Image sampling probe: minification (mip selection, trilinear blend, mip generation). Coordinates are
-- physical pixels: the container's UIScale of 2/3 cancels the 1.5 DPI.
return function(Target)
	local Root = Instance.new("Frame")
	Root.Name = "Root"
	Root.Size = UDim2.fromOffset(1950, 1200)
	Root.BackgroundTransparency = 1
	Root.BorderSizePixel = 0
	local Scale = Instance.new("UIScale")
	Scale.Scale = 2 / 3
	Scale.Parent = Root
	Root.Parent = Target

	local function Image(Uri, X, Y, W, H)
		local Label = Instance.new("ImageLabel")
		Label.Name = `{X}_{Y}`
		Label.Position = UDim2.fromOffset(X, Y)
		Label.Size = UDim2.fromOffset(W, H)
		Label.BackgroundTransparency = 1
		Label.BorderSizePixel = 0
		Label.Image = Uri
		Label.Parent = Root
		return Label
	end

	local Photo = "rbxasset://textures/ui/LuaApp/graphic/Auth/FPSBackground.png" -- 1024x1024
	local Grid = "rbxasset://textures/grid16.png" -- 512x512
	local Wide = "rbxasset://textures/ui/LuaApp/graphic/EducationalBackground.png" -- 541x261

	-- Row 1: 1024² photo at power-of-two and in-between sizes
	local X = 6
	for _, Size in { 512, 384, 256, 200, 128, 100, 64, 48, 32, 24, 16, 12, 8, 6, 4, 2, 1 } do
		Image(Photo, X, 6, Size, Size)
		X += Size + 6
	end

	-- Row 2: 512² grid (single-texel lines)
	X = 6
	for _, Size in { 256, 170, 128, 100, 90, 64, 50, 45, 32, 20, 16, 10, 8, 4, 2 } do
		Image(Grid, X, 530, Size, Size)
		X += Size + 6
	end

	-- Row 3: anisotropic scaling and a non-power-of-two texture
	Image(Photo, 6, 800, 1024, 96)
	Image(Photo, 6, 902, 300, 60)
	Image(Photo, 312, 902, 60, 290)
	Image(Grid, 378, 902, 512, 40)
	Image(Grid, 378, 948, 40, 240)
	Image(Wide, 424, 948, 270, 130)
	Image(Wide, 700, 948, 180, 87)
	Image(Wide, 886, 948, 100, 48)
	Image(Wide, 992, 902, 541, 261)
	Image(Wide, 1539, 902, 300, 200)
end
