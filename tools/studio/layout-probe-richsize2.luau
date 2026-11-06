-- RichText base size under UIScale: line height and width for several texts, dense sizes/scales.
return function(Target)
	local Results = {}
	local Labels = {}
	local Face = Font.new("rbxasset://fonts/families/FredokaOne.json")
	for _, Scale in { 0.885, 1.3 } do
		local Holder = Instance.new("Frame")
		Holder.Size = UDim2.fromOffset(1000, 1000)
		Holder.BackgroundTransparency = 1
		Holder.Parent = Target
		local UIScale = Instance.new("UIScale")
		UIScale.Scale = Scale
		UIScale.Parent = Holder
		for TextSize = 8, 100 do
			for Index, Text in { "Hi", "Quick Craft", "" } do
				for _, Rich in { true, false } do
					local Label = Instance.new("TextLabel")
					Label.Size = UDim2.fromOffset(600, 200)
					Label.FontFace = Face
					Label.TextSize = TextSize
					Label.RichText = Rich
					Label.Text = Text
					Label.Parent = Holder
					table.insert(Labels, { Key = `{Scale}_{TextSize}_{Index}_{Rich}`, Label = Label })
				end
			end
		end
	end
	task.wait()
	for _, Entry in Labels do
		Results[Entry.Key] = { Entry.Label.TextBounds.X * 1.5, Entry.Label.TextBounds.Y * 1.5 }
	end
	return Results
end
