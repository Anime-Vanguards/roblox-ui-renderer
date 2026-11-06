-- Text compositing probe: the same glyphs ("@&W", FredokaOne 30) in many text colours over many label
-- background colours, at TextTransparency 0 .. 0.98 (one value per row). Colours are pseudo-random per
-- cell (seeded), except the first columns: white on black (coverage reference), black on white and a few
-- saturated pairs. tools/analysis/text_blend.py fits Studio's blend of (text colour, background,
-- coverage, transparency).
-- rur compare --fixture fixtures/text-probe-blend.luau --viewport 2060x1150 --time 0.2 --background 0,0,0
return function(Target)
	local Transparencies = { 0, 0, 0, 0, 0.1, 0.2, 0.25, 0.3, 0.4, 0.5, 0.5, 0.6, 0.7, 0.75, 0.8, 0.9, 0.95, 0.98 }
	local Fixed = {
		{ Color3.fromRGB(255, 255, 255), Color3.fromRGB(0, 0, 0) },
		{ Color3.fromRGB(0, 0, 0), Color3.fromRGB(255, 255, 255) },
		{ Color3.fromRGB(255, 0, 128), Color3.fromRGB(0, 255, 128) },
		{ Color3.fromRGB(1, 254, 127), Color3.fromRGB(254, 1, 128) },
	}

	-- Park-Miller (exact in doubles)
	local Seed = 12345
	local function Random255()
		Seed = (Seed * 16807) % 2147483647
		return Seed % 256
	end

	for Row, Transparency in Transparencies do
		for Column = 0, 15 do
			local TextColor, Background
			if Column < #Fixed then
				TextColor, Background = Fixed[Column + 1][1], Fixed[Column + 1][2]
			else
				TextColor = Color3.fromRGB(Random255(), Random255(), Random255())
				Background = Color3.fromRGB(Random255(), Random255(), Random255())
			end
			local Label = Instance.new("TextLabel")
			Label.Name = `B{Row - 1}_{Column}`
			Label.BorderSizePixel = 0
			Label.BackgroundColor3 = Background
			Label.TextColor3 = TextColor
			Label.TextTransparency = Transparency
			Label.FontFace = Font.new("rbxasset://fonts/families/FredokaOne.json")
			Label.TextSize = 30
			Label.Text = "@&W"
			Label.TextXAlignment = Enum.TextXAlignment.Left
			Label.TextYAlignment = Enum.TextYAlignment.Top
			Label.Size = UDim2.fromOffset(126, 60)
			Label.Position = UDim2.fromOffset(4 + Column * 128, 4 + (Row - 1) * 62)
			Label.Parent = Target
		end
	end
end
