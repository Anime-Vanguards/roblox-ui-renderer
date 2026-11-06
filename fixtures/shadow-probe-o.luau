-- UIShadow probe O (2026-09-27): rounded corners on shapes thinner than 6 sigma, the
-- button bodies' shadows (BlurRadius 24, sigma 18 device px; the gaps of two button grids).
-- Rows: UICorner radius 0 / 4 / 5 / 8 / 12 / 20. Columns: 177 x 57, 120 x 40, 90 x 30, 200 x 70 logical.
-- White shadow, Transparency 0, on transparent frames.
-- compare: --viewport 1200x900 --time 0.2 --background 0,0,0
return function(Target)
	local Radii = { 0, 4, 5, 8, 12, 20 }
	local Sizes = { { 177, 57 }, { 120, 40 }, { 90, 30 }, { 200, 70 } }
	local ColumnX = { 40, 337, 594, 804 }
	for R, Radius in Radii do
		for C, Size in Sizes do
			local F = Instance.new("Frame")
			F.Name = `R{Radius}_{Size[1]}x{Size[2]}`
			F.Position = UDim2.fromOffset(ColumnX[C], 40 + (R - 1) * 145)
			F.Size = UDim2.fromOffset(Size[1], Size[2])
			F.BackgroundTransparency = 1
			F.BorderSizePixel = 0
			if Radius > 0 then
				local Corner = Instance.new("UICorner")
				Corner.CornerRadius = UDim.new(0, Radius)
				Corner.Parent = F
			end
			local Shadow = Instance.new("UIShadow")
			Shadow.Color = Color3.new(1, 1, 1)
			Shadow.BlurRadius = UDim.new(0, 24)
			Shadow.Parent = F
			F.Parent = Target
		end
	end
end
