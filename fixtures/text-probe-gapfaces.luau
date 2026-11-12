-- Faces sized with an hhea line gap, by size (notes/text.md "Whole-pixel text", colour emoji): a TwemojiMozilla
-- emoji pair (the fallback face of SourceSans), Permanent Marker and Ubuntu at every even TextSize from 10 to 64,
-- one label each, to fit the scale Studio gives such a face at each size.
-- rur compare --fixture fixtures/text-probe-gapfaces.luau --viewport 1600x980 --time 0.2 --background 0,0,0
return function(Target)
	local Kinds = {
		{ "E", "rbxasset://fonts/families/SourceSansPro.json", "\u{1F600}\u{1F525}" },
		{ "M", "rbxasset://fonts/families/PermanentMarker.json", "abg Hx" },
		{ "U", "rbxasset://fonts/families/Ubuntu.json", "abg Hx" },
	}
	local Index = 0
	for _, Kind in Kinds do
		for Size = 10, 64, 2 do
			local L = Instance.new("TextLabel")
			L.Name = string.format("%s%d", Kind[1], Size)
			L.BackgroundTransparency = 1
			L.TextColor3 = Color3.fromRGB(255, 255, 255)
			L.FontFace = Font.new(Kind[2])
			L.Text = Kind[3]
			L.TextSize = Size
			L.TextXAlignment = Enum.TextXAlignment.Left
			L.TextYAlignment = Enum.TextYAlignment.Top
			L.Position = UDim2.fromOffset(6 + (Index % 9) * 176, 6 + (Index // 9) * 96)
			L.Size = UDim2.fromOffset(170, 90)
			L.Parent = Target
			Index += 1
		end
	end
end
