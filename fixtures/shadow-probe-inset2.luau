-- UIShadow probe: Inset with other things: clear and half transparent fills, children, strokes, gradients,
-- rotation, clipping, images, text, several shadows, ZIndex, ShowBehindParent, UIScale, CanvasGroup.
-- compare --viewport 1200x900 --time 0.3 --background 0,0,0
return function(Target)
	local function Make(Class, X, Y, W, H, Props, Parent)
		local F = Instance.new(Class)
		F.Position = UDim2.fromOffset(X, Y)
		F.Size = UDim2.fromOffset(W, H)
		F.BackgroundColor3 = Color3.fromRGB(40, 40, 60)
		F.BorderSizePixel = 0
		for Key, Value in Props or {} do
			(F :: any)[Key] = Value
		end
		F.Parent = Parent or Target
		return F
	end
	local function Shadow(Parent, Props)
		local S = Instance.new("UIShadow")
		S.Color = Color3.fromRGB(255, 60, 60)
		S.BlurRadius = UDim.new(0, 16)
		S.Inset = true
		for Key, Value in Props or {} do
			(S :: any)[Key] = Value
		end
		S.Parent = Parent
		return S
	end
	local function Corner(Parent, Radius)
		local C = Instance.new("UICorner")
		C.CornerRadius = UDim.new(0, Radius)
		C.Parent = Parent
	end
	local Fredoka = Font.new("rbxasset://fonts/families/FredokaOne.json")

	-- row 1
	Shadow(Make("Frame", 20, 20, 160, 120, { BackgroundTransparency = 1 }))
	Shadow(Make("Frame", 200, 20, 160, 120, { BackgroundTransparency = 0.5 }))
	local WithChild = Make("Frame", 380, 20, 160, 120)
	Shadow(WithChild)
	Make("Frame", 0, 40, 100, 40, { BackgroundColor3 = Color3.fromRGB(60, 200, 90) }, WithChild)
	local Outer = Make("Frame", 566, 26, 148, 108)
	Corner(Outer, 20)
	local OuterStroke = Instance.new("UIStroke")
	OuterStroke.Thickness = 6
	OuterStroke.Color = Color3.fromRGB(60, 90, 255)
	OuterStroke.Parent = Outer
	Shadow(Outer)
	local Inner = Make("Frame", 740, 20, 160, 120)
	Corner(Inner, 20)
	local InnerStroke = Instance.new("UIStroke")
	InnerStroke.Thickness = 8
	InnerStroke.BorderStrokePosition = Enum.BorderStrokePosition.Inner
	InnerStroke.Color = Color3.fromRGB(60, 90, 255)
	InnerStroke.Parent = Inner
	Shadow(Inner)
	local Graded = Make("Frame", 920, 20, 160, 120, { BackgroundColor3 = Color3.new(1, 1, 1) })
	local Gradient = Instance.new("UIGradient")
	Gradient.Color = ColorSequence.new(Color3.fromRGB(20, 40, 160), Color3.fromRGB(20, 160, 60))
	Gradient.Transparency = NumberSequence.new(0, 0.6)
	Gradient.Parent = Graded
	Shadow(Graded)

	-- row 2
	Shadow(Make("Frame", 40, 190, 140, 90, { Rotation = 25 }))
	local Turned = Make("Frame", 220, 190, 140, 90, { Rotation = -40 })
	Corner(Turned, 24)
	Shadow(Turned, { Offset = UDim2.fromOffset(10, 0) })
	local Clip = Make("Frame", 400, 170, 140, 120, { ClipsDescendants = true, BackgroundColor3 = Color3.fromRGB(20, 20, 20) })
	Shadow(Make("Frame", 60, 50, 140, 100, nil, Clip))
	Shadow(Make("ImageLabel", 560, 170, 160, 120, { Image = "rbxasset://textures/ui/GuiImagePlaceholder.png" }))
	Shadow(Make("ImageLabel", 740, 170, 160, 120, { Image = "rbxasset://textures/ui/GuiImagePlaceholder.png", BackgroundTransparency = 1 }))
	Shadow(Make("TextLabel", 920, 170, 160, 120, { Text = "Inset text", TextSize = 40, TextColor3 = Color3.new(1, 1, 1), TextWrapped = true, FontFace = Fredoka }))

	-- row 3
	local Both = Make("Frame", 40, 340, 140, 100)
	Shadow(Both, { Inset = false, Color = Color3.fromRGB(60, 255, 60) })
	Shadow(Both)
	local TwoInset = Make("Frame", 220, 340, 140, 100)
	Shadow(TwoInset, { ZIndex = 2, Offset = UDim2.fromOffset(-20, 0) })
	Shadow(TwoInset, { ZIndex = 1, Offset = UDim2.fromOffset(20, 0), Color = Color3.fromRGB(60, 255, 60) })
	local TwoInsetB = Make("Frame", 400, 340, 140, 100)
	Shadow(TwoInsetB, { ZIndex = 1, Offset = UDim2.fromOffset(-20, 0) })
	Shadow(TwoInsetB, { ZIndex = 2, Offset = UDim2.fromOffset(20, 0), Color = Color3.fromRGB(60, 255, 60) })
	Shadow(Make("Frame", 580, 340, 140, 100), { ShowBehindParent = false })
	local Scaled = Make("Frame", 760, 340, 80, 60)
	Shadow(Scaled)
	local Scale = Instance.new("UIScale")
	Scale.Scale = 1.5
	Scale.Parent = Scaled
	local Group = Make("CanvasGroup", 920, 340, 160, 100, { GroupTransparency = 0.3 })
	Corner(Group, 20)
	Shadow(Group)
	Make("Frame", 100, 20, 100, 60, { BackgroundColor3 = Color3.fromRGB(60, 200, 90) }, Group)

	-- row 4: the ZIndex of the shadow against children, a shadowed child in a shadowed parent, a button, a
	-- scrolling frame, a border
	local ZHigh = Make("Frame", 40, 500, 140, 100)
	Shadow(ZHigh, { ZIndex = 5 })
	Make("Frame", 0, 30, 80, 40, { BackgroundColor3 = Color3.fromRGB(60, 200, 90), ZIndex = 3 }, ZHigh)
	local ZLow = Make("Frame", 220, 500, 140, 100)
	Shadow(ZLow, { ZIndex = -5 })
	Make("Frame", 0, 30, 80, 40, { BackgroundColor3 = Color3.fromRGB(60, 200, 90), ZIndex = -8 }, ZLow)
	local Nest = Make("Frame", 400, 500, 140, 100)
	Shadow(Nest)
	Shadow(Make("Frame", 30, 20, 150, 60, { BackgroundColor3 = Color3.fromRGB(60, 90, 200) }, Nest), { Color = Color3.fromRGB(255, 255, 60) })
	local Button = Make("TextButton", 580, 500, 140, 100, { Text = "", AutoButtonColor = false })
	Corner(Button, 50)
	Shadow(Button, { BlurRadius = UDim.new(0, 30), Transparency = 0.4, Color = Color3.fromRGB(255, 255, 255) })
	local Scroll = Make("ScrollingFrame", 760, 500, 140, 100, { CanvasSize = UDim2.fromOffset(0, 300), ScrollBarThickness = 8 })
	Shadow(Scroll)
	Make("Frame", 10, 10, 60, 200, { BackgroundColor3 = Color3.fromRGB(60, 200, 90) }, Scroll)
	local Bordered = Make("Frame", 940, 510, 120, 80, { BorderSizePixel = 6, BorderColor3 = Color3.fromRGB(60, 90, 255) })
	Shadow(Bordered)

	-- row 5: Inset with Mode = Text, the same without Inset, a light frame with a dark soft inner shadow
	local Label = Make("TextLabel", 40, 660, 300, 100, { Text = "Glow", TextSize = 90, TextColor3 = Color3.fromRGB(40, 40, 60), BackgroundTransparency = 1, FontFace = Fredoka })
	Shadow(Label, { Mode = Enum.ApplyShadowMode.Text, BlurRadius = UDim.new(0, 8) })
	local Label2 = Make("TextLabel", 380, 660, 300, 100, { Text = "Glow", TextSize = 90, TextColor3 = Color3.fromRGB(40, 40, 60), FontFace = Fredoka })
	Shadow(Label2, { Mode = Enum.ApplyShadowMode.Text, BlurRadius = UDim.new(0, 8) })
	Shadow(Make("Frame", 720, 660, 140, 100, { BackgroundColor3 = Color3.fromRGB(230, 230, 240) }), { Color = Color3.new(0, 0, 0), Transparency = 0.5, BlurRadius = UDim.new(0, 30), Offset = UDim2.fromOffset(0, 8) })
	local Pill = Make("Frame", 900, 660, 160, 60, { BackgroundColor3 = Color3.fromRGB(230, 230, 240) })
	Corner(Pill, 30)
	Shadow(Pill, { Color = Color3.new(0, 0, 0), Transparency = 0.3, BlurRadius = UDim.new(0, 10), Offset = UDim2.fromOffset(0, 4), Spread = UDim2.fromOffset(-4, -4) })
end
