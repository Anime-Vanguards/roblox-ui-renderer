-- Text alignment anchors a hair beside a half device pixel: each cell's label sits under a UIScale chosen so that
-- its left edge, centre or right edge (by TextXAlignment) lands at N + 0.5 + e device px for one device scale
-- (rows 1-4: 1x, 1.5x, 2x, 3x), e = -0.012 .. 0.012 px in steps of 0.001 (columns). Which way the client rounds
-- the anchor shows as the pen of the text, one device px apart.
-- compare: --viewport 1200x900 --time 0.2 --background 0,0,0
return function(Target)
	local Face = Font.new("rbxasset://fonts/families/SourceSansPro.json")
	local Aligns = { Enum.TextXAlignment.Left, Enum.TextXAlignment.Center, Enum.TextXAlignment.Right }
	local Offsets = { 10, 50, 90 } -- the anchor's local x (the label is at local x 10, 80 wide)
	local Row = 0
	for _, Scale in { 1, 1.5, 2, 3 } do
		for AlignIndex, Align in Aligns do
			Row += 1
			for Column = 0, 24 do
				local E = (Column - 12) * 0.001
				local Anchor = Offsets[AlignIndex]
				local N = math.floor(Anchor * Scale)
				local UIScale = (N + 0.5 + E) / (Anchor * Scale)
				local Holder = Instance.new("Frame")
				Holder.Name = string.format("r%d_%s_c%02d", Row, Align.Name, Column)
				Holder.BackgroundTransparency = 1
				Holder.Position = UDim2.fromOffset((Column % 12) * 100, 4 + ((Row - 1) * 3 + Column // 12) * 24)
				Holder.Size = UDim2.fromOffset(98, 22)
				Holder.Parent = Target
				local Scaler = Instance.new("UIScale")
				Scaler.Scale = UIScale
				Scaler.Parent = Holder
				local Label = Instance.new("TextLabel")
				Label.Name = "L"
				Label.BackgroundTransparency = 1
				Label.FontFace = Face
				Label.TextSize = 14
				Label.Text = "Hd"
				Label.TextColor3 = Color3.new(1, 1, 1)
				Label.TextXAlignment = Align
				Label.Position = UDim2.fromOffset(10, 2)
				Label.Size = UDim2.fromOffset(80, 16)
				Label.Parent = Holder
			end
		end
	end
end
