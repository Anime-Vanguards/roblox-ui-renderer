-- UIStroke ZIndex against the frame's own background and its children: Inner strokes on opaque frames
-- with negative / zero / positive ZIndex, and children with lower / higher ZIndex than the stroke.
-- compare: --viewport 900x300 --time 0.2 --background 20,20,26
return function(Target)
	local function Frame(Name, X, Y, StrokeZ, ChildZ)
		local F = Instance.new("Frame")
		F.Name = Name
		F.Position = UDim2.fromOffset(X, Y)
		F.Size = UDim2.fromOffset(120, 80)
		F.BackgroundColor3 = Color3.fromRGB(70, 70, 90)
		F.BorderSizePixel = 0
		F.Parent = Target

		local Corner = Instance.new("UICorner")
		Corner.CornerRadius = UDim.new(0, 12)
		Corner.Parent = F

		local Stroke = Instance.new("UIStroke")
		Stroke.Thickness = 6
		Stroke.Color = Color3.fromRGB(255, 190, 40)
		Stroke.BorderStrokePosition = Enum.BorderStrokePosition.Center
		Stroke.ZIndex = StrokeZ
		Stroke.Parent = F

		if ChildZ then
			local Child = Instance.new("Frame")
			Child.Name = "Child"
			Child.Position = UDim2.fromOffset(-10, 30)
			Child.Size = UDim2.new(1, 20, 0, 20)
			Child.BackgroundColor3 = Color3.fromRGB(255, 80, 120)
			Child.BorderSizePixel = 0
			Child.ZIndex = ChildZ
			Child.Parent = F
		end
	end

	Frame("NegNoChild", 40, 40, -5, nil)
	Frame("ZeroNoChild", 200, 40, 0, nil)
	Frame("PosNoChild", 360, 40, 5, nil)
	Frame("StrokeAboveChild", 520, 40, 5, 1)
	Frame("StrokeBelowChild", 680, 40, 1, 5)
	Frame("NegStrokeChild", 40, 170, -5, 1)
	Frame("EqualChild", 200, 170, 1, 1)
end
