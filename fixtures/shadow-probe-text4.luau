-- UIShadow probe: how strong a Mode = Text glow is once its width passes what the glyph's distance field holds.
-- A full block glyph, clear, with a red shadow on black (the red channel is the shadow's alpha): rows 1 and 2 at
-- TextSize 40 by BlurRadius, row 3 at TextSize 80.
-- compare --viewport 1400x560 --time 0.3 --background 0,0,0
return function(Target)
	local function Label(X, Y, W, H, Size, Blur)
		local L = Instance.new("TextLabel")
		L.Position = UDim2.fromOffset(X, Y)
		L.Size = UDim2.fromOffset(W, H)
		L.BackgroundTransparency = 1
		L.BorderSizePixel = 0
		L.Text = "\u{2588}"
		L.TextColor3 = Color3.new(0, 0, 1)
		L.TextTransparency = 1
		L.FontFace = Font.fromEnum(Enum.Font.SourceSans)
		L.TextSize = Size
		local S = Instance.new("UIShadow")
		S.Mode = Enum.ApplyShadowMode.Text
		S.Color = Color3.new(1, 0, 0)
		S.BlurRadius = UDim.new(0, Blur)
		S.Parent = L
		L.Parent = Target
	end
	for Index, Blur in { 6, 8, 10, 11, 12, 13, 14, 16, 18, 20, 24, 28, 32, 40, 56, 80 } do
		Label(10 + ((Index - 1) % 8) * 172, 10 + ((Index - 1) // 8) * 160, 164, 150, 40, Blur)
	end
	for Index, Blur in { 12, 16, 20, 24, 28, 32, 48, 64 } do
		Label(10 + (Index - 1) * 172, 330, 164, 220, 80, Blur)
	end
end
