-- Horizontal text pen placement (tools/analysis/layout_textpen.py): Left / Center / Right labels
-- whose content box starts at device-pixel fractions 0, .25, .5 and .75 (DPI 1.5), with odd and even
-- box widths and a range of string widths, so the line start takes every quarter fraction. Every
-- string starts with an isolated "I" stem that the analysis locates in both images.
return function(Target)
	local Face = Font.new("rbxasset://fonts/families/SourceSansPro.json")
	local Texts = { "I", "I  x", "I  xx", "I  xxx", "I  xxxx", "I  xm", "I  mm", "I  xmx" }
	local Widths = { 60, 61 }
	local Alignments = { "Left", "Center", "Right" }
	-- Logical x offsets that land on device fractions 0, .25, .5, .75 (DPI 1.5): L even, L + 1/6
	-- (L even), L odd, L + 1/2 (L even)
	local Fractions = {
		{ Name = "f00", Scale = 0 },
		{ Name = "f25", Scale = 1 / 6 },
		{ Name = "f50", Scale = 1 },
		{ Name = "f75", Scale = 1 / 2 },
	}
	local RowWidth = 1200

	local Row = 0
	for _, Alignment in Alignments do
		for _, Fraction in Fractions do
			local Holder = Instance.new("Frame")
			Holder.Name = `{Alignment}_{Fraction.Name}`
			Holder.BackgroundTransparency = 1
			Holder.Position = UDim2.fromOffset(0, 4 + Row * 34)
			Holder.Size = UDim2.fromOffset(RowWidth, 32)
			Holder.Parent = Target
			Row += 1

			local Column = 0
			for _, Width in Widths do
				for TextIndex, Text in Texts do
					local Label = Instance.new("TextLabel")
					Label.Name = `{Alignment}_{Fraction.Name}_w{Width}_t{TextIndex}`
					Label.BackgroundTransparency = 1
					Label.TextColor3 = Color3.new(1, 1, 1)
					Label.FontFace = Face
					Label.TextSize = 20
					Label.Text = Text
					Label.TextXAlignment = Enum.TextXAlignment[Alignment]
					Label.TextYAlignment = Enum.TextYAlignment.Center
					Label.Size = UDim2.fromOffset(Width, 28)
					-- 72 logical columns (even) keep the fraction; the scale term adds it
					Label.Position = UDim2.new(Fraction.Scale / RowWidth, 4 + Column * 72, 0, 2)
					Label.Parent = Holder
					Column += 1
				end
			end
		end
	end
end
