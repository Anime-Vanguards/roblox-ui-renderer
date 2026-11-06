-- A window's orbit rings: circular frames 630-1796 px across (UICorner at half the size) with a
-- 1.34 px Outer UIStroke at Transparency 0.93-0.965, inside a resampled CanvasGroup. Four quadrants, each
-- showing the upper-left arc of a 964.76 px ring (Ring720's size) centred off-screen to the lower right:
--   TL plain frame, opaque stroke      TR plain frame, stroke at 0.93
--   BL in a CanvasGroup, opaque stroke BR in a CanvasGroup, stroke at 0.93
-- compare: --viewport 1200x780 --time 0.2 --background 20,16,32
return function(Target)
	local Purple = Color3.new(0.6627451181411743, 0.27450981736183167, 1)
	local Size = 964.764343
	local function Ring(Parent, Transparency)
		local F = Instance.new("Frame")
		F.Name = "Ring"
		F.BackgroundTransparency = 1
		F.Size = UDim2.fromOffset(Size, Size)
		-- the arc passes through the quadrant's middle: centre at (Size * 0.62, Size * 0.62) from its corner
		F.Position = UDim2.fromOffset(40, 30)
		local Corner = Instance.new("UICorner")
		Corner.CornerRadius = UDim.new(0.5, 0)
		Corner.Parent = F
		local Stroke = Instance.new("UIStroke")
		Stroke.Color = Purple
		Stroke.Thickness = 1.339950442314148
		Stroke.Transparency = Transparency
		Stroke.BorderStrokePosition = Enum.BorderStrokePosition.Outer
		Stroke.Parent = F
		F.Parent = Parent
	end
	local function Quadrant(Name, X, Y, Group, Transparency)
		local Q = Instance.new(Group and "CanvasGroup" or "Frame")
		Q.Name = Name
		Q.BackgroundTransparency = 1
		Q.ClipsDescendants = true
		Q.Position = UDim2.fromOffset(X, Y)
		Q.Size = UDim2.fromOffset(600, 390)
		Q.Parent = Target
		Ring(Q, Transparency)
	end
	Quadrant("PlainOpaque", 0, 0, false, 0)
	Quadrant("PlainFaint", 600, 0, false, 0.93)
	Quadrant("GroupOpaque", 0, 390, true, 0)
	Quadrant("GroupFaint", 600, 390, true, 0.93)
end
