-- TextTruncate AtEnd / SplitWord in boxes of several widths (unwrapped and wrapped), left aligned.
return function(Target)
	local Face = Font.new("rbxasset://fonts/families/SourceSansPro.json")
	local Widths = { 16, 26, 36, 52, 56, 90, 92, 106 }
	local Row = 0
	for _, Mode in { "AtEnd", "SplitWord" } do
		for _, Wrapped in { false, true } do
			for Column, Width in Widths do
				local Label = Instance.new("TextLabel")
				Label.Name = `{Mode}_{if Wrapped then "w" else "u"}_{Width}`
				Label.Position = UDim2.fromOffset(10 + (Column - 1) * 135, 10 + Row * 60)
				Label.Size = UDim2.fromOffset(Width, if Wrapped then 45 else 30)
				Label.BackgroundColor3 = Color3.fromRGB(40, 40, 60)
				Label.BorderSizePixel = 0
				Label.TextColor3 = Color3.new(1, 1, 1)
				Label.FontFace = Face
				Label.TextSize = 20
				Label.TextXAlignment = Enum.TextXAlignment.Left
				Label.TextYAlignment = Enum.TextYAlignment.Top
				Label.TextWrapped = Wrapped
				Label.TextTruncate = Enum.TextTruncate[Mode]
				Label.Text = "Guild Halls of the Northern Reach"
				Label.Parent = Target
			end
			Row += 1
		end
	end
end
