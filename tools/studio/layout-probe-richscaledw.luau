-- Rich TextScaled limited by the box width (round 3): box widths swept in 0.1 px steps with a tall box,
-- to find how far a line may overflow the width (a "View Full Details" label picks 33 px, 226 px
-- wide in a 225.675 px box; button "Secondary" picks 48 px, rejecting 49 px at 213 px in 212.4).
return function(Target)
	local Results = {}
	local Labels = {}
	local Face = Font.new("rbxasset://fonts/families/FredokaOne.json")
	local Holder = Instance.new("Frame")
	Holder.Size = UDim2.fromOffset(1000, 1000)
	Holder.BackgroundTransparency = 1
	Holder.Parent = Target
	local Texts = { "Secondary", "Cyan", "Review & craft", "View Full Details" }
	for Index, Text in Texts do
		for Step = 0, 1199 do
			local Px = 120 + Step / 10
			for _, Rich in { true, false } do
				local Label = Instance.new("TextLabel")
				Label.Size = UDim2.new(Px / 1.5 / 1000, 0, 0, 150)
				Label.FontFace = Face
				Label.RichText = Rich
				Label.Text = Text
				Label.TextScaled = true
				Label.Parent = Holder
				table.insert(Labels, { Key = `{Index}_{Step}_{Rich}`, Label = Label })
			end
		end
	end
	task.wait()
	for _, Entry in Labels do
		Results[Entry.Key] = { Entry.Label.TextBounds.X * 1.5, Entry.Label.TextBounds.Y * 1.5, Entry.Label.AbsoluteSize.X * 1.5 }
	end
	return Results
end
