-- Legacy `kern` table pairs in Permanent Marker (HarfBuzz splits a pair's value: half on the first glyph's advance,
-- the rest on the second glyph's advance and offset). Pairs of -92 ("DT", "dt", "DI"), -82 ("LT") and -20 ("ab")
-- units at every even TextSize from 10 to 64, so the second glyph's offset runs from a fraction of a pixel to over
-- two: where Studio draws the second glyph against its pen in each text mode.
-- rur compare --fixture fixtures/text-probe-kernoffset.luau --viewport 1600x980 --time 0.2 --background 0,0,0
return function(Target)
	local Index = 0
	for Size = 10, 64, 2 do
		local L = Instance.new("TextLabel")
		L.Name = string.format("K%d", Size)
		L.BackgroundTransparency = 1
		L.TextColor3 = Color3.fromRGB(255, 255, 255)
		L.FontFace = Font.new("rbxasset://fonts/families/PermanentMarker.json")
		L.Text = "DT dt LT ab DI"
		L.TextSize = Size
		L.TextXAlignment = Enum.TextXAlignment.Left
		L.TextYAlignment = Enum.TextYAlignment.Top
		L.Position = UDim2.fromOffset(6 + (Index % 4) * 396, 6 + (Index // 4) * 106)
		L.Size = UDim2.fromOffset(390, 100)
		L.Parent = Target
		Index += 1
	end
end
