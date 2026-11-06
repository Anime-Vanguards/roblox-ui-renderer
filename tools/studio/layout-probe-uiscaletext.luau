-- Text under UIScale: effective size (TextBounds.Y of one line) and TextBounds units.
return function(Target)
	local Results = {}
	local Labels = {}
	local SSP = Font.new("rbxasset://fonts/families/SourceSansPro.json")
	local Count = 0
	for _, Scale in { 0.5, 0.75, 0.87, 0.885, 1.3, 1.7 } do
		local Holder = Instance.new("Frame")
		Holder.Position = UDim2.fromOffset(0, Count * 10)
		Holder.Size = UDim2.fromOffset(1000, 1000)
		Holder.BackgroundTransparency = 1
		Holder.Parent = Target
		local UIScale = Instance.new("UIScale")
		UIScale.Scale = Scale
		UIScale.Parent = Holder
		Count += 1
		for TextSize = 8, 40, 1 do
			local Label = Instance.new("TextLabel")
			Label.Size = UDim2.fromOffset(300, 60)
			Label.FontFace = SSP
			Label.TextSize = TextSize
			Label.Text = "Hello World"
			Label.Parent = Holder
			table.insert(Labels, { Key = `{Scale}_{TextSize}`, Label = Label })
		end
	end
	task.wait()
	for _, Entry in Labels do
		Results[Entry.Key] = { Entry.Label.TextBounds.X * 1.5, Entry.Label.TextBounds.Y * 1.5 }
	end
	return Results
end
