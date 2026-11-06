-- Per-glyph pen positions (tools/analysis/layout_textpen.py --glyphs): left-aligned lines of
-- isolated "I" stems at whole device pixels, SourceSans and FredokaOne, plain and rich, TextSize 12
-- to 30 at DPI 1.5, so glyph advances take many fractional 26.6 values.
return function(Target)
	local Faces = {
		{ Name = "ssp", Face = Font.new("rbxasset://fonts/families/SourceSansPro.json") },
		{ Name = "fred", Face = Font.new("rbxasset://fonts/families/FredokaOne.json") },
	}
	local Column = 0
	for _, Entry in Faces do
		for _, Rich in { false, true } do
			local Y = 4
			for TextSize = 12, 30 do
				local Label = Instance.new("TextLabel")
				Label.Name = `{Entry.Name}_{if Rich then "rich" else "plain"}_{TextSize}`
				Label.BackgroundTransparency = 1
				Label.TextColor3 = Color3.new(1, 1, 1)
				Label.FontFace = Entry.Face
				Label.TextSize = TextSize
				Label.RichText = Rich
				Label.Text = "I  I  I  I  I  I  I"
				Label.TextXAlignment = Enum.TextXAlignment.Left
				Label.TextYAlignment = Enum.TextYAlignment.Center
				Label.Size = UDim2.fromOffset(290, TextSize + 6)
				Label.Position = UDim2.fromOffset(4 + Column * 298, Y)
				Label.Parent = Target
				Y += TextSize + 8
			end
			Column += 1
		end
	end
end
