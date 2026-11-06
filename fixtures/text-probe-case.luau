-- Rich text <uc> / <uppercase> and <sc> / <smallcaps>: fonts with and without a small-caps feature, mixed
-- with bold / size / colour runs, centred and AutomaticSize X (their widths change with the case), accented
-- and non-letter characters.
-- rur compare --fixture fixtures/text-probe-case.luau --viewport 1200x900 --time 0.2 --background 20,20,26
return function(Target)
	local Fonts = {
		"rbxasset://fonts/families/SourceSansPro.json",
		"rbxasset://fonts/families/GothamSSm.json",
		"rbxasset://fonts/families/FredokaOne.json",
		"rbxasset://fonts/families/Roboto.json",
		"rbxasset://fonts/families/Montserrat.json",
	}
	local Texts = {
		"Plain <uc>upper case</uc> text",
		"Plain <sc>Small Caps</sc> text",
		"<uppercase>ÀÉÎ ßx 12!?</uppercase> <smallcaps>ßàé Qq</smallcaps>",
		"<uc><b>Bold</b> <font size=\"30\">big</font> <font color=\"#ff8040\">hot</font></uc>",
		"<sc><b>Bold</b> <font size=\"30\">Big</font> <i>ital</i></sc>",
	}
	for FontIndex, Family in Fonts do
		for TextIndex, Text in Texts do
			local L = Instance.new("TextLabel")
			L.Name = `F{FontIndex}T{TextIndex}`
			L.BorderSizePixel = 0
			L.BackgroundColor3 = Color3.fromRGB(34, 40, 60)
			L.TextColor3 = Color3.fromRGB(255, 255, 255)
			L.FontFace = Font.new(Family)
			L.RichText = true
			L.TextSize = 20
			L.Text = Text
			L.Size = UDim2.fromOffset(0, 34)
			L.AutomaticSize = Enum.AutomaticSize.X
			L.Position = UDim2.fromOffset(6 + (TextIndex - 1) * 238, 6 + (FontIndex - 1) * 84)
			L.Parent = Target
			-- The same text centred in a fixed box
			local C = L:Clone()
			C.Name ..= "Center"
			C.AutomaticSize = Enum.AutomaticSize.None
			C.Size = UDim2.fromOffset(230, 34)
			C.Position = UDim2.fromOffset(6 + (TextIndex - 1) * 238, 44 + (FontIndex - 1) * 84)
			C.Parent = Target
		end
	end
end
