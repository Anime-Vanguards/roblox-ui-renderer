-- TextBounds of unwrapped / wrapped text near its natural width, for several space layouts.
return function(Target)
	local Results = {}
	local Labels = {}
	local Strings = { "aaaa bbbb", "aaaa bbbb cccc", "aaaa  bbbb", "aaaabbbb", "aaaa bbbb ", " aaaa bbbb", "a b c d e f g h", "aaaa bbbb c" }
	local Face = Font.new("rbxasset://fonts/families/SourceSansPro.json")
	for Index, Text in Strings do
		for _, Wrapped in { false, true } do
			for Width = 0, 140 do
				local Label = Instance.new("TextLabel")
				Label.Size = UDim2.fromOffset(Width, 100)
				Label.FontFace = Face
				Label.TextSize = 20
				Label.Text = Text
				Label.TextWrapped = Wrapped
				Label.Parent = Target
				table.insert(Labels, { Key = `{Index}_{if Wrapped then "w" else "u"}_{Width}`, Label = Label })
			end
		end
	end
	task.wait()
	for _, Entry in Labels do
		Results[Entry.Key] = { Entry.Label.TextBounds.X, Entry.Label.TextBounds.Y }
	end
	return Results
end
