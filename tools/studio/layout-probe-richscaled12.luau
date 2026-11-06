-- Rich TextScaled (round 4): boxes where the first estimate fits (so the search grows past it), at
-- TextSize 1 .. 40, to see what the growth step depends on.
return function(Target)
	local Results = {}
	local Labels = {}
	local Face = Font.new("rbxasset://fonts/families/FredokaOne.json")
	local Holder = Instance.new("Frame")
	Holder.Size = UDim2.fromOffset(1000, 1000)
	Holder.BackgroundTransparency = 1
	Holder.Parent = Target
	local Cases = {
		{ "Cyan", 125.0, 61.5 }, { "Cyan", 125.1, 61.5 }, { "Cyan", 122.3, 61.5 }, { "Cyan", 92.5, 45.25 },
		{ "Cyan", 115.0, 56.3 }, { "Review &amp; craft", 152.8, 225 }, { "Review &amp; craft", 147.3, 225 },
		{ "Secondary", 130.0, 30.5 }, { "Cyan", 250.0, 123.0 }, { "Cyan", 62.5, 30.75 },
	}
	for Index, Case in Cases do
		for TextSize = 1, 40 do
			local Label = Instance.new("TextLabel")
			Label.Size = UDim2.new(Case[2] / 1.5 / 1000, 0, Case[3] / 1.5 / 1000, 0)
			Label.FontFace = Face
			Label.RichText = true
			Label.Text = Case[1]
			Label.TextSize = TextSize
			Label.TextScaled = true
			Label.Parent = Holder
			table.insert(Labels, { Key = `{Index}_{TextSize}`, Label = Label })
		end
	end
	task.wait()
	for _, Entry in Labels do
		Results[Entry.Key] = { Entry.Label.TextBounds.X * 1.5, Entry.Label.TextBounds.Y * 1.5, Entry.Label.AbsoluteSize.X * 1.5, Entry.Label.AbsoluteSize.Y * 1.5 }
	end
	return Results
end
