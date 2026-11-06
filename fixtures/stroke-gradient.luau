-- UIStroke gradient probe: Border strokes with a UIGradient on frames with no background, so only the stroke's
-- pixels show. Rows: rounded Outer strokes at six rotations, sharp Outer strokes, rounded Center / Inner
-- strokes, three-keypoint colours and a transparency taper; frames at whole and fractional logical positions.
-- compare: --viewport 1200x800 --time 0.3 --background 20,20,26
-- analysis: python tools/analysis/stroke_gradient.py stroke-gradient
return function(Target)
	local RedBlue = ColorSequence.new(Color3.fromRGB(255, 0, 0), Color3.fromRGB(0, 0, 255))
	local Three = ColorSequence.new({
		ColorSequenceKeypoint.new(0, Color3.fromRGB(255, 200, 0)),
		ColorSequenceKeypoint.new(0.35, Color3.fromRGB(20, 220, 90)),
		ColorSequenceKeypoint.new(1, Color3.fromRGB(120, 40, 255)),
	})
	local Taper = NumberSequence.new({ NumberSequenceKeypoint.new(0, 0), NumberSequenceKeypoint.new(0.5, 0.3), NumberSequenceKeypoint.new(1, 1) })

	local function Probe(Name, X, Y, W, H, Radius, Stroke, Gradient)
		local F = Instance.new("Frame")
		F.Name = Name
		F.Position = UDim2.new(0, 0, 0, 0) + UDim2.fromScale(X / 1200, Y / 800)
		F.Size = UDim2.fromOffset(W, H)
		F.BackgroundTransparency = 1
		F.BorderSizePixel = 0
		if Radius then
			local Corner = Instance.new("UICorner")
			Corner.CornerRadius = UDim.new(0, Radius)
			Corner.Parent = F
		end
		local S = Instance.new("UIStroke")
		S.ApplyStrokeMode = Enum.ApplyStrokeMode.Border
		S.Color = Color3.new(1, 1, 1)
		for Key, Value in Stroke do
			(S :: any)[Key] = Value
		end
		S.Parent = F
		local G = Instance.new("UIGradient")
		for Key, Value in Gradient do
			(G :: any)[Key] = Value
		end
		G.Parent = S
		F.Parent = Target
	end

	local Rotations = { 0, 45, 90, 135, 180, 30 }
	for Index, Rotation in Rotations do
		local X = 30 + (Index - 1) * 190
		Probe(`Round{Rotation}`, X, 30, 150, 80, 14, { Thickness = 6 }, { Color = RedBlue, Rotation = Rotation })
		Probe(`Sharp{Rotation}`, X + 0.4, 150.3, 150, 80, nil, { Thickness = 5 }, { Color = RedBlue, Rotation = Rotation })
	end
	local Positions = { Enum.BorderStrokePosition.Center, Enum.BorderStrokePosition.Inner }
	for Index, Position in Positions do
		Probe(`Pos{Index}`, 30 + (Index - 1) * 190, 270, 150, 80, 14, { Thickness = 8, BorderStrokePosition = Position }, { Color = RedBlue, Rotation = 0 })
	end
	Probe("Three0", 410, 270, 150, 80, 10, { Thickness = 6 }, { Color = Three, Rotation = 0 })
	Probe("Three60", 600, 270, 150, 80, 10, { Thickness = 6 }, { Color = Three, Rotation = 60 })
	Probe("Taper", 790, 270, 150, 80, 14, { Thickness = 6, Color = Color3.fromRGB(255, 190, 40) }, { Transparency = Taper, Rotation = 180 })
	Probe("Offset", 980.7, 270.2, 150, 80, 14, { Thickness = 6 }, { Color = RedBlue, Rotation = 0, Offset = Vector2.new(0.2, 0) })
	Probe("Thin", 30, 400, 300, 40, 20, { Thickness = 2 }, { Color = Three, Rotation = 0 })
	Probe("Thick", 380.5, 400.5, 300, 120, 30, { Thickness = 12 }, { Color = RedBlue, Rotation = 20 })
end
