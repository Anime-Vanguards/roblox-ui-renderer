-- TextTruncate AtEnd with a first word that does not fit: character fallback widths.
return function(Target)
	local Face = Font.new("rbxasset://fonts/families/SourceSansPro.json")
	local Row = 0
	for _, Text in { "aaaa bbbb cccc", "Guild Halls", "aaaa", "Gaaa bbbb" } do
		for Column, Width in { 16, 22, 24, 26, 28, 30, 32, 40 } do
			local Label = Instance.new("TextLabel")
			Label.Name = `T{Row}_{Width}`
			Label.Position = UDim2.fromOffset(10 + (Column - 1) * 60, 10 + Row * 40)
			Label.Size = UDim2.fromOffset(Width, 30)
			Label.BackgroundColor3 = Color3.fromRGB(40, 40, 60)
			Label.BorderSizePixel = 0
			Label.TextColor3 = Color3.new(1, 1, 1)
			Label.FontFace = Face
			Label.TextSize = 20
			Label.TextXAlignment = Enum.TextXAlignment.Left
			Label.TextTruncate = Enum.TextTruncate.AtEnd
			Label.Text = Text
			Label.Parent = Target
		end
		Row += 1
	end
end
