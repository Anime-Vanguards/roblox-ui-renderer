-- Text UIGradient probe 2: full-block glyphs (Arimo U+2588) so every pixel of a glyph quad is covered,
-- under rotated gradients (which diagonal splits the quad), with UIStroke (gradient on the label vs on the
-- stroke), TextTransparency together with a transparency ramp, and multi-line text.
-- rur compare --fixture fixtures/text-probe-gradient2.luau --viewport 2060x1150 --time 0.2 --background 0,0,0
return function(Target)
	local Ramp = ColorSequence.new({
		ColorSequenceKeypoint.new(0, Color3.fromRGB(0, 255, 64)),
		ColorSequenceKeypoint.new(1, Color3.fromRGB(255, 0, 192)),
	})
	local Blocks = string.rep("\u{2588}", 12)

	local Column, Y = 0, 4
	local function Place(Instance_, Width, Height)
		if Y + Height > 1140 then
			Y = 4
			Column += 1
		end
		Instance_.Position = UDim2.fromOffset(4 + Column * 680, Y)
		Instance_.Size = UDim2.fromOffset(Width, Height)
		Y += Height + 6
	end

	local function Label(Name, Width, Height, Props, GradientProps, Stroke)
		local L = Instance.new("TextLabel")
		L.Name = Name
		L.BackgroundTransparency = 1
		L.TextColor3 = Color3.new(1, 1, 1)
		L.FontFace = Font.new("rbxasset://fonts/families/Arimo.json")
		L.TextSize = 40
		L.Text = Blocks
		L.TextXAlignment = Enum.TextXAlignment.Left
		L.TextYAlignment = Enum.TextYAlignment.Top
		for Key, Value in Props or {} do
			(L :: any)[Key] = Value
		end
		if GradientProps then
			local G = Instance.new("UIGradient")
			G.Color = Ramp
			for Key, Value in GradientProps do
				(G :: any)[Key] = Value
			end
			G.Parent = L
		end
		if Stroke then
			local S = Instance.new("UIStroke")
			S.Thickness = 3
			S.Color = Color3.fromRGB(255, 255, 255)
			for Key, Value in Stroke do
				if Key ~= "Gradient" then
					(S :: any)[Key] = Value
				end
			end
			if Stroke.Gradient then
				local G = Instance.new("UIGradient")
				G.Color = Ramp
				for Key, Value in Stroke.Gradient do
					(G :: any)[Key] = Value
				end
				G.Parent = S
			end
			S.Parent = L
		end
		Place(L, Width, Height)
		L.Parent = Target
		return L
	end

	for _, Rotation in { 45, 30, 60, 135, -60, -135, 90, 15 } do
		Label(`Rot{Rotation}`, 640, 44, nil, { Rotation = Rotation })
	end
	Label("Rot45Tall", 300, 130, { Text = Blocks .. "\n" .. Blocks .. "\n" .. Blocks, TextSize = 40 }, { Rotation = 45 })
	Label("FadeRamp", 640, 44, { TextTransparency = 0.3 }, { Transparency = NumberSequence.new(0, 1) })
	Label("FadeRampT", 640, 44, { TextTransparency = 0.7 }, { Transparency = NumberSequence.new({ NumberSequenceKeypoint.new(0, 0.2), NumberSequenceKeypoint.new(1, 0.9) }) })
	Label("TintRamp", 640, 44, { TextColor3 = Color3.fromRGB(200, 120, 250) }, { Rotation = 30 })
	Label("LabelGradStroke", 640, 60, { Text = "WAVE Oxo", TextSize = 48, FontFace = Font.new("rbxasset://fonts/families/FredokaOne.json") }, { Rotation = 0 }, {})
	Label("StrokeGrad", 640, 60, { Text = "WAVE Oxo", TextSize = 48, FontFace = Font.new("rbxasset://fonts/families/FredokaOne.json") }, nil, { Gradient = { Rotation = 0 } })
	Label("StrokeGrad90", 640, 60, { Text = "WAVE Oxo", TextSize = 48, FontFace = Font.new("rbxasset://fonts/families/FredokaOne.json") }, nil, { Gradient = { Rotation = 90 }, Transparency = 0.3 })
	Label("BothGrad", 640, 60, { Text = "WAVE Oxo", TextSize = 48, FontFace = Font.new("rbxasset://fonts/families/FredokaOne.json") }, { Rotation = 90 }, { Gradient = { Rotation = 0 }, Color = Color3.fromRGB(20, 20, 20) })
	Label("Lines90", 400, 140, { Text = Blocks .. "\n" .. Blocks .. "\n" .. Blocks, LineHeight = 1 }, { Rotation = 90, Offset = Vector2.new(0, 0.1) })
end
