-- Colour emoji (Twemoji / RobloxEmoji COLR glyphs) in text: do they keep their own colours under TextColor3,
-- TextTransparency, a UIGradient, rich-text colour and a legacy TextStroke; what does a UIStroke outline;
-- how wide are a variation selector (U+FE0F) and a ZWJ sequence; synthetic bold, small sizes, rotation.
-- rur compare --fixture fixtures/text-probe-emoji.luau --viewport 1318x656 --time 0.2 --background 20,20,26
return function(Target)
	local Column, Y = 0, 2
	local function Label(Name, Props)
		if Y + 44 > 235 then
			Y = 2
			Column += 1
		end
		local L = Instance.new("TextLabel")
		L.Name = Name
		L.BorderSizePixel = 0
		L.BackgroundColor3 = Color3.fromRGB(40, 44, 56)
		L.TextColor3 = Color3.fromRGB(255, 255, 255)
		L.FontFace = Font.new("rbxasset://fonts/families/FredokaOne.json")
		L.TextSize = 32
		L.TextXAlignment = Enum.TextXAlignment.Left
		L.Size = UDim2.fromOffset(256, 42)
		L.Position = UDim2.fromOffset(2 + Column * 262, Y)
		for Key, Value in Props do
			(L :: any)[Key] = Value
		end
		L.Parent = Target
		Y += 46
		return L
	end
	local Emoji = "\u{1F3DC}\u{1F45F}\u{1F600}"

	Label("Plain", { Text = "Ab " .. Emoji })
	Label("Red", { Text = "Ab " .. Emoji, TextColor3 = Color3.fromRGB(255, 0, 0) })
	Label("Black", { Text = "Ab " .. Emoji, TextColor3 = Color3.fromRGB(0, 0, 0) })
	Label("Half", { Text = "Ab " .. Emoji, TextTransparency = 0.5 })
	Label("Clear", { Text = "Ab " .. Emoji, TextTransparency = 1 })

	local Stroked = Label("Stroke", { Text = "Ab " .. Emoji })
	local S = Instance.new("UIStroke")
	S.Thickness = 3
	S.Color = Color3.fromRGB(40, 90, 255)
	S.Parent = Stroked
	local G = Label("Gradient", { Text = "Ab " .. Emoji })
	local Gradient = Instance.new("UIGradient")
	Gradient.Color = ColorSequence.new(Color3.fromRGB(255, 40, 40), Color3.fromRGB(40, 255, 40))
	Gradient.Parent = G
	local GT = Label("GradientFade", { Text = "Ab " .. Emoji })
	local Fade = Instance.new("UIGradient")
	Fade.Transparency = NumberSequence.new(0, 1)
	Fade.Parent = GT
	Label("Legacy", { Text = "Ab " .. Emoji, TextStrokeTransparency = 0, TextStrokeColor3 = Color3.fromRGB(40, 90, 255) })
	Label("Rich", { Text = 'Ab <font color="#00FF00">\u{1F600}</font><font transparency="0.5">\u{1F600}</font>', RichText = true })

	Label("Selector", { Text = "A\u{1F3DC}\u{FE0F}B" })
	Label("NoSelector", { Text = "A\u{1F3DC}B" })
	Label("Zwj", { Text = "A\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F466}B" })
	Label("ZwjCouple", { Text = "A\u{1F468}\u{200D}\u{2764}\u{FE0F}\u{200D}\u{1F468}B" })
	Label("Robux", { Text = "A\u{E002}\u{E000}B" })

	Label("Bold", { Text = "Ab " .. Emoji, FontFace = Font.new("rbxasset://fonts/families/SourceSansPro.json", Enum.FontWeight.Bold) })
	Label("Small", { Text = "Ab " .. Emoji .. " ok", TextSize = 14 })
	Label("Center", { Text = Emoji, TextXAlignment = Enum.TextXAlignment.Center })
	Label("Rotated", { Text = "Ab " .. Emoji, Rotation = 12 })
	Label("Scaled", { Text = "Ab " .. Emoji, TextScaled = true })
end
