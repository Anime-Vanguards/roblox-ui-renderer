-- CanvasGroup probe: sharp images inside groups at integral vs half-pixel physical positions/sizes, opaque
-- and transparent group backgrounds, with and without GroupTransparency (the group texture is resampled
-- when its rect is fractional).
return function(Target)
	local function Group(Name, X, Y, W, H, Props)
		local G = Instance.new("CanvasGroup")
		G.Name = Name
		G.Position = UDim2.fromOffset(X, Y)
		G.Size = UDim2.fromOffset(W, H)
		G.BackgroundColor3 = Color3.fromRGB(40, 70, 130)
		G.BorderSizePixel = 0
		for Key, Value in Props or {} do
			(G :: any)[Key] = Value
		end
		G.Parent = Target

		local Label = Instance.new("ImageLabel")
		Label.Name = "Motor"
		Label.Position = UDim2.fromOffset(10, 10)
		Label.Size = UDim2.fromOffset(64, 64)
		Label.BackgroundTransparency = 1
		Label.Image = "rbxasset://textures/ui/Motor.png"
		Label.Parent = G

		local Bar = Instance.new("Frame")
		Bar.Name = "Bar"
		Bar.Position = UDim2.fromOffset(84, 10)
		Bar.Size = UDim2.fromOffset(20, 64)
		Bar.BackgroundColor3 = Color3.fromRGB(250, 220, 60)
		Bar.BorderSizePixel = 0
		Bar.Parent = G
		return G
	end

	local Row = { { 10, 10, 120, 90 }, { 150, 10, 121, 90 }, { 290, 11, 120, 91 }, { 431, 11, 121, 91 } }
	for Index, R in Row do
		Group(`Opaque{Index}`, R[1], R[2], R[3], R[4])
		Group(`Clear{Index}`, R[1], R[2] + 120, R[3], R[4], { BackgroundTransparency = 1 })
		Group(`Faded{Index}`, R[1], R[2] + 240, R[3], R[4], { BackgroundTransparency = 1, GroupTransparency = 0.1 })
	end
end
