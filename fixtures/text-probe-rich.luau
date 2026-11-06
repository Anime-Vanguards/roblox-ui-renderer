-- Legacy TextStroke and rich text styling: TextStrokeTransparency / TextStrokeColor3 with and without
-- TextTransparency, <stroke>, <u>, <s>, <mark>, <b> / <i> on faces that have them (SourceSans) and on
-- one that does not (FredokaOne: synthetic bold / italic), <font color / transparency / size / weight>.
-- rur compare --fixture fixtures/text-probe-rich.luau --viewport 2060x1150 --time 0.2 --background 0,0,0
return function(Target)
	local Column, Y = 0, 4
	local function Label(Name, Props, Height)
		Height = Height or 56
		if Y + Height > 1140 then
			Y = 4
			Column += 1
		end
		local L = Instance.new("TextLabel")
		L.Name = Name
		L.BorderSizePixel = 0
		L.BackgroundColor3 = Color3.fromRGB(30, 60, 110)
		L.BackgroundTransparency = 0
		L.TextColor3 = Color3.fromRGB(255, 230, 120)
		L.FontFace = Font.new("rbxasset://fonts/families/FredokaOne.json")
		L.TextSize = 36
		L.TextXAlignment = Enum.TextXAlignment.Left
		L.TextYAlignment = Enum.TextYAlignment.Top
		L.Size = UDim2.fromOffset(660, Height)
		L.Position = UDim2.fromOffset(4 + Column * 680, Y)
		for Key, Value in Props do
			(L :: any)[Key] = Value
		end
		L.Parent = Target
		Y += Height + 4
		return L
	end

	-- Legacy TextStroke
	Label("Legacy0", { Text = "WAVE Oxo 18", TextStrokeTransparency = 0, TextStrokeColor3 = Color3.fromRGB(200, 20, 60) })
	Label("Legacy5", { Text = "WAVE Oxo 18", TextStrokeTransparency = 0.5, TextStrokeColor3 = Color3.fromRGB(200, 20, 60) })
	Label("LegacyT", { Text = "WAVE Oxo 18", TextStrokeTransparency = 0.2, TextTransparency = 0.4, TextStrokeColor3 = Color3.fromRGB(200, 20, 60) })
	Label("LegacySmall", { Text = "Small legacy stroke text 123", TextSize = 14, TextStrokeTransparency = 0, TextStrokeColor3 = Color3.fromRGB(0, 0, 0) }, 24)
	Label("LegacySS", { Text = "SourceSans stroke", FontFace = Font.new("rbxasset://fonts/families/SourceSansPro.json"), TextStrokeTransparency = 0, TextStrokeColor3 = Color3.fromRGB(0, 0, 0) })

	-- Rich text runs
	Label("RichColor", { RichText = true, Text = 'Plain <font color="#FF4040">red</font> <font color="#40FF80" transparency="0.5">half</font> end' })
	Label("RichStroke", { RichText = true, Text = 'A <stroke color="#C01040" thickness="3">stroked</stroke> B <stroke color="#000000" thickness="2" transparency="0.5">half</stroke>' })
	Label("RichStrokeJ", { RichText = true, Text = '<stroke color="#C01040" joins="miter" thickness="4">MITER</stroke> <stroke color="#C01040" joins="bevel" thickness="4">BEVEL</stroke>' })
	Label("RichUS", { RichText = true, Text = 'Under <u>line</u> and <s>strike</s> <u><s>both</s></u>' })
	Label("RichMark", { RichText = true, Text = 'Mark <mark color="#E0C040">this</mark> and <mark color="#40C0E0" transparency="0.5">half</mark>' })
	Label("RichBoldFO", { RichText = true, Text = 'Fredoka <b>bold</b> and <i>italic</i> <b><i>both</i></b>' })
	Label("RichBoldSS", {
		RichText = true,
		FontFace = Font.new("rbxasset://fonts/families/SourceSansPro.json"),
		Text = 'SourceSans <b>bold</b> and <i>italic</i> <b><i>both</i></b>',
	})
	Label("RichWeight", { RichText = true, FontFace = Font.new("rbxasset://fonts/families/GothamSSm.json"), Text = 'Gotham <font weight="heavy">heavy</font> <font weight="light">light</font> <b>bold</b>' })
	Label("RichSize", { RichText = true, Text = 'Size <font size="20">small</font> <font size="48">big</font> end' }, 70)
	Label("RichUIStroke", { RichText = true, Text = 'UIStroke + <font color="#FF4040">red run</font> + <stroke color="#40FF40" thickness="2">tag</stroke>' }).Name = "RichUIStroke"
	local S = Instance.new("UIStroke")
	S.Thickness = 3
	S.Color = Color3.fromRGB(0, 0, 0)
	S.Parent = Target:FindFirstChild("RichUIStroke")
end
