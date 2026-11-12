-- UIShadow probe: the distance field behind Mode = Text, on glyphs large enough to show its texels. Clear text
-- with a red shadow on black, so the red channel is the shadow's alpha. Rows 1 and 2: one letter at TextSize 100
-- under UIScale 3 (a 450 device px em at 1.5x), by blur and by letter; row 3: the same letter without UIScale,
-- by blur and by TextSize.
-- compare --viewport 1400x900 --time 0.3 --background 0,0,0
return function(Target)
	local Bold = Font.new("rbxasset://fonts/families/SourceSansPro.json", Enum.FontWeight.Bold)
	local function Label(X, Y, W, H, Text, Size, Blur, Scale, Props)
		local L = Instance.new("TextLabel")
		L.Position = UDim2.fromOffset(X, Y)
		L.Size = UDim2.fromOffset(W / (Scale or 1), H / (Scale or 1))
		L.BackgroundTransparency = 1
		L.BorderSizePixel = 0
		L.Text = Text
		L.TextColor3 = Color3.new(0, 0, 1)
		L.TextTransparency = 1
		L.FontFace = Bold
		L.TextSize = Size
		for Key, Value in Props or {} do
			(L :: any)[Key] = Value
		end
		local S = Instance.new("UIShadow")
		S.Mode = Enum.ApplyShadowMode.Text
		S.Color = Color3.new(1, 0, 0)
		S.BlurRadius = UDim.new(0, Blur)
		S.Parent = L
		if Scale then
			local U = Instance.new("UIScale")
			U.Scale = Scale
			U.Parent = L
		end
		L.Parent = Target
		return L
	end
	-- 1: "L" at a 450 px em, blur 2, 8, 30, 120
	for Index, Blur in { 2, 8, 30, 120 } do
		Label(10 + (Index - 1) * 340, 10, 330, 330, "L", 100, Blur, 3)
	end
	-- 2: other letters at blur 30: a round one, one with a dot, a diagonal, another face
	Label(10, 350, 330, 330, "o", 100, 30, 3)
	Label(350, 350, 330, 330, "i", 100, 30, 3)
	Label(690, 350, 330, 330, "A", 100, 30, 3)
	Label(1030, 350, 330, 330, "L", 100, 30, 3, { FontFace = Font.new("rbxasset://fonts/families/GothamSSm.json", Enum.FontWeight.Medium) })
	-- 3: "L" without UIScale: TextSize 100 by blur, then smaller sizes
	for Index, Blur in { 2, 8, 24, 48 } do
		Label(10 + (Index - 1) * 200, 690, 190, 200, "L", 100, Blur)
	end
	Label(810, 690, 190, 200, "L", 50, 8)
	Label(1010, 690, 190, 200, "L", 25, 8)
	Label(1210, 690, 180, 200, "L", 12, 4)
end
