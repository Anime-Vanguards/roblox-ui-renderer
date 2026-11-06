-- Text UIStroke shape probe: white Contextual strokes on black, the text fill hidden with
-- TextTransparency = 1 so the stroke's own coverage is visible. Big "I"/"O" glyphs for edge and corner
-- profiles, small "Hill Oxo" labels like UI text, and a TextTransparency sweep (red stroke, white text).
-- compare: --viewport 1600x860 --time 0.3 --background 0,0,0
return function(Target)
	local function Label(Name, Text, Size, X, Y, W, H, Thickness, TextTransparency, StrokeColor, TextColor, StrokeTransparency)
		local Label = Instance.new("TextLabel")
		Label.Name = Name
		Label.BackgroundTransparency = 1
		Label.FontFace = Font.new("rbxasset://fonts/families/FredokaOne.json")
		Label.TextSize = Size
		Label.Text = Text
		Label.TextColor3 = TextColor or Color3.new(1, 1, 1)
		Label.TextTransparency = TextTransparency
		Label.TextXAlignment = Enum.TextXAlignment.Left
		Label.TextYAlignment = Enum.TextYAlignment.Top
		Label.Position = UDim2.fromOffset(X, Y)
		Label.Size = UDim2.fromOffset(W, H)
		Label.Parent = Target

		local Stroke = Instance.new("UIStroke")
		Stroke.Thickness = Thickness
		Stroke.Color = StrokeColor or Color3.new(1, 1, 1)
		Stroke.Transparency = StrokeTransparency or 0
		Stroke.Parent = Label
		return Label
	end

	for Index, Thickness in { 1, 2, 3, 4.5, 6, 9, 12 } do
		Label(`I{Index}`, "I", 96, 20 + (Index - 1) * 100, 20, 80, 110, Thickness, 1)
		Label(`O{Index}`, "O", 96, 20 + (Index - 1) * 110 + 760, 20, 100, 110, Thickness, 1)
	end

	for Index, Thickness in { 1, 1.5, 2, 2.2125, 3, 4 } do
		Label(`H{Index}`, "Hill Oxo", 24, 20 + (Index - 1) * 150, 170, 140, 40, Thickness, 1)
		Label(`G{Index}`, "Hill Oxo", 40, 20 + (Index - 1) * 250, 230, 240, 60, Thickness, 1)
	end

	local Gray = Color3.fromRGB(128, 128, 128)
	for Index, Transparency in { 0, 0.25, 0.5, 0.75, 0.95, 1 } do
		Label(`T{Index}`, "Il", 64, 20 + (Index - 1) * 120, 320, 110, 90, 4, Transparency, Color3.new(1, 0, 0), Color3.new(1, 1, 1))
		Label(`S{Index}`, "Il", 64, 20 + (Index - 1) * 120, 430, 110, 90, 4, Transparency, Color3.new(1, 0, 0), Color3.new(1, 1, 1), 0.5)
	end
	local Backdrop = Instance.new("Frame")
	Backdrop.Name = "GrayBackdrop"
	Backdrop.BackgroundColor3 = Gray
	Backdrop.BorderSizePixel = 0
	Backdrop.Position = UDim2.fromOffset(0, 310)
	Backdrop.Size = UDim2.fromOffset(740, 220)
	Backdrop.ZIndex = 0
	Backdrop.Parent = Target

	-- Fill vs no stroke: the same text with an invisible stroke, and without a stroke
	Label("FillStroked", "Hill Oxo", 40, 760, 330, 240, 60, 3, 0, Color3.new(1, 1, 1), Color3.new(1, 1, 1), 1)
	local Plain = Instance.new("TextLabel")
	Plain.Name = "FillPlain"
	Plain.BackgroundTransparency = 1
	Plain.FontFace = Font.new("rbxasset://fonts/families/FredokaOne.json")
	Plain.TextSize = 40
	Plain.Text = "Hill Oxo"
	Plain.TextColor3 = Color3.new(1, 1, 1)
	Plain.TextXAlignment = Enum.TextXAlignment.Left
	Plain.TextYAlignment = Enum.TextYAlignment.Top
	Plain.Position = UDim2.fromOffset(1020, 330)
	Plain.Size = UDim2.fromOffset(240, 60)
	Plain.Parent = Target

	-- Stroke above text fill vs below: black text, white stroke
	Label("BlackText", "Il", 64, 760, 420, 110, 90, 4, 0, Color3.new(1, 1, 1), Color3.new(0, 0, 0))
end
