-- Text in turned CanvasGroups by the group's position (notes/text.md "Text in a turned CanvasGroup"): the same
-- group (turned 20 degrees: "AB" at 60, "AB" at 17, a white bar) at sixteen positions a quarter of a device pixel
-- apart in x and in y. If the glyphs' offset in the texture follows the fraction of the group's position, it
-- changes along the rows; if it belongs to the text, it does not.
-- rur compare --fixture fixtures/text-probe-groupturned-pos.luau --viewport 1200x900 --time 0.2 --background 0,0,0
return function(Target)
	for I = 0, 15 do
		local Fx, Fy = (I % 4) / 6, (I // 4) / 6 -- a quarter device px at 1.5x is 1/6 logical px
		local G = Instance.new("CanvasGroup")
		G.Name = string.format("P%d_%d", I % 4, I // 4)
		G.Size = UDim2.fromOffset(150, 120)
		G.AnchorPoint = Vector2.new(0.5, 0.5)
		G.Position = UDim2.fromOffset(100 + (I % 4) * 200 + Fx, 100 + (I // 4) * 190 + Fy)
		G.Rotation = 20
		G.BackgroundColor3 = Color3.fromRGB(30, 60, 110)
		G.BorderSizePixel = 0
		G.Parent = Target
		for K, Spec in { { "AB", 60, 2 }, { "AB 17", 17, 74 } } do
			local L = Instance.new("TextLabel")
			L.Name = string.format("L%d", K)
			L.BackgroundTransparency = 1
			L.TextColor3 = Color3.fromRGB(255, 255, 255)
			L.Text = Spec[1]
			L.TextSize = Spec[2]
			L.TextXAlignment = Enum.TextXAlignment.Left
			L.Position = UDim2.fromOffset(10, Spec[3])
			L.Size = UDim2.fromOffset(130, Spec[2] + 6)
			L.Parent = G
		end
		local Bar = Instance.new("Frame")
		Bar.Name = "Bar"
		Bar.BackgroundColor3 = Color3.fromRGB(255, 255, 255)
		Bar.BorderSizePixel = 0
		Bar.Position = UDim2.fromOffset(100, 100)
		Bar.Size = UDim2.fromOffset(40, 10)
		Bar.Parent = G
	end
end
