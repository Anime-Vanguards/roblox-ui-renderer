-- Rich text decorations at every TextSize from 8 to 60 (FredokaOne; text-probe-decor showed the <u> / <s> rects
-- depend on the size only): an underline under glyphs, then a strike and an underline over spaces only, so the
-- lines can be read without glyphs over them. White on the label's dark background.
-- rur compare --fixture fixtures/text-probe-decor2.luau --viewport 2060x1150 --time 0.2 --background 0,0,0
return function(Target)
	local Column, Y = 0, 4
	for Size = 8, 60 do
		local Height = math.ceil(Size * 1.2) + 2
		if Y + Height > 1140 then
			Y = 4
			Column += 1
		end
		local L = Instance.new("TextLabel")
		L.Name = `Decor{Size}`
		L.BorderSizePixel = 0
		L.BackgroundColor3 = Color3.fromRGB(20, 24, 40)
		L.TextColor3 = Color3.fromRGB(255, 255, 255)
		L.FontFace = Font.new("rbxasset://fonts/families/FredokaOne.json")
		L.RichText = true
		L.TextSize = Size
		L.TextXAlignment = Enum.TextXAlignment.Left
		L.TextYAlignment = Enum.TextYAlignment.Top
		L.Text = "<u>ab</u> <s>      </s> <u>      </u>"
		L.Size = UDim2.fromOffset(500, Height)
		L.Position = UDim2.fromOffset(4 + Column * 510, Y)
		L.Parent = Target
		Y += Height + 3
	end
end
