-- TextScaled: does the property assignment order (TextWrapped after TextScaled) change TextBounds?
return function(Target)
	local Results = {}
	local Labels = {}
	local SSP = Font.new("rbxasset://fonts/families/SourceSansPro.json")
	local Count = 0
	for _, Order in { "wrapFirst", "scaledFirst", "noWrapSet" } do
		for _, Wrapped in { false, true } do
			for _, Size in { { 123, 21 }, { 100, 40 }, { 60, 60 } } do
				Count += 1
				local Label = Instance.new("TextLabel")
				Label.Position = UDim2.fromOffset((Count % 8) * 140, (Count // 8) * 70)
				Label.Size = UDim2.fromOffset(Size[1], Size[2])
				Label.FontFace = SSP
				Label.Text = "Disable samples"
				if Order == "wrapFirst" then
					Label.TextWrapped = Wrapped
					Label.TextScaled = true
				elseif Order == "scaledFirst" then
					Label.TextScaled = true
					Label.TextWrapped = Wrapped
				else
					Label.TextScaled = true
				end
				Label.Parent = Target
				table.insert(Labels, { Key = `{Order}_{if Wrapped then "w" else "u"}_{Size[1]}x{Size[2]}`, Label = Label })
			end
		end
	end
	task.wait()
	task.wait()
	for _, Entry in Labels do
		Results[Entry.Key] = { Entry.Label.TextBounds.X * 1.5, Entry.Label.TextBounds.Y * 1.5, if Entry.Label.TextWrapped then 1 else 0 }
	end
	return Results
end
