-- UIShadow probe F: UIScale interaction (own vs ancestor scale; blur, spread, offset) on black.
return function(Target)
	local function Cell(X, Y, OwnScale, ParentScale, Props)
		local Holder = Instance.new("Frame")
		Holder.BackgroundTransparency = 1
		Holder.Position = UDim2.fromOffset(X, Y)
		Holder.Size = UDim2.fromOffset(200, 150)
		Holder.Parent = Target
		if ParentScale then
			local S = Instance.new("UIScale")
			S.Scale = ParentScale
			S.Parent = Holder
		end
		local F = Instance.new("Frame")
		F.BackgroundTransparency = 1
		F.BorderSizePixel = 0
		F.Position = UDim2.fromOffset(40, 40)
		F.Size = UDim2.fromOffset(80, 50)
		F.Parent = Holder
		if OwnScale then
			local S = Instance.new("UIScale")
			S.Scale = OwnScale
			S.Parent = F
		end
		local Shadow = Instance.new("UIShadow")
		Shadow.Color = Color3.new(1, 1, 1)
		Shadow.BlurRadius = UDim.new(0, 16)
		for Key, Value in Props or {} do
			(Shadow :: any)[Key] = Value
		end
		Shadow.Parent = F
	end
	Cell(20, 20, nil, nil)
	Cell(320, 20, 1.5, nil)
	Cell(620, 20, nil, 1.5)
	Cell(920, 20, 1.5, nil, { Spread = UDim2.fromOffset(20, 20) })
	Cell(1220, 20, nil, 1.5, { Spread = UDim2.fromOffset(20, 20) })
	Cell(20, 300, 1.5, nil, { Offset = UDim2.fromOffset(20, 20) })
	Cell(320, 300, nil, 1.5, { Offset = UDim2.fromOffset(20, 20) })
	Cell(620, 300, 0.5, nil, { BlurRadius = UDim.new(0, 30) })
	Cell(920, 300, nil, 0.5, { BlurRadius = UDim.new(0, 30) })
	Cell(1220, 300, 1.5, nil, { BlurRadius = UDim.new(0.2, 0) })
end
