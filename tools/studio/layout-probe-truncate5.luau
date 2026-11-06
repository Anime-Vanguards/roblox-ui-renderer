-- TextTruncate AtEnd with a first word narrower than the ellipsis (layout-probe-truncate4): at which box
-- widths does Studio switch from "A…" to cutting into the next word? Fine box steps, many first words.
return function(Target)
	local Results = {}
	local Labels = {}
	local Face = Font.new("rbxasset://fonts/families/SourceSansPro.json")
	local Strings = { "A somewhat longer", "W some words", "I am here now", "x yyyyyyyyyyyy", "ii some words", "V someone", "T something", "O sometimes", "j somewhere", "I something", "Ab somewhat", "A b c d e f g" }
	local Count = 0
	for Index, Text in Strings do
		for Width = 14, 70 do
			Count += 1
			local Label = Instance.new("TextLabel")
			Label.Position = UDim2.fromOffset(((Count - 1) % 8) * 150, ((Count - 1) // 8) * 10)
			Label.Size = UDim2.fromOffset(Width, 20)
			Label.FontFace = Face
			Label.TextSize = 16
			Label.Text = Text
			Label.TextTruncate = Enum.TextTruncate.AtEnd
			Label.Parent = Target
			table.insert(Labels, { Key = `{Index}_{Width}`, Label = Label })
		end
	end
	task.wait()
	for _, Entry in Labels do
		Results[Entry.Key] = { Entry.Label.TextBounds.X * 1.5 }
	end
	return Results
end
