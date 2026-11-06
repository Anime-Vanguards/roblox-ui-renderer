-- UIShadow probe B: width sweep at fixed thin height, blur scaling, and UICorner radii (white shadow on black).
return function(Target)
	local Cells = {
		-- width sweep, H = 40, blur 40
		{ X = 60, Y = 70, W = 40, H = 40, Blur = 40 },
		{ X = 220, Y = 70, W = 50, H = 40, Blur = 40 },
		{ X = 390, Y = 70, W = 60, H = 40, Blur = 40 },
		{ X = 570, Y = 70, W = 70, H = 40, Blur = 40 },
		{ X = 760, Y = 70, W = 80, H = 40, Blur = 40 },
		{ X = 960, Y = 70, W = 100, H = 40, Blur = 40 },
		{ X = 1180, Y = 70, W = 120, H = 40, Blur = 40 },
		{ X = 1420, Y = 70, W = 160, H = 40, Blur = 40 },
		{ X = 1700, Y = 70, W = 240, H = 40, Blur = 40 },
		-- blur scaling: same shapes relative to sigma at blur 20
		{ X = 60, Y = 260, W = 150, H = 20, Blur = 20 },
		{ X = 300, Y = 260, W = 20, H = 20, Blur = 20 },
		{ X = 400, Y = 260, W = 150, H = 50, Blur = 20 },
		-- corner radii on 200x200, blur 40
		{ X = 700, Y = 280, W = 200, H = 200, Blur = 40, Radius = 20 },
		{ X = 1020, Y = 280, W = 200, H = 200, Blur = 40, Radius = 50 },
		{ X = 1340, Y = 280, W = 200, H = 200, Blur = 40, Radius = 100 },
		{ X = 1660, Y = 280, W = 200, H = 120, Blur = 40, Radius = 60 },
		-- corner radius with small blur
		{ X = 80, Y = 420, W = 200, H = 200, Blur = 10, Radius = 40 },
		{ X = 380, Y = 420, W = 200, H = 200, Blur = 4, Radius = 60 },
		-- blur larger than the frame
		{ X = 180, Y = 850, W = 60, H = 60, Blur = 120 },
		{ X = 700, Y = 780, W = 300, H = 120, Blur = 120 },
		{ X = 1500, Y = 780, W = 120, H = 120, Blur = 120, Radius = 30 },
	}
	for _, Cell in Cells do
		local F = Instance.new("Frame")
		F.Position = UDim2.fromOffset(Cell.X, Cell.Y)
		F.Size = UDim2.fromOffset(Cell.W, Cell.H)
		F.BackgroundColor3 = Color3.fromRGB(0, 0, 255)
		F.BorderSizePixel = 0
		if Cell.Radius then
			local Corner = Instance.new("UICorner")
			Corner.CornerRadius = UDim.new(0, Cell.Radius)
			Corner.Parent = F
		end
		local Shadow = Instance.new("UIShadow")
		Shadow.Color = Color3.new(1, 1, 1)
		Shadow.BlurRadius = UDim.new(0, Cell.Blur)
		Shadow.Parent = F
		F.Parent = Target
	end
end
