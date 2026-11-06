-- Image sampling probe: magnification of small opaque textures (Motor.png 16x16, StoneBlockSide.png
-- 32x32) at integer and fractional scales, odd positions and non-square sizes.
return function(Target)
	local function Image(Uri, X, Y, W, H, Props)
		local Label = Instance.new("ImageLabel")
		Label.Name = `{X}_{Y}`
		Label.Position = UDim2.fromOffset(X, Y)
		Label.Size = UDim2.fromOffset(W, H)
		Label.BackgroundTransparency = 1
		Label.BorderSizePixel = 0
		Label.Image = Uri
		for Key, Value in Props or {} do
			(Label :: any)[Key] = Value
		end
		Label.Parent = Target
		return Label
	end

	local Motor = "rbxasset://textures/ui/Motor.png"
	local Stone = "rbxasset://textures/ui/LegacyRbxGui/StoneBlockSide.png"

	-- Row 1: square sizes at even positions (edges on whole physical pixels)
	local X = 10
	for _, Size in { 16, 32, 48, 64, 96, 100, 128 } do
		Image(Motor, X, 10, Size, Size)
		X += Size + 10
	end

	-- Row 2: odd positions and odd sizes (edges on half physical pixels)
	X = 11
	for _, Size in { 11, 13, 17, 21, 27, 33, 45, 57, 71, 91 } do
		Image(Motor, X, 151, Size, Size)
		X += Size + 11
	end

	-- Row 3: non-square
	X = 10
	for _, Dims in { { 64, 32 }, { 32, 64 }, { 100, 40 }, { 37, 81 }, { 150, 20 }, { 20, 150 }, { 7, 90 } } do
		Image(Motor, X, 260, Dims[1], Dims[2])
		X += Dims[1] + 10
	end

	-- Row 4: 32x32 texture
	X = 10
	for _, Size in { 21, 32, 43, 64, 75, 96, 128, 160 } do
		Image(Stone, X, 430, Size, Size)
		X += Size + 11
	end

	-- Row 5: Pixelated resampling
	X = 10
	for _, Size in { 16, 21, 32, 45, 64, 100 } do
		Image(Motor, X, 610, Size, Size, { ResampleMode = Enum.ResamplerMode.Pixelated })
		X += Size + 11
	end
end
