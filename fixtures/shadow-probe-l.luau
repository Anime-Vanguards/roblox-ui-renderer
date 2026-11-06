-- UIShadow probe L (round 4): long bars at BlurRadius 20 (sigma 15 device px at 1.5 DPI), heights 2..40 logical, and
-- at BlurRadius 60 (sigma 45 px), heights 6..84: the amplitude of shapes thinner than 6 sigma at two more blur sizes
-- (white shadow on black, transparent frames, shadows kept apart).
return function(Target)
	local function Bar(X, Y, W, H, Blur)
		local F = Instance.new("Frame")
		F.Position = UDim2.fromOffset(X, Y)
		F.Size = UDim2.fromOffset(W, H)
		F.BackgroundTransparency = 1
		F.BorderSizePixel = 0
		local Shadow = Instance.new("UIShadow")
		Shadow.Color = Color3.new(1, 1, 1)
		Shadow.BlurRadius = UDim.new(0, Blur)
		Shadow.Parent = F
		F.Parent = Target
	end
	for Column = 0, 1 do
		local Y = 40
		for Index = 1, 10 do
			local H = 2 * (Column * 10 + Index)
			Bar(40 + Column * 300, Y, 220, H, 20)
			Y += H + 52
		end
	end
	for Column = 0, 2 do
		local Y = 100
		for Index = 1, 4 do
			local H = 6 * (Column * 4 + Index) + (Column == 2 and 6 * (Index - 1) or 0)
			Bar(700 + Column * 440, Y, 260, H, 60)
			Y += H + 190
		end
	end
end
