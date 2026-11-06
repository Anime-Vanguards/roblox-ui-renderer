-- UIShadow probe A (transparent frames): big squares at several blur radii and elongated frames (white shadow on black).
return function(Target)
	local Cells = {
		{ X = 60, Y = 60, W = 300, H = 300, Blur = 20 },
		{ X = 520, Y = 60, W = 300, H = 300, Blur = 40 },
		{ X = 1060, Y = 110, W = 300, H = 300, Blur = 80 },
		{ X = 1500, Y = 60, W = 300, H = 20, Blur = 40 },
		{ X = 1500, Y = 200, W = 300, H = 40, Blur = 40 },
		{ X = 1500, Y = 360, W = 300, H = 60, Blur = 40 },
		{ X = 60, Y = 520, W = 300, H = 80, Blur = 40 },
		{ X = 60, Y = 720, W = 300, H = 100, Blur = 40 },
		{ X = 60, Y = 940, W = 300, H = 140, Blur = 40 },
		{ X = 520, Y = 540, W = 40, H = 40, Blur = 40 },
		{ X = 700, Y = 540, W = 80, H = 80, Blur = 40 },
		{ X = 920, Y = 540, W = 120, H = 120, Blur = 40 },
		{ X = 520, Y = 780, W = 20, H = 20, Blur = 40 },
		{ X = 700, Y = 780, W = 60, H = 60, Blur = 40 },
		{ X = 920, Y = 800, W = 160, H = 160, Blur = 40 },
		{ X = 1250, Y = 560, W = 140, H = 300, Blur = 40 },
		{ X = 1540, Y = 560, W = 60, H = 300, Blur = 40 },
		{ X = 1740, Y = 560, W = 20, H = 300, Blur = 40 },
	}
	for _, Cell in Cells do
		local F = Instance.new("Frame")
		F.Position = UDim2.fromOffset(Cell.X, Cell.Y)
		F.Size = UDim2.fromOffset(Cell.W, Cell.H)
		F.BackgroundTransparency = 1
		F.BorderSizePixel = 0
		local Shadow = Instance.new("UIShadow")
		Shadow.Color = Color3.new(1, 1, 1)
		Shadow.BlurRadius = UDim.new(0, Cell.Blur)
		Shadow.Parent = F
		F.Parent = Target
	end
end
