-- Where lines of a LineHeight other than 1 start: four lines "Hg" at LineHeight 0.55 to 1.45 in two sizes, so the
-- line pitch (the line's height x LineHeight, device px) lands on every fraction of a pixel at device scales 1, 1.5,
-- 2 and 3 (notes/text.md "Line tops at fractional pitches"). Plain text, top-aligned, no stroke.
-- compare: --viewport 1240x300 --time 0.2 --background 0,0,0
return function(Target)
	for Row, Size in { 20, 17 } do
		for Column = 0, 18 do
			local Factor = 0.55 + Column * 0.05
			local Label = Instance.new("TextLabel")
			Label.Name = string.format("S%d_L%d", Size, math.round(Factor * 100))
			Label.BackgroundTransparency = 1
			Label.TextColor3 = Color3.new(1, 1, 1)
			Label.FontFace = Font.new("rbxasset://fonts/families/SourceSansPro.json")
			Label.TextSize = Size
			Label.LineHeight = Factor
			Label.Text = "Hg\nHg\nHg\nHg"
			Label.TextXAlignment = Enum.TextXAlignment.Left
			Label.TextYAlignment = Enum.TextYAlignment.Top
			Label.Position = UDim2.fromOffset(4 + Column * 64, 4 + (Row - 1) * 145)
			Label.Size = UDim2.fromOffset(60, 140)
			Label.Parent = Target
		end
	end
end
