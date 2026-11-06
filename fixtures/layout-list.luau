-- UIListLayout: fill direction, alignments, padding (offset/scale), sort orders, anchor points,
-- invisible children, UIScale'd children, scale-sized children and odd container sizes.
return function(Target)
	local Colors = {
		Color3.fromRGB(80, 160, 255),
		Color3.fromRGB(255, 120, 80),
		Color3.fromRGB(120, 230, 120),
		Color3.fromRGB(230, 200, 90),
		Color3.fromRGB(200, 100, 255),
	}

	local Count = 0
	local function Cell(Name, W, H)
		local Column = Count % 6
		local Row = Count // 6
		Count += 1
		local F = Instance.new("Frame")
		F.Name = Name
		F.Position = UDim2.fromOffset(10 + Column * 197, 10 + Row * 178)
		F.Size = UDim2.fromOffset(W or 181, H or 163)
		F.BackgroundColor3 = Color3.fromRGB(40, 40, 52)
		F.BorderSizePixel = 0
		F.Parent = Target
		return F
	end

	local function Item(Parent, Name, W, H, Props)
		local F = Instance.new("Frame")
		F.Name = Name
		F.Size = if typeof(W) == "UDim2" then W else UDim2.fromOffset(W, H)
		F.BackgroundColor3 = Colors[(#Parent:GetChildren() % #Colors) + 1]
		F.BorderSizePixel = 0
		for Key, Value in Props or {} do
			(F :: any)[Key] = Value
		end
		F.Parent = Parent
		return F
	end

	local function List(Parent, Props)
		local L = Instance.new("UIListLayout")
		L.SortOrder = Enum.SortOrder.LayoutOrder
		for Key, Value in Props or {} do
			(L :: any)[Key] = Value
		end
		L.Parent = Parent
		return L
	end

	local function Standard(Parent)
		Item(Parent, "A", 50, 20)
		Item(Parent, "B", 37, 31)
		Item(Parent, "C", 61, 15)
	end

	-- Vertical alignments
	for _, HA in { "Left", "Center", "Right" } do
		for _, VA in { "Top", "Center", "Bottom" } do
			local C = Cell(`V_{HA}_{VA}`)
			List(C, { HorizontalAlignment = Enum.HorizontalAlignment[HA], VerticalAlignment = Enum.VerticalAlignment[VA], Padding = UDim.new(0, 5) })
			Standard(C)
		end
	end

	-- Horizontal alignments
	for _, HA in { "Left", "Center", "Right" } do
		for _, VA in { "Top", "Center", "Bottom" } do
			local C = Cell(`H_{HA}_{VA}`)
			List(C, { FillDirection = Enum.FillDirection.Horizontal, HorizontalAlignment = Enum.HorizontalAlignment[HA], VerticalAlignment = Enum.VerticalAlignment[VA], Padding = UDim.new(0, 3) })
			Standard(C)
		end
	end

	-- Padding scale + offset, vertical and horizontal
	do
		local C = Cell("PadScaleV")
		List(C, { Padding = UDim.new(0.05, 3) })
		Standard(C)
	end
	do
		local C = Cell("PadScaleH")
		List(C, { FillDirection = Enum.FillDirection.Horizontal, Padding = UDim.new(0.03, 1) })
		Standard(C)
	end

	-- Sort orders
	do
		local C = Cell("SortName")
		List(C, { SortOrder = Enum.SortOrder.Name, Padding = UDim.new(0, 2) })
		Item(C, "Zeta", 40, 20)
		Item(C, "alpha", 50, 20)
		Item(C, "Beta", 60, 20)
		Item(C, "beta", 70, 20)
		Item(C, "Alpha", 80, 20)
	end
	do
		local C = Cell("SortLayoutOrder")
		List(C, { Padding = UDim.new(0, 2) })
		Item(C, "P", 40, 20, { LayoutOrder = 3 })
		Item(C, "Q", 50, 20, { LayoutOrder = -1 })
		Item(C, "R", 60, 20, { LayoutOrder = 3 })
		Item(C, "S", 70, 20, { LayoutOrder = 0 })
		Item(C, "T", 80, 20, { LayoutOrder = 1 })
	end

	-- Anchor points and positions inside a list
	do
		local C = Cell("Anchors")
		List(C, { HorizontalAlignment = Enum.HorizontalAlignment.Center, Padding = UDim.new(0, 4) })
		Item(C, "A", 50, 20, { AnchorPoint = Vector2.new(0.5, 0.5) })
		Item(C, "B", 60, 20, { AnchorPoint = Vector2.new(1, 0), Position = UDim2.fromOffset(30, 30) })
		Item(C, "C", 70, 20, { Position = UDim2.new(0.5, 10, 0.5, 10) })
		Item(C, "D", 44, 20, { AnchorPoint = Vector2.new(0.3, 0.7) })
	end

	-- Invisible children are skipped
	do
		local C = Cell("Invisible")
		List(C, { Padding = UDim.new(0, 6) })
		Item(C, "A", 50, 20)
		Item(C, "B", 60, 25, { Visible = false })
		Item(C, "C", 70, 20)
	end

	-- UIScale'd children
	do
		local C = Cell("Scaled")
		List(C, { Padding = UDim.new(0, 4), HorizontalAlignment = Enum.HorizontalAlignment.Center })
		local A = Item(C, "A", 80, 30)
		local S = Instance.new("UIScale")
		S.Scale = 0.75
		S.Parent = A
		Item(C, "B", 60, 20)
		local D = Item(C, "D", 50, 40)
		local S2 = Instance.new("UIScale")
		S2.Scale = 1.3
		S2.Parent = D
	end
	do
		local C = Cell("ScaledH")
		List(C, { FillDirection = Enum.FillDirection.Horizontal, Padding = UDim.new(0, 4), VerticalAlignment = Enum.VerticalAlignment.Center })
		local A = Item(C, "A", 40, 30)
		local S = Instance.new("UIScale")
		S.Scale = 0.6
		S.Parent = A
		Item(C, "B", 30, 20)
		local D = Item(C, "D", 30, 40)
		local S2 = Instance.new("UIScale")
		S2.Scale = 1.5
		S2.Parent = D
	end

	-- Scale-sized children
	do
		local C = Cell("ScaleSized")
		List(C, { Padding = UDim.new(0, 3) })
		Item(C, "A", UDim2.new(0.5, 0, 0, 20))
		Item(C, "B", UDim2.new(1, -20, 0.2, 0))
		Item(C, "C", UDim2.new(0.33, 7, 0.1, 3))
	end
	do
		local C = Cell("ScaleSizedH")
		List(C, { FillDirection = Enum.FillDirection.Horizontal, Padding = UDim.new(0, 3), HorizontalAlignment = Enum.HorizontalAlignment.Center })
		Item(C, "A", UDim2.new(0.2, 0, 0.5, 0))
		Item(C, "B", UDim2.new(0.3, -5, 1, -20))
		Item(C, "C", UDim2.new(0.15, 3, 0.25, 1))
	end

	-- Odd container sizes (half-pixel centering)
	do
		local C = Cell("OddCenter", 180, 163)
		List(C, { HorizontalAlignment = Enum.HorizontalAlignment.Center, VerticalAlignment = Enum.VerticalAlignment.Center, Padding = UDim.new(0, 5) })
		Item(C, "A", 51, 21)
		Item(C, "B", 37, 30)
		Item(C, "C", 62, 16)
	end
	do
		local C = Cell("OddCenterH", 180, 162)
		List(C, { FillDirection = Enum.FillDirection.Horizontal, HorizontalAlignment = Enum.HorizontalAlignment.Center, VerticalAlignment = Enum.VerticalAlignment.Center, Padding = UDim.new(0, 5) })
		Item(C, "A", 51, 21)
		Item(C, "B", 37, 30)
		Item(C, "C", 62, 17)
	end

	-- UIPadding on the container
	do
		local C = Cell("Padded")
		local P = Instance.new("UIPadding")
		P.PaddingLeft = UDim.new(0, 7)
		P.PaddingRight = UDim.new(0.1, 0)
		P.PaddingTop = UDim.new(0, 9)
		P.PaddingBottom = UDim.new(0.05, 2)
		P.Parent = C
		List(C, { HorizontalAlignment = Enum.HorizontalAlignment.Right, VerticalAlignment = Enum.VerticalAlignment.Bottom, Padding = UDim.new(0, 4) })
		Standard(C)
	end

	-- Overflowing content (larger than the container)
	do
		local C = Cell("Overflow")
		List(C, { HorizontalAlignment = Enum.HorizontalAlignment.Center, VerticalAlignment = Enum.VerticalAlignment.Center, Padding = UDim.new(0, 10) })
		Item(C, "A", 220, 60)
		Item(C, "B", 100, 70)
		Item(C, "C", 90, 50)
	end

	-- Nested lists and grandchildren
	do
		local C = Cell("Nested")
		List(C, { Padding = UDim.new(0, 4) })
		local Inner = Item(C, "Inner", 150, 80)
		List(Inner, { FillDirection = Enum.FillDirection.Horizontal, HorizontalAlignment = Enum.HorizontalAlignment.Right, Padding = UDim.new(0, 2) })
		Item(Inner, "X", 30, 30)
		Item(Inner, "Y", 25, 50)
		Item(C, "After", 60, 20)
	end
end
