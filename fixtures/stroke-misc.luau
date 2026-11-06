-- UIStroke odds and ends: rotated frames, StrokeSizingMode.ScaledSize, BorderOffset, gradient
-- strokes (rotations, transparency taper), stacked strokes with ZIndex, a gradient under a text
-- stroke, and text stroke thicknesses whose radius sits on a 0.1 px boundary.
-- compare: --viewport 1400x760 --time 0.3 --background 20,20,26
return function(Target)
	local function Frame(Name, X, Y, W, H, Radius, Props, Rotation)
		local F = Instance.new("Frame")
		F.Name = Name
		F.Position = UDim2.fromOffset(X, Y)
		F.Size = UDim2.fromOffset(W, H)
		F.BackgroundColor3 = Color3.fromRGB(70, 70, 90)
		F.BorderSizePixel = 0
		F.Rotation = Rotation or 0
		if Radius then
			local Corner = Instance.new("UICorner")
			Corner.CornerRadius = UDim.new(0, Radius)
			Corner.Parent = F
		end
		local Stroke = Instance.new("UIStroke")
		Stroke.ApplyStrokeMode = Enum.ApplyStrokeMode.Border
		Stroke.Color = Color3.fromRGB(255, 190, 40)
		for Key, Value in Props do
			(Stroke :: any)[Key] = Value
		end
		Stroke.Parent = F
		F.Parent = Target
		return F, Stroke
	end

	-- Rotated frames
	Frame("Rot1", 40, 40, 100, 50, nil, { Thickness = 2 }, 15)
	Frame("Rot2", 200, 40, 100, 50, 12, { Thickness = 2 }, 15)
	Frame("Rot3", 360, 40, 100, 50, nil, { Thickness = 6, BorderStrokePosition = Enum.BorderStrokePosition.Center }, 45)
	Frame("Rot4", 520, 40, 100, 50, 12, { Thickness = 6 }, 45)

	-- ScaledSize: thickness relative to the frame size
	Frame("Scaled1", 700, 40, 100, 50, nil, { Thickness = 0.05, StrokeSizingMode = Enum.StrokeSizingMode.ScaledSize })
	Frame("Scaled2", 840, 40, 200, 80, 10, { Thickness = 0.05, StrokeSizingMode = Enum.StrokeSizingMode.ScaledSize })
	Frame("Scaled3", 1080, 40, 60, 120, nil, { Thickness = 0.1, StrokeSizingMode = Enum.StrokeSizingMode.ScaledSize })

	-- BorderOffset
	Frame("Offset1", 40, 190, 100, 50, nil, { Thickness = 3, BorderOffset = UDim.new(0, 4) })
	Frame("Offset2", 200, 190, 100, 50, 10, { Thickness = 3, BorderOffset = UDim.new(0, 4) })
	Frame("Offset3", 360, 190, 100, 50, 10, { Thickness = 3, BorderOffset = UDim.new(0, -6) })
	Frame("Offset4", 520, 190, 100, 50, nil, { Thickness = 4, BorderOffset = UDim.new(0.1, 0), BorderStrokePosition = Enum.BorderStrokePosition.Center })

	-- Gradient strokes
	local function Gradient(Parent, Rotation, Color, Transparency)
		local G = Instance.new("UIGradient")
		G.Rotation = Rotation
		if Color then
			G.Color = Color
		end
		if Transparency then
			G.Transparency = Transparency
		end
		G.Parent = Parent
	end
	local RedBlue = ColorSequence.new(Color3.fromRGB(255, 0, 0), Color3.fromRGB(0, 0, 255))
	local Taper = NumberSequence.new({ NumberSequenceKeypoint.new(0, 0), NumberSequenceKeypoint.new(0.5, 0.2), NumberSequenceKeypoint.new(1, 1) })
	for Index, Rotation in { 45, 90, 180 } do
		local _, S = Frame(`Grad{Index}`, 700 + (Index - 1) * 140, 200, 110, 60, 12, { Thickness = 5, Color = Color3.new(1, 1, 1) })
		Gradient(S, Rotation, RedBlue)
	end
	local _, TaperStroke = Frame("Taper", 1120, 200, 110, 60, 12, { Thickness = 4, Color = Color3.new(0, 0, 0) })
	Gradient(TaperStroke, 180, nil, Taper)

	-- Stacked strokes on one frame
	local Stacked = Frame("Stacked", 40, 320, 140, 70, 14, { Thickness = 8, Color = Color3.fromRGB(255, 60, 60), ZIndex = -1 })
	local Mid = Instance.new("UIStroke")
	Mid.Thickness = 3
	Mid.Color = Color3.new(1, 1, 1)
	Mid.BorderStrokePosition = Enum.BorderStrokePosition.Center
	Mid.ApplyStrokeMode = Enum.ApplyStrokeMode.Border
	Mid.ZIndex = 2
	Mid.Parent = Stacked
	local InnerStroke = Instance.new("UIStroke")
	InnerStroke.Thickness = 2
	InnerStroke.Color = Color3.fromRGB(40, 220, 255)
	InnerStroke.BorderStrokePosition = Enum.BorderStrokePosition.Inner
	InnerStroke.ApplyStrokeMode = Enum.ApplyStrokeMode.Border
	InnerStroke.Transparency = 0.3
	InnerStroke.Parent = Stacked

	-- Text stroke with a UIGradient under the stroke, and a text fill gradient
	local function TextLabel(Name, X, Y, Thickness, Text)
		local Label = Instance.new("TextLabel")
		Label.Name = Name
		Label.BackgroundTransparency = 1
		Label.FontFace = Font.new("rbxasset://fonts/families/FredokaOne.json")
		Label.TextSize = 40
		Label.Text = Text
		Label.TextColor3 = Color3.new(1, 1, 1)
		Label.TextXAlignment = Enum.TextXAlignment.Left
		Label.Position = UDim2.fromOffset(X, Y)
		Label.Size = UDim2.fromOffset(220, 60)
		Label.Parent = Target
		local Stroke = Instance.new("UIStroke")
		Stroke.Thickness = Thickness
		Stroke.Color = Color3.new(1, 1, 1)
		Stroke.Parent = Label
		return Label, Stroke
	end
	local _, TextStroke = TextLabel("TextGrad", 240, 320, 4, "Gradient")
	Gradient(TextStroke, 90, RedBlue)
	local FillLabel = TextLabel("FillGrad", 500, 320, 3, "Fill")
	FillLabel:FindFirstChildOfClass("UIStroke").Color = Color3.new(0, 0, 0)
	Gradient(FillLabel, 0, ColorSequence.new(Color3.fromRGB(255, 255, 0), Color3.fromRGB(0, 255, 255)))

	-- Text stroke radius boundaries (1.5 x thickness x 10 lands on integers)
	for Index, Thickness in { 0.2, 1.4, 2.8, 3.4, 4.6, 0.6 } do
		local Label = TextLabel(`Edge{Index}`, 40 + (Index - 1) * 200, 440, Thickness, "Iol")
		Label.TextTransparency = 1
		Label.Size = UDim2.fromOffset(180, 60)
	end
end
