-- UIShadow probe C (transparent frames): UICorner radius sweeps (white shadow on black).
return function(Target)
	local Cells = {}
	-- Row 1: blur 30, 120x120 (c capped at 6 sigma), radius sweep
	for Index, Radius in { 0, 4, 8, 15, 22.5, 30, 45, 60 } do
		table.insert(Cells, { X = 45 + (Index - 1) * 245, Y = 50, W = 120, H = 120, Blur = 30, Radius = Radius })
	end
	-- Row 2: blur 30, 120x40 radius sweep, then 60x60 squares
	for Index, Radius in { 0, 5, 10, 15, 20 } do
		table.insert(Cells, { X = 45 + (Index - 1) * 245, Y = 330, W = 120, H = 40, Blur = 30, Radius = Radius })
	end
	for Index, Radius in { 0, 15, 30 } do
		table.insert(Cells, { X = 45 + (4 + Index) * 245 + 30, Y = 320, W = 60, H = 60, Blur = 30, Radius = Radius })
	end
	-- Row 3: blur 16, 160x50 and 80x80 radius sweeps
	for Index, Radius in { 0, 6, 12, 25 } do
		table.insert(Cells, { X = 30 + (Index - 1) * 230, Y = 530, W = 160, H = 50, Blur = 16, Radius = Radius })
	end
	for Index, Radius in { 8, 16, 40 } do
		table.insert(Cells, { X = 30 + (3 + Index) * 230 + 40, Y = 515, W = 80, H = 80, Blur = 16, Radius = Radius })
	end
	-- Row 4: blur 60, big frames with radius, and a pill
	table.insert(Cells, { X = 90, Y = 800, W = 200, H = 150, Blur = 60, Radius = 20 })
	table.insert(Cells, { X = 560, Y = 800, W = 200, H = 150, Blur = 60, Radius = 60 })
	table.insert(Cells, { X = 1030, Y = 830, W = 260, H = 60, Blur = 30, Radius = 30 })
	table.insert(Cells, { X = 1460, Y = 800, W = 120, H = 120, Blur = 30, Radius = 60 })
	table.insert(Cells, { X = 1760, Y = 820, W = 100, H = 100, Blur = 12, Radius = 50 })
	for _, Cell in Cells do
		local F = Instance.new("Frame")
		F.Position = UDim2.fromOffset(Cell.X, Cell.Y)
		F.Size = UDim2.fromOffset(Cell.W, Cell.H)
		F.BackgroundTransparency = 1
		F.BorderSizePixel = 0
		if Cell.Radius > 0 then
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
