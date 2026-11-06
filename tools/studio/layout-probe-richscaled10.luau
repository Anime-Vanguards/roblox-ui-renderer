-- Rich TextScaled (round 4): start sizes that do not fit the box. "Hi" in a 60.5 px tall box swept over
-- box widths (0.25 px) at TextSize 41 (61 px), 45, 60 and 100.
return function(Target)
	local Results = {}
	local Labels = {}
	local Face = Font.new("rbxasset://fonts/families/FredokaOne.json")
	local Holder = Instance.new("Frame")
	Holder.Size = UDim2.fromOffset(1000, 1000)
	Holder.BackgroundTransparency = 1
	Holder.Parent = Target
	local Cases = { { "Hi", 60.5, 41 }, { "Hi", 60.5, 45 }, { "Hi", 60.5, 60 }, { "Hi", 60.5, 100 } }
	for Index, Case in Cases do
		for Step = 0, 2799 do
			local Px = 100 + Step / 4
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
