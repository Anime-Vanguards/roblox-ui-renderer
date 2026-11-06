-- Direct children and WorldModels share the frame's camera, lighting and normal GUI compositing.
return function(Target)
	for Index = 1, 3 do
		local Frame = Instance.new("ViewportFrame")
		Frame.Name = "Viewport" .. Index
		Frame.Position = UDim2.fromOffset(20 + (Index - 1) * 260, 30)
		Frame.Size = UDim2.fromOffset(240, 280)
		Frame.BackgroundColor3 = Color3.fromRGB(30, 36, 50)
		Frame.BorderSizePixel = 0
		Frame.Ambient = Color3.fromRGB(100, 100, 100)
		Frame.LightColor = Color3.new(1, 1, 1)
		Frame.LightDirection = Vector3.new(-1, -1, -1)
		Frame.ImageTransparency = if Index == 3 then 0.45 else 0
		Frame.Rotation = if Index == 3 then 8 else 0
		Frame.Parent = Target
		local Corner = Instance.new("UICorner", Frame)
		Corner.CornerRadius = UDim.new(0, 32)
		local Camera = Instance.new("Camera")
		Camera.CFrame = CFrame.lookAt(Vector3.new(6, 4, 9), Vector3.new(0, 0, 0))
		Camera.FieldOfView = 35
		Frame.CurrentCamera = Camera -- deliberately outside the ViewportFrame
		local World = if Index == 1 then Frame else Instance.new("WorldModel", Frame)
		local Model = Instance.new("Model", World)
		for PartIndex = 1, 3 do
			local Part = Instance.new("Part", Model)
			Part.Anchored = true
			Part.Size = Vector3.new(2, 2, 2)
			Part.CFrame = CFrame.new((PartIndex - 2) * 1.8, 0, 0)
			Part.Color = ({ Color3.fromRGB(245, 90, 70), Color3.fromRGB(80, 220, 130), Color3.fromRGB(90, 145, 250) })[PartIndex]
			Part.Shape = ({ Enum.PartType.Block, Enum.PartType.Ball, Enum.PartType.Cylinder })[PartIndex]
		end
	end
end
