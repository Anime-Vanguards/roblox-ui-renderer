-- Rich TextScaled (round 7): the start layout of a prompt's button labels (FredokaOne, TextSize 24,
-- RichText, TextWrapped, a 60 x 23.50282 local box under UIScale 0.885): TextBounds with TextScaled off (the
-- start layout Studio lays out) and on, for single words wider than the box and multi-word texts.
return function(Target)
	local Results = {}
	local Labels = {}
	local Face = Font.new("rbxasset://fonts/families/FredokaOne.json")
	local Texts = { "Mystery", "Interact", "Chat", "Purple", "Dark neutral", "Hide Prompt", "Review &amp; craft", "Secondary", "Waxo 42" }
	local Scales = { 0.885, 1, 0.75 }
	local Boxes = { { 60, 23.50282 }, { 50, 23.50282 }, { 60, 40 }, { 45, 60 } }
	for ScaleIndex, Scale in Scales do
		local Holder = Instance.new("Frame")
		Holder.Size = UDim2.fromOffset(1000, 1000)
		Holder.BackgroundTransparency = 1
		Holder.Parent = Target
		if Scale ~= 1 then
			local U = Instance.new("UIScale")
			U.Scale = Scale
			U.Parent = Holder
		end
		for BoxIndex, Box in Boxes do
			for TextIndex, Text in Texts do
				for _, Scaled in { false, true } do
					for _, Wrapped in { true, false } do
						local Label = Instance.new("TextLabel")
						Label.Size = UDim2.new(Box[1] / 1000, 0, Box[2] / 1000, 0)
						Label.FontFace = Face
						Label.RichText = true
						Label.Text = Text
						Label.TextSize = 24
						Label.TextScaled = Scaled
						Label.TextWrapped = Wrapped
						Label.Parent = Holder
						table.insert(Labels, { Key = `{ScaleIndex}_{BoxIndex}_{TextIndex}_{if Scaled then "s" else "n"}{if Wrapped then "w" else "u"}`, Label = Label })
					end
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
