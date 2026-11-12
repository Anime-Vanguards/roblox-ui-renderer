-- Synthetic italic on scaled glyph quads (whole-pixel text mode, notes/text.md "Whole-pixel text"): faces without
-- an italic file, at text sizes that are not whole pixels at 1.5x (15 -> 22.5 px drawn as the 22 px glyph on a
-- quad 22.5 / 22 as large), as FontFace Italic, as rich <i>, with descenders, with a UIStroke and with the legacy
-- TextStroke. Glyphs that end above the baseline (quotes, "^", "*") lean the other way. The last two sizes are
-- whole pixels (unscaled quads, control).
-- rur compare --fixture fixtures/text-probe-synthscaled.luau --viewport 1200x900 --time 0.2 --background 0,0,0
return function(Target)
	local Families = { "FredokaOne", "Bangers", "LuckiestGuy" }
	local Sizes = { 15, 21, 35, 49, 16, 36 }
	local Y = 6
	for S, Size in Sizes do
		for F, Family in Families do
			for V = 1, 3 do
				local L = Instance.new("TextLabel")
				L.Name = string.format("S%dF%dV%d", Size, F, V)
				L.BackgroundColor3 = Color3.fromRGB(28, 30, 44)
				L.BorderSizePixel = 0
				L.TextColor3 = Color3.fromRGB(245, 245, 245)
				L.TextSize = Size
				L.TextXAlignment = Enum.TextXAlignment.Left
				L.Position = UDim2.fromOffset(6 + (V - 1) * 398, Y)
				L.Size = UDim2.fromOffset(392, math.ceil(Size * 1.3))
				if V == 1 then
					L.FontFace = Font.new("rbxasset://fonts/families/" .. Family .. ".json", Enum.FontWeight.Regular, Enum.FontStyle.Italic)
					L.Text = "Italic gjpqy Wolf 42 \"^'*~-"
				elseif V == 2 then
					L.FontFace = Font.new("rbxasset://fonts/families/" .. Family .. ".json")
					L.RichText = true
					L.Text = "up <i>Italic gjpqy</i> <b><i>Wolf</i></b>"
				else
					L.FontFace = Font.new("rbxasset://fonts/families/" .. Family .. ".json", Enum.FontWeight.Regular, Enum.FontStyle.Italic)
					L.Text = "Stroke gjpqy 42"
					if F % 2 == 1 then
						local Stroke = Instance.new("UIStroke")
						Stroke.Thickness = 1.5
						Stroke.Color = Color3.fromRGB(200, 40, 40)
						Stroke.Parent = L
					else
						L.TextStrokeTransparency = 0.2
						L.TextStrokeColor3 = Color3.fromRGB(40, 90, 200)
					end
				end
				L.Parent = Target
			end
			Y += math.ceil(Size * 1.3) + 4
		end
	end
end
