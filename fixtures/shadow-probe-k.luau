-- UIShadow probe K (round 4): squares and small rectangles 10..170 logical at BlurRadius 40 (sigma 30 device px at 1.5 DPI):
-- the peak and corners of shapes smaller than 6 sigma (white shadow on black, transparent frames).
return function(Target)
	local Cells = {
		{ 10, 10 }, { 20, 20 }, { 30, 30 }, { 40, 40 }, { 50, 50 }, { 60, 60 }, { 70, 70 }, { 80, 80 }, { 90, 90 },
		{ 100, 100 }, { 110, 110 }, { 130, 130 }, { 150, 150 }, { 170, 170 },
		{ 20, 60 }, { 40, 100 }, { 60, 140 }, { 30, 170 }, { 100, 40 },
	}
	local X, Y, RowH = 0, 0, 0
	for _, Cell in Cells do
		local W, H = Cell[1], Cell[2]
		if X + W + 128 > 2000 then
			X = 0
			Y += RowH
			RowH = 0
		end
		local F = Instance.new("Frame")
		F.Position = UDim2.fromOffset(X + 64, Y + 64)
		F.Size = UDim2.fromOffset(W, H)
		F.BackgroundTransparency = 1
		F.BorderSizePixel = 0
		local Shadow = Instance.new("UIShadow")
		Shadow.Color = Color3.new(1, 1, 1)
		Shadow.BlurRadius = UDim.new(0, 40)
		Shadow.Parent = F
		F.Parent = Target
		X += W + 128
		RowH = math.max(RowH, H + 128)
	end
end
