-- Text in CanvasGroups turned 180 degrees, every TextSize 14..29 ("Hg", left-aligned and centred labels), against the
-- same labels in unturned groups and turned 180 degrees on their own inside plain Frames. Whole device pixels and
-- even device sizes everywhere, so a half turn maps pixels to pixels and the capture shows the group's texture
-- unresampled. Whole-pixel quads (TextSize 16, 20, 24, 28 at 1.5x) on a fractional line top or pen are resampled
-- there as rotated text is (notes/text.md "Text in a turned CanvasGroup").
-- rur compare --fixture fixtures/text-probe-groupturned-even.luau --viewport 1600x900 --time 0.2 --background 0,0,0
return function(Target)
	local function Fill(Parent, From, To, Turn)
		local Y = 4
		for Size = From, To do
			local L = Instance.new("TextLabel")
			L.Name = string.format("S%d", Size)
			L.BackgroundTransparency = 1
			L.TextColor3 = Color3.fromRGB(255, 255, 255)
			L.Text = "Hg"
			L.TextSize = Size
			L.TextXAlignment = Enum.TextXAlignment.Left
			L.Position = UDim2.fromOffset(8, Y)
			L.Size = UDim2.fromOffset(80, Size + 4)
			L.Rotation = Turn
			L.Parent = Parent
			local R = L:Clone()
			R.Name = string.format("C%d", Size)
			R.TextXAlignment = Enum.TextXAlignment.Center
			R.Position = UDim2.fromOffset(100, Y)
			R.Size = UDim2.fromOffset(Size * 2 + 1, Size + 3)
			R.Parent = Parent
			Y += Size + 6
		end
	end
	local Column = 0
	for _, Spec in { { "CanvasGroup", 180, 0 }, { "CanvasGroup", 0, 0 }, { "Frame", 0, 180 } } do
		for _, Range in { { 14, 21 }, { 22, 29 } } do
			local G = Instance.new(Spec[1])
			G.Name = string.format("%s%d_%d_%d", Spec[1], Spec[2], Spec[3], Range[1])
			G.Size = UDim2.fromOffset(228, 256)
			G.Position = UDim2.fromOffset(4 + Column * 240, 4)
			G.Rotation = Spec[2]
			G.BackgroundColor3 = Color3.fromRGB(30, 60, 110)
			G.BorderSizePixel = 0
			G.Parent = Target
			Fill(G, Range[1], Range[2], Spec[3])
			Column += 1
		end
	end
end
