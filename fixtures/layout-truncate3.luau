-- TextTruncate SplitWord with wrapped text cut by the box height (2 visible lines).
return function(Target)
	local Face = Font.new("rbxasset://fonts/families/SourceSansPro.json")
	local Widths = { 50, 64, 72, 78, 84, 90, 100, 108 }
	for Row, Text in { "Guild Halls of the Northern Reach", "aaaa bbbb cccc dddd eeee" } do
		for Column, Width in Widths do
			local Label = Instance.new("TextLabel")
			Label.Name = `S{Row}_{Width}`
			Label.Position = UDim2.fromOffset(10 + (Column - 1) * 118, 10 + (Row - 1) * 60)
			Label.Size = UDim2.fromOffset(Width, 45)
			Label.BackgroundColor3 = Color3.fromRGB(40, 40, 60)
			Label.BorderSizePixel = 0
			Label.TextColor3 = Color3.new(1, 1, 1)
			Label.FontFace = Face
			Label.TextSize = 20
			Label.TextXAlignment = Enum.TextXAlignment.Left
			Label.TextYAlignment = Enum.TextYAlignment.Top
			Label.TextWrapped = true
			Label.TextTruncate = Enum.TextTruncate.SplitWord
			Label.Text = Text
			Label.Parent = Target
		end
	end
end
