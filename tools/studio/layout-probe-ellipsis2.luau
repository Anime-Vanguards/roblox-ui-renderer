-- Lone truncation ellipsis width (AtEnd, "WWWW..." in a box narrower than "W" + ellipsis) per size,
-- next to the plain "\u{2026}" TextBounds, for a few fonts.
return function(Target)
	local Results = {}
	local Labels = {}
	local Fonts = {
		SSP = Font.new("rbxasset://fonts/families/SourceSansPro.json"),
		MB = Font.new("rbxasset://fonts/families/Montserrat.json", Enum.FontWeight.Bold),
		Fre = Font.new("rbxasset://fonts/families/FredokaOne.json"),
	}
	for Key, Face in Fonts do
		for Size = 8, 60 do
			local Plain = Instance.new("TextLabel")
			Plain.Size = UDim2.fromOffset(500, 80)
			Plain.FontFace = Face
			Plain.TextSize = Size
			Plain.Text = "\u{2026}"
			Plain.Parent = Target
			local Truncated = Instance.new("TextLabel")
			Truncated.Size = UDim2.fromOffset(Size * 1.4, 80)
			Truncated.FontFace = Face
			Truncated.TextSize = Size
			Truncated.Text = "WWWWWWWWWWWWWW"
			Truncated.TextTruncate = Enum.TextTruncate.AtEnd
			Truncated.Parent = Target
			table.insert(Labels, { Key = `{Key}_{Size}`, Plain = Plain, Truncated = Truncated })
		end
	end
	task.wait()
	for _, Entry in Labels do
		Results[Entry.Key] = { Entry.Plain.TextBounds.X * 1.5, Entry.Truncated.TextBounds.X * 1.5, Entry.Truncated.AbsoluteSize.X * 1.5 }
	end
	return Results
end
