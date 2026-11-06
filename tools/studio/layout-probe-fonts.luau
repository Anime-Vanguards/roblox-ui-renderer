-- TextBounds of candidate families for Enum.Font mappings (Arial.json, GothamSSm Heavy, ...).
return function(Target)
	local Specs = {
		{ "Arial", "rbxasset://fonts/families/Arial.json", "Regular" },
		{ "ArialBold", "rbxasset://fonts/families/Arial.json", "Bold" },
		{ "Arimo", "rbxasset://fonts/families/Arimo.json", "Regular" },
		{ "ArimoBold", "rbxasset://fonts/families/Arimo.json", "Bold" },
		{ "LegacyArimo", "rbxasset://fonts/families/LegacyArimo.json", "Regular" },
		{ "GothamHeavy", "rbxasset://fonts/families/GothamSSm.json", "Heavy" },
		{ "GothamBold", "rbxasset://fonts/families/GothamSSm.json", "Bold" },
		{ "GothamExtraBold", "rbxasset://fonts/families/GothamSSm.json", "ExtraBold" },
		{ "MontserratBlack", "rbxasset://fonts/families/Montserrat.json", "Heavy" },
		{ "MontserratExtraBold", "rbxasset://fonts/families/Montserrat.json", "ExtraBold" },
		{ "SourceSansBold", "rbxasset://fonts/families/SourceSansPro.json", "Bold" },
		{ "Creepster", "rbxasset://fonts/families/Creepster.json", "Regular" },
	}
	local Strings = { "Arial Hamburgefonstiv 0123", "GothamBlack Hamburgefonstiv 0123", "Hello World", "WAVE", "i", "The quick brown fox", "AV To", "0123456789" }
	local Sizes = { 14, 18, 24 }

	local TextService = game:GetService("TextService")
	for _, Spec in Specs do
		local Params = Instance.new("GetTextBoundsParams")
		Params.Text = "Warm up"
		Params.Font = Font.new(Spec[2], Enum.FontWeight[Spec[3]])
		Params.Size = 20
		pcall(TextService.GetTextBoundsAsync, TextService, Params)
	end
	task.wait(0.3)

	local Labels = {}
	for _, Spec in Specs do
		for _, Size in Sizes do
			for Index, Text in Strings do
				local Label = Instance.new("TextLabel")
				Label.Size = UDim2.fromOffset(1000, 60)
				Label.FontFace = Font.new(Spec[2], Enum.FontWeight[Spec[3]])
				Label.TextSize = Size
				Label.Text = Text
				Label.Parent = Target
				table.insert(Labels, { Key = `{Spec[1]}_{Size}_{Index}`, Label = Label })
			end
		end
	end
	task.wait()
	local Results = {}
	for _, Entry in Labels do
		Results[Entry.Key] = { Entry.Label.TextBounds.X, Entry.Label.TextBounds.Y }
	end
	return Results
end
