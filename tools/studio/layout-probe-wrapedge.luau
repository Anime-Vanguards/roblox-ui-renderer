-- Wrapped text edge cases: first glyph wider than the box (empty first line?), multiple
-- paragraphs, runs of spaces, TextBounds.X of lines cut off by the box height.
return function(Target)
	local Results = {}
	local Labels = {}
	local SSP = Font.new("rbxasset://fonts/families/SourceSansPro.json")
	local Fredoka = Font.new("rbxasset://fonts/families/FredokaOne.json")
	local Cases = {
		{ "x32_10", SSP, 32, "x", 10, 500 },
		{ "x32_16", SSP, 32, "x", 16, 500 },
		{ "x32_20", SSP, 32, "x", 20, 500 },
		{ "xx32_10", SSP, 32, "xx", 10, 500 },
		{ "xnx32_10", SSP, 32, "x\nx", 10, 500 },
		{ "anx32_10", SSP, 32, "a\nxx", 10, 500 },
		{ "sp14_30", SSP, 14, "a  b   c    d", 30, 500 },
		{ "sp14_20", SSP, 14, "a  b   c    d", 20, 500 },
		{ "sp14_45", SSP, 14, "a  b   c    d", 45, 500 },
		{ "sp32_10", SSP, 32, "a  b   c    d", 10, 500 },
		{ "fsp32_10", Fredoka, 32, "a  b   c    d", 10, 500 },
		{ "cut_1", SSP, 20, "a bb cccccccccc", 70, 20 },
		{ "cut_2", SSP, 20, "a bb cccccccccc dddd", 70, 45 },
		{ "wide_first", SSP, 20, "WWWWWW i", 30, 500 },
		{ "nl_space", SSP, 20, "aaaa \n bbbb", 40, 500 },
		{ "tab", SSP, 20, "aaaa\tbbbb", 40, 500 },
		{ "nbsp", SSP, 20, "aaaa\u{A0}bbbb cc", 60, 500 },
		{ "hyphen", SSP, 20, "one-two-three", 50, 500 },
	}
	for _, Case in Cases do
		local Label = Instance.new("TextLabel")
		Label.Size = UDim2.fromOffset(Case[5], Case[6])
		Label.FontFace = Case[2]
		Label.TextSize = Case[3]
		Label.Text = Case[4]
		Label.TextWrapped = true
		Label.Parent = Target
		table.insert(Labels, { Key = Case[1], Label = Label })
	end
	task.wait()
	for _, Entry in Labels do
		Results[Entry.Key] = { Entry.Label.TextBounds.X * 1.5, Entry.Label.TextBounds.Y * 1.5 }
	end
	return Results
end
