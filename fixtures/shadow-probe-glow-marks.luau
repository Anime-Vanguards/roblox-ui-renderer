-- UIShadow probe: the glow of Mode = Text around rich-text underlines and strikethroughs, and around colour
-- emoji. Red shadows of blue text on black; the underlines run under a wide gap so their glow is apart from the
-- glyphs'. Clear text (TextTransparency 1) unless a cell says otherwise.
-- compare --viewport 1200x900 --time 0.3 --background 0,0,0
return function(Target)
	local Fredoka = Font.new("rbxasset://fonts/families/FredokaOne.json")
	local Gap = "i" .. string.rep(" ", 14) .. "i"
	local function Label(X, Y, W, H, Text, Props, ShadowProps)
		local L = Instance.new("TextLabel")
		L.Position = UDim2.fromOffset(X, Y)
		L.Size = UDim2.fromOffset(W, H)
		L.BackgroundTransparency = 1
		L.BorderSizePixel = 0
		L.RichText = true
		L.Text = Text
		L.TextColor3 = Color3.new(0, 0, 1)
		L.TextTransparency = 1
		L.FontFace = Fredoka
		L.TextSize = 40
		for Key, Value in Props or {} do
			(L :: any)[Key] = Value
		end
		local S = Instance.new("UIShadow")
		S.Mode = Enum.ApplyShadowMode.Text
		S.Color = Color3.new(1, 0, 0)
		S.BlurRadius = UDim.new(0, 8)
		for Key, Value in ShadowProps or {} do
			(S :: any)[Key] = Value
		end
		S.Parent = L
		L.Parent = Target
		return L
	end

	-- A: an underline under a gap, BlurRadius 2, 4, 8, 16
	for Index, Blur in { 2, 4, 8, 16 } do
		Label(10 + (Index - 1) * 295, 10, 280, 80, "<u>" .. Gap .. "</u>", nil, { BlurRadius = UDim.new(0, Blur) })
	end
	-- B: a strikethrough the same way
	for Index, Blur in { 2, 4, 8, 16 } do
		Label(10 + (Index - 1) * 295, 110, 280, 80, "<s>" .. Gap .. "</s>", nil, { BlurRadius = UDim.new(0, Blur) })
	end
	-- C: underlines at TextSize 20 and 70, a half transparent shadow, the text shown, an underlined word
	Label(10, 210, 280, 80, "<u>" .. Gap .. "</u>", { TextSize = 20 })
	Label(305, 210, 400, 100, "<u>" .. Gap .. "</u>", { TextSize = 70 })
	Label(720, 210, 230, 80, "<u>" .. Gap .. "</u>", { TextSize = 30 }, { Transparency = 0.5 })
	Label(965, 210, 225, 80, "<u>Glow</u> it", { TextTransparency = 0 })
	-- D: colour emoji alone, BlurRadius 4, 8, 16 at TextSize 40, and at 80
	for Index, Blur in { 4, 8, 16 } do
		Label(10 + (Index - 1) * 160, 330, 150, 110, "\u{1F600}", nil, { BlurRadius = UDim.new(0, Blur) })
	end
	Label(490, 330, 180, 130, "\u{1F600}", { TextSize = 80 })
	Label(690, 330, 150, 110, "\u{1F525}")
	Label(860, 330, 150, 110, "\u{2764}\u{FE0F}")
	Label(1030, 330, 160, 110, "\u{1F600}", { TextTransparency = 0 })
	-- E: an emoji among letters, shown and clear, under a blue shadow, half transparent, plain (not rich) text
	Label(10, 470, 220, 100, "a\u{1F600}b", { TextTransparency = 0, RichText = false })
	Label(250, 470, 220, 100, "a\u{1F600}b", { RichText = false })
	Label(490, 470, 220, 100, "\u{1F525}\u{1F600}", { RichText = false }, { Color = Color3.fromRGB(0, 120, 255) })
	Label(730, 470, 220, 100, "\u{1F600}", { RichText = false }, { Transparency = 0.5 })
	Label(970, 470, 220, 100, "\u{1F600}x", { RichText = false, TextSize = 24 })
	-- F: a strikethrough over shown text, a mark (highlight) under it
	Label(10, 600, 280, 80, "<s>Glow</s>", { TextTransparency = 0 })
	Label(305, 600, 280, 80, '<mark color="#00ff00">Glow</mark>', { TextTransparency = 0 }) -- the glow lies over the mark
	-- H: lines whose thickness has a fraction of a device pixel at 1.5x (TextSize 20 and 50)
	Label(600, 600, 280, 70, "<s>" .. Gap .. "</s>", { TextSize = 20 })
	Label(895, 600, 295, 90, "<s>" .. Gap .. "</s>", { TextSize = 50 })
	Label(10, 700, 300, 90, "<u>" .. Gap .. "</u>", { TextSize = 50 })
	Label(320, 700, 400, 90, "<u>i</u> <s>i</s> <u><s>both</s></u>", { TextSize = 50 }, { BlurRadius = UDim.new(0, 5) })
	-- G: for reference, clear bars as tall as the TextSize 40 underline (3 device px) with an outer UIShadow,
	-- BlurRadius 2, 4, 8, 16
	for Index, Blur in { 2, 4, 8, 16 } do
		local Bar = Instance.new("Frame")
		Bar.Position = UDim2.fromOffset(40 + (Index - 1) * 295, 840)
		Bar.Size = UDim2.fromOffset(220, 2)
		Bar.BackgroundTransparency = 1
		Bar.BorderSizePixel = 0
		local S = Instance.new("UIShadow")
		S.Color = Color3.new(1, 0, 0)
		S.BlurRadius = UDim.new(0, Blur)
		S.Parent = Bar
		Bar.Parent = Target
	end
end
