-- ViewportFrame probe: how ImageTransparency, BackgroundTransparency and ImageColor3 combine the frame's
-- background with its picture. Each cell holds one white SmoothPlastic block lit head-on, over a striped
-- backdrop so a faded background shows what is under it.
-- compare: --viewport 960x500 --time 0.2 --background 46,46,46
return function(Target)
	local Cells = {
		-- { ImageTransparency, BackgroundTransparency, ImageColor3, with parts, GUI child }
		{ 0, 0, Color3.new(1, 1, 1), true, false },
		{ 0.45, 0, Color3.new(1, 1, 1), true, false },
		{ 0.45, 0.5, Color3.new(1, 1, 1), true, false },
		{ 0, 0.5, Color3.new(1, 1, 1), true, false },
		{ 1, 0, Color3.new(1, 1, 1), true, false },
		{ 0.8, 0.3, Color3.new(1, 1, 1), true, false },
		{ 0, 0, Color3.fromRGB(255, 120, 60), true, false },
		{ 0.45, 0, Color3.fromRGB(255, 120, 60), true, false },
		{ 0.45, 0, Color3.new(1, 1, 1), false, false },
		{ 0.45, 0, Color3.new(1, 1, 1), true, true },
		{ 0.2, 1, Color3.new(1, 1, 1), true, false },
		{ 0.6, 0, Color3.fromRGB(90, 200, 255), true, false },
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
		Frame.Position = UDim2.fromOffset(20 + Column * 155, 20 + Row * 240)
		Frame.Size = UDim2.fromOffset(140, 220)
		Frame.BackgroundColor3 = Color3.fromRGB(30, 36, 50)
		Frame.BorderSizePixel = 0
		Frame.ImageTransparency = Cell[1]
		Frame.BackgroundTransparency = Cell[2]
		Frame.ImageColor3 = Cell[3]
		Frame.Ambient = Color3.fromRGB(100, 100, 100)
		Frame.LightColor = Color3.new(1, 1, 1)
		Frame.LightDirection = Vector3.new(0, 0, -1)
		Frame.Parent = Target
		local Camera = Instance.new("Camera")
		Camera.CFrame = CFrame.new(0, 0, 10)
		Camera.FieldOfView = 40
		Camera.Parent = Frame
		Frame.CurrentCamera = Camera
		if Cell[4] then
			local Part = Instance.new("Part")
			Part.Anchored = true
			Part.Material = Enum.Material.SmoothPlastic
			Part.Size = Vector3.new(3, 4, 1)
			Part.CFrame = CFrame.new(0, 0, 0)
			Part.Color = Color3.fromRGB(230, 230, 230)
			Part.TopSurface = Enum.SurfaceType.Smooth
			Part.BottomSurface = Enum.SurfaceType.Smooth
			Part.Parent = Frame
		end
		if Cell[5] then
			local Label = Instance.new("Frame")
			Label.Position = UDim2.fromOffset(10, 180)
			Label.Size = UDim2.fromOffset(120, 30)
			Label.BackgroundColor3 = Color3.fromRGB(220, 60, 60)
			Label.BorderSizePixel = 0
			Label.Parent = Frame
		end
	end
end
