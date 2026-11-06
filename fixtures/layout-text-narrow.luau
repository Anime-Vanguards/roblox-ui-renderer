-- Wrapped text in boxes narrower than one glyph (character-level breaking, spaces at breaks).
return function(Target)
	local Face = Font.new("rbxasset://fonts/families/SourceSansPro.json")
	local Texts = { "aaaa bbbb cccc", "a b c d e f g h", "aaaa  bbbb", " aaaa bbbb", "aaaa bbbb " }
	local Widths = { 0, 8, 9, 20 }
	local Column = 0
	for _, Text in Texts do
		for _, Width in Widths do
			local Label = Instance.new("TextLabel")
			Label.Name = `N{Column}`
			Label.Position = UDim2.fromOffset(20 + Column * 42, 10)
			Label.Size = UDim2.fromOffset(Width, 330)
			Label.BackgroundColor3 = Color3.fromRGB(40, 40, 60)
			Label.BorderSizePixel = 0
			Label.TextColor3 = Color3.new(1, 1, 1)
			Label.FontFace = Face
			Label.TextSize = 20
			Label.TextWrapped = true
			Label.TextXAlignment = Enum.TextXAlignment.Left
			Label.TextYAlignment = Enum.TextYAlignment.Top
			Label.Text = Text
			Label.Parent = Target
			Column += 1
		end
	end
end
