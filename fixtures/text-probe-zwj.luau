-- A zero-width joiner (U+200D) between, after and before letters, and other default ignorables, in six faces at
-- TextSize 14, 20 and 32: "A<ZWJ>B" against "AB", "A<ZWJ>", "<ZWJ>B", "fi" split by a ZWJ (a ligature it may block),
-- "A<ZWNJ>B" and "A<WJ>B". Labels are left aligned on a dim background (TextBounds in studio-nodes.json).
-- rur compare --fixture fixtures/text-probe-zwj.luau --viewport 800x700 --time 0.2 --background 0,0,0
return function(Target)
	local Families = { "SourceSansPro", "FredokaOne", "GothamSSm", "Arimo", "Bangers", "Ubuntu" }
	local Texts = {
		{ "AzB", "A\u{200D}B" },
		{ "AB", "AB" },
		{ "Az", "A\u{200D}" },
		{ "zB", "\u{200D}B" },
		{ "fzi", "f\u{200D}i" },
		{ "fi", "fi" },
		{ "AnB", "A\u{200C}B" },
		{ "AwB", "A\u{2060}B" },
		{ "xzyz", "x\u{200D}y\u{200D}" },
	}
	local Sizes = { 14, 20, 32 }
	local Y = 4
	for F, Family in Families do
		local X = 4
		for S, Size in Sizes do
			for T, Case in Texts do
				local L = Instance.new("TextLabel")
				L.Name = string.format("F%dS%d_%s", F, Size, Case[1])
				L.BackgroundColor3 = Color3.fromRGB(40, 44, 64)
				L.BorderSizePixel = 0
				L.TextColor3 = Color3.fromRGB(255, 255, 255)
				L.FontFace = Font.new("rbxasset://fonts/families/" .. Family .. ".json")
				L.TextSize = Size
				L.Text = Case[2]
				L.TextXAlignment = Enum.TextXAlignment.Left
				L.Position = UDim2.fromOffset(X, Y)
				L.Size = UDim2.fromOffset(Size * 2.4, math.ceil(Size * 1.4))
				L.Parent = Target
				X += math.ceil(Size * 2.4) + 4
			end
			if S == 1 then
				X = 4
				Y += 26
			elseif S == 2 then
				X = 4
				Y += 32
			end
		end
		Y += 50
	end
end
