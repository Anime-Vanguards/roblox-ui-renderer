-- Rich TextScaled (round 7): wrapped labels whose TextSize 24 start does not fit, at UIScale 1 (every
-- whole unscaled size has its own pixel size, so TextBounds shows the N the search lands on): single words
-- and two-word texts, box widths 40 .. 90 and heights 16 .. 58 local px, to see which start layout feeds the
-- size estimate when the wrapped start breaks a word.
return function(Target)
	local Results = {}
	local Labels = {}
	local Face = Font.new("rbxasset://fonts/families/FredokaOne.json")
	local Texts = { "Mystery", "Interact", "Purple", "Secondary", "Hide Prompt", "Dark neutral", "Waxo 42" }
	local Holder = Instance.new("Frame")
	Holder.Size = UDim2.fromOffset(1000, 1000)
	Holder.BackgroundTransparency = 1
	Holder.Parent = Target
	for TextIndex, Text in Texts do
		for WidthStep = 0, 20 do
			for HeightStep = 0, 14 do
				local W = 40 + WidthStep * 2.5
				local H = 16 + HeightStep * 3
				local Label = Instance.new("TextLabel")
				Label.Size = UDim2.new(W / 1000, 0, H / 1000, 0)
				Label.FontFace = Face
				Label.RichText = true
				Label.Text = Text
				Label.TextSize = 24
				Label.TextScaled = true
				Label.TextWrapped = true
				Label.Parent = Holder
				table.insert(Labels, { Key = `{TextIndex}_{WidthStep}_{HeightStep}`, Label = Label })
				local Plain = Instance.new("TextLabel")
				Plain.Size = UDim2.new(W / 1000, 0, H / 1000, 0)
				Plain.FontFace = Face
				Plain.RichText = true
				Plain.Text = Text
				Plain.TextSize = 24
				Plain.TextWrapped = true
				Plain.Parent = Holder
				table.insert(Labels, { Key = `{TextIndex}_{WidthStep}_{HeightStep}_n`, Label = Plain })
			end
		end
	end
	task.wait()
	for _, Entry in Labels do
		Results[Entry.Key] = { Entry.Label.TextBounds.X * 1.5, Entry.Label.TextBounds.Y * 1.5 }
	end
	return Results
end
