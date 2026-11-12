-- ViewportFrame probe: what else takes part in the layer a ViewportFrame's background and picture form
-- (a border, a UIGradient, a UIStroke, Rotation, a UICorner, an odd logical position at 1.5x), each with
-- ImageTransparency 0.45 and a tint, over a striped backdrop.
-- compare: --viewport 960x500 --time 0.2 --background 46,46,46
return function(Target)
	local Cells = {
		{ Border = 4 },
		{ Border = 4, BorderMode = Enum.BorderMode.Inset },
		{ Gradient = true },
		{ Stroke = true },
		{ Rotation = 8 },
		{ Corner = 24 },
		{ Odd = true },
		{ Rotation = 8, Corner = 24, Odd = true },
		{ Gradient = true, Transparency = 0 },
		{ Border = 4, Transparency = 0 },
		{ Stroke = true, Corner = 24 },
		{ Tint = false, Rotation = -20 },
	}
	for Index = 0, 7 do
		local Stripe = Instance.new("Frame")
		Stripe.Position = UDim2.fromOffset(0, Index * 62)
		Stripe.Size = UDim2.fromOffset(960, 31)
		Stripe.BackgroundColor3 = Color3.fromRGB(200, 170, 40)
		Stripe.BorderSizePixel = 0
		Stripe.Parent = Target
	end
	for Index, Cell in Cells do
		local Column = (Index - 1) % 6
		local Row = (Index - 1) // 6
		local Frame = Instance.new("ViewportFrame")
		Frame.Name = "Cell" .. Index
		Frame.Position = UDim2.fromOffset(25 + Column * 155 + (if Cell.Odd then 1 else 0), 25 + Row * 240 + (if Cell.Odd then 1 else 0))
		Frame.Size = UDim2.fromOffset(130, 205)
		Frame.BackgroundColor3 = Color3.fromRGB(30, 36, 50)
		Frame.BorderColor3 = Color3.fromRGB(240, 240, 240)
		Frame.BorderSizePixel = Cell.Border or 0
		Frame.BorderMode = Cell.BorderMode or Enum.BorderMode.Outline
		Frame.ImageTransparency = Cell.Transparency or 0.45
		Frame.ImageColor3 = if Cell.Tint == false then Color3.new(1, 1, 1) else Color3.fromRGB(255, 200, 140)
		Frame.Rotation = Cell.Rotation or 0
		Frame.Ambient = Color3.fromRGB(100, 100, 100)
		Frame.LightColor = Color3.new(1, 1, 1)
		Frame.LightDirection = Vector3.new(0, 0, -1)
		Frame.Parent = Target
		if Cell.Gradient then
			local Gradient = Instance.new("UIGradient")
			Gradient.Color = ColorSequence.new(Color3.new(1, 1, 1), Color3.fromRGB(80, 255, 120))
			Gradient.Transparency = NumberSequence.new(0, 0.6)
			Gradient.Rotation = 90
			Gradient.Parent = Frame
		end
		if Cell.Stroke then
			local Stroke = Instance.new("UIStroke")
			Stroke.Thickness = 3
			Stroke.Color = Color3.fromRGB(240, 80, 80)
			Stroke.ApplyStrokeMode = Enum.ApplyStrokeMode.Border
			Stroke.Parent = Frame
		end
		if Cell.Corner then
			local Corner = Instance.new("UICorner")
			Corner.CornerRadius = UDim.new(0, Cell.Corner)
			Corner.Parent = Frame
		end
		local Camera = Instance.new("Camera")
		Camera.CFrame = CFrame.new(0, 0, 10)
		Camera.FieldOfView = 40
		Camera.Parent = Frame
		Frame.CurrentCamera = Camera
		local Part = Instance.new("Part")
		Part.Anchored = true
		Part.Material = Enum.Material.SmoothPlastic
		Part.Size = Vector3.new(2.6, 3.6, 1)
		Part.Color = Color3.fromRGB(230, 230, 230)
		Part.TopSurface = Enum.SurfaceType.Smooth
		Part.BottomSurface = Enum.SurfaceType.Smooth
		Part.Parent = Frame
	end
end
