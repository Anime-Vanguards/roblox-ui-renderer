-- Fallback glyph widths (a label "1.5%  →  3%" in FredokaOne 36: Studio 233 px wide, offline
-- 255 with the arrow from Arimo): the arrow alone and in context, in FredokaOne and in the fonts that
-- could supply it, plus other symbols FredokaOne lacks; bold / italic / light faces, an emoji.
return function(Target)
	local Results = {}
	local Labels = {}
	local Faces = {
		{ "Fre", Font.new("rbxasset://fonts/families/FredokaOne.json") },
		{ "Arimo", Font.new("rbxasset://fonts/families/Arimo.json") },
		{ "SSP", Font.new("rbxasset://fonts/families/SourceSansPro.json") },
		{ "Gotham", Font.new("rbxasset://fonts/families/GothamSSm.json") },
		{ "Noto", Font.new("rbxasset://fonts/families/NotoSans.json") },
		{ "Roboto", Font.new("rbxasset://fonts/families/Roboto.json") },
		{ "GothamBold", Font.new("rbxasset://fonts/families/GothamSSm.json", Enum.FontWeight.Bold) },
		{ "FreBold", Font.new("rbxasset://fonts/families/FredokaOne.json", Enum.FontWeight.Bold) },
		{ "RobotoItalic", Font.new("rbxasset://fonts/families/Roboto.json", Enum.FontWeight.Regular, Enum.FontStyle.Italic) },
		{ "RobotoLight", Font.new("rbxasset://fonts/families/Roboto.json", Enum.FontWeight.Light) },
	}
	local Texts = { "→", "1.5%  →  3%", "1.5%    3%", "←↑↓", "★", "✓", "•", "×", "…", "€", "™", "→→→→", "😀", "♥", "⚔", "½" }
	local Count = 0
	for _, Pair in Faces do
		for TextIndex, Text in Texts do
			for _, Size in { 36, 20 } do
				Count += 1
				local L = Instance.new("TextLabel")
				L.AutomaticSize = Enum.AutomaticSize.XY
				L.Size = UDim2.new()
				L.Position = UDim2.fromOffset(((Count - 1) % 6) * 200, ((Count - 1) // 6) * 30)
				L.FontFace = Pair[2]
				L.TextSize = Size
				L.Text = Text
				L.Parent = Target
				table.insert(Labels, { Key = `{Pair[1]}_{TextIndex}_{Size}`, Label = L })
			end
		end
	end
	task.wait()
	for _, Entry in Labels do
		Results[Entry.Key] = { Entry.Label.TextBounds.X * 1.5, Entry.Label.TextBounds.Y * 1.5 }
	end
	return Results
end
