-- UIShadow vertex alpha against half size / sigma: white blurred shadows of transparent bars 50 logical px wide and
-- 1 to 32 px high, at BlurRadius 10, 11, 12 and 13 (index i: BlurRadius 10 + i // 32, height 1 + i % 32), so the
-- bars' half heights cover 0.05 to 3.2 sigma at many ratios. Each bar's centre pixel is the profile's peak times
-- the vertex alpha, round(255 g(h / sigma) (1 - Transparency)), whose g was fitted, not found.
-- compare: --viewport 1200x900 --time 0.2 --background 0,0,0
return function(Target)
	for Index = 0, 127 do
		local Column, Row = Index % 12, Index // 12
		local Height = 1 + Index % 32
		local F = Instance.new("Frame")
		F.Name = `B{10 + Index // 32}_H{Height}`
		F.Position = UDim2.fromOffset(25 + Column * 96, 30 + Row * 80 + (32 - Height) // 2)
		F.Size = UDim2.fromOffset(50, Height)
		F.BackgroundTransparency = 1
		F.BorderSizePixel = 0
		local Shadow = Instance.new("UIShadow")
		Shadow.Color = Color3.new(1, 1, 1)
		Shadow.BlurRadius = UDim.new(0, 10 + Index // 32)
		Shadow.Parent = F
		F.Parent = Target
	end
end
