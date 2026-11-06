-- CanvasGroup texture resolution vs the group's size and origin (a wide group of orbit rings: a 2324 x 1202 px group
-- overhanging the screen). Each row is a CanvasGroup 50 logical px high holding sharp white bars (2 logical px
-- wide, whole logical positions: hard edges on the device grid) and a small circle with a 1.34 px Outer stroke.
-- Rows vary the group's width (below and above 2048 device px at 1.5 DPI) and its origin (whole, fractional,
-- overhanging the left edge). The bars' profiles give the texture's scale and phase.
-- compare: --viewport 1200x780 --time 0.2 --background 0,0,0
local Rows = {
	{ X = 0, W = 1000 },
	{ X = 0, W = 1300 },
	{ X = 0, W = 1365 },
	{ X = 0, W = 1366 },
	{ X = 0, W = 1400 },
	{ X = 0, W = 1548 },
	{ X = 0, W = 2000 },
	{ X = 0, W = 2800 },
	{ X = -174, W = 1548 },
	{ X = -174.491333, W = 1548.98267 },
	{ X = -400, W = 1400 },
	{ X = -0.33, W = 1000.4 },
	{ X = -0.33, W = 1548.98267 },
	{ X = 0.4, W = 1100.3 },
}
return function(Target)
	for Index, Row in Rows do
		local Group = Instance.new("CanvasGroup")
		Group.Name = "Row" .. Index
		Group.BackgroundTransparency = 1
		Group.Position = UDim2.fromOffset(Row.X, 2 + (Index - 1) * 55)
		Group.Size = UDim2.fromOffset(Row.W, 50)
		Group.Parent = Target
		-- bars at group-local x so that they sit on screen at 20 .. 1180 logical
		for K = 0, 11 do
			local Bar = Instance.new("Frame")
			Bar.Name = "Bar" .. K
			Bar.BorderSizePixel = 0
			Bar.BackgroundColor3 = Color3.new(1, 1, 1)
			Bar.Position = UDim2.fromOffset(math.floor(20 + K * 97 - Row.X), 8 + (K % 3))
			Bar.Size = UDim2.fromOffset(2 + (K % 2), 30)
			Bar.Parent = Group
		end
		local Ring = Instance.new("Frame")
		Ring.Name = "Ring"
		Ring.BackgroundTransparency = 1
		Ring.Position = UDim2.fromOffset(math.floor(1120 - Row.X), 10)
		Ring.Size = UDim2.fromOffset(30, 30)
		local Corner = Instance.new("UICorner")
		Corner.CornerRadius = UDim.new(0.5, 0)
		Corner.Parent = Ring
		local Stroke = Instance.new("UIStroke")
		Stroke.Color = Color3.new(1, 1, 1)
		Stroke.Thickness = 1.339950442314148
		Stroke.BorderStrokePosition = Enum.BorderStrokePosition.Outer
		Stroke.Parent = Ring
		Ring.Parent = Group
	end
end
