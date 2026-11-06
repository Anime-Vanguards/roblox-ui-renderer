-- Rich TextScaled (round 3): at which size does Studio draw the glyphs of a rich TextScaled label whose
-- box is not a whole number of pixels, or whose width limits the size? Height-limited "Cyan" and
-- width-limited "Dark neutral" (FredokaOne) in boxes swept by fractions of a pixel, white on a dark
-- holder, TextXAlignment Left / TextYAlignment Top so the pen sits at the box corner.
-- rur compare --fixture fixtures/layout-richscaled-quads.luau --viewport 1200x900 --time 0.2 --background 0,0,0
return function(Target)
	local Holder = Instance.new("Frame")
	Holder.Name = "Holder"
	Holder.BorderSizePixel = 0
	Holder.BackgroundColor3 = Color3.fromRGB(20, 24, 40)
	Holder.Size = UDim2.fromOffset(1200, 900)
	Holder.Parent = Target
	local Face = Font.new("rbxasset://fonts/families/FredokaOne.json")
	local Index = 0
	local function Label(Name, Text, WidthPx, HeightPx, Rich, TextSize)
		local Column, Row = Index % 4, Index // 4
		Index += 1
		local L = Instance.new("TextLabel")
		L.Name = Name
		L.BackgroundTransparency = 1
		L.TextColor3 = Color3.fromRGB(255, 255, 255)
		L.FontFace = Face
		L.RichText = Rich
		L.Text = Text
		L.TextSize = TextSize or 14
		L.TextScaled = true
		L.TextXAlignment = Enum.TextXAlignment.Left
		L.TextYAlignment = Enum.TextYAlignment.Top
		-- scale sizes: whole-pixel offsets cannot express fractions of a logical px
		L.Size = UDim2.fromScale(WidthPx / 1.5 / 1200, HeightPx / 1.5 / 900)
		L.Position = UDim2.fromOffset(6 + Column * 298, 6 + Row * 64)
		L.Parent = Holder
	end
	for _, H in { 60, 60.25, 60.5, 60.75, 61, 61.4, 61.8, 62.2 } do
		Label(`Cyan_{H}`, "Cyan", 420, H, true)
	end
	for _, W in { 158, 158.5, 159.3, 160, 161, 162, 163, 164.5 } do
		Label(`Dark_{W}`, "Dark neutral", W, 80, true)
	end
	for _, H in { 60, 60.5, 61, 61.5 } do
		Label(`CyanPlain_{H}`, "Cyan", 420, H, false)
	end
	for _, H in { 44.4, 44.8, 45.2, 45.6 } do
		Label(`CyanTS30_{H}`, "Cyan", 420, H, true, 30)
	end
	-- Under UIScale 0.885: a UI kit's button labels' boxes (local units; FredokaOne, TextSize 14)
	local Scaled = Instance.new("Frame")
	Scaled.Name = "Scaled"
	Scaled.BackgroundTransparency = 1
	Scaled.Position = UDim2.fromOffset(0, 420)
	Scaled.Size = UDim2.fromOffset(1200, 480)
	Scaled.Parent = Holder
	local UIScale = Instance.new("UIScale")
	UIScale.Scale = 0.885
	UIScale.Parent = Scaled
	local Row = 0
	for _, Case in {
		{ "Dark neutral", 120, 28.2 }, { "Small", 80, 23.4 }, { "Authored gradient", 140, 28.2 }, { "Reset", 120, 28.2 },
		{ "Primary", 160, 38.192 }, { "Cyan", 160, 33.8 }, { "Secondary", 160, 38.192 }, { "Review & craft", 140, 28.2 },
		{ "Dark tinted", 120, 28.2 }, { "Medium", 160, 33.8 }, { "Large", 240, 41.6 }, { "Rectangular", 160, 38.192 },
	} do
		local L = Instance.new("TextLabel")
		L.Name = `S_{Case[1]}`
		L.BackgroundTransparency = 1
		L.TextColor3 = Color3.fromRGB(255, 255, 255)
		L.FontFace = Face
		L.RichText = true
		L.Text = Case[1]
		L.TextScaled = true
		L.Size = UDim2.new(0, Case[2], Case[3] / 480, 0)
		L.Position = UDim2.fromOffset(6 + (Row % 4) * 300, 6 + (Row // 4) * 60)
		L.Parent = Scaled
		Row += 1
	end
end
