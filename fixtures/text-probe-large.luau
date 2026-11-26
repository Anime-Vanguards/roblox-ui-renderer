-- "Hg" at TextSize 60..72 in a plain Frame, an unturned CanvasGroup and a CanvasGroup turned 180 degrees (whole
-- device pixels, even device sizes). Open: from TextSize 68 (102 px at 1.5x) the client's glyphs differ from the
-- renderer's by about a column, in all three alike (notes/text.md "Text in a turned CanvasGroup", the residue).
-- rur compare --fixture fixtures/text-probe-large.luau --viewport 1600x900 --time 0.2 --background 0,0,0
return function(Target)
	for Column, Spec in { { "Frame", 0 }, { "CanvasGroup", 0 }, { "CanvasGroup", 180 } } do
		local G = Instance.new(Spec[1])
		G.Name = string.format("%s%d", Spec[1], Spec[2])
		G.Size = UDim2.fromOffset(200, 580)
		G.Position = UDim2.fromOffset(4 + (Column - 1) * 240, 4)
		G.Rotation = Spec[2]
		G.BackgroundColor3 = Color3.fromRGB(30, 60, 110)
		G.BorderSizePixel = 0
		G.Parent = Target
		local Y = 4
		for Size = 60, 72, 2 do
			local L = Instance.new("TextLabel")
			L.Name = string.format("S%d", Size)
			L.BackgroundTransparency = 1
			L.TextColor3 = Color3.fromRGB(255, 255, 255)
			L.Text = "Hg"
			L.TextSize = Size
			L.TextXAlignment = Enum.TextXAlignment.Left
			L.Position = UDim2.fromOffset(8, Y)
			L.Size = UDim2.fromOffset(180, Size + 4)
			L.Parent = G
			Y += Size + 6
		end
	end
end
