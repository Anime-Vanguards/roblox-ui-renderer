-- UIStroke draw order among strokes of equal ZIndex (round 4): two Center strokes with the same footprint (the one
-- drawn last covers the other) parented red-then-blue and blue-then-red, a three-stroke stack, and the stories'
-- Inner gradient + Outer black pair on a rounded frame, in both child orders.
-- compare-unstable: Studio draws strokes of equal ZIndex in an order that changes from mount to mount (notes/strokes.md)
-- compare: --viewport 900x400 --time 0.2 --background 20,20,26
return function(Target)
	local function Frame(X, Y, Radius)
		local F = Instance.new("Frame")
		F.Position = UDim2.fromOffset(X, Y)
		F.Size = UDim2.fromOffset(140, 90)
		F.BackgroundColor3 = Color3.fromRGB(70, 70, 90)
		F.BorderSizePixel = 0
		F.Parent = Target
		if Radius then
			local Corner = Instance.new("UICorner")
			Corner.CornerRadius = UDim.new(0, Radius)
			Corner.Parent = F
		end
		return F
	end
	local function Stroke(F, Color, Position, Thickness, Z, Gradient)
		local S = Instance.new("UIStroke")
		S.Color = Color
		S.Thickness = Thickness or 8
		S.BorderStrokePosition = Position or Enum.BorderStrokePosition.Center
		S.ZIndex = Z or 1
		if Gradient then
			local G = Instance.new("UIGradient")
			G.Color = ColorSequence.new(Color3.fromRGB(255, 255, 0), Color3.fromRGB(255, 128, 0))
			G.Parent = S
		end
		S.Parent = F
		return S
	end
	local Red, Blue, Green = Color3.fromRGB(230, 40, 40), Color3.fromRGB(40, 90, 230), Color3.fromRGB(40, 200, 90)
	local A = Frame(40, 40)
	Stroke(A, Red)
	Stroke(A, Blue)
	local B = Frame(240, 40)
	Stroke(B, Blue)
	Stroke(B, Red)
	local C = Frame(440, 40)
	Stroke(C, Red)
	Stroke(C, Blue)
	Stroke(C, Green)
	local D = Frame(640, 40)
	Stroke(D, Red, nil, nil, 2)
	Stroke(D, Blue, nil, nil, 1)
	local E = Frame(40, 220, 16)
	Stroke(E, Color3.new(1, 1, 1), Enum.BorderStrokePosition.Inner, 2.5, 1, true)
	Stroke(E, Color3.new(0, 0, 0), Enum.BorderStrokePosition.Outer, 3, 1)
	local F = Frame(240, 220, 16)
	Stroke(F, Color3.new(0, 0, 0), Enum.BorderStrokePosition.Outer, 3, 1)
	Stroke(F, Color3.new(1, 1, 1), Enum.BorderStrokePosition.Inner, 2.5, 1, true)
end
