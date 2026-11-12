-- UIShadow probe: Mode = Text. Red shadows of blue text on black, so the red channel is the alpha byte of the
-- shadow wherever no glyph covers it; the labels with TextTransparency 1 show it under the glyphs too.
-- compare --viewport 1200x900 --time 0.3 --background 0,0,0
return function(Target)
	local Fredoka = Font.new("rbxasset://fonts/families/FredokaOne.json")
	local function Label(X, Y, W, H, Props, ShadowProps)
		local L = Instance.new("TextLabel")
		L.Position = UDim2.fromOffset(X, Y)
		L.Size = UDim2.fromOffset(W, H)
		L.BackgroundTransparency = 1
		L.BorderSizePixel = 0
		L.Text = "Glow"
		L.TextColor3 = Color3.new(0, 0, 1)
		L.FontFace = Fredoka
		L.TextSize = 40
		for Key, Value in Props or {} do
			(L :: any)[Key] = Value
		end
		local S = Instance.new("UIShadow")
		S.Mode = Enum.ApplyShadowMode.Text
		S.Color = Color3.new(1, 0, 0)
		S.BlurRadius = UDim.new(0, 8)
		for Key, Value in ShadowProps or {} do
			(S :: any)[Key] = Value
		end
		S.Parent = L
		L.Parent = Target
		return L, S
	end

	-- A: blur radii
	for Index, Blur in { 0, 2, 4, 8, 16, 32 } do
		Label(20 + (Index - 1) * 195, 20, 180, 80, nil, { BlurRadius = UDim.new(0, Blur) })
	end
	-- B: the same with clear text
	for Index, Blur in { 0, 2, 4, 8, 16, 32 } do
		Label(20 + (Index - 1) * 195, 110, 180, 80, { TextTransparency = 1 }, { BlurRadius = UDim.new(0, Blur) })
	end
	-- C: offset, spread, transparency, half transparent text, colour over a light label
	Label(20, 200, 180, 80, { TextTransparency = 1 }, { Offset = UDim2.fromOffset(12, 18) })
	Label(215, 200, 180, 80, { TextTransparency = 1 }, { Spread = UDim2.fromOffset(8, 8) })
	Label(410, 200, 180, 80, { TextTransparency = 1 }, { Spread = UDim2.fromOffset(-4, -4) })
	Label(605, 200, 180, 80, { TextTransparency = 1 }, { Transparency = 0.5 })
	Label(800, 200, 180, 80, { TextTransparency = 0.5 })
	Label(995, 200, 180, 80, { BackgroundTransparency = 0, BackgroundColor3 = Color3.fromRGB(230, 230, 240), TextColor3 = Color3.fromRGB(30, 30, 30) }, { Color = Color3.fromRGB(0, 120, 255), Offset = UDim2.fromOffset(0, 4), BlurRadius = UDim.new(0, 6) })
	-- D: a long stem and a big dot for the kernel, sizes
	Label(20, 290, 100, 160, { Text = "l", TextSize = 150, TextTransparency = 1 }, { BlurRadius = UDim.new(0, 8) })
	Label(130, 290, 100, 160, { Text = "l", TextSize = 150, TextTransparency = 1 }, { BlurRadius = UDim.new(0, 24) })
	Label(240, 290, 140, 160, { Text = ".", TextSize = 300, TextTransparency = 1, TextYAlignment = Enum.TextYAlignment.Bottom }, { BlurRadius = UDim.new(0, 16) })
	Label(400, 290, 120, 70, { TextSize = 14 })
	Label(400, 370, 120, 70, { TextSize = 24 })
	Label(540, 290, 300, 160, { TextSize = 110 })
	Label(860, 290, 150, 70, { FontFace = Font.fromEnum(Enum.Font.SourceSans), Text = "Source 12" })
	Label(860, 370, 150, 70, { FontFace = Font.new("rbxasset://fonts/families/GothamSSm.json", Enum.FontWeight.Bold, Enum.FontStyle.Italic), Text = "Bold it" })
	Label(1020, 290, 160, 160, { Text = "Two\nlines", TextXAlignment = Enum.TextXAlignment.Right, TextYAlignment = Enum.TextYAlignment.Top })
	-- E: rich text, strokes, gradients, rotation, scale, clipping
	Label(20, 470, 240, 80, { RichText = true, Text = 'a<b>b</b><font color="#00ff00">c</font><font size="60">d</font><u>e</u>' })
	Label(280, 470, 180, 80, { RichText = true, Text = '<stroke color="#00ff00" thickness="3">Glow</stroke>' })
	local Stroked = Label(480, 470, 180, 80)
	local Stroke = Instance.new("UIStroke")
	Stroke.Thickness = 3
	Stroke.Color = Color3.new(0, 1, 0)
	Stroke.Parent = Stroked
	Label(680, 470, 180, 80, { TextStrokeTransparency = 0, TextStrokeColor3 = Color3.new(0, 1, 0) })
	local Graded = Label(880, 470, 180, 80, { TextColor3 = Color3.new(1, 1, 1) })
	local Gradient = Instance.new("UIGradient")
	Gradient.Color = ColorSequence.new(Color3.new(0, 0, 1), Color3.new(0, 1, 0))
	Gradient.Transparency = NumberSequence.new(0, 1)
	Gradient.Parent = Graded
	Label(1080, 470, 100, 80, { Text = "\u{1F600}a" })
	-- F
	Label(20, 580, 180, 80, { Rotation = 20 })
	local Scaled = Label(240, 590, 100, 50)
	local Scale = Instance.new("UIScale")
	Scale.Scale = 1.5
	Scale.Parent = Scaled
	local Clip = Instance.new("Frame")
	Clip.Position = UDim2.fromOffset(440, 580)
	Clip.Size = UDim2.fromOffset(120, 60)
	Clip.BackgroundColor3 = Color3.fromRGB(0, 40, 0)
	Clip.BorderSizePixel = 0
	Clip.ClipsDescendants = true
	Clip.Parent = Target
	local Inside = Label(20, 10, 180, 80)
	Inside.Parent = Clip
	Label(600, 580, 100, 80, { TextScaled = true, Text = "Scaled text" })
	Label(720, 580, 100, 40, { TextTruncate = Enum.TextTruncate.AtEnd, Text = "Truncated text" })
	Label(840, 580, 100, 60, { ClipsDescendants = true, Text = "Clipped", TextSize = 50 })
	local Two = Label(960, 580, 200, 80, nil, { Offset = UDim2.fromOffset(-10, 0), ZIndex = 2 })
	local Second = Instance.new("UIShadow")
	Second.Mode = Enum.ApplyShadowMode.Text
	Second.Color = Color3.new(0, 1, 0)
	Second.BlurRadius = UDim.new(0, 8)
	Second.Offset = UDim2.fromOffset(10, 0)
	Second.Parent = Two
	-- G: buttons, boxes, a label with a fill, a text shadow beside a shape shadow, Inset
	local Button = Instance.new("TextButton")
	Button.Position = UDim2.fromOffset(20, 700)
	Button.Size = UDim2.fromOffset(180, 80)
	Button.BackgroundColor3 = Color3.fromRGB(0, 40, 0)
	Button.BorderSizePixel = 0
	Button.AutoButtonColor = false
	Button.Text = "Button"
	Button.TextColor3 = Color3.new(0, 0, 1)
	Button.FontFace = Fredoka
	Button.TextSize = 40
	local ButtonShadow = Instance.new("UIShadow")
	ButtonShadow.Mode = Enum.ApplyShadowMode.Text
	ButtonShadow.Color = Color3.new(1, 0, 0)
	ButtonShadow.BlurRadius = UDim.new(0, 8)
	ButtonShadow.Parent = Button
	Button.Parent = Target
	local Box = Instance.new("TextBox")
	Box.Position = UDim2.fromOffset(220, 700)
	Box.Size = UDim2.fromOffset(180, 80)
	Box.BackgroundColor3 = Color3.fromRGB(0, 40, 0)
	Box.BorderSizePixel = 0
	Box.Text = "Box"
	Box.TextColor3 = Color3.new(0, 0, 1)
	Box.FontFace = Fredoka
	Box.TextSize = 40
	local BoxShadow = ButtonShadow:Clone()
	BoxShadow.Parent = Box
	Box.Parent = Target
	local Mixed = Label(420, 700, 180, 80, { BackgroundTransparency = 0, BackgroundColor3 = Color3.fromRGB(0, 40, 0) })
	local Shape = Instance.new("UIShadow")
	Shape.Color = Color3.new(0, 1, 0)
	Shape.BlurRadius = UDim.new(0, 8)
	Shape.Parent = Mixed
	Label(620, 700, 180, 80, { TextTransparency = 1 }, { Inset = true })
	Label(820, 700, 180, 80, nil, { Inset = true, BlurRadius = UDim.new(0, 4) })
	Label(1020, 700, 160, 80, { Text = "" })
	local Frame = Instance.new("Frame")
	Frame.Position = UDim2.fromOffset(1020, 800)
	Frame.Size = UDim2.fromOffset(160, 60)
	Frame.BackgroundColor3 = Color3.fromRGB(0, 40, 0)
	Frame.BorderSizePixel = 0
	local FrameShadow = ButtonShadow:Clone()
	FrameShadow.Parent = Frame
	Frame.Parent = Target
end
