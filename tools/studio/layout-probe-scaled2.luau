-- TextScaled TextBounds: RichText x TextWrapped x TextSize property (does TextBounds follow the
-- scaled size?).
return function(Target)
	local Results = {}
	local Labels = {}
	local SSP = Font.new("rbxasset://fonts/families/SourceSansPro.json")
	local Count = 0
	for _, Rich in { false, true } do
		for _, Wrapped in { false, true } do
			for _, TextSize in { 8, 14, 40 } do
				for _, Size in { { 123, 21 }, { 100, 40 }, { 60, 60 } } do
					Count += 1
					local Label = Instance.new("TextLabel")
					Label.Position = UDim2.fromOffset((Count % 8) * 140, (Count // 8) * 70)
					Label.Size = UDim2.fromOffset(Size[1], Size[2])
					Label.FontFace = SSP
					Label.TextSize = TextSize
					Label.Text = "Disable samples"
					Label.RichText = Rich
					Label.TextWrapped = Wrapped
					Label.TextScaled = true
					Label.Parent = Target
					table.insert(Labels, { Key = `{if Rich then "r" else "p"}{if Wrapped then "w" else "u"}_{TextSize}_{Size[1]}x{Size[2]}`, Label = Label })
				end
			end
		end
	end
	task.wait()
	local _ = Labels[1].Label.AbsoluteSize
	task.wait()
	for _, Entry in Labels do
		Results[Entry.Key] = { Entry.Label.TextBounds.X * 1.5, Entry.Label.TextBounds.Y * 1.5 }
	end
	return Results
end
