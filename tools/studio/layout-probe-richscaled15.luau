-- Rich TextScaled (round 4): UITextSizeConstraint. "Hi" in a 60.5 px tall box swept over widths
-- (1 px, the dip zone of TextSize 8) with (TextSize, MinTextSize, MaxTextSize) combinations.
return function(Target)
	local Results = {}
	local Labels = {}
	local Face = Font.new("rbxasset://fonts/families/FredokaOne.json")
	local Holder = Instance.new("Frame")
	Holder.Size = UDim2.fromOffset(1000, 1000)
	Holder.BackgroundTransparency = 1
	Holder.Parent = Target
	local Cases = {
		{ 8, 1, 100 }, { 8, 8, 100 }, { 14, 8, 100 }, { 8, 12, 100 }, { 8, 20, 40 },
		{ 8, 39, 40 }, { 8, 1, 30 }, { 14, 1, 38 }, { 60, 1, 100 }, { 60, 1, 45 }, { 8, 1, 45 },
	}
	for Index, Case in Cases do
		for Step = 0, 319 do
			local Px = 300 + Step
			local Label = Instance.new("TextLabel")
			Label.Size = UDim2.new(Px / 1.5 / 1000, 0, 60.5 / 1.5 / 1000, 0)
			Label.FontFace = Face
			Label.RichText = true
			Label.Text = "Hi"
			Label.TextSize = Case[1]
			Label.TextScaled = true
			local Constraint = Instance.new("UITextSizeConstraint")
			Constraint.MinTextSize = Case[2]
			Constraint.MaxTextSize = Case[3]
			Constraint.Parent = Label
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
