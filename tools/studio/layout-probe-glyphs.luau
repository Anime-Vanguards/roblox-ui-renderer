-- Prefix widths of strings (TextBounds of every prefix) to locate per-glyph advance mismatches.
return function(Target)
	local Cases = {
		{ "SSB18", Font.new("rbxasset://fonts/families/SourceSansPro.json", Enum.FontWeight.Bold), 18, "SourceSansBold Hamburgefonstiv 0123" },
		{ "Cre18", Font.new("rbxasset://fonts/families/Creepster.json"), 18, "Creepster Hamburgefonstiv 0123" },
		{ "Cre24", Font.new("rbxasset://fonts/families/Creepster.json"), 24, "The quick brown fox" },
		{ "SS14", Font.new("rbxasset://fonts/families/SourceSansPro.json"), 14, "Supercalifragilisticexpialidocious word" },
		{ "SS14b", Font.new("rbxasset://fonts/families/SourceSansPro.json"), 14, "one-two-three four-five" },
	}
	local Labels = {}
	for _, Case in Cases do
		local Text = Case[4]
		for Length = 1, #Text do
			local Label = Instance.new("TextLabel")
			Label.Size = UDim2.fromOffset(1000, 40)
			Label.FontFace = Case[2]
			Label.TextSize = Case[3]
			Label.Text = string.sub(Text, 1, Length) .. "|"
			Label.Parent = Target
			table.insert(Labels, { Key = `{Case[1]}_{Length}`, Label = Label })
		end
	end
	task.wait()
	local Results = {}
	for _, Entry in Labels do
		Results[Entry.Key] = { Entry.Label.TextBounds.X }
	end
	return Results
end
