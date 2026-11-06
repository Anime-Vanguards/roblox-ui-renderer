-- Text UIStroke under UIScale, with RichText, and with StrokeSizingMode.ScaledSize: "I" glyphs and short
-- words with TextTransparency 1 (white stroke on black) so the stroker radius can be read off.
-- compare: --viewport 1400x520 --time 0.3 --background 0,0,0
return function(Target)
	local function Label(Parent, Name, Text, Size, X, Y, W, H, Thickness, Props)
		local Label = Instance.new("TextLabel")
		Label.Name = Name
		Label.BackgroundTransparency = 1
		Label.FontFace = Font.new("rbxasset://fonts/families/FredokaOne.json")
		Label.TextSize = Size
		Label.Text = Text
		Label.TextColor3 = Color3.new(1, 1, 1)
		Label.TextTransparency = 1
		Label.TextXAlignment = Enum.TextXAlignment.Left
		Label.TextYAlignment = Enum.TextYAlignment.Top
		Label.Position = UDim2.fromOffset(X, Y)
		Label.Size = UDim2.fromOffset(W, H)
		for Key, Value in Props or {} do
			(Label :: any)[Key] = Value
		end
		Label.Parent = Parent

		local Stroke = Instance.new("UIStroke")
		Stroke.Thickness = Thickness
		Stroke.Color = Color3.new(1, 1, 1)
		Stroke.Parent = Label
		return Label, Stroke
	end

	-- UIScale containers
	for Index, Scale in { 0.885, 1.5, 0.5 } do
		local Holder = Instance.new("Frame")
		Holder.Name = `Scaled{Index}`
		Holder.BackgroundTransparency = 1
		Holder.Position = UDim2.fromOffset(20 + (Index - 1) * 460, 20)
		Holder.Size = UDim2.fromOffset(300, 120)
		Holder.Parent = Target
		local UIScale = Instance.new("UIScale")
		UIScale.Scale = Scale
		UIScale.Parent = Holder
		Label(Holder, "I", "I", 40, 0, 0, 60, 60, 2.5)
		Label(Holder, "Word", "Hope", 40, 70, 0, 200, 60, 1.7)
	end

	-- RichText vs plain at the same size
	Label(Target, "Plain", "I bold", 40, 20, 220, 200, 60, 2.5)
	Label(Target, "Rich", "I <b>bold</b>", 40, 260, 220, 200, 60, 2.5, { RichText = true })

	-- ScaledSize text strokes
	local _, A = Label(Target, "ScaledA", "I", 40, 520, 220, 200, 60, 0.02)
	A.StrokeSizingMode = Enum.StrokeSizingMode.ScaledSize
	local _, B = Label(Target, "ScaledB", "I", 40, 760, 220, 100, 120, 0.05)
	B.StrokeSizingMode = Enum.StrokeSizingMode.ScaledSize

	-- Text stroke transparency with a stroke gradient (opaque text)
	local Grad, GradStroke = Label(Target, "GradT", "Oxo", 48, 20, 340, 200, 70, 4)
	Grad.TextTransparency = 0
	Grad.TextColor3 = Color3.new(0, 0, 0)
	GradStroke.Transparency = 0.3
	local Gradient = Instance.new("UIGradient")
	Gradient.Color = ColorSequence.new(Color3.fromRGB(255, 0, 0), Color3.fromRGB(0, 0, 255))
	Gradient.Transparency = NumberSequence.new(0, 0.6)
	Gradient.Rotation = 30
	Gradient.Parent = GradStroke
end
