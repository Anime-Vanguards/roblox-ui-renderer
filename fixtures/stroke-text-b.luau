-- Text UIStroke radius probe: "I" glyphs with fractional thicknesses (TextTransparency 1, white stroke
-- on black) so the stroker radius can be read off the straight stem edges; two text sizes.
-- compare: --viewport 1600x400 --time 0.3 --background 0,0,0
return function(Target)
	local Thicknesses = { 0.25, 0.5, 0.75, 1.25, 1.5, 1.75, 2.2125, 2.5, 2.75, 3.3, 3.5, 3.75, 4.25, 4.5, 5.5, 7.5 }
	for Row, Size in { 64, 30 } do
		for Index, Thickness in Thicknesses do
			local Label = Instance.new("TextLabel")
			Label.Name = `R{Row}_{Index}`
			Label.BackgroundTransparency = 1
			Label.FontFace = Font.new("rbxasset://fonts/families/FredokaOne.json")
			Label.TextSize = Size
			Label.Text = "I"
			Label.TextColor3 = Color3.new(1, 1, 1)
			Label.TextTransparency = 1
			Label.TextXAlignment = Enum.TextXAlignment.Left
			Label.TextYAlignment = Enum.TextYAlignment.Top
			Label.Position = UDim2.fromOffset(20 + (Index - 1) * 96, 20 + (Row - 1) * 120)
			Label.Size = UDim2.fromOffset(80, 100)
			Label.Parent = Target

			local Stroke = Instance.new("UIStroke")
			Stroke.Thickness = Thickness
			Stroke.Color = Color3.new(1, 1, 1)
			Stroke.Parent = Label
		end
	end
end
