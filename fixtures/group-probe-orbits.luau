-- A CanvasGroup of orbit rings rebuilt from a story's scene: the group's rect (-174.491333, 49.3548279, 1548.98267 x 801.290344),
-- overhanging the viewport on both sides, with its children at their exported rects (Frames with a full
-- UICorner and a faint 1.34 px Outer stroke; other children as plain frames). Isolates the group's
-- resampling from the rest of the window.
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
	Group.Position = UDim2.fromOffset(-174.491333, 49.3548279)
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
end
