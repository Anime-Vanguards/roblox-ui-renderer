-- CanvasGroup texture area limit: a wide group of orbit rings rebuilt from a story's scene (rect -174.491333,
-- 49.3548279, 1548.98267 x 801.290344: 2324 x 1202 device px at 1.5 DPI, overhanging the screen) with its faint
-- 1.34 px Outer ring strokes (as fixtures/group-probe-orbits.luau), plus sharp white bars at whole logical
-- positions: vertical ones read the texture's x scale, horizontal ones its y scale. Studio fits the texture into
-- 2048 x 1024 px of area (2015 x 1043 texels here; notes/images.md round 11).
-- compare: --viewport 1200x780 --time 0.2 --background 12,10,24
local Children = {
	{ Name = "Ring1340", Class = "Frame", X = 731.6129759999999, Y = -2.6798401, W = 1795.53357, H = 1795.53357, Corner = 897.7668, T = 0.9649999737739563, Th = 1.339950442314148, Bg = 1 },
	{ Name = "Carrier1340_1", Class = "Frame", X = 731.6129759999999, Y = -2.6798401, W = 1795.53357, H = 1795.53357, Corner = None, T = None, Th = None, Bg = 1 },
	{ Name = "Carrier1340_2", Class = "Frame", X = 731.6129759999999, Y = -2.6798401, W = 1795.53357, H = 1795.53357, Corner = None, T = None, Th = None, Bg = 1 },
	{ Name = "Carrier1340_3", Class = "Frame", X = 731.6129759999999, Y = -2.6798401, W = 1795.53357, H = 1795.53357, Corner = None, T = None, Th = None, Bg = 1 },
	{ Name = "Ring1010", Class = "Frame", X = 952.7047729999999, Y = 218.41195710000002, W = 1353.34998, H = 1353.34998, Corner = 676.675, T = 0.949999988079071, Th = 1.339950442314148, Bg = 1 },
	{ Name = "Carrier1010_1", Class = "Frame", X = 952.7047729999999, Y = 218.41195710000002, W = 1353.34998, H = 1353.34998, Corner = None, T = None, Th = None, Bg = 1 },
	{ Name = "Carrier1010_2", Class = "Frame", X = 952.7047729999999, Y = 218.41195710000002, W = 1353.34998, H = 1353.34998, Corner = None, T = None, Th = None, Bg = 1 },
	{ Name = "Carrier1010_3", Class = "Frame", X = 952.7047729999999, Y = 218.41195710000002, W = 1353.34998, H = 1353.34998, Corner = None, T = None, Th = None, Bg = 1 },
	{ Name = "Ring720", Class = "Frame", X = 1146.997559, Y = 412.7047731, W = 964.764343, H = 964.764343, Corner = 482.3822, T = 0.9300000071525574, Th = 1.339950442314148, Bg = 1 },
	{ Name = "Carrier720_1", Class = "Frame", X = 1146.997559, Y = 412.7047731, W = 964.764343, H = 964.764343, Corner = None, T = None, Th = None, Bg = 1 },
	{ Name = "Carrier720_2", Class = "Frame", X = 1146.997559, Y = 412.7047731, W = 964.764343, H = 964.764343, Corner = None, T = None, Th = None, Bg = 1 },
	{ Name = "Carrier720_3", Class = "Frame", X = 1146.997559, Y = 412.7047731, W = 964.764343, H = 964.764343, Corner = None, T = None, Th = None, Bg = 1 },
	{ Name = "Ring470", Class = "Frame", X = 1314.491333, Y = 580.1985781, W = 629.776733, H = 629.776733, Corner = 314.8884, T = 0.8999999761581421, Th = 1.339950442314148, Bg = 1 },
	{ Name = "Carrier470_1", Class = "Frame", X = 1314.491333, Y = 580.1985781, W = 629.776733, H = 629.776733, Corner = None, T = None, Th = None, Bg = 1 },
	{ Name = "Carrier470_2", Class = "Frame", X = 1314.491333, Y = 580.1985781, W = 629.776733, H = 629.776733, Corner = None, T = None, Th = None, Bg = 1 },
	{ Name = "Carrier470_3", Class = "Frame", X = 1314.491333, Y = 580.1985781, W = 629.776733, H = 629.776733, Corner = None, T = None, Th = None, Bg = 1 },
}
return function(Target)
	local Group = Instance.new("CanvasGroup")
	Group.Name = "Orbits"
	Group.BackgroundTransparency = 1
	local GX, GY = -174.491333, 49.3548279
	Group.Position = UDim2.fromOffset(GX, GY)
	Group.Size = UDim2.fromOffset(1548.98267, 801.290344)
	Group.Parent = Target
	local Purple = Color3.new(0.6627451181411743, 0.27450981736183167, 1)
	for _, C in Children do
		if C.Class ~= "Frame" then continue end
		local F = Instance.new("Frame")
		F.Name = C.Name
		F.BackgroundTransparency = C.Bg
		F.BackgroundColor3 = Purple
		F.Position = UDim2.fromOffset(C.X, C.Y)
		F.Size = UDim2.fromOffset(C.W, C.H)
		if C.Corner then
			local K = Instance.new("UICorner")
			K.CornerRadius = UDim.new(0, C.Corner)
			K.Parent = F
		end
		if C.Th then
			local S = Instance.new("UIStroke")
			S.Color = Purple
			S.Thickness = C.Th
			S.Transparency = C.T
			S.BorderStrokePosition = Enum.BorderStrokePosition.Outer
			S.Parent = F
		end
		F.Parent = Group
	end
	-- sharp bars (screen positions fixed): vertical ones for the x scale, horizontal ones for the y scale
	for K = 0, 9 do
		local Bar = Instance.new("Frame")
		Bar.Name = "VBar" .. K
		Bar.BorderSizePixel = 0
		Bar.BackgroundColor3 = Color3.new(1, 1, 1)
		Bar.Position = UDim2.fromOffset(math.floor(30 + K * 113) - GX, math.floor(70) - GY)
		Bar.Size = UDim2.fromOffset(2, 40)
		Bar.Parent = Group
		local HBar = Instance.new("Frame")
		HBar.Name = "HBar" .. K
		HBar.BorderSizePixel = 0
		HBar.BackgroundColor3 = Color3.new(1, 1, 1)
		HBar.Position = UDim2.fromOffset(math.floor(60) - GX, math.floor(130 + K * 61) - GY)
		HBar.Size = UDim2.fromOffset(40, 2)
		HBar.Parent = Group
	end
end
