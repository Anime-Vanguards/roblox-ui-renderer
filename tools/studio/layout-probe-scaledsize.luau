-- TextScaled size granularity: height-limited labels with fractional box heights (via scale),
-- with and without UIScale, plus width-limited labels.
return function(Target)
	local Results = {}
	local Labels = {}
	local SSP = Font.new("rbxasset://fonts/families/SourceSansPro.json")
	local Count = 0
	for _, Scale in { 1, 0.885, 1.3 } do
		local Holder = Instance.new("Frame")
		Holder.Size = UDim2.fromOffset(1000, 1000)
		Holder.BackgroundTransparency = 1
		Holder.Parent = Target
		if Scale ~= 1 then
			local UIScale = Instance.new("UIScale")
			UIScale.Scale = Scale
			UIScale.Parent = Holder
		end
		for Step = 0, 40 do
			local Height = 20 + Step * 0.35
			Count += 1
			local Label = Instance.new("TextLabel")
			Label.Size = UDim2.new(0, 300, Height / 1000, 0)
			Label.FontFace = SSP
			Label.Text = "Hi"
			Label.TextScaled = true
			Label.Parent = Holder
			table.insert(Labels, { Key = `H{Scale}_{Step}`, Label = Label })
		end
		for Step = 0, 30 do
			local Width = 60 + Step * 0.7
			local Label = Instance.new("TextLabel")
			Label.Size = UDim2.new(Width / 1000, 0, 0, 200)
			Label.FontFace = SSP
			Label.Text = "Samples"
			Label.TextScaled = true
			Label.Parent = Holder
			table.insert(Labels, { Key = `W{Scale}_{Step}`, Label = Label })
		end
	end
	task.wait()
	for _, Entry in Labels do
		Results[Entry.Key] = { Entry.Label.TextBounds.X * 1.5, Entry.Label.TextBounds.Y * 1.5, Entry.Label.AbsoluteSize.X * 1.5, Entry.Label.AbsoluteSize.Y * 1.5 }
	end
	return Results
end
