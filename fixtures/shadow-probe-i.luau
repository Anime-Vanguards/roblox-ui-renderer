-- UIShadow probe I (round 4): long bars 600 logical wide, heights 5..80, BlurRadius 40 (sigma 30 device px at 1.5 DPI),
-- 124 logical apart so their shadows do not overlap: the cross-section of shapes thinner than 6 sigma (white shadow on
-- black, transparent frames).
return function(Target)
	local Columns = {
		{ X = 60, Heights = { 5, 10, 15, 20, 25, 30, 35 } },
		{ X = 700, Heights = { 40, 45, 50, 55, 60 } },
		{ X = 1340, Heights = { 65, 70, 75, 80 } },
	}
	for _, Column in Columns do
		local Y = 64
		for _, H in Column.Heights do
			local F = Instance.new("Frame")
			F.Position = UDim2.fromOffset(Column.X, Y)
			F.Size = UDim2.fromOffset(600, H)
			F.BackgroundTransparency = 1
			F.BorderSizePixel = 0
			local Shadow = Instance.new("UIShadow")
			Shadow.Color = Color3.new(1, 1, 1)
			Shadow.BlurRadius = UDim.new(0, 40)
			Shadow.Parent = F
			F.Parent = Target
			Y += H + 124
		end
	end
end
