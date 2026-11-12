-- UIShadow probe: Inset geometry (white shadows in black frames on black, so a pixel is the alpha byte of the
-- shadow): blur radii, corner radii, spread, offset, thin shapes, transparency, hard shadows.
-- compare --viewport 1200x900 --time 0.3 --background 0,0,0
return function(Target)
	local function Cell(X, Y, W, H, Radius, Props)
		local F = Instance.new("Frame")
		F.Position = UDim2.fromOffset(X, Y)
		F.Size = UDim2.fromOffset(W, H)
		F.BackgroundColor3 = Color3.new(0, 0, 0)
		F.BorderSizePixel = 0
		if Radius then
			local Corner = Instance.new("UICorner")
			Corner.CornerRadius = UDim.new(0, Radius)
			Corner.Parent = F
		end
		local Shadow = Instance.new("UIShadow")
		Shadow.Color = Color3.new(1, 1, 1)
		Shadow.Inset = true
		for Key, Value in Props do
			(Shadow :: any)[Key] = Value
		end
		Shadow.Parent = F
		F.Parent = Target
		return F
	end

	-- A: sharp frames, blur radii
	for Index, Blur in { 2, 6, 12, 24, 48 } do
		Cell(20 + (Index - 1) * 220, 20, 200, 160, nil, { BlurRadius = UDim.new(0, Blur) })
	end
	-- B: corner radii
	Cell(20, 200, 200, 160, 16, { BlurRadius = UDim.new(0, 12) })
	Cell(240, 200, 200, 160, 40, { BlurRadius = UDim.new(0, 12) })
	Cell(460, 200, 200, 160, 80, { BlurRadius = UDim.new(0, 24) })
	Cell(680, 200, 200, 160, 16, { BlurRadius = UDim.new(0, 48) })
	Cell(900, 200, 200, 160, 60, { BlurRadius = UDim.new(0, 6) })
	-- C: spread and offset
	Cell(20, 380, 200, 160, nil, { BlurRadius = UDim.new(0, 12), Spread = UDim2.fromOffset(16, 16) })
	Cell(240, 380, 200, 160, nil, { BlurRadius = UDim.new(0, 12), Spread = UDim2.fromOffset(-16, -16) })
	Cell(460, 380, 200, 160, 24, { BlurRadius = UDim.new(0, 12), Spread = UDim2.fromOffset(40, 0) })
	Cell(680, 380, 200, 160, nil, { BlurRadius = UDim.new(0, 12), Offset = UDim2.fromOffset(10, 16) })
	Cell(900, 380, 200, 160, 24, { BlurRadius = UDim.new(0, 12), Offset = UDim2.fromOffset(-30, 0) })
	-- D: transparency, thin shapes, scale blur, everything at once
	Cell(20, 560, 200, 160, nil, { BlurRadius = UDim.new(0, 12), Transparency = 0.5 })
	Cell(240, 560, 200, 12, nil, { BlurRadius = UDim.new(0, 12) })
	Cell(240, 590, 200, 30, nil, { BlurRadius = UDim.new(0, 24) })
	Cell(240, 640, 40, 40, nil, { BlurRadius = UDim.new(0, 24) })
	Cell(300, 640, 60, 60, 30, { BlurRadius = UDim.new(0, 12) })
	Cell(380, 640, 60, 80, 8, { BlurRadius = UDim.new(0, 48) })
	Cell(460, 560, 200, 160, nil, { BlurRadius = UDim.new(0.2, 0) })
	Cell(680, 560, 200, 160, 30, { BlurRadius = UDim.new(0, 16), Spread = UDim2.fromOffset(-20, 10), Offset = UDim2.fromOffset(8, -12), Transparency = 0.25 })
	Cell(900, 560, 200, 160, nil, { BlurRadius = UDim.new(0, 12), Spread = UDim2.fromScale(0.2, 0.2) })
	-- E: hard shadows, and one moved out of its frame
	Cell(20, 740, 200, 140, nil, { BlurRadius = UDim.new(0, 0) })
	Cell(240, 740, 200, 140, nil, { BlurRadius = UDim.new(0, 0), Spread = UDim2.fromOffset(-16, -16) })
	Cell(460, 740, 200, 140, 24, { BlurRadius = UDim.new(0, 0), Offset = UDim2.fromOffset(10, 10) })
	Cell(680, 740, 200, 140, 24, { BlurRadius = UDim.new(0, 0), Spread = UDim2.fromOffset(-17, -17), Transparency = 0.5 })
	Cell(900, 740, 200, 140, nil, { BlurRadius = UDim.new(0, 12), Offset = UDim2.fromOffset(250, 0) })
end
