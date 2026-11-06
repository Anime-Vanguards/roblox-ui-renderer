-- UIShadow probe N (round 4): the amplitude of thin shapes. Bars 80 logical wide and 1..64 logical high at BlurRadius 20
-- (sigma 15 device px at 1.5 DPI; half-heights 0.05..3.2 sigma), shadows kept apart (white shadow on black).
return function(Target)
	local Index = 0
	local X, Y, RowH = 30, 30, 0
	for H = 1, 64 do
		if X + 80 + 30 > 2000 then
			X = 30
			Y += RowH + 62
			RowH = 0
		end
		local F = Instance.new("Frame")
		F.Position = UDim2.fromOffset(X, Y)
		F.Size = UDim2.fromOffset(80, H)
		F.BackgroundTransparency = 1
		F.BorderSizePixel = 0
		local Shadow = Instance.new("UIShadow")
		Shadow.Color = Color3.new(1, 1, 1)
		Shadow.BlurRadius = UDim.new(0, 20)
		Shadow.Parent = F
		F.Parent = Target
		X += 80 + 62
		RowH = math.max(RowH, H)
		Index += 1
	end
end
