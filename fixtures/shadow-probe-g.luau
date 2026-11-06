-- UIShadow probe G (round 4): one huge-blur square corner (BlurRadius 300: sigma 225 device px at 1.5 DPI) to read the
-- shadow's lookup profile across its straight edges and the corner's triangle fan (white shadow on black, transparent frame).
return function(Target)
	local F = Instance.new("Frame")
	F.Position = UDim2.fromOffset(480, 480)
	F.Size = UDim2.fromOffset(3000, 3000)
	F.BackgroundTransparency = 1
	F.BorderSizePixel = 0
	local Shadow = Instance.new("UIShadow")
	Shadow.Color = Color3.new(1, 1, 1)
	Shadow.BlurRadius = UDim.new(0, 300)
	Shadow.Parent = F
	F.Parent = Target
end
