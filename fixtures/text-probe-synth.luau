-- Synthetic bold / italic: faces without a bold or italic file (FredokaOne, Bangers, LuckiestGuy,
-- PermanentMarker) asked for weight 700 / italic, through rich text tags and through FontFace, at several
-- sizes; white text on the label's dark background.
-- rur compare --fixture fixtures/text-probe-synth.luau --viewport 2060x1150 --time 0.2 --background 0,0,0
return function(Target)
	local Column, Y = 0, 4
	local function Label(Name, Props, Height)
		if Y + Height > 1140 then
			Y = 4
			Column += 1
		end
		local L = Instance.new("TextLabel")
		L.Name = Name
		L.BorderSizePixel = 0
		L.BackgroundColor3 = Color3.fromRGB(20, 24, 40)
		L.TextColor3 = Color3.fromRGB(255, 255, 255)
		L.FontFace = Font.new("rbxasset://fonts/families/FredokaOne.json")
		L.TextXAlignment = Enum.TextXAlignment.Left
		L.TextYAlignment = Enum.TextYAlignment.Top
		L.Size = UDim2.fromOffset(1000, Height)
		L.Position = UDim2.fromOffset(4 + Column * 1020, Y)
		for Key, Value in Props do
			(L :: any)[Key] = Value
		end
		L.Parent = Target
		Y += Height + 4
		return L
	end

	for _, Size in { 12, 18, 24, 36, 48, 72 } do
		local Height = math.ceil(Size * 1.25 / 2) * 2
		Label(`Rich{Size}`, { RichText = true, TextSize = Size, Text = "Waxo <b>Waxo</b> <i>Waxo</i> <b><i>Waxo</i></b>" }, Height)
	end
	for _, Size in { 18, 36 } do
		local Height = math.ceil(Size * 1.25 / 2) * 2
		Label(`FaceBold{Size}`, { TextSize = Size, Text = "Waxo Hamburg", FontFace = Font.new("rbxasset://fonts/families/FredokaOne.json", Enum.FontWeight.Bold) }, Height)
		Label(`FaceItalic{Size}`, { TextSize = Size, Text = "Waxo Hamburg", FontFace = Font.new("rbxasset://fonts/families/FredokaOne.json", Enum.FontWeight.Regular, Enum.FontStyle.Italic) }, Height)
		Label(`FaceHeavy{Size}`, { TextSize = Size, Text = "Waxo Hamburg", FontFace = Font.new("rbxasset://fonts/families/FredokaOne.json", Enum.FontWeight.Heavy) }, Height)
	end
	for _, Family in { "Bangers", "LuckiestGuy", "PermanentMarker" } do
		Label(`Rich{Family}`, { RichText = true, TextSize = 36, Text = "Waxo <b>Waxo</b> <i>Waxo</i>", FontFace = Font.new(`rbxasset://fonts/families/{Family}.json`) }, 46)
	end
	Label("RichStrokeBold", { RichText = true, TextSize = 36, Text = "Waxo <b>Waxo</b> <i>Waxo</i>" }, 46)
	local S = Instance.new("UIStroke")
	S.Thickness = 2
	S.Color = Color3.fromRGB(200, 40, 80)
	S.Parent = Target:FindFirstChild("RichStrokeBold")
end
