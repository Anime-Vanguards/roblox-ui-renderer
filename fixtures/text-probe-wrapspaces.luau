-- Wrapped text with runs of spaces at the breaks (data/studio/wrap-1.5x.json: "a  b   c    d" in a 30 px box measures
-- 14 x 60 device px in the quarter-pixel client and 45 x 60 with whole pixels). The text in boxes 10 .. 60 px wide,
-- SourceSansPro 20 and FredokaOne 14, left aligned, and the same texts with single spaces for reference; each box
-- has a background so the lines show where the client breaks them. Captured flag off and flag on.
-- rur compare --fixture fixtures/text-probe-wrapspaces.luau --viewport 1000x760 --time 0.2 --background 0,0,0
return function(Target)
	local Faces = {
		{ "Source", Font.new("rbxasset://fonts/families/SourceSansPro.json"), 20 },
		{ "Fredoka", Font.new("rbxasset://fonts/families/FredokaOne.json"), 14 },
	}
	local Texts = { "a  b   c    d", "a b c d", "ab  cd", "a   bcd   e" }
	local Widths = { 10, 20, 30, 40, 50, 60 }
	for FaceIndex, Face in Faces do
		for TextIndex, Text in Texts do
			for WidthIndex, Width in Widths do
				local L = Instance.new("TextLabel")
				L.Name = string.format("%s_t%d_w%d", Face[1], TextIndex, Width)
				L.BackgroundColor3 = Color3.fromRGB(40, 40, 60)
				L.BorderSizePixel = 0
				L.TextColor3 = Color3.fromRGB(255, 255, 255)
				L.FontFace = Face[2]
				L.TextSize = Face[3]
				L.Text = Text
				L.TextWrapped = true
				L.TextXAlignment = Enum.TextXAlignment.Left
				L.TextYAlignment = Enum.TextYAlignment.Top
				L.Position = UDim2.fromOffset(6 + (WidthIndex - 1) * 80 + (FaceIndex - 1) * 490, 6 + (TextIndex - 1) * 185)
				L.Size = UDim2.fromOffset(Width, 175)
				L.Parent = Target
			end
		end
	end
end
