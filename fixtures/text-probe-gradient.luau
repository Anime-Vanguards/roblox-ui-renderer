-- Text UIGradient probe: slow horizontal ramps (R up, G down, B 64 -> 192) over wide labels of "I"
-- stems, next to frames with the same gradient, so fully covered text pixels show the gradient colour
-- column by column. Variants: text colour, TextTransparency, label width, Offset, Rotation 180, a
-- multi-key sequence and a transparency ramp. tools/analysis/text_gradient.py fits the model.
-- rur compare --fixture fixtures/text-probe-gradient.luau --viewport 2060x1150 --time 0.2 --background 0,0,0
return function(Target)
	local Ramp = ColorSequence.new({
		ColorSequenceKeypoint.new(0, Color3.fromRGB(0, 255, 64)),
		ColorSequenceKeypoint.new(1, Color3.fromRGB(255, 0, 192)),
	})
	local MultiKey = ColorSequence.new({
		ColorSequenceKeypoint.new(0, Color3.fromRGB(255, 253, 190)),
		ColorSequenceKeypoint.new(0.15, Color3.fromRGB(255, 247, 161)),
		ColorSequenceKeypoint.new(0.55, Color3.fromRGB(255, 221, 26)),
		ColorSequenceKeypoint.new(1, Color3.fromRGB(255, 119, 0)),
	})

	local Y = 4
	local function Gradient(Parent, Props)
		local G = Instance.new("UIGradient")
		G.Color = Ramp
		for Key, Value in Props or {} do
			(G :: any)[Key] = Value
		end
		G.Parent = Parent
	end

	local function Row(Name, Width, Shift, TextProps, GradientProps)
		local Label = Instance.new("TextLabel")
		Label.Name = Name
		Label.BackgroundTransparency = 1
		Label.TextColor3 = Color3.new(1, 1, 1)
		Label.FontFace = Font.new("rbxasset://fonts/families/SourceSansPro.json", Enum.FontWeight.Bold)
		Label.TextSize = 40
		Label.Text = string.rep("I", 160)
		Label.TextXAlignment = Enum.TextXAlignment.Left
		Label.TextYAlignment = Enum.TextYAlignment.Top
		Label.ClipsDescendants = false
		Label.Size = UDim2.fromOffset(Width, 40)
		Label.Position = UDim2.fromOffset(4 + Shift, Y)
		for Key, Value in TextProps or {} do
			(Label :: any)[Key] = Value
		end
		Gradient(Label, GradientProps)
		Label.Parent = Target
		Y += 44
	end

	local function FrameRow(Name, Width, GradientProps)
		local Frame = Instance.new("Frame")
		Frame.Name = Name
		Frame.BorderSizePixel = 0
		Frame.BackgroundColor3 = Color3.new(1, 1, 1)
		Frame.Size = UDim2.fromOffset(Width, 20)
		Frame.Position = UDim2.fromOffset(4, Y)
		Gradient(Frame, GradientProps)
		Frame.Parent = Target
		Y += 24
	end

	FrameRow("F2000", 2000)
	Row("T2000a", 2000, 0)
	Row("T2000b", 2000, 0, { Text = " " .. string.rep("I", 160) })
	FrameRow("F1000", 1000)
	Row("T1000", 1000, 0)
	Row("T1000x", 1000, 3)
	Row("Grey", 2000, 0, { TextColor3 = Color3.fromRGB(128, 200, 60) })
	Row("Faded", 2000, 0, { TextTransparency = 0.5 })
	Row("Offset", 2000, 0, nil, { Offset = Vector2.new(0.2, 0) })
	Row("Rot180", 2000, 0, nil, { Rotation = 180 })
	FrameRow("FMulti", 2000, { Color = MultiKey })
	Row("Multi", 2000, 0, nil, { Color = MultiKey })
	Row("Alpha", 2000, 0, nil, { Transparency = NumberSequence.new(0, 1) })
	Row("Rot90", 600, 0, { Size = UDim2.fromOffset(600, 120), TextWrapped = true, Text = string.rep("I", 120) }, { Rotation = 90 })
end
