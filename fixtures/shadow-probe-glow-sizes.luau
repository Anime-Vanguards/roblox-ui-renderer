-- UIShadow probe: the glow of Mode = Text at text sizes that are not whole device pixels. TextScaled labels of
-- several heights, labels under UIScale, and TextSizes whose device size has a fraction (27, 33, 41 at 1.5x), at
-- several BlurRadius; red shadows of blue text on black, most of them clear so the red channel is the glow's alpha.
-- compare --viewport 1200x900 --time 0.3 --background 0,0,0
return function(Target)
	local Fredoka = Font.new("rbxasset://fonts/families/FredokaOne.json")
	local function Label(X, Y, W, H, Props, Blur)
		local L = Instance.new("TextLabel")
		L.Position = UDim2.fromOffset(X, Y)
		L.Size = UDim2.fromOffset(W, H)
		L.BackgroundTransparency = 1
		L.BorderSizePixel = 0
		L.Text = "Glow"
		L.TextColor3 = Color3.new(0, 0, 1)
		L.TextTransparency = 1
		L.FontFace = Fredoka
		L.TextSize = 30
		for Key, Value in Props or {} do
			(L :: any)[Key] = Value
		end
		local S = Instance.new("UIShadow")
		S.Mode = Enum.ApplyShadowMode.Text
		S.Color = Color3.new(1, 0, 0)
		S.BlurRadius = UDim.new(0, Blur or 8)
		S.Parent = L
		L.Parent = Target
		return L
	end
	local function Scaled(L, Scale)
		local U = Instance.new("UIScale")
		U.Scale = Scale
		U.Parent = L
		return L
	end

	-- A: TextScaled labels of several heights, BlurRadius 8
	for Index, H in { 29, 37, 46, 53, 71, 95 } do
		Label(10 + (Index - 1) * 195, 10, 180, H, { TextScaled = true })
	end
	-- B: one TextScaled height at BlurRadius 2, 4, 16, 24, and with the text shown
	for Index, Blur in { 2, 4, 16, 24 } do
		Label(10 + (Index - 1) * 195, 140, 180, 53, { TextScaled = true }, Blur)
	end
	Label(790, 140, 180, 53, { TextScaled = true, TextTransparency = 0 })
	Label(985, 140, 180, 53, { TextScaled = true, FontFace = Font.fromEnum(Enum.Font.SourceSansBold), Text = "Hello" })
	-- C: UIScale 0.75, 1.3, 1.7 and 2.25 (TextSize 30), BlurRadius 8 and 3
	Scaled(Label(10, 250, 120, 40), 0.75)
	Scaled(Label(150, 250, 120, 40), 1.3)
	Scaled(Label(340, 250, 120, 40), 1.7)
	Scaled(Label(570, 250, 120, 40), 2.25)
	Scaled(Label(870, 250, 120, 40, nil, 3), 1.3)
	Scaled(Label(1040, 250, 80, 40, { FontFace = Font.fromEnum(Enum.Font.SourceSans), Text = "Hi" }, 3), 1.7)
	-- D: TextSizes with a fraction of a device pixel at 1.5x, BlurRadius 4 and 8
	for Index, Size in { 13, 27, 33, 41, 55 } do
		Label(10 + (Index - 1) * 235, 400, 225, 70, { TextSize = Size }, 4)
		Label(10 + (Index - 1) * 235, 490, 225, 70, { TextSize = Size }, 8)
	end
	-- E: SourceSans words at a fractional size, TextScaled in a wide box, two lines
	Label(10, 590, 280, 60, { FontFace = Font.fromEnum(Enum.Font.SourceSans), Text = "Source 27", TextSize = 27 }, 6)
	Label(310, 590, 280, 60, { FontFace = Font.fromEnum(Enum.Font.SourceSans), Text = "Source scaled", TextScaled = true }, 6)
	Label(610, 590, 280, 120, { Text = "Two\nlines", TextScaled = true }, 6)
	Scaled(Label(910, 590, 180, 60, { TextScaled = true }, 6), 1.3)
end
