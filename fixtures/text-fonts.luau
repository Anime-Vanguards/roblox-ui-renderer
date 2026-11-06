-- Legacy Enum.Font families and weights (white on black), one label each.
return function(Target)
	local Fonts = {
		"Gotham", "GothamBold", "GothamMedium", "GothamBlack", "GothamSemibold",
		"SourceSans", "SourceSansBold", "SourceSansSemibold", "SourceSansLight", "SourceSansItalic",
		"Arial", "ArialBold", "Legacy", "FredokaOne", "Code", "Roboto", "RobotoMono", "Ubuntu",
		"Arcade", "Cartoon", "Bangers", "LuckiestGuy", "Oswald", "Nunito", "Highway", "SciFi",
		"Fantasy", "Antique", "PermanentMarker", "DenkOne", "Creepster", "Sarpanch", "Kalam", "Michroma",
		"TitilliumWeb", "JosefinSans", "AmaticSC", "Bodoni", "Garamond", "IndieFlower", "PatrickHand",
		"SpecialElite", "Zekton", "BuilderSans", "BuilderSansBold", "BuilderSansExtraBold", "Montserrat", "MontserratBold",
	}
	for Index, Name in Fonts do
		local Ok, Font = pcall(function()
			return (Enum.Font :: any)[Name]
		end)
		if not Ok then
			continue
		end
		local Label = Instance.new("TextLabel")
		Label.Name = Name
		Label.BackgroundTransparency = 1
		Label.TextColor3 = Color3.new(1, 1, 1)
		Label.Font = Font
		Label.TextSize = 18
		Label.Text = Name .. " Hamburgefonstiv 0123"
		Label.TextXAlignment = Enum.TextXAlignment.Left
		Label.Size = UDim2.fromOffset(300, 24)
		Label.Position = UDim2.fromOffset(10 + ((Index - 1) % 3) * 320, 8 + ((Index - 1) // 3) * 28)
		Label.Parent = Target
	end
end
