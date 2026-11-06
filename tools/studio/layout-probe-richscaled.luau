-- Rich vs plain text: TextBounds for fixed sizes and TextScaled (height-limited) under UIScale.
return function(Target)
	local Results = {}
	local Labels = {}
	local Face = Font.new("rbxasset://fonts/families/FredokaOne.json")
	for _, Scale in { 1, 0.885 } do
		local Holder = Instance.new("Frame")
		Holder.Size = UDim2.fromOffset(1000, 1000)
		Holder.BackgroundTransparency = 1
		Holder.Parent = Target
		if Scale ~= 1 then
			local UIScale = Instance.new("UIScale")
			UIScale.Scale = Scale
			UIScale.Parent = Holder
		end
		for _, Rich in { false, true } do
			for TextSize = 14, 40, 2 do
				local Label = Instance.new("TextLabel")
				Label.Size = UDim2.fromOffset(400, 100)
				Label.FontFace = Face
				Label.TextSize = TextSize
				Label.RichText = Rich
				Label.Text = "Cyan"
				Label.Parent = Holder
				table.insert(Labels, { Key = `F{Scale}_{Rich}_{TextSize}`, Label = Label })
			end
			for Step = 0, 20 do
				local Label = Instance.new("TextLabel")
				Label.Size = UDim2.new(0, 240, (25 + Step * 0.4) / 1000, 0)
				Label.FontFace = Face
				Label.RichText = Rich
				Label.Text = "Cyan"
				Label.TextScaled = true
				Label.Parent = Holder
				table.insert(Labels, { Key = `S{Scale}_{Rich}_{Step}`, Label = Label })
			end
		end
	end
	task.wait()
	for _, Entry in Labels do
		Results[Entry.Key] = { Entry.Label.TextBounds.X * 1.5, Entry.Label.TextBounds.Y * 1.5, Entry.Label.AbsoluteSize.Y * 1.5 }
	end
	return Results
end
