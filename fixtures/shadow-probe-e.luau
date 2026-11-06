-- UIShadow probe E: interactions (transparency, multiple shadows, siblings, rotation, UIScale, text mode,
-- strokes, clipping).
return function(Target)
	local function Frame(Props, Parent)
		local F = Instance.new("Frame")
		F.BorderSizePixel = 0
		F.BackgroundColor3 = Color3.fromRGB(230, 230, 240)
		for Key, Value in Props do
			(F :: any)[Key] = Value
		end
		F.Parent = Parent or Target
		return F
	end

	local function Shadow(Parent, Props)
		local S = Instance.new("UIShadow")
		S.Color = Color3.fromRGB(255, 60, 60)
		S.BlurRadius = UDim.new(0, 16)
		for Key, Value in Props or {} do
			(S :: any)[Key] = Value
		end
		S.Parent = Parent
		return S
	end

	-- 1: semi-transparent frame
	Shadow(Frame({ Position = UDim2.fromOffset(40, 40), Size = UDim2.fromOffset(120, 80), BackgroundTransparency = 0.5 }))

	-- 2: two shadows with different ZIndex
	local Two = Frame({ Position = UDim2.fromOffset(240, 40), Size = UDim2.fromOffset(120, 80) })
	Shadow(Two, { ZIndex = -1, Offset = UDim2.fromOffset(-18, 0) })
	Shadow(Two, { ZIndex = -2, Offset = UDim2.fromOffset(18, 0), Color = Color3.fromRGB(60, 255, 60) })

	-- 3: shadow over an earlier sibling and a later sibling over the shadow
	Frame({ Position = UDim2.fromOffset(440, 30), Size = UDim2.fromOffset(90, 100), BackgroundColor3 = Color3.fromRGB(60, 90, 200) })
	Shadow(Frame({ Position = UDim2.fromOffset(500, 50), Size = UDim2.fromOffset(100, 60) }), { BlurRadius = UDim.new(0, 30), Color = Color3.fromRGB(0, 0, 0) })
	Frame({ Position = UDim2.fromOffset(590, 40), Size = UDim2.fromOffset(40, 90), BackgroundColor3 = Color3.fromRGB(60, 200, 90) })

	-- 4: rotated frame
	Shadow(Frame({ Position = UDim2.fromOffset(720, 40), Size = UDim2.fromOffset(120, 70), Rotation = 25 }))

	-- 5: UIScale 1.5
	local Scaled = Frame({ Position = UDim2.fromOffset(940, 50), Size = UDim2.fromOffset(80, 50) })
	Shadow(Scaled)
	local Scale = Instance.new("UIScale")
	Scale.Scale = 1.5
	Scale.Parent = Scaled

	-- 6: text mode shadow
	local Label = Instance.new("TextLabel")
	Label.Position = UDim2.fromOffset(40, 200)
	Label.Size = UDim2.fromOffset(200, 60)
	Label.BackgroundTransparency = 1
	Label.Text = "Glow"
	Label.TextColor3 = Color3.new(1, 1, 1)
	Label.FontFace = Font.new("rbxasset://fonts/families/FredokaOne.json")
	Label.TextSize = 40
	Label.Parent = Target
	Shadow(Label, { Mode = Enum.ApplyShadowMode.Text, BlurRadius = UDim.new(0, 8) })

	-- 7: shape mode on a transparent label
	local Plain = Label:Clone()
	Plain.Position = UDim2.fromOffset(300, 200)
	Plain:FindFirstChildOfClass("UIShadow").Mode = Enum.ApplyShadowMode.Shape
	Plain.Parent = Target

	-- 8: frame with an outer UIStroke
	local Stroked = Frame({ Position = UDim2.fromOffset(560, 210), Size = UDim2.fromOffset(120, 60) })
	local Stroke = Instance.new("UIStroke")
	Stroke.Thickness = 6
	Stroke.Color = Color3.fromRGB(40, 40, 40)
	Stroke.Parent = Stroked
	Shadow(Stroked)

	-- 9: shadowed child inside a clipping parent
	local Clip = Frame({ Position = UDim2.fromOffset(780, 190), Size = UDim2.fromOffset(160, 100), ClipsDescendants = true, BackgroundColor3 = Color3.fromRGB(30, 30, 40) })
	Shadow(Frame({ Position = UDim2.fromOffset(40, 30), Size = UDim2.fromOffset(80, 40) }, Clip), { BlurRadius = UDim.new(0, 24) })

	-- 10: text mode with stroke-less label, larger blur and offset
	local Label2 = Label:Clone()
	Label2.Position = UDim2.fromOffset(1000, 200)
	local LabelShadow = Label2:FindFirstChildOfClass("UIShadow")
	LabelShadow.BlurRadius = UDim.new(0, 16)
	LabelShadow.Offset = UDim2.fromOffset(4, 6)
	LabelShadow.Color = Color3.fromRGB(0, 0, 0)
	Label2.Parent = Target
end
