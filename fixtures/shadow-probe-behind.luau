-- UIShadow probe: ShowBehindParent = false on frames that show what is behind them: clear and half
-- transparent fills, rounded corners, offset, hard shadows, rotation, text, images, strokes, groups.
-- compare --viewport 1200x700 --time 0.3 --background 0,0,0
return function(Target)
	local function Make(Class, X, Y, W, H, Props, ShadowProps)
		local F = Instance.new(Class)
		F.Position = UDim2.fromOffset(X, Y)
		F.Size = UDim2.fromOffset(W, H)
		F.BackgroundColor3 = Color3.fromRGB(40, 40, 60)
		F.BackgroundTransparency = 1
		F.BorderSizePixel = 0
		for Key, Value in Props or {} do
			if Key == "Corner" then
				local C = Instance.new("UICorner")
				C.CornerRadius = UDim.new(0, Value)
				C.Parent = F
			else
				(F :: any)[Key] = Value
			end
		end
		local S = Instance.new("UIShadow")
		S.Color = Color3.fromRGB(255, 60, 60)
		S.BlurRadius = UDim.new(0, 16)
		S.ShowBehindParent = false
		for Key, Value in ShadowProps or {} do
			(S :: any)[Key] = Value
		end
		S.Parent = F
		F.Parent = Target
		return F
	end
	local Fredoka = Font.new("rbxasset://fonts/families/FredokaOne.json")

	-- row 1
	Make("Frame", 40, 40, 140, 100)
	Make("Frame", 240, 40, 140, 100, { Corner = 24 })
	Make("Frame", 440, 40, 140, 100, { BackgroundTransparency = 0.5 })
	Make("Frame", 640, 40, 140, 100, { Corner = 24 }, { Offset = UDim2.fromOffset(20, 10) })
	Make("Frame", 840, 40, 140, 100, { Corner = 24 }, { BlurRadius = UDim.new(0, 0), Spread = UDim2.fromOffset(20, 20) })
	Make("Frame", 1040, 40, 140, 100, nil, { ShowBehindParent = true })
	-- row 2
	Make("Frame", 40, 240, 140, 100, { Rotation = 20 })
	Make("Frame", 240, 240, 140, 100, { Rotation = -35, Corner = 24 })
	Make("TextLabel", 440, 240, 140, 100, { Text = "Text", TextSize = 50, TextColor3 = Color3.new(1, 1, 1), FontFace = Fredoka })
	Make("ImageLabel", 640, 240, 140, 100, { Image = "rbxasset://textures/ui/GuiImagePlaceholder.png", ImageTransparency = 0.5 })
	local Stroked = Make("Frame", 840, 240, 140, 100, { Corner = 24 })
	local Stroke = Instance.new("UIStroke")
	Stroke.Thickness = 4
	Stroke.Color = Color3.fromRGB(60, 90, 255)
	Stroke.Transparency = 0.5
	Stroke.Parent = Stroked
	Make("Frame", 1040, 240, 140, 100, { Corner = 24 }, { Inset = true })
	-- row 3
	local Group = Make("CanvasGroup", 40, 440, 140, 100, { Corner = 24, GroupTransparency = 0.5, BackgroundTransparency = 0 })
	local Child = Instance.new("Frame")
	Child.Size = UDim2.fromOffset(60, 60)
	Child.BackgroundColor3 = Color3.fromRGB(60, 200, 90)
	Child.BorderSizePixel = 0
	Child.Parent = Group
	Make("TextLabel", 240, 440, 140, 100, { Text = "Text", TextSize = 50, TextColor3 = Color3.new(0, 0, 1), TextTransparency = 0.5, FontFace = Fredoka }, { Mode = Enum.ApplyShadowMode.Text, BlurRadius = UDim.new(0, 8) })
	Make("Frame", 440, 440, 140, 100, { Corner = 50 }, { Spread = UDim2.fromOffset(-30, -30) })
	Make("Frame", 640, 440, 140, 100, { Corner = 24 }, { Transparency = 0.5, BlurRadius = UDim.new(0, 4) })
	Make("Frame", 840, 440, 140, 100, { BackgroundTransparency = 0, Corner = 24 }, { BlurRadius = UDim.new(0, 4) })
	local Two = Make("Frame", 1040, 440, 140, 100, { Corner = 24 }, { Offset = UDim2.fromOffset(-10, 0) })
	local Second = Instance.new("UIShadow")
	Second.Color = Color3.fromRGB(60, 255, 60)
	Second.BlurRadius = UDim.new(0, 16)
	Second.Offset = UDim2.fromOffset(10, 0)
	Second.Parent = Two
end
