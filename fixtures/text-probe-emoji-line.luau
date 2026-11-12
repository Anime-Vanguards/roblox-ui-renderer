-- Which font sets the baseline of a line with glyphs of a fallback face (colour emoji): lines of emoji only,
-- emoji before and after letters, at several sizes, in three label faces, top, centre and bottom aligned,
-- wrapped over two lines, and rich text with an emoji run larger or smaller than the rest
-- (notes/text.md "Baseline of fallback glyphs").
-- rur compare --fixture fixtures/text-probe-emoji-line.luau --viewport 1320x700 --time 0.2 --background 20,20,26
return function(Target)
	local Faces = {
		Font.new("rbxasset://fonts/families/FredokaOne.json"),
		Font.new("rbxasset://fonts/families/SourceSansPro.json"),
		Font.new("rbxasset://fonts/families/GothamSSm.json", Enum.FontWeight.Bold),
	}
	local Emoji = "\u{1F3DC}\u{1F45F}\u{1F600}"
	local Count = 0
	local function Label(Name, Props)
		local L = Instance.new("TextLabel")
		L.Name = Name
		L.BorderSizePixel = 0
		L.BackgroundColor3 = Color3.fromRGB(40, 44, 56)
		L.TextColor3 = Color3.fromRGB(255, 255, 255)
		L.TextXAlignment = Enum.TextXAlignment.Left
		L.Size = UDim2.fromOffset(160, 54)
		L.Position = UDim2.fromOffset(2 + (Count % 8) * 164, 2 + (Count // 8) * 58)
		for Key, Value in Props do
			(L :: any)[Key] = Value
		end
		L.Parent = Target
		Count += 1
		return L
	end
	local Sizes = { 14, 20, 32, 41 }
	for FaceIndex, Face in Faces do
		for _, Size in Sizes do
			Label(`Only{FaceIndex}_{Size}`, { FontFace = Face, TextSize = Size, Text = Emoji })
			Label(`First{FaceIndex}_{Size}`, { FontFace = Face, TextSize = Size, Text = Emoji .. " Ag" })
		end
	end
	local Alignments = { Enum.TextYAlignment.Top, Enum.TextYAlignment.Center, Enum.TextYAlignment.Bottom }
	for FaceIndex, Face in Faces do
		for AlignIndex, Alignment in Alignments do
			Label(`Align{FaceIndex}_{AlignIndex}`, { FontFace = Face, TextSize = 25, Text = Emoji, TextYAlignment = Alignment })
		end
	end
	for FaceIndex, Face in Faces do
		Label(`Wrap{FaceIndex}`, { FontFace = Face, TextSize = 20, Text = "Ag yes " .. Emoji .. Emoji .. Emoji, TextWrapped = true })
		Label(`WrapFirst{FaceIndex}`, { FontFace = Face, TextSize = 20, Text = Emoji .. Emoji .. Emoji .. " Ag yes then", TextWrapped = true })
		Label(`RichLarge{FaceIndex}`, { FontFace = Face, TextSize = 18, RichText = true, Text = `Ag <font size="34">{Emoji}</font> g` })
		Label(`RichSmall{FaceIndex}`, { FontFace = Face, TextSize = 34, RichText = true, Text = `<font size="18">{Emoji}</font> Ag` })
		Label(`RichOnly{FaceIndex}`, { FontFace = Face, TextSize = 18, RichText = true, Text = `<font size="34">{Emoji}</font>` })
	end
end
