-- Text UIStroke reach at large thicknesses: white Contextual strokes of an "I" (FredokaOne, its fill clear) at
-- TextSize 48, 72, 96 and 120 (rows) and Thickness 5 to 13 (columns). Each stroked stem's width is the stem's plus
-- twice the stroke's reach in device px, which a client stops growing past some reach at device scale 3.
-- compare: --viewport 880x680 --time 0.3 --background 0,0,0
return function(Target)
	for Row, Size in { 48, 72, 96, 120 } do
		for Column = 0, 8 do
			local Label = Instance.new("TextLabel")
			Label.Name = `S{Size}_T{Column + 5}`
			Label.BackgroundTransparency = 1
			Label.FontFace = Font.new("rbxasset://fonts/families/FredokaOne.json")
			Label.TextSize = Size
			Label.Text = "I"
			Label.TextTransparency = 1
			Label.TextXAlignment = Enum.TextXAlignment.Center
			Label.TextYAlignment = Enum.TextYAlignment.Top
			Label.Position = UDim2.fromOffset(20 + Column * 95, 20 + (Row - 1) * 165)
			Label.Size = UDim2.fromOffset(80, 150)
			Label.Parent = Target
			local Stroke = Instance.new("UIStroke")
			Stroke.Thickness = Column + 5
			Stroke.Color = Color3.new(1, 1, 1)
			Stroke.Parent = Label
		end
	end
end
