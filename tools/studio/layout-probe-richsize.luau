-- RichText size under UIScale (TextBounds.Y of one line) next to plain text, many sizes/scales.
return function(Target)
	local Results = {}
	local Labels = {}
	local Face = Font.new("rbxasset://fonts/families/FredokaOne.json")
	for _, Scale in { 0.885, 0.87, 0.9, 0.75, 1.1, 1.3, 0.5, 0.95 } do
		local Holder = Instance.new("Frame")
		Holder.Size = UDim2.fromOffset(1000, 1000)
		Holder.BackgroundTransparency = 1
		Holder.Parent = Target
		local UIScale = Instance.new("UIScale")
		UIScale.Scale = Scale
		UIScale.Parent = Holder
		for TextSize = 8, 60 do
			local Label = Instance.new("TextLabel")
			Label.Size = UDim2.fromOffset(400, 100)
			Label.FontFace = Face
			Label.TextSize = TextSize
			Label.RichText = true
			Label.Text = "Hi"
			Label.Parent = Holder
			table.insert(Labels, { Key = `{Scale}_{TextSize}`, Label = Label })
		end
	end
	task.wait()
	for _, Entry in Labels do
		Results[Entry.Key] = { Entry.Label.TextBounds.Y * 1.5 }
	end
	return Results
end
