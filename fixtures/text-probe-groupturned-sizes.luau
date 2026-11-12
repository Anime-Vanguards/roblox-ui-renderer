-- Text in turned CanvasGroups by size (notes/text.md "Text in a turned CanvasGroup"): "Hg" at every TextSize from
-- 14 to 70 by 2 in three groups turned 20 degrees and the same labels in three unturned groups (control), so a
-- glyph's offset in the group's texture can be read per size. Group positions are whole device pixels.
-- rur compare --fixture fixtures/text-probe-groupturned-sizes.luau --viewport 1600x900 --time 0.2 --background 0,0,0
return function(Target)
	local Sizes = {}
	for Size = 14, 70, 2 do
		table.insert(Sizes, Size)
	end
	for Column = 1, 3 do
		for Turned = 0, 1 do
			local G = Instance.new("CanvasGroup")
			G.Name = string.format("G%d_%d", Column, Turned)
			G.Size = UDim2.fromOffset(150, 520)
			G.AnchorPoint = Vector2.new(0.5, 0.5)
			G.Position = UDim2.fromOffset(if Turned == 0 then 90 + (Column - 1) * 170 else 700 + (Column - 1) * 330, 450)
			G.Rotation = Turned * 20
			G.BackgroundColor3 = Color3.fromRGB(30, 60, 110)
			G.BorderSizePixel = 0
			G.Parent = Target
			local Y = 4
			for Index = Column, #Sizes, 3 do
				local Size = Sizes[Index]
				local L = Instance.new("TextLabel")
				L.Name = string.format("S%d", Size)
				L.BackgroundTransparency = 1
				L.TextColor3 = Color3.fromRGB(255, 255, 255)
				L.Text = "Hg"
				L.TextSize = Size
				L.TextXAlignment = Enum.TextXAlignment.Left
				L.Position = UDim2.fromOffset(8, Y)
				L.Size = UDim2.fromOffset(136, Size + 4)
				L.Parent = G
				Y += Size + 6
			end
		end
	end
end
