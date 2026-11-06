-- Baseline placement probe: fonts with and without an hhea lineGap at several sizes, vertical
-- alignments and sub-pixel label positions (white on black). tools/analysis/text_baseline.py
-- measures the vertical offset of every label against Studio.
return function(Target)
	local Fonts = { "Legacy", "PermanentMarker", "Garamond", "Ubuntu", "Bodoni", "FredokaOne", "GothamBold", "SourceSans" }
	local Sizes = { 12, 14, 16, 18, 20, 24, 28 }
	local Alignments = { Enum.TextYAlignment.Top, Enum.TextYAlignment.Center, Enum.TextYAlignment.Bottom }

	local Index = 0
	for FontIndex, Name in Fonts do
		for SizeIndex, Size in Sizes do
			for AlignIndex, Alignment in Alignments do
				local Label = Instance.new("TextLabel")
				Label.Name = `{Name}_{Size}_{Alignment.Name}`
				Label.BackgroundTransparency = 1
				Label.TextColor3 = Color3.new(1, 1, 1)
				Label.FontFace = Font.fromEnum((Enum.Font :: any)[Name])
				Label.TextSize = Size
				Label.Text = "Hxbg"
				Label.TextXAlignment = Enum.TextXAlignment.Left
				Label.TextYAlignment = Alignment
				Label.Size = UDim2.fromOffset(66, 44)
				Label.AnchorPoint = Vector2.new(0, ((SizeIndex + AlignIndex + FontIndex) % 7) * 0.0107)
				Label.Position = UDim2.fromOffset(4 + (Index % 14) * 70, 4 + (Index // 14) * 48)
				Label.Parent = Target
				Index += 1
			end
		end
	end
end
