-- Tied quarter-pixel pens of 25 px glyphs (device scale 1.5): Studio scales a glyph's position back to
-- device pixels by the reciprocal of its logical size over its pixel size, which is one single-precision
-- step under 1.5 at 25 px and exactly 1.5 at every other size drawn at quarter pixels, so 25 px ties fall
-- down far more often (notes/text.md "Tied pens of 25 px glyphs"). Plain TextSize 17 (25.5 px, drawn at 25)
-- in three faces, left, right and centred at many origins; the same under a UIScale (TextSize 19 at 0.9);
-- rich lines with a 25 px run inside 30 px text and a 30 px run inside 25 px text.
-- rur compare --fixture fixtures/text-probe-tie-size.luau --viewport 1500x700 --time 0.2 --background 0,0,0
return function(Target)
	local Faces = {
		Font.new("rbxasset://fonts/families/FredokaOne.json"),
		Font.new("rbxasset://fonts/families/SourceSansPro.json"),
		Font.new("rbxasset://fonts/families/GothamSSm.json", Enum.FontWeight.Bold),
	}
	local Texts = { "Review craft", "Dark neutral", "GHIJKLMN", "yzABCDEF", "WXYZ0123", "aaaa bbbb", "ijklmnop", "Equip 45%" }
	local Alignments = { Enum.TextXAlignment.Left, Enum.TextXAlignment.Right, Enum.TextXAlignment.Center }
	local Count = 0
	local function Add(Name, Text, Face, Size, Alignment, Width, Rich, Scale)
		local Index = Count
		Count += 1
		local Holder = Instance.new("Frame")
		Holder.Name = Name
		Holder.BackgroundTransparency = 1
		Holder.Size = UDim2.fromOffset(Width, 26)
		Holder.Position = UDim2.fromOffset(4 + (Index % 10) * 149 + (Index // 10 % 3) / 1.5, 4 + (Index // 10) * 29)
		local Label = Instance.new("TextLabel")
		Label.Name = "Label"
		Label.BackgroundTransparency = 1
		Label.TextColor3 = Color3.new(1, 1, 1)
		Label.FontFace = Face
		Label.TextSize = Size
		Label.RichText = Rich
		Label.Text = Text
		Label.TextXAlignment = Alignment
		Label.Size = UDim2.fromScale(1, 1)
		Label.Parent = Holder
		if Scale then
			local UIScale = Instance.new("UIScale")
			UIScale.Scale = Scale
			UIScale.Parent = Holder
		end
		Holder.Parent = Target
	end
	-- plain 25 px text: 3 faces x 8 texts x 3 alignments
	for FaceIndex, Face in Faces do
		for TextIndex, Text in Texts do
			for AlignIndex, Alignment in Alignments do
				Add(`Plain{FaceIndex}_{TextIndex}_{AlignIndex}`, Text, Face, 17, Alignment, 120 + (Count * 7) % 23, false)
			end
		end
	end
	-- the same size under a UIScale
	for TextIndex, Text in Texts do
		for AlignIndex, Alignment in Alignments do
			Add(`Scaled{TextIndex}_{AlignIndex}`, Text, Faces[1], 19, Alignment, 120 + (Count * 7) % 23, false, 0.9)
		end
	end
	-- rich text: a 25 px run in 30 px text, a 30 px run in 25 px text, and all of it 25 px
	for TextIndex, Text in Texts do
		local Head, Tail = string.sub(Text, 1, 4), string.sub(Text, 5)
		for AlignIndex, Alignment in Alignments do
			Add(`RunSmall{TextIndex}_{AlignIndex}`, `{Head}<font size="17">{Tail}</font>`, Faces[1], 20, Alignment, 130 + (Count * 7) % 17, true)
			Add(`RunLarge{TextIndex}_{AlignIndex}`, `{Head}<font size="20">{Tail}</font>`, Faces[1], 17, Alignment, 130 + (Count * 7) % 17, true)
			Add(`Rich{TextIndex}_{AlignIndex}`, `<font color="#ffffff">{Text}</font>`, Faces[2], 17, Alignment, 120 + (Count * 7) % 23, true)
		end
	end
end
