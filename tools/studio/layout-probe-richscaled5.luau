-- Rich TextScaled (round 3): fixed box heights swept over box widths (1 px steps), to see how the width
-- moves the height-limited picks ("Cyan" in a 60.5 px box picks 59 at 900 px wide, 60 elsewhere?).
return function(Target)
	local Results = {}
	local Labels = {}
	local Face = Font.new("rbxasset://fonts/families/FredokaOne.json")
	local Holder = Instance.new("Frame")
	Holder.Size = UDim2.fromOffset(1000, 1000)
	Holder.BackgroundTransparency = 1
	Holder.Parent = Target
	local Cases = { { "Cyan", 60.5 }, { "Cyan", 56.3 }, { "Cyan", 61.5 }, { "Hi", 60.5 }, { "Hi", 56.3 } }
	for Index, Case in Cases do
		for Step = 0, 1399 do
			local Px = 100 + Step
			local Label = Instance.new("TextLabel")
			Label.Size = UDim2.new(Px / 1.5 / 1000, 0, Case[2] / 1.5 / 1000, 0)
			Label.FontFace = Face
			Label.RichText = true
			Label.Text = Case[1]
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
