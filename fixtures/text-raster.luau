-- Text raster probe: white text on black across fonts and sizes (FreeType hinting mode calibration).
return function(Target)
	local Specs = {
		{ "FredokaOne", "Regular", { 10, 12, 14, 16, 18, 20, 24, 28, 32, 40, 48 } },
		{ "GothamSSm", "Bold", { 12, 14, 18, 24, 32 } },
		{ "GothamSSm", "Medium", { 14, 20, 28 } },
		{ "GothamSSm", "Heavy", { 16, 24 } },
		{ "SourceSansPro", "Regular", { 14, 20, 28 } },
		{ "BuilderSans", "Bold", { 14, 22 } },
	}
	local Y = 4
	local X = 6
	local Column = 0
	for _, Spec in Specs do
		for _, Size in Spec[3] do
			local Label = Instance.new("TextLabel")
			Label.Name = `{Spec[1]}-{Spec[2]}-{Size}`
			Label.BackgroundTransparency = 1
			Label.TextColor3 = Color3.new(1, 1, 1)
			Label.FontFace = Font.new(`rbxasset://fonts/families/{Spec[1]}.json`, Enum.FontWeight[Spec[2]])
			Label.TextSize = Size
			Label.Text = "Hamburgefonstiv 0123 Glow"
			Label.TextXAlignment = Enum.TextXAlignment.Left
			Label.Size = UDim2.fromOffset(620, Size + 8)
			Label.Position = UDim2.fromOffset(X + Column * 640, Y)
			Label.Parent = Target
			Y += Size + 12
			if Y > 1000 then
				Y = 4
				Column += 1
			end
		end
	end
end
