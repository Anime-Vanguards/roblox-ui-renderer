-- UIShadow probe: the distance field behind Mode = Text, read back. A SourceSans Bold "L" at TextSize 100
-- under UIScale 6 (a 900 device px text size at 1.5x: 16 px a texel), clear, with a red shadow on black (the
-- red channel is the shadow's alpha) at BlurRadius 20, 50, 100 and 170, all within the field's reach.
-- compare --viewport 2200x780 --time 0.3 --background 0,0,0
return function(Target)
	local Bold = Font.new("rbxasset://fonts/families/SourceSansPro.json", Enum.FontWeight.Bold)
	for Index, Blur in { 20, 50, 100, 170 } do
		local L = Instance.new("TextLabel")
		L.Position = UDim2.fromOffset(10 + (Index - 1) * 545, 10)
		L.Size = UDim2.fromOffset(535 / 6, 760 / 6)
		L.BackgroundTransparency = 1
		L.BorderSizePixel = 0
		L.Text = "L"
		L.TextColor3 = Color3.new(0, 0, 1)
		L.TextTransparency = 1
		L.FontFace = Bold
		L.TextSize = 100
		local S = Instance.new("UIShadow")
		S.Mode = Enum.ApplyShadowMode.Text
		S.Color = Color3.new(1, 0, 0)
		S.BlurRadius = UDim.new(0, Blur)
		S.Parent = L
		local U = Instance.new("UIScale")
		U.Scale = 6
		U.Parent = L
		L.Parent = Target
	end
end
