-- Text inside resampled CanvasGroups (Round 8, notes/text.md): a group whose size is not a whole number of
-- device px at 1.5x (odd logical sizes, fractional positions) renders into an N x N texture, N = ceil(ceil(Q /
-- DPI) * DPI) != Q, drawn back over Q px. Each group holds labels of several kinds: plain whole-size text,
-- odd TextSize (fractional quad scale 16.5 / 16), TextScaled, a UIStroke, a UIGradient, rich <i> / <b> /
-- <font size>, the legacy TextStroke. Row 0: groups at whole logical sizes and positions (N = Q, control).
-- rur compare --fixture fixtures/text-probe-grouptext.luau --viewport 1200x900 --time 0.2 --background 0,0,0
return function(Target)
	local Kinds = {
		{ Text = "Select an emote to play...", Size = 14 },
		{ Text = "Select an emote to play...", Size = 11 },
		{ Text = "Scaled text fits", Size = 11, Scaled = true },
		{ Text = "Stroke 1.5 odd", Size = 13, Stroke = 1.5 },
		{ Text = "Gradient label", Size = 16, Gradient = true },
		{ Text = "<i>Italic</i> <b>Bold</b> <font size=\"19\">big</font>", Size = 15, Rich = true },
		{ Text = "Legacy stroke", Size = 17, Legacy = true },
		{ Text = "Waxo 42 wiggle", Size = 21, Stroke = 1 },
	}
	local Groups = {
		{ 8, 8, 280, 200 },
		{ 300, 8, 280, 200 },
		{ 592, 8, 280, 200 },
		{ 8.336, 220.72, 162.8, 198.82 },
		{ 180.5, 220.3, 201, 199 },
		{ 392.17, 221.9, 233.3, 203.7 },
		{ 640.9, 220.45, 277, 211 },
		{ 8.61, 450.25, 279.4, 201.1 },
		{ 300.2, 450.8, 281, 205.3 },
		{ 594.45, 451.1, 283.7, 199.9 },
		{ 8.1, 670.63, 187.33, 205 },
		{ 206.7, 670.2, 259, 207.5 },
		{ 480.3, 671.3, 301.1, 199.7 },
		{ 795.6, 670.9, 245.4, 201.3 },
	}
	for G, Box in Groups do
		local Group = Instance.new("CanvasGroup")
		Group.Name = string.format("G%02d", G)
		Group.Position = UDim2.fromOffset(Box[1], Box[2])
		Group.Size = UDim2.fromOffset(Box[3], Box[4])
		Group.BackgroundColor3 = Color3.fromRGB(70 + G * 7, 24, 60 + G * 5)
		Group.BorderSizePixel = 0
		Group.Parent = Target
		for K, Kind in Kinds do
			local L = Instance.new("TextLabel")
			L.Name = string.format("K%d", K)
			L.BackgroundTransparency = 1
			L.TextColor3 = Color3.fromRGB(245, 245, 245)
			L.FontFace = Font.new("rbxasset://fonts/families/FredokaOne.json")
			L.TextSize = Kind.Size
			L.Text = Kind.Text
			L.RichText = Kind.Rich == true
			L.TextScaled = Kind.Scaled == true
			L.TextWrapped = Kind.Scaled == true
			L.Position = UDim2.new(0, 4 + (G % 3) * 0.37, 0, 3 + (K - 1) * 24 + (G % 4) * 0.29)
			L.Size = UDim2.new(1, -8, 0, 22)
			if Kind.Stroke then
				local Stroke = Instance.new("UIStroke")
				Stroke.Thickness = Kind.Stroke
				Stroke.Color = Color3.fromRGB(0, 0, 0)
				Stroke.Parent = L
			end
			if Kind.Gradient then
				local Gradient = Instance.new("UIGradient")
				Gradient.Color = ColorSequence.new(Color3.fromRGB(255, 200, 60), Color3.fromRGB(80, 200, 255))
				Gradient.Parent = L
			end
			if Kind.Legacy then
				L.TextStrokeTransparency = 0.2
				L.TextStrokeColor3 = Color3.fromRGB(10, 10, 30)
			end
			L.Parent = Group
		end
	end
end
