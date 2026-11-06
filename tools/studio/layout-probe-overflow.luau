-- TextBounds of unwrapped text wider than its box: vary the box width around the natural width.
return function(Target)
	local Results = {}
	local Labels = {}
	local Cases = {
		{ "Heavy", Font.new("rbxasset://fonts/families/GothamSSm.json", Enum.FontWeight.Heavy), "GothamBlack Hamburgefonstiv 0123", 18 },
		{ "Src", Font.new("rbxasset://fonts/families/SourceSansPro.json"), "Hello World wide text here", 20 },
		{ "Src1", Font.new("rbxasset://fonts/families/SourceSansPro.json"), "WWWWWWWWWW", 20 },
	}
	for _, Case in Cases do
		for Width = 0, 320, 1 do
			local Label = Instance.new("TextLabel")
			Label.Size = UDim2.fromOffset(Width, 30)
			Label.FontFace = Case[2]
			Label.TextSize = Case[4]
			Label.Text = Case[3]
			Label.Parent = Target
			table.insert(Labels, { Key = `{Case[1]}_{Width}`, Label = Label })
		end
	end
	task.wait()
	for _, Entry in Labels do
		Results[Entry.Key] = { Entry.Label.TextBounds.X, Entry.Label.TextBounds.Y }
	end
	return Results
end
