-- Overlapping glyphs: are glyph quads blended one by one (double blend where two glyphs cover the same
-- pixel) and in which order? Lines overlapped with LineHeight 0.5, kerned / italic pairs whose bitmaps
-- overlap, rich colour runs on overlapping lines, and UIStroke bands of neighbouring glyphs.
-- rur compare --fixture fixtures/text-probe-overlap.luau --viewport 2060x1150 --time 0.2 --background 0,0,0
return function(Target)
	local Y = 4
	local function Label(Name, Props, Stroke)
		local L = Instance.new("TextLabel")
		L.Name = Name
		L.BorderSizePixel = 0
		L.BackgroundColor3 = Color3.fromRGB(0, 0, 0)
		L.BackgroundTransparency = 1
		L.TextColor3 = Color3.new(1, 1, 1)
		L.FontFace = Font.new("rbxasset://fonts/families/FredokaOne.json")
		L.TextSize = 40
		L.TextXAlignment = Enum.TextXAlignment.Left
		L.TextYAlignment = Enum.TextYAlignment.Top
		L.Size = UDim2.fromOffset(640, 100)
		L.Position = UDim2.fromOffset(4, Y)
		for Key, Value in Props do
			(L :: any)[Key] = Value
		end
		if Stroke then
			local S = Instance.new("UIStroke")
			for Key, Value in Stroke do
				(S :: any)[Key] = Value
			end
			S.Parent = L
		end
		L.Parent = Target
		return L
	end

	local Column = 0
	local function Next(Height)
		Y += Height
		if Y > 1040 then
			Y = 4
			Column += 1
		end
	end
	local function Place(L, Height)
		L.Position = UDim2.fromOffset(4 + Column * 680, Y)
		Next(Height)
	end

	-- Lines overlapping by half a line
	for _, Case in {
		{ "Lines0", { TextTransparency = 0 } },
		{ "Lines5", { TextTransparency = 0.5 } },
		{ "LinesRB", { TextTransparency = 0.5, TextColor3 = Color3.fromRGB(255, 40, 40), BackgroundTransparency = 0, BackgroundColor3 = Color3.fromRGB(30, 60, 200) } },
		{ "LinesRB0", { TextTransparency = 0, TextColor3 = Color3.fromRGB(255, 40, 40), BackgroundTransparency = 0, BackgroundColor3 = Color3.fromRGB(30, 60, 200) } },
	} do
		Place(Label(Case[1], { Text = "WAVE WAVE\nMWMW WAVE", LineHeight = 0.5, Size = UDim2.fromOffset(640, 70) }), 74)
		for Key, Value in Case[2] do
			(Target:FindFirstChild(Case[1]) :: any)[Key] = Value
		end
	end

	-- Rich colour runs on overlapping lines: which line is drawn on top
	Place(Label("RichLines", {
		RichText = true,
		Text = '<font color="#FF2020">WAVE WAVE</font>\n<font color="#2040FF">MWMW WAVE</font>',
		LineHeight = 0.5,
		Size = UDim2.fromOffset(640, 70),
	}), 74)
	Place(Label("RichLinesT", {
		RichText = true,
		Text = '<font color="#FF2020" transparency="0.4">WAVE WAVE</font>\n<font color="#2040FF" transparency="0.4">MWMW WAVE</font>',
		LineHeight = 0.5,
		Size = UDim2.fromOffset(640, 70),
	}), 74)

	-- Kerned pairs and italics whose bitmaps overlap
	Place(Label("KernT", { Text = "AVATAR WAVY Tofu LTYA", TextSize = 60, TextTransparency = 0.5, Size = UDim2.fromOffset(640, 70) }), 74)
	Place(Label("ItalicT", {
		Text = "ffff jjjj fjfj ((ff))",
		FontFace = Font.new("rbxasset://fonts/families/SourceSansPro.json", Enum.FontWeight.Regular, Enum.FontStyle.Italic),
		TextSize = 60,
		TextTransparency = 0.5,
		Size = UDim2.fromOffset(640, 70),
	}), 74)
	Place(Label("ItalicRB", {
		Text = "ffff jjjj fjfj ((ff))",
		FontFace = Font.new("rbxasset://fonts/families/SourceSansPro.json", Enum.FontWeight.Regular, Enum.FontStyle.Italic),
		TextSize = 60,
		TextColor3 = Color3.fromRGB(255, 200, 40),
		BackgroundTransparency = 0,
		BackgroundColor3 = Color3.fromRGB(40, 80, 160),
		Size = UDim2.fromOffset(640, 70),
	}), 74)

	-- Text strokes of neighbouring glyphs overlapping, stroke / fill transparency
	for _, Case in {
		{ "Stroke00", 0, 0 },
		{ "Stroke50", 0.5, 0 },
		{ "Stroke05", 0, 0.5 },
		{ "Stroke55", 0.5, 0.5 },
		{ "Stroke31", 0.3, 1 },
	} do
		Place(Label(Case[1], {
			Text = "WAVE mmm Oxo",
			TextSize = 48,
			TextColor3 = Color3.fromRGB(255, 220, 60),
			TextTransparency = Case[3],
			BackgroundTransparency = 0,
			BackgroundColor3 = Color3.fromRGB(40, 90, 150),
			Size = UDim2.fromOffset(640, 80),
		}, { Thickness = 4, Color = Color3.fromRGB(200, 30, 60), Transparency = Case[2] }), 84)
	end
	Place(Label("StrokeLines", {
		Text = "WAVE\nMWMW",
		LineHeight = 0.6,
		TextSize = 48,
		TextColor3 = Color3.fromRGB(255, 220, 60),
		BackgroundTransparency = 0,
		BackgroundColor3 = Color3.fromRGB(40, 90, 150),
		Size = UDim2.fromOffset(640, 90),
	}, { Thickness = 4, Color = Color3.fromRGB(200, 30, 60), Transparency = 0.4 }), 94)
end
