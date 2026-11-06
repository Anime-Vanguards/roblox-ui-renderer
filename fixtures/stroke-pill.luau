-- UIStroke on pills (UICorner 0.5 scale: the radius is half the raw short side) at fractional device
-- heights and positions, a UI kit's button shape (stage list / button story rims). White strokes on
-- transparent frames over black, so each pixel is the coverage.
-- compare: --viewport 1200x800 --time 0.2 --background 0,0,0
return function(Target)
	local Index = 0
	local function Pill(X, Y, W, H, Thickness, Position, Scale)
		local Frame = Instance.new("Frame")
		Frame.Name = string.format("P%d", Index)
		Index += 1
		Frame.Position = UDim2.fromOffset(X, Y)
		Frame.Size = UDim2.fromOffset(W, H)
		Frame.BackgroundTransparency = 1
		Frame.BorderSizePixel = 0
		local Corner = Instance.new("UICorner")
		Corner.CornerRadius = UDim.new(Scale or 0.5, 0)
		Corner.Parent = Frame
		local Stroke = Instance.new("UIStroke")
		Stroke.Color = Color3.new(1, 1, 1)
		Stroke.Thickness = Thickness
		Stroke.BorderStrokePosition = Position or Enum.BorderStrokePosition.Outer
		Stroke.Parent = Frame
		Frame.Parent = Target
	end
	-- Row 1-3: height 40 + k / 5 (device heights 60.3 .. 61.2), y offsets on a 1/3 grid, Outer 2
	for K = 0, 9 do
		Pill(20 + K * 115, 20 + (K % 3) / 3, 90, 40 + K / 5, 2)
		Pill(20 + K * 115, 100 + (K % 3) / 3 + 0.25, 90, 40.1 + K / 5, 1.77)
		Pill(20 + K * 115, 180, 90, 38.35 + K / 5, 2.2125, Enum.BorderStrokePosition.Center)
	end
	-- Row 4: the button shape (raw 57.525 x 177 at y .5), Outer 1.77 and 2.2125, Inner 1.3275
	for K = 0, 3 do
		Pill(20 + K * 230, 260 + K * 0.25, 177, 57.525, ({ 1.77, 2.2125, 1.3275, 3 })[K + 1], ({ Enum.BorderStrokePosition.Outer, Enum.BorderStrokePosition.Outer, Enum.BorderStrokePosition.Inner, Enum.BorderStrokePosition.Outer })[K + 1])
	end
	-- Row 5: radius slightly below half (0.49) and exactly half an integer height
	for K = 0, 5 do
		Pill(20 + K * 160, 360 + K / 6, 120, 30 + K / 3, 2, nil, 0.49)
		Pill(20 + K * 160, 440 + K / 6, 120, 30 + K * 2 / 3, 2, nil, 0.5)
	end
end
