-- Hard UIShadows (BlurRadius 0) with Transparency k / 40 (k = 1 .. 39) and a few near byte boundaries: white
-- shadows of transparent 40 x 30 frames on black. Each square's value is its alpha byte: floor(255 a), or the
-- 8-bit vertex alpha min(255, floor(256 a)) other transparencies take (a = 1 - Transparency). Rows 5-6: the same
-- transparencies with ShowBehindParent false and a Spread of 12 px, so the frame cuts its shadow to a ring.
-- compare: --viewport 640x560 --time 0.2 --background 0,0,0
return function(Target)
	local Values = {}
	for K = 1, 39 do
		table.insert(Values, K / 40)
	end
	for _, Extra in { 0.001, 0.002, 0.004, 0.996, 0.998, 0.5 - 1 / 512, 0.5 + 1 / 512, 1 / 3, 2 / 3, 0, 0.0005, 0.9995 } do
		table.insert(Values, Extra)
	end
	for Index, Transparency in Values do
		local Column, Row = (Index - 1) % 12, (Index - 1) // 12
		local F = Instance.new("Frame")
		F.Name = `T{Index}`
		F.Position = UDim2.fromOffset(20 + Column * 50, 20 + Row * 50)
		F.Size = UDim2.fromOffset(40, 30)
		F.BackgroundTransparency = 1
		F.BorderSizePixel = 0
		local Shadow = Instance.new("UIShadow")
		Shadow.Color = Color3.new(1, 1, 1)
		Shadow.BlurRadius = UDim.new(0, 0)
		Shadow.Transparency = Transparency
		Shadow.Parent = F
		F.Parent = Target
	end
	for Index = 1, 24 do
		local Transparency = Values[Index * 2 - 1]
		local Column, Row = (Index - 1) % 12, (Index - 1) // 12
		local F = Instance.new("Frame")
		F.Name = `C{Index}`
		F.Position = UDim2.fromOffset(26 + Column * 50, 290 + Row * 60)
		F.Size = UDim2.fromOffset(28, 24)
		F.BackgroundTransparency = 1
		F.BorderSizePixel = 0
		local Shadow = Instance.new("UIShadow")
		Shadow.Color = Color3.new(1, 1, 1)
		Shadow.BlurRadius = UDim.new(0, 0)
		Shadow.Spread = UDim2.fromOffset(12, 12)
		Shadow.ShowBehindParent = false
		Shadow.Transparency = Transparency
		Shadow.Parent = F
		F.Parent = Target
	end
end
