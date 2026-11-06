-- UIShadow probe H (round 4): sharp square corners across BlurRadius 2..160 (sigma 1.5..120 device px) to read the
-- corner fan's segment count and the profile at small blur (white shadow on black, transparent frames, 3 * sigma apart).
return function(Target)
	local Blurs = { 2, 3, 4, 6, 8, 10, 13, 16, 20, 26, 32, 50, 64, 100, 160 }
	local X, Y, RowH = 20, 20, 0
	for _, Blur in Blurs do
		local Pad = math.ceil(1.5 * Blur) + 4
		local Size = math.max(3 * Blur + 10, 40)
		if X + 2 * Pad + Size > 1980 then
			X = 20
			Y += RowH
			RowH = 0
		end
		local F = Instance.new("Frame")
		F.Position = UDim2.fromOffset(X + Pad, Y + Pad)
		F.Size = UDim2.fromOffset(Size, Size)
		F.BackgroundTransparency = 1
		F.BorderSizePixel = 0
		local Shadow = Instance.new("UIShadow")
		Shadow.Color = Color3.new(1, 1, 1)
		Shadow.BlurRadius = UDim.new(0, Blur)
		Shadow.Parent = F
		F.Parent = Target
		X += 2 * Pad + Size
		RowH = math.max(RowH, 2 * Pad + Size)
	end
end
