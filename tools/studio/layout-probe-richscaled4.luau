-- Rich TextScaled at UIScale 1: does the pick depend on the text or the box width? Fine box heights
-- across [N, N + 1) for several texts, fonts and widths.
return function(Target)
	local Results = {}
	local Labels = {}
	local Fredoka = Font.new("rbxasset://fonts/families/FredokaOne.json")
	local Source = Font.new("rbxasset://fonts/families/SourceSansPro.json")
	local Holder = Instance.new("Frame")
	Holder.Size = UDim2.fromOffset(1000, 1000)
	Holder.BackgroundTransparency = 1
	Holder.Parent = Target
	local Cases = {
		{ "Cyan", Fredoka, 600 },
		{ "Hi", Fredoka, 600 },
		{ "iiiiii", Fredoka, 600 },
		{ "Cyan", Fredoka, 300 },
		{ "Cyan", Source, 600 },
		{ "WWWW", Fredoka, 600 },
	}
	for N = 44, 70, 2 do
		for Step = 0, 19 do
			local Box = N + Step / 10
			for Index, Case in Cases do
				local Label = Instance.new("TextLabel")
				Label.Size = UDim2.new(0, Case[3], Box / 1.5 / 1000, 0)
				Label.FontFace = Case[2]
				Label.RichText = true
				Label.Text = Case[1]
				Label.TextScaled = true
				Label.Parent = Holder
				table.insert(Labels, { Key = `{N}_{Step}_{Index}`, Label = Label })
			end
		end
	end
	task.wait()
	for _, Entry in Labels do
		Results[Entry.Key] = { Entry.Label.TextBounds.X * 1.5, Entry.Label.TextBounds.Y * 1.5, Entry.Label.AbsoluteSize.Y * 1.5 }
	end
	return Results
end
