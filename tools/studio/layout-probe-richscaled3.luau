-- Rich TextScaled at UIScale 1: box heights sampled finely across [N, N + 1) physical px, for one and
-- two lines, to pin where each pixel size starts.
return function(Target)
	local Results = {}
	local Labels = {}
	local Face = Font.new("rbxasset://fonts/families/FredokaOne.json")
	local Holder = Instance.new("Frame")
	Holder.Size = UDim2.fromOffset(1000, 1000)
	Holder.BackgroundTransparency = 1
	Holder.Parent = Target
	for N = 22, 70 do
		for Step = 0, 39 do
			local Box = N + Step / 40
			for Index, Text in { "Cyan", "Cyan\nCyan" } do
				local Label = Instance.new("TextLabel")
				Label.Size = UDim2.new(0, 600, Box * Index / 1.5 / 1000, 0)
				Label.FontFace = Face
				Label.RichText = true
				Label.Text = Text
				Label.TextScaled = true
				Label.Parent = Holder
				table.insert(Labels, { Key = `{N}_{Step}_{Index}`, Label = Label })
			end
		end
	end
	task.wait()
	for _, Entry in Labels do
		Results[Entry.Key] = { Entry.Label.TextBounds.X * 1.5, Entry.Label.TextBounds.Y * 1.5, Entry.Label.AbsoluteSize.Y * 1.5 }
	end
	return Results
end
