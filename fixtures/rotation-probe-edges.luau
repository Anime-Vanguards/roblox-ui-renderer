-- Edges of turned elements: plain and rounded fills at many angles, small, thin and fractional sizes, half
-- transparent fills, borders, and turned parents that clip their children (plain and rounded).
-- compare: --viewport 1200x760 --time 0.2 --background 0,0,0
return function(Target)
	local WHITE, RED, BLUE = Color3.new(1, 1, 1), Color3.fromRGB(255, 60, 60), Color3.fromRGB(60, 120, 255)
	local function Box(Parent, X, Y, W, H, Color, Props)
		local Frame = Instance.new("Frame")
		Frame.Position = UDim2.fromOffset(X, Y)
		Frame.Size = UDim2.fromOffset(W, H)
		Frame.BackgroundColor3 = Color
		Frame.BorderSizePixel = 0
		for Key, Value in Props or {} do
			(Frame :: any)[Key] = Value
		end
		Frame.Parent = Parent
		return Frame
	end
	local function Round(Frame, Radius)
		local Corner = Instance.new("UICorner")
		Corner.CornerRadius = UDim.new(0, Radius)
		Corner.Parent = Frame
	end
	local Angles = { 1, 3, 7, 15, 22.5, 30, 45, 60, 75, 89, 91, 135, 180.5, 200, 270.3, 350 }
	-- rows 1 and 2: plain and rounded fills
	for Index, Angle in Angles do
		local X = 14 + (Index - 1) * 74
		Box(Target, X, 20, 56, 36, WHITE, { Rotation = Angle, Name = `Plain{Index}` })
		Round(Box(Target, X, 100, 56, 36, WHITE, { Rotation = Angle, Name = `Round{Index}` }), 10)
	end
	-- row 3: small, thin and fractional sizes at two angles
	local Sizes = { { 3, 3 }, { 10, 2 }, { 1, 40 }, { 33.3, 21.7 }, { 60, 4 }, { 7.5, 7.5 }, { 2, 2 }, { 48, 1 } }
	for Index, Size in Sizes do
		local X = 20 + (Index - 1) * 146
		Box(Target, X, 190, Size[1], Size[2], WHITE, { Rotation = 10, Name = `SizeA{Index}` })
		Box(Target, X + 70.4, 190.6, Size[1], Size[2], WHITE, { Rotation = 37, Name = `SizeB{Index}` })
	end
	-- row 4: half transparent fills, one over another; borders
	for Index, Angle in { 5, 20, 45, 70 } do
		local X = 20 + (Index - 1) * 150
		Box(Target, X, 270, 90, 50, RED, { Rotation = Angle, BackgroundTransparency = 0.5, Name = `HalfA{Index}` })
		Box(Target, X + 30, 290, 90, 50, BLUE, { Rotation = -Angle, BackgroundTransparency = 0.5, Name = `HalfB{Index}` })
		Box(Target, 640 + (Index - 1) * 140, 280, 80, 44, WHITE, { Rotation = Angle, BorderSizePixel = 3, BorderColor3 = RED, Name = `Border{Index}` })
	end
	-- row 5: turned parents that clip a child reaching out of them, plain and rounded; a turned child in each
	for Index, Angle in { 8, 25, 45, 66, 100, 190 } do
		local X = 20 + (Index - 1) * 196
		local Plain = Box(Target, X, 400, 110, 64, RED, { Rotation = Angle, ClipsDescendants = true, Name = `Clip{Index}` })
		Box(Plain, 40, -20, 100, 60, WHITE, { Name = "Out" })
		Box(Plain, -10, 40, 60, 40, BLUE, { Rotation = 30, Name = "Turned" })
	end
	for Index, Angle in { 8, 25, 45, 66, 100, 190 } do
		local X = 20 + (Index - 1) * 196
		local Rounded = Box(Target, X, 560, 110, 64, RED, { Rotation = Angle, ClipsDescendants = true, Name = `ClipRound{Index}` })
		Round(Rounded, 16)
		Box(Rounded, 40, -20, 100, 60, WHITE, { Name = "Out" })
		Round(Box(Rounded, -10, 40, 60, 40, BLUE, { Rotation = 30, Name = "Turned" }), 8)
	end
	-- a plain parent clipping turned children at its edges
	local Holder = Box(Target, 20, 680, 1160, 60, Color3.fromRGB(30, 30, 30), { ClipsDescendants = true, Name = "Holder" })
	for Index, Angle in { 4, 17, 33, 45, 58, 80, 123, 211 } do
		Box(Holder, 20 + (Index - 1) * 142, (Index % 2 == 0) and -18 or 34, 90, 44, WHITE, { Rotation = Angle, Name = `Edge{Index}` })
	end
end
