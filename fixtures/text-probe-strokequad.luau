-- Stroke band quads of single glyphs (Round 8, notes/text.md): a list row's "VIEW" (FredokaOne 25 px, UIStroke
-- 0.75 -> radius 1.1 px) draws the "W" band (25 px bitmap, 27 px quad with its padding) over a 28 px wide quad.
-- One glyph per label, far apart, so no neighbour hides a band edge: letters, digits and punctuation of
-- FredokaOne / GothamSSm / SourceSansPro at several sizes and thin / thick strokes, white on a flat dark ground.
-- rur compare --fixture fixtures/text-probe-strokequad.luau --viewport 1200x900 --time 0.2 --background 0,0,0
return function(Target)
	local Chars = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789&%?@#"
	local Configs = {
		{ Font = "rbxasset://fonts/families/FredokaOne.json", Size = 16.666666666666668, Stroke = 0.75 },
		{ Font = "rbxasset://fonts/families/FredokaOne.json", Size = 20, Stroke = 1.5 },
		{ Font = "rbxasset://fonts/families/GothamSSm.json", Size = 18, Stroke = 1 },
		{ Font = "rbxasset://fonts/families/SourceSansPro.json", Size = 24, Stroke = 2 },
	}
	local Ground = Instance.new("Frame")
	Ground.Name = "Ground"
	Ground.BorderSizePixel = 0
	Ground.BackgroundColor3 = Color3.fromRGB(19, 19, 19)
	Ground.Size = UDim2.fromScale(1, 1)
	Ground.Parent = Target
	local Index = 0
	for C, Config in Configs do
		for I = 1, #Chars do
			local Column = Index % 30
			local Row = Index // 30
			Index += 1
			local L = Instance.new("TextLabel")
			L.Name = string.format("C%dG%02d", C, I)
			L.BackgroundTransparency = 1
			L.TextColor3 = Color3.fromRGB(255, 255, 255)
			L.FontFace = Font.new(Config.Font)
			L.TextSize = Config.Size
			L.Text = string.sub(Chars, I, I)
			L.Position = UDim2.fromOffset(4 + Column * 39.5, 4 + Row * 38)
			L.Size = UDim2.fromOffset(36, 34)
			L.Parent = Ground
			local Stroke = Instance.new("UIStroke")
			Stroke.Thickness = Config.Stroke
			Stroke.Color = Color3.fromRGB(0, 0, 0)
			Stroke.Parent = L
		end
	end
end
