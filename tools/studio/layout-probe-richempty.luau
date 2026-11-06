-- Rich text lines without glyphs (empty text, <br/>, "\n") at sizes whose pixel size is fractional
-- (TextSize x UIScale x 1.5 with a .5), next to plain text.
return function(Target)
	local Results = {}
	local Labels = {}
	local Face = Font.new("rbxasset://fonts/families/FredokaOne.json")
	local Texts = { "", "A<br/>", "<br/>A", "A<br/><br/>B", "A\n\nB", "<br/>", "\n", "A", "<font size=\"21\">A</font><br/>", "A<br/><font size=\"21\"> </font>" }
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
		for TextSize = 8, 40 do
			for Index, Text in Texts do
				for _, Rich in { true, false } do
					local Label = Instance.new("TextLabel")
					Label.Size = UDim2.fromOffset(400, 300)
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
