-- UIShadow probe D: Spread, Offset, Transparency, Color, scale BlurRadius, blur 0 (transparent frames, black backdrop).
return function(Target)
	local Cells = {
		{ Spread = UDim2.fromOffset(10, 10) },
		{ Spread = UDim2.fromOffset(20, 4) },
		{ Spread = UDim2.fromOffset(-10, -10) },
		{ Spread = UDim2.fromScale(0.2, 0.2) },
		{ Offset = UDim2.fromOffset(10, 15) },
		{ Offset = UDim2.fromScale(0.1, 0.25) },
		{ Transparency = 0.5 },
		{ Color = Color3.fromRGB(255, 60, 0) },
		{ Blur = UDim.new(0.25, 0) },
		{ Blur = UDim.new(0, 0) },
		{ Blur = UDim.new(0, 0), Spread = UDim2.fromOffset(6, 6) },
		{ Blur = UDim.new(0, 4) },
		{ Blur = UDim.new(0.5, 10) },
		{ Spread = UDim2.fromOffset(10, 10), Radius = 12 },
		{ Offset = UDim2.fromOffset(-12, 8), Spread = UDim2.fromOffset(4, 4), Radius = 20 },
		{ Transparency = 0.8, Color = Color3.fromRGB(0, 200, 255) },
	}
	for Index, Cell in Cells do
		local Column = (Index - 1) % 6
		local Row = (Index - 1) // 6
		local F = Instance.new("Frame")
		F.Position = UDim2.fromOffset(80 + Column * 320, 90 + Row * 330)
		F.Size = UDim2.fromOffset(120, 80)
		F.BackgroundTransparency = 1
		F.BorderSizePixel = 0
		if Cell.Radius then
			local Corner = Instance.new("UICorner")
			Corner.CornerRadius = UDim.new(0, Cell.Radius)
			Corner.Parent = F
		end
		local Shadow = Instance.new("UIShadow")
		Shadow.Color = Cell.Color or Color3.new(1, 1, 1)
		Shadow.BlurRadius = Cell.Blur or UDim.new(0, 20)
		if Cell.Spread then
			Shadow.Spread = Cell.Spread
		end
		if Cell.Offset then
			Shadow.Offset = Cell.Offset
		end
		if Cell.Transparency then
			Shadow.Transparency = Cell.Transparency
		end
		Shadow.Parent = F
		F.Parent = Target
	end
end
