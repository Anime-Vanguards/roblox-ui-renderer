-- Rich TextScaled (round 7): a story's button labels (FredokaOne, RichText, TextScaled,
-- TextWrapped, a 60 x 20.8 local box under UIScale 1.062), TextSize 10 .. 40, and the same boxes under
-- UIScale 0.885 and 1.
return function(Target)
	local Results = {}
	local Labels = {}
	local Face = Font.new("rbxasset://fonts/families/FredokaOne.json")
	local Texts = { "Locked", "\u{A5}15000", "\u{A5}20000", "\u{A5}10000", "\u{A5}5000", "Equipped", "Mystery", "Interact" }
	for ScaleIndex, Scale in { 1.062, 0.885, 1 } do
		local Holder = Instance.new("Frame")
		Holder.Size = UDim2.fromOffset(1000, 1000)
		Holder.BackgroundTransparency = 1
		Holder.Parent = Target
		if Scale ~= 1 then
			local U = Instance.new("UIScale")
			U.Scale = Scale
			U.Parent = Holder
		end
		for TextIndex, Text in Texts do
			for TextSize = 10, 40 do
				local Label = Instance.new("TextLabel")
				Label.Size = UDim2.new(60 / 1000, 0, 20.8 / 1000, 0)
				Label.FontFace = Face
				Label.RichText = true
				Label.Text = Text
				Label.TextSize = TextSize
				Label.TextScaled = true
				Label.TextWrapped = true
				Label.Parent = Holder
				table.insert(Labels, { Key = `{ScaleIndex}_{TextIndex}_{TextSize}`, Label = Label })
			end
		end
	end
	task.wait()
	for _, Entry in Labels do
		Results[Entry.Key] = { Entry.Label.TextBounds.X * 1.5, Entry.Label.TextBounds.Y * 1.5 }
	end
	return Results
end
