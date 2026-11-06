-- UIShadow measurement fixture: one shadow per 260x200 cell so tails never overlap.
return function(Target)
	local Cases = {
		{ Blur = 8 },
		{ Blur = 16 },
		{ Blur = 32 },
		{ Blur = 16, Radius = 20 },
		{ Blur = 16, Spread = UDim2.fromOffset(10, 4) },
		{ Blur = 16, Offset = UDim2.fromOffset(12, 18) },
		{ Blur = 16, Inset = true },
		{ Blur = 16, Transparency = 0.6 },
		{ Blur = 16, Clear = true },
		{ Blur = 16, Behind = false },
		{ Blur = 0, Spread = UDim2.fromOffset(6, 6) },
		{ Blur = 16, Radius = 40, Small = true },
	}
	for Index, Case in Cases do
		local Column = (Index - 1) % 4
		local Row = (Index - 1) // 4
		local F = Instance.new("Frame")
		F.Size = if Case.Small then UDim2.fromOffset(40, 40) else UDim2.fromOffset(100, 70)
		F.Position = UDim2.fromOffset(80 + Column * 260, 65 + Row * 200)
		F.BackgroundColor3 = Color3.fromRGB(235, 235, 245)
		F.BackgroundTransparency = if Case.Clear then 1 else 0
		F.BorderSizePixel = 0
		if Case.Radius then
			local Corner = Instance.new("UICorner")
			Corner.CornerRadius = UDim.new(0, Case.Radius)
			Corner.Parent = F
		end
		local Shadow = Instance.new("UIShadow")
		Shadow.Color = Color3.fromRGB(255, 50, 50)
		Shadow.BlurRadius = UDim.new(0, Case.Blur)
		if Case.Spread then
			Shadow.Spread = Case.Spread
		end
		if Case.Offset then
			Shadow.Offset = Case.Offset
		end
		if Case.Inset then
			Shadow.Inset = true
		end
		if Case.Transparency then
			Shadow.Transparency = Case.Transparency
		end
		if Case.Behind == false then
			Shadow.ShowBehindParent = false
		end
		Shadow.Parent = F
		F.Parent = Target
	end
end
