-- Text placement probe: the same white label on black at a grid of whole-device-pixel positions over
-- the whole viewport. tools/analysis/text_subpixel.py fits each label's sub-pixel offset against our
-- whole-pixel glyph placement (a Studio viewport whose projection is stretched shows up as offsets
-- that grow with x or y). The grid spans the compared viewport (5 columns, 12 rows): the calibration
-- runs at 1200x780, which fits Studio's viewport on a 1080p monitor at 100% and a 4K one at 150%.
-- rur compare --fixture fixtures/text-probe-place.luau --viewport 1200x780 --time 0.2 --background 0,0,0
return function(Target)
	local Size = Target.AbsoluteSize
	-- Even steps: every label on a whole device pixel at 1.5x too (an odd step puts every other row on a
	-- half pixel, which the stretch fit reads as a 0.37 px stretch)
	local ColumnStep = 2 * math.floor((Size.X - 80) / 8)
	local RowStep = 2 * math.floor((Size.Y - 40) / 22)
	for Column = 0, 4 do
		for Row = 0, 11 do
			local Label = Instance.new("TextLabel")
			Label.Name = `P{Column}_{Row}`
			Label.BackgroundTransparency = 1
			Label.TextColor3 = Color3.new(1, 1, 1)
			Label.FontFace = Font.new("rbxasset://fonts/families/SourceSansPro.json", Enum.FontWeight.Bold)
			Label.TextSize = 24
			Label.Text = "HxH"
			Label.TextXAlignment = Enum.TextXAlignment.Left
			Label.TextYAlignment = Enum.TextYAlignment.Top
			Label.Size = UDim2.fromOffset(60, 30)
			Label.Position = UDim2.fromOffset(10 + Column * ColumnStep, 4 + Row * RowStep)
			Label.Parent = Target
		end
	end
end
