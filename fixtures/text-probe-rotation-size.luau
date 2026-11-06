-- Isolates odd/even text sizes, direct/inherited rotation, and a lowercase m's stroke band.
-- rur compare --fixture fixtures/text-probe-rotation-size.luau --viewport 1200x900 --time 0.2 --background 20,20,26
return function(Target)
	for Index, Size in { 10, 11, 12, 13, 14, 15, 30 } do
		for Column = 1, 3 do
			local Parent = Target
			local X, Y = 35 + (Column - 1) * 160, 20 + (Index - 1) * 58
			if Column == 3 then
				Parent = Instance.new("Frame")
				Parent.Name = `Parent{Size}`
				Parent.Position = UDim2.fromOffset(X, Y)
				Parent.Size = UDim2.fromOffset(120, 40)
				Parent.Rotation = 14
				Parent.BackgroundTransparency = 1
				Parent.Parent = Target
				X, Y = 0, 0
			end

			local Label = Instance.new("TextLabel")
			Label.Name = `Size{Size}Mode{Column}`
			Label.Position = UDim2.fromOffset(X, Y)
			Label.Size = UDim2.fromOffset(120, 40)
			Label.Rotation = if Column == 2 then 14 else 0
			Label.BackgroundTransparency = 1
			Label.TextColor3 = Color3.new(1, 1, 1)
			Label.FontFace = Font.fromEnum(Enum.Font.GothamBlack)
			Label.TextSize = Size
			Label.Text = "NEW"
			Label.Parent = Parent
		end
	end

	for Index, Text in { "Items", "m", "Items", "m" } do
		local Label = Instance.new("TextLabel")
		Label.Name = `Stroke{Index}`
		Label.Position = UDim2.fromOffset(550, 20 + (Index - 1) * 58)
		Label.Size = UDim2.fromOffset(160, 40)
		Label.BackgroundTransparency = 1
		Label.TextColor3 = Color3.new(1, 1, 1)
		Label.TextTransparency = if Index > 2 then 1 else 0
		Label.FontFace = Font.fromEnum(Enum.Font.GothamBold)
		Label.TextSize = 15
		Label.Text = Text
		Label.Parent = Target

		local Stroke = Instance.new("UIStroke")
		Stroke.Thickness = 1.5
		Stroke.Color = Color3.fromRGB(255, 70, 90)
		Stroke.Parent = Label
	end
end
