-- Rich TextScaled (round 4): does the pick depend on the label's history (the size picked before)?
-- The same boxes ("Hi", 60.5 px tall, 100 .. 800 px wide) three ways: fresh; first laid out in a
-- 30.5 px tall box and resized after a frame; first laid out in a 120.5 px tall box and resized.
return function(Target)
	local Results = {}
	local Labels = {}
	local Face = Font.new("rbxasset://fonts/families/FredokaOne.json")
	local Holder = Instance.new("Frame")
	Holder.Size = UDim2.fromOffset(1000, 1000)
	Holder.BackgroundTransparency = 1
	Holder.Parent = Target
	local function Make(Px, H, TextSize)
		local Label = Instance.new("TextLabel")
		Label.Size = UDim2.new(Px / 1.5 / 1000, 0, H / 1.5 / 1000, 0)
		Label.FontFace = Face
		Label.RichText = true
		Label.Text = "Hi"
		Label.TextSize = TextSize
		Label.TextScaled = true
		Label.Parent = Holder
		return Label
	end
	local Later = {}
	for _, TextSize in { 8, 41 } do
		for Step = 0, 175 do
			local Px = 100 + Step * 4
			table.insert(Labels, { Key = `fresh{TextSize}_{Step}`, Label = Make(Px, 60.5, TextSize) })
			local A = Make(Px, 30.5, TextSize)
			local B = Make(Px, 120.5, TextSize)
			table.insert(Labels, { Key = `from30_{TextSize}_{Step}`, Label = A })
			table.insert(Labels, { Key = `from120_{TextSize}_{Step}`, Label = B })
			table.insert(Later, { A, Px })
			table.insert(Later, { B, Px })
		end
	end
	task.wait()
	for _, Entry in Labels do
		local _ = Entry.Label.TextBounds
	end
	for _, Pair in Later do
		Pair[1].Size = UDim2.new(Pair[2] / 1.5 / 1000, 0, 60.5 / 1.5 / 1000, 0)
	end
	task.wait()
	for _, Entry in Labels do
		Results[Entry.Key] = { Entry.Label.TextBounds.X * 1.5, Entry.Label.TextBounds.Y * 1.5, Entry.Label.AbsoluteSize.X * 1.5, Entry.Label.AbsoluteSize.Y * 1.5 }
	end
	return Results
end
