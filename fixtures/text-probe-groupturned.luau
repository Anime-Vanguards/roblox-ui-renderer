-- Text drawn into a turned CanvasGroup's texture (no gradient): where the glyphs land in the texture before it
-- is drawn turned. Groups at angles 0, 2, 20, 45, 60 and -13, at whole and fractional positions, each holding
-- "AB" at TextSize 60, a line at 30 and one at 17 (an odd size: a scaled quad in whole-pixel text).
-- rur compare --fixture fixtures/text-probe-groupturned.luau --viewport 1200x900 --time 0.2 --background 0,0,0
return function(Target)
	local Angles = { 0, 2, 20, 45, 60, -13 }
	local Offsets = { 0, 0.37 }
	for Row, Offset in Offsets do
		for I, Angle in Angles do
			local G = Instance.new("CanvasGroup")
			G.Name = string.format("G%d_%d", Row, I)
			G.Size = UDim2.fromOffset(170, 190)
			G.AnchorPoint = Vector2.new(0.5, 0.5)
			G.Position = UDim2.fromOffset(105 + (I - 1) * 196 + Offset, 120 + (Row - 1) * 260 + Offset * 0.8)
			G.Rotation = Angle
			G.BackgroundColor3 = Color3.fromRGB(30, 60, 110)
			G.BorderSizePixel = 0
			G.Parent = Target
			for K, Spec in { { "AB", 60, 0 }, { "Mythic 42", 30, 80 }, { "Waxo gjpq 17", 17, 125 } } do
				local L = Instance.new("TextLabel")
				L.Name = string.format("L%d", K)
				L.BackgroundTransparency = 1
				L.TextColor3 = Color3.fromRGB(255, 255, 255)
				L.Text = Spec[1]
				L.TextSize = Spec[2]
				L.Position = UDim2.fromOffset(6, Spec[3] + 4)
				L.Size = UDim2.new(1, -12, 0, Spec[2] + 10)
				L.Parent = G
			end
		end
	end
end
