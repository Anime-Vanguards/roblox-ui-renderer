-- Text UIStroke radius vs fractional thickness: "I" glyphs (TextTransparency 1, white stroke on black)
-- with thickness 1 + k/32, k = 0..31, to map the stroker radius quantization.
-- compare: --viewport 1600x300 --time 0.3 --background 0,0,0
return function(Target)
	for K = 0, 31 do
		local Label = Instance.new("TextLabel")
		Label.Name = `F{K}`
		Label.BackgroundTransparency = 1
		Label.FontFace = Font.new("rbxasset://fonts/families/FredokaOne.json")
		Label.TextSize = 48
		Label.Text = "I"
		Label.TextColor3 = Color3.new(1, 1, 1)
		Label.TextTransparency = 1
		Label.TextXAlignment = Enum.TextXAlignment.Left
		Label.TextYAlignment = Enum.TextYAlignment.Top
		Label.Position = UDim2.fromOffset(20 + (K % 16) * 96, 20 + (K // 16) * 120)
		Label.Size = UDim2.fromOffset(80, 100)
		Label.Parent = Target

		local Stroke = Instance.new("UIStroke")
		Stroke.Thickness = 1 + K / 32
		Stroke.Color = Color3.new(1, 1, 1)
		Stroke.Parent = Label
	end
end
