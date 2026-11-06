-- Rich TextScaled (round 4): the start size. "Hi" in a 60.5 px tall box swept over widths (1 px) at
-- TextSizes that fit the box (10 .. 30) and that do not (45 .. 100), and "Cyan" likewise.
return function(Target)
	local Results = {}
	local Labels = {}
	local Face = Font.new("rbxasset://fonts/families/FredokaOne.json")
	local Holder = Instance.new("Frame")
	Holder.Size = UDim2.fromOffset(1000, 1000)
	Holder.BackgroundTransparency = 1
	Holder.Parent = Target
	local Cases = {}
	for _, TextSize in { 10, 14, 20, 30, 41, 45, 60, 100 } do
		table.insert(Cases, { "Hi", 60.5, TextSize })
	end
	for _, TextSize in { 14, 45 } do
		table.insert(Cases, { "Cyan", 60.5, TextSize })
	end
	for Index, Case in Cases do
		for Step = 0, 1399 do
			local Px = 100 + Step
			local Label = Instance.new("TextLabel")
			Label.Size = UDim2.new(Px / 1.5 / 1000, 0, Case[2] / 1.5 / 1000, 0)
			Label.FontFace = Face
			Label.RichText = true
			Label.Text = Case[1]
			Label.TextSize = Case[3]
			Label.TextScaled = true
			Label.Parent = Holder
			table.insert(Labels, { Key = `{Index}_{Step}`, Label = Label })
		end
	end
	task.wait()
	for _, Entry in Labels do
		Results[Entry.Key] = { Entry.Label.TextBounds.X * 1.5, Entry.Label.TextBounds.Y * 1.5, Entry.Label.AbsoluteSize.X * 1.5, Entry.Label.AbsoluteSize.Y * 1.5 }
	end
	return Results
end
