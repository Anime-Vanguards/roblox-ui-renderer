-- UIShadow probe M (round 4): UICorner radius and the shadow mesh's arc tessellation. Rows 1-2: 200 x 200 frames at
-- BlurRadius 10 with radius 40..100 logical (reads the arc segment count and the high-tessellation threshold); row 3:
-- fractional and scale radii (radius rounding); row 4: radius above 3 sigma at BlurRadius 40 (white shadow on black).
return function(Target)
	local function Cell(X, Y, W, H, Blur, Radius)
		local F = Instance.new("Frame")
		F.Position = UDim2.fromOffset(X, Y)
		F.Size = UDim2.fromOffset(W, H)
		F.BackgroundTransparency = 1
		F.BorderSizePixel = 0
		local Corner = Instance.new("UICorner")
		Corner.CornerRadius = Radius
		Corner.Parent = F
		local Shadow = Instance.new("UIShadow")
		Shadow.Color = Color3.new(1, 1, 1)
		Shadow.BlurRadius = UDim.new(0, Blur)
		Shadow.Parent = F
		F.Parent = Target
	end
	for Index, Radius in { 50, 52, 54, 56, 58, 60, 62, 64 } do
		Cell(20 + (Index - 1) * 245, 20, 200, 200, 10, UDim.new(0, Radius))
	end
	for Index, Radius in { 66, 70, 75, 80, 90, 100, 45, 40 } do
		Cell(20 + (Index - 1) * 245, 265, 200, 200, 10, UDim.new(0, Radius))
	end
	for Index, Radius in { UDim.new(0, 15.3), UDim.new(0, 22.5), UDim.new(0, 22.7), UDim.new(0, 30.5), UDim.new(0, 10.4), UDim.new(0, 10.7), UDim.new(0.25, 0) } do
		Cell(40 + (Index - 1) * 200, 530, 120, 120, 16, Radius)
	end
	Cell(1440, 540, 200, 60, 16, UDim.new(0.5, 0))
	Cell(1700, 540, 200, 57, 16, UDim.new(0.5, 0))
	for Index, Radius in { 30, 45, 60, 70, 80 } do
		Cell(70 + (Index - 1) * 390, 800, 200, 200, 40, UDim.new(0, Radius))
	end
end
