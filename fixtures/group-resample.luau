-- CanvasGroup texture resampling probe: opaque groups (black background, white bars) whose physical
-- position and size have every fractional part 0 / .25 / .5 / .75 on both axes. Vertical bars in the left
-- part measure the horizontal mapping, horizontal bars in the right part the vertical one; bar edges fall
-- at many sub-pixel phases. Row 5: small, large and transparent / rounded / faded groups.
-- Geometry is given in physical pixels (1.5 DPI), converted to Scale on a 1000 x 1000 holder.
-- compare: --viewport 1200x900 --time 0.2 --background 20,20,26
-- analysis: python tools/analysis/image_group.py group-resample
return function(Target)
	local Holder = Instance.new("Frame")
	Holder.Name = "Holder"
	Holder.Size = UDim2.fromOffset(1000, 1000)
	Holder.BackgroundTransparency = 1
	Holder.Parent = Target

	-- Physical px -> UDim on the 1000-logical holder
	local function U(Px: number): UDim
		local Logical = Px / 1.5
		local Offset = math.floor(Logical)
		return UDim.new((Logical - Offset) / 1000, Offset)
	end

	local function Bar(Parent, Name, X, Y, W, H, GW, GH)
		local B = Instance.new("Frame")
		B.Name = Name
		B.Position = UDim2.new(X / GW, 0, Y / GH, 0)
		B.Size = UDim2.new(W / GW, 0, H / GH, 0)
		B.BackgroundColor3 = Color3.new(1, 1, 1)
		B.BorderSizePixel = 0
		B.Parent = Parent
		return B
	end

	local function Group(Name, X, Y, W, H, Props)
		local G = Instance.new("CanvasGroup")
		G.Name = Name
		G.Position = UDim2.new(U(X), U(Y))
		G.Size = UDim2.new(U(W), U(H))
		G.BackgroundColor3 = Color3.new(0, 0, 0)
		G.BorderSizePixel = 0
		for Key, Value in Props or {} do
			(G :: any)[Key] = Value
		end
		G.Parent = Holder

		-- Vertical bars (horizontal mapping) in x < 0.45 W, horizontal bars (vertical mapping) in x > 0.55 W
		local Index = 0
		local BX = 4.3
		while BX + 6 < W * 0.45 do
			Index += 1
			local BW = 2.6 + (Index % 3) * 1.37
			Bar(G, `V{Index}`, BX, 6, BW, H - 12, W, H)
			BX += BW + 3.1 + (Index % 4) * 1.13
		end
		Index = 0
		local BY = 4.6
		while BY + 6 < H - 4 do
			Index += 1
			local BH = 2.4 + (Index % 3) * 1.29
			Bar(G, `H{Index}`, W * 0.55, BY, W * 0.45 - 5, BH, W, H)
			BY += BH + 2.9 + (Index % 4) * 1.07
		end
		return G
	end

	local Fractions = { 0, 0.25, 0.5, 0.75 }
	for Row = 0, 3 do
		for Column = 0, 3 do
			local A, B = Fractions[Column + 1], Fractions[Row + 1]
			-- x: position fraction A, size fraction B; y: position fraction B, size fraction A
			Group(`G{Row}{Column}`, 30 + Column * 440 + A, 30 + Row * 250 + B, 330 + B, 200 + A)
		end
	end

	-- Row 5: small and large groups, a transparent group with a UICorner, a faded group
	Group("Small", 30.5, 1030.25, 61.75, 45.5)
	Group("Large", 130.25, 1030.75, 700.5, 290.25)
	local Clear = Group("Clear", 870.75, 1030.5, 330.5, 200.25, { BackgroundTransparency = 1 })
	local Corner = Instance.new("UICorner")
	Corner.CornerRadius = UDim.new(0, 30)
	Corner.Parent = Clear
	Group("Faded", 1250.5, 1030.25, 330.25, 200.75, { GroupTransparency = 0.5 })
end
