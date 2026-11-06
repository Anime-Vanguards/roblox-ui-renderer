-- Text under a UIScale (a UI kit's buttons: FredokaOne TextSize 35, RichText, UIScale 0.885, UIStroke): at
-- which pixel size are the glyphs rasterized and where do they sit? Rows under several scales, plain and
-- rich, with and without a UIStroke, next to unscaled reference rows at the neighbouring sizes; white text
-- on the holder's dark background.
-- rur compare --fixture fixtures/text-probe-uiscale.luau --viewport 2060x1150 --time 0.2 --background 0,0,0
return function(Target)
	local Column, Y = 0, 4
	local function Row(Name, Scale, Props, Height, Stroke)
		if Y + Height > 1140 then
			Y = 4
			Column += 1
		end
		local Holder = Instance.new("Frame")
		Holder.Name = Name
		Holder.BorderSizePixel = 0
		Holder.BackgroundColor3 = Color3.fromRGB(20, 24, 40)
		Holder.Size = UDim2.fromOffset(660, Height)
		Holder.Position = UDim2.fromOffset(4 + Column * 680, Y)
		local L = Instance.new("TextLabel")
		L.Name = "Label"
		L.BackgroundTransparency = 1
		L.TextColor3 = Color3.fromRGB(255, 255, 255)
		L.FontFace = Font.new("rbxasset://fonts/families/FredokaOne.json")
		L.TextXAlignment = Enum.TextXAlignment.Left
		L.TextYAlignment = Enum.TextYAlignment.Top
		L.Size = UDim2.fromOffset(640, Height)
		L.Position = UDim2.fromOffset(6, 2)
		for Key, Value in Props do
			(L :: any)[Key] = Value
		end
		L.Parent = Holder
		if Stroke then
			local S = Instance.new("UIStroke")
			S.Thickness = Stroke
			S.Color = Color3.fromRGB(0, 0, 0)
			S.Parent = L
		end
		if Scale then
			local U = Instance.new("UIScale")
			U.Scale = Scale
			U.Parent = Holder
		end
		Holder.Parent = Target
		Y += math.ceil(Height * (Scale or 1)) + 6
		return L
	end

	local Text = "Quick Craft Waxo 42"
	-- The button case and its neighbours
	Row("Rich885", 0.885, { TextSize = 35, RichText = true, Text = Text }, 48)
	Row("Plain885", 0.885, { TextSize = 35, Text = Text }, 48)
	Row("Rich885Stroke", 0.885, { TextSize = 35, RichText = true, Text = Text }, 48, 2.5)
	Row("Plain885Stroke", 0.885, { TextSize = 35, Text = Text }, 48, 2.5)
	-- Unscaled references at the candidate sizes (29 -> 43.5 px, 30 -> 45 px, 31 -> 46.5 px)
	Row("Ref29", nil, { TextSize = 29, Text = Text }, 40)
	Row("Ref30", nil, { TextSize = 30, Text = Text }, 40)
	Row("Ref31", nil, { TextSize = 31, Text = Text }, 40)
	Row("Ref30Rich", nil, { TextSize = 30, RichText = true, Text = Text }, 40)
	-- Other scales and sizes
	Row("Rich75", 0.75, { TextSize = 35, RichText = true, Text = Text }, 48)
	Row("Plain75", 0.75, { TextSize = 35, Text = Text }, 48)
	Row("Rich125", 1.25, { TextSize = 24, RichText = true, Text = Text }, 34)
	Row("Plain125", 1.25, { TextSize = 24, Text = Text }, 34)
	Row("Plain90", 0.9, { TextSize = 24, Text = Text }, 34)
	Row("Rich90", 0.9, { TextSize = 24, RichText = true, Text = Text }, 34)
	Row("Plain60", 0.6, { TextSize = 40, Text = Text }, 54)
	Row("SourceSans885", 0.885, { TextSize = 35, Text = Text, FontFace = Font.new("rbxasset://fonts/families/SourceSansPro.json") }, 48)
	Row("Gotham885", 0.885, { TextSize = 35, RichText = true, Text = Text, FontFace = Font.new("rbxasset://fonts/families/GothamSSm.json", Enum.FontWeight.Bold) }, 48)
end
