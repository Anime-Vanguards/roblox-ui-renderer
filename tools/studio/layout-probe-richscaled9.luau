-- Rich TextScaled (round 4): boxes shaped like their text, where the first size estimate (the start
-- layout scaled by sqrt(box area / text area)) itself fits. "Cyan" and "Secondary" swept over box
-- widths (0.1 px) at three box heights; TextSize 8 (Instance.new default) and 9 (13.5 px start).
return function(Target)
	local Results = {}
	local Labels = {}
	local Face = Font.new("rbxasset://fonts/families/FredokaOne.json")
	local Holder = Instance.new("Frame")
	Holder.Size = UDim2.fromOffset(1000, 1000)
	Holder.BackgroundTransparency = 1
	Holder.Parent = Target
	local Cases = {
		{ "Cyan", 30.5, 55, 20, 8 },
		{ "Cyan", 45.25, 85, 25, 8 },
		{ "Cyan", 61.5, 115, 30, 8 },
		{ "Secondary", 30.5, 120, 40, 8 },
		{ "Secondary", 45.25, 185, 50, 8 },
		{ "Cyan", 45.25, 85, 25, 9 },
	}
	for Index, Case in Cases do
		for Step = 0, Case[4] * 10 - 1 do
			local Px = Case[3] + Step / 10
			local Label = Instance.new("TextLabel")
			Label.Size = UDim2.new(Px / 1.5 / 1000, 0, Case[2] / 1.5 / 1000, 0)
			Label.FontFace = Face
			Label.RichText = true
			Label.Text = Case[1]
			Label.TextSize = Case[5]
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
