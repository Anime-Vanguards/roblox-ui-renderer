-- Rich TextScaled (round 4): the pick as a function of the start TextSize. "Hi" in 60.5 px tall boxes
-- of four widths at TextSize 1 .. 100 and at fractional TextSizes 8 .. 11.
return function(Target)
	local Results = {}
	local Labels = {}
	local Face = Font.new("rbxasset://fonts/families/FredokaOne.json")
	local Holder = Instance.new("Frame")
	Holder.Size = UDim2.fromOffset(1000, 1000)
	Holder.BackgroundTransparency = 1
	Holder.Parent = Target
	for Index, Px in { 300, 350, 450, 600, 900 } do
		for Step = 1, 100 do
			local Label = Instance.new("TextLabel")
			Label.Size = UDim2.new(Px / 1.5 / 1000, 0, 60.5 / 1.5 / 1000, 0)
			Label.FontFace = Face
			Label.RichText = true
			Label.Text = "Hi"
			Label.TextSize = Step
			Label.TextScaled = true
			Label.Parent = Holder
			table.insert(Labels, { Key = `i{Index}_{Step}`, Label = Label })
		end
		for Step = 0, 30 do
			local Label = Instance.new("TextLabel")
			Label.Size = UDim2.new(Px / 1.5 / 1000, 0, 60.5 / 1.5 / 1000, 0)
			Label.FontFace = Face
			Label.RichText = true
			Label.Text = "Hi"
			Label.TextSize = 8 + Step / 10
			Label.TextScaled = true
			Label.Parent = Holder
			table.insert(Labels, { Key = `f{Index}_{Step}`, Label = Label })
		end
	end
	task.wait()
	for _, Entry in Labels do
		Results[Entry.Key] = { Entry.Label.TextBounds.X * 1.5, Entry.Label.TextBounds.Y * 1.5, Entry.Label.AbsoluteSize.X * 1.5, Entry.Label.AbsoluteSize.Y * 1.5 }
	end
	return Results
end
