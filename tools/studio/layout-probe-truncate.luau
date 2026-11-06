-- TextTruncate AtEnd / SplitWord: TextBounds and TextFits over box widths (wrapped and unwrapped).
return function(Target)
	local Results = {}
	local Labels = {}
	local Face = Font.new("rbxasset://fonts/families/SourceSansPro.json")
	local Strings = { "Guild Halls of the Northern Reach", "aaaa bbbb cccc", "Supercalifragilistic", "ab" }
	for Index, Text in Strings do
		for _, Mode in { "AtEnd", "SplitWord" } do
			for _, Wrapped in { false, true } do
				for Width = 0, 160, 2 do
					local Label = Instance.new("TextLabel")
					Label.Size = UDim2.fromOffset(Width, if Wrapped then 45 else 30)
					Label.FontFace = Face
					Label.TextSize = 20
					Label.Text = Text
					Label.TextWrapped = Wrapped
					Label.TextTruncate = Enum.TextTruncate[Mode]
					Label.Parent = Target
					table.insert(Labels, { Key = `{Index}_{Mode}_{if Wrapped then "w" else "u"}_{Width}`, Label = Label })
				end
			end
		end
	end
	task.wait()
	for _, Entry in Labels do
		Results[Entry.Key] = { Entry.Label.TextBounds.X * 1.5, Entry.Label.TextBounds.Y * 1.5, if Entry.Label.TextFits then 1 else 0 }
	end
	return Results
end
