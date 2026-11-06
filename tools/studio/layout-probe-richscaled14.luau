-- Rich TextScaled (round 4): a UI kit's button labels (FredokaOne, TextSize 24 = a 36 px start that is too
-- wide for the box) under UIScale 0.885 and at UIScale 1, box widths swept in 0.05 local px (scale sizes in a 1000 px frame: offsets are whole numbers).
return function(Target)
	local Results = {}
	local Labels = {}
	local Face = Font.new("rbxasset://fonts/families/FredokaOne.json")
	local Holders = {}
	for Index, Scale in { 0.885, 1 } do
		local Holder = Instance.new("Frame")
		Holder.Size = UDim2.fromOffset(1000, 1000)
		Holder.BackgroundTransparency = 1
		Holder.Parent = Target
		if Scale ~= 1 then
			local UIScale = Instance.new("UIScale")
			UIScale.Scale = Scale
			UIScale.Parent = Holder
		end
		Holders[Index] = Holder
	end
	local Cases = {
		{ "Disable samples", 130, 24.678, 24 },
		{ "Disable samples", 130, 24.678, 8 },
		{ "Secondary", 150, 38.192, 24 },
		{ "Primary", 150, 38.192, 24 },
	}
	for HolderIndex, Holder in Holders do
		for Index, Case in Cases do
			for Step = 0, 399 do
				local W = Case[2] + Step / 20
				local Label = Instance.new("TextLabel")
				Label.Size = UDim2.new(W / 1000, 0, Case[3] / 1000, 0)
				Label.FontFace = Face
				Label.RichText = true
				Label.Text = Case[1]
				Label.TextSize = Case[4]
				Label.TextScaled = true
				Label.Parent = Holder
				table.insert(Labels, { Key = `{HolderIndex}_{Index}_{Step}`, Label = Label })
			end
		end
	end
	task.wait()
	for _, Entry in Labels do
		Results[Entry.Key] = { Entry.Label.TextBounds.X * 1.5, Entry.Label.TextBounds.Y * 1.5, Entry.Label.AbsoluteSize.X * 1.5, Entry.Label.AbsoluteSize.Y * 1.5 }
	end
	return Results
end
