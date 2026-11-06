-- Unwrapped text slightly wider than its box (TextBounds drops a space run) vs. fitting boxes.
return function(Target)
	local Face = Font.new("rbxasset://fonts/families/SourceSansPro.json")
	local Cases = {
		{ "aaaa bbbb", 80 }, { "aaaa bbbb", 71 }, { "aaaa bbbb", 70 }, { "aaaa bbbb", 60 },
		{ "aaaa bbbb cccc", 110 }, { "aaaa bbbb cccc", 103 }, { "aaaa bbbb cccc", 100 },
		{ "aaaa  bbbb", 80 }, { "aaaa  bbbb", 71 }, { "aaaa bbbb ", 74 }, { " aaaa bbbb", 74 },
		{ "a b c d e f g h", 90 }, { "a b c d e f g h", 85 }, { "a b c d e f g h", 80 },
	}
	for Index, Case in Cases do
		for Column, Alignment in { "Left", "Center", "Right" } do
			local Label = Instance.new("TextLabel")
			Label.Name = `L{Index}_{Alignment}`
			Label.Position = UDim2.fromOffset(20 + (Column - 1) * 170, 10 + (Index - 1) * 34)
			Label.Size = UDim2.fromOffset(Case[2], 28)
			Label.BackgroundColor3 = Color3.fromRGB(40, 40, 60)
			Label.BorderSizePixel = 0
			Label.TextColor3 = Color3.new(1, 1, 1)
			Label.FontFace = Face
			Label.TextSize = 20
			Label.TextXAlignment = Enum.TextXAlignment[Alignment]
			Label.Text = Case[1]
			Label.Parent = Target
		end
	end
end
