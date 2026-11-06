-- Width of candidate ellipsis strings (plain text) vs truncated labels, several fonts and sizes.
return function(Target)
	local Results = {}
	local Labels = {}
	local Fonts = {
		SSP = Font.new("rbxasset://fonts/families/SourceSansPro.json"),
		GB = Font.new("rbxasset://fonts/families/GothamSSm.json", Enum.FontWeight.Bold),
		Fre = Font.new("rbxasset://fonts/families/FredokaOne.json"),
	}
	for Key, Face in Fonts do
		for _, Size in { 13, 20, 32 } do
			for Index, Text in { "...", "\u{2026}", "G...", "G\u{2026}", "a\u{2026}", "a...", "....", ". . ." } do
				local Label = Instance.new("TextLabel")
				Label.Size = UDim2.fromOffset(500, 50)
				Label.FontFace = Face
				Label.TextSize = Size
				Label.Text = Text
				Label.Parent = Target
				table.insert(Labels, { Key = `{Key}_{Size}_{Index}`, Label = Label })
			end
			-- lone truncated ellipsis: a long word in a box just wide enough for the ellipsis
			for Width = 4, 40, 2 do
				local Label = Instance.new("TextLabel")
				Label.Size = UDim2.fromOffset(Width, 50)
				Label.FontFace = Face
				Label.TextSize = Size
				Label.Text = "WWWWWWWWWWWWWW"
				Label.TextTruncate = Enum.TextTruncate.AtEnd
				Label.Parent = Target
				table.insert(Labels, { Key = `{Key}_{Size}_trunc{Width}`, Label = Label })
			end
		end
	end
	task.wait()
	for _, Entry in Labels do
		Results[Entry.Key] = { Entry.Label.TextBounds.X * 1.5 }
	end
	return Results
end
