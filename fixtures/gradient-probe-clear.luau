-- UIGradient transparency 1 (round 9, a button's shine): does a gradient whose transparency is 1 over the whole
-- frame still draw? White frames on a black background (compare with --background 0,0,0), each a
-- different way of reaching T = 1; the last row are controls.
-- compare: --viewport 1200x900 --time 0.2 --background 0,0,0
return function(Target)
	local SWEEP = NumberSequence.new({
		NumberSequenceKeypoint.new(0, 1),
		NumberSequenceKeypoint.new(0.5, 0.45),
		NumberSequenceKeypoint.new(1, 1),
	})
	local function Frame(Col, Row, Props, Radius, Background)
		local F = Instance.new("Frame")
		F.Position = UDim2.fromOffset(20 + Col * 190, 20 + Row * 110)
		F.Size = UDim2.fromOffset(170, 90)
		F.BackgroundColor3 = Background or Color3.new(1, 1, 1)
		F.BorderSizePixel = 0
		if Props then
			local Gradient = Instance.new("UIGradient")
			for Key, Value in Props do
				(Gradient :: any)[Key] = Value
			end
			Gradient.Parent = F
		end
		if Radius then
			local Corner = Instance.new("UICorner")
			Corner.CornerRadius = UDim.new(0, Radius)
			Corner.Parent = F
		end
		F.Parent = Target
		return F
	end
	-- row 0: the shine sequence pushed off the frame
	Frame(0, 0, { Transparency = SWEEP, Offset = Vector2.new(-1.1, 0), Rotation = 30 }, 7)
	Frame(1, 0, { Transparency = SWEEP, Offset = Vector2.new(-1.1, 0), Rotation = 30 })
	Frame(2, 0, { Transparency = SWEEP, Offset = Vector2.new(-1.1, 0), Rotation = 0 })
	Frame(3, 0, { Transparency = SWEEP, Offset = Vector2.new(1.1, 0), Rotation = 0 })
	Frame(4, 0, { Transparency = SWEEP, Offset = Vector2.new(0, 1.1), Rotation = 90 })
	-- row 1: constant 1
	Frame(0, 1, { Transparency = NumberSequence.new(1) })
	Frame(1, 1, { Transparency = NumberSequence.new(1) }, 7)
	Frame(2, 1, { Transparency = NumberSequence.new(1), Rotation = 30 })
	Frame(3, 1, { Transparency = NumberSequence.new(1, 0.5) })
	Frame(4, 1, { Transparency = NumberSequence.new(0.999) })
	-- row 2: near 1
	Frame(0, 2, { Transparency = NumberSequence.new(0.998) })
	Frame(1, 2, { Transparency = NumberSequence.new(0.996) })
	Frame(2, 2, { Transparency = NumberSequence.new(0.994) })
	Frame(3, 2, { Transparency = NumberSequence.new(0.99) })
	Frame(4, 2, { Transparency = NumberSequence.new(0.98) })
	-- row 3: other colours / BackgroundTransparency, gradient T = 1
	Frame(0, 3, { Transparency = NumberSequence.new(1) }, nil, Color3.fromRGB(255, 0, 0))
	Frame(1, 3, { Transparency = NumberSequence.new(1) }, nil, Color3.fromRGB(128, 128, 128))
	local F = Frame(2, 3, { Transparency = NumberSequence.new(1) })
	F.BackgroundTransparency = 0.5
	Frame(3, 3, { Transparency = NumberSequence.new(1), Color = ColorSequence.new(Color3.fromRGB(0, 255, 0)) })
	-- row 4: controls
	local C1 = Frame(0, 4, nil)
	C1.BackgroundTransparency = 1
	local C2 = Frame(1, 4, nil)
	C2.BackgroundTransparency = 0.999
	local C3 = Frame(2, 4, nil)
	C3.BackgroundTransparency = 0.998
	Frame(3, 4, { Transparency = NumberSequence.new(0.5) })
end
