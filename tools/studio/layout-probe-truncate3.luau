-- AtEnd character fallback: which one-character prefixes survive, per first character.
return function(Target)
	local Results = {}
	local Labels = {}
	local Face = Font.new("rbxasset://fonts/families/SourceSansPro.json")
	for _, Char in { "i", "l", "r", "a", "e", "o", "n", "G", "m", "W" } do
		for _, Pattern in { "4w", "1w", "4" } do
			local Text = if Pattern == "4w" then string.rep(Char, 4) .. " bbbb" elseif Pattern == "1w" then Char .. "bbb bbbb" else string.rep(Char, 4)
			for Width = 12, 50 do
				local Label = Instance.new("TextLabel")
				Label.Size = UDim2.fromOffset(Width, 30)
				Label.FontFace = Face
				Label.TextSize = 20
				Label.Text = Text
				Label.TextTruncate = Enum.TextTruncate.AtEnd
				Label.Parent = Target
				table.insert(Labels, { Key = `{Char}_{Pattern}_{Width}`, Label = Label })
			end
		end
	end
	task.wait()
	for _, Entry in Labels do
		Results[Entry.Key] = { Entry.Label.TextBounds.X * 1.5 }
	end
	return Results
end
