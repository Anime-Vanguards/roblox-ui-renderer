-- Where Studio puts each glyph: runs of one letter (no kerning) in faces with and without a line gap at
-- several sizes, white on black. The pen's fraction shows in each glyph's quarter-pixel bitmap.
-- compare: --viewport 1500x800 --time 0.2 --background 0,0,0
return function(Target)
	local Fonts = { "SourceSans", "Gotham", "FredokaOne", "Arial", "Ubuntu", "PermanentMarker", "Bangers", "Code" }
	local Sizes = { 9, 14, 18, 23, 30 }
	local Y = 6
	for _, Size in Sizes do
		for Index, Name in Fonts do
			local Label = Instance.new("TextLabel")
			Label.Name = `{Name}{Size}`
			Label.BackgroundTransparency = 1
			Label.TextColor3 = Color3.new(1, 1, 1)
			Label.Font = (Enum.Font :: any)[Name]
			Label.TextSize = Size
			Label.Text = string.rep("n", 20) .. string.rep("H", 12)
			Label.TextXAlignment = Enum.TextXAlignment.Left
			Label.Size = UDim2.fromOffset(740, Size + 6)
			Label.Position = UDim2.fromOffset(8 + ((Index - 1) % 2) * 748, Y + ((Index - 1) // 2) * (Size + 8))
			Label.Parent = Target
		end
		Y += 4 * (Size + 8) + 6
	end
end
