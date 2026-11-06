-- TextScaled: chosen size (TextBounds) over box sizes, wrapped/unwrapped, with UIScale and
-- UITextSizeConstraint.
return function(Target)
	local Results = {}
	local Labels = {}
	local SSP = Font.new("rbxasset://fonts/families/SourceSansPro.json")
	local Gotham = Font.new("rbxasset://fonts/families/GothamSSm.json", Enum.FontWeight.Bold)
	local Texts = { "Disable samples", "Hi", "The quick brown fox jumps" }
	local Count = 0
	local function Add(Key, Face, Text, W, H, Wrapped, Scale, MinSize, MaxSize)
		Count += 1
		local Holder = Instance.new("Frame")
		Holder.Position = UDim2.fromOffset((Count % 8) * 140, (Count // 8) % 8 * 110)
		Holder.Size = UDim2.fromOffset(130, 100)
		Holder.BackgroundTransparency = 1
		Holder.Parent = Target
		if Scale then
			local UIScale = Instance.new("UIScale")
			UIScale.Scale = Scale
			UIScale.Parent = Holder
		end
		local Label = Instance.new("TextLabel")
		Label.Size = UDim2.fromOffset(W, H)
		Label.FontFace = Face
		Label.Text = Text
		Label.TextScaled = true
		Label.TextWrapped = Wrapped
		Label.Parent = Holder
		if MinSize or MaxSize then
			local Constraint = Instance.new("UITextSizeConstraint")
			Constraint.MinTextSize = MinSize or 1
			Constraint.MaxTextSize = MaxSize or 100
			Constraint.Parent = Label
		end
		table.insert(Labels, { Key = Key, Label = Label })
	end
	for TextIndex, Text in Texts do
		for _, Size in { { 60, 20 }, { 123, 21 }, { 124, 22 }, { 100, 40 }, { 80, 80 }, { 200, 13 }, { 37, 50 } } do
			for _, Wrapped in { false, true } do
				Add(`{TextIndex}_{Size[1]}x{Size[2]}_{if Wrapped then "w" else "u"}`, SSP, Text, Size[1], Size[2], Wrapped)
				Add(`G{TextIndex}_{Size[1]}x{Size[2]}_{if Wrapped then "w" else "u"}`, Gotham, Text, Size[1], Size[2], Wrapped)
			end
		end
	end
	for _, Scale in { 0.87, 1.3 } do
		for _, Size in { { 123, 21 }, { 100, 40 } } do
			Add(`S{Scale}_{Size[1]}x{Size[2]}`, SSP, "Disable samples", Size[1], Size[2], false, Scale)
		end
	end
	Add("C_max12", SSP, "Disable samples", 123, 40, false, nil, nil, 12)
	Add("C_min30", SSP, "Disable samples", 123, 20, false, nil, 30, nil)
	Add("C_max15w", SSP, "The quick brown fox jumps", 80, 80, true, nil, nil, 15)
	task.wait()
	for _, Entry in Labels do
		local _ = Entry.Label.AbsoluteSize
	end
	task.wait(0.3)
	for _, Entry in Labels do
		Results[Entry.Key] = { Entry.Label.TextBounds.X * 1.5, Entry.Label.TextBounds.Y * 1.5 }
	end
	return Results
end
