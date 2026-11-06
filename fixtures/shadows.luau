-- UIShadow: blur radius, spread, offset, inset, transparency, with corners.
return function(Target)
	local function Frame(X, Y, W, H, Radius, ShadowProps, Color)
		local F = Instance.new("Frame")
		F.Position = UDim2.fromOffset(X, Y)
		F.Size = UDim2.fromOffset(W, H)
		F.BackgroundColor3 = Color or Color3.fromRGB(230, 230, 240)
		F.BorderSizePixel = 0
		if Radius then
			local Corner = Instance.new("UICorner")
			Corner.CornerRadius = UDim.new(0, Radius)
			Corner.Parent = F
		end
		local Shadow = Instance.new("UIShadow")
		Shadow.Color = Color3.fromRGB(255, 60, 60)
		for Key, Value in ShadowProps do
			(Shadow :: any)[Key] = Value
		end
		Shadow.Parent = F
		F.Parent = Target
		return F
	end

	for Index, Blur in { 0, 4, 12, 24, 48 } do
		Frame(40 + (Index - 1) * 140, 40, 90, 60, nil, { BlurRadius = UDim.new(0, Blur) })
		Frame(40 + (Index - 1) * 140, 160, 90, 60, 16, { BlurRadius = UDim.new(0, Blur) })
	end

	Frame(40, 290, 90, 60, 12, { BlurRadius = UDim.new(0, 12), Spread = UDim2.fromOffset(8, 8) })
	Frame(180, 290, 90, 60, 12, { BlurRadius = UDim.new(0, 12), Offset = UDim2.fromOffset(10, 15) })
	Frame(320, 290, 90, 60, 12, { BlurRadius = UDim.new(0, 12), Transparency = 0.5 })
	Frame(460, 290, 90, 60, 12, { BlurRadius = UDim.new(0, 12), Inset = true })
	Frame(600, 290, 90, 60, 12, { BlurRadius = UDim.new(0, 24), Spread = UDim2.fromOffset(2, 3), Color = Color3.fromRGB(0, 230, 118) })

	local Clear = Frame(40, 400, 90, 60, 12, { BlurRadius = UDim.new(0, 16) })
	Clear.BackgroundTransparency = 1
	Frame(180, 400, 90, 60, 12, { BlurRadius = UDim.new(0, 16), ShowBehindParent = false })
	Frame(320, 400, 90, 60, 12, { BlurRadius = UDim.new(0.2, 0) })
end
