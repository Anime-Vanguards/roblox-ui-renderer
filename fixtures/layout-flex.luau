-- UIListLayout flex: HorizontalFlex/VerticalFlex (Fill, SpaceBetween, SpaceAround, SpaceEvenly),
-- Wraps, ItemLineAlignment, UIFlexItem (Grow, Shrink, Fill, Custom ratios, per-item alignment).
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
		F.Position = UDim2.fromOffset(10 + Column * 197, 10 + Row * 136)
		F.Size = UDim2.fromOffset(W or 181, H or 121)
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

	local function Flex(Parent, Props)
		local F = Instance.new("UIFlexItem")
		for Key, Value in Props or {} do
			(F :: any)[Key] = Value
		end
		F.Parent = Parent
		return F
	end

	local function Standard(Parent)
		Item(Parent, "A", 30, 20)
		Item(Parent, "B", 23, 31)
		Item(Parent, "C", 41, 15)
	end

	-- Main-axis flex modes, horizontal and vertical
	for _, Mode in { "Fill", "SpaceBetween", "SpaceAround", "SpaceEvenly" } do
		local C = Cell(`HF_{Mode}`)
		List(C, { FillDirection = Enum.FillDirection.Horizontal, HorizontalFlex = Enum.UIFlexAlignment[Mode], Padding = UDim.new(0, 4) })
		Standard(C)
	end
	for _, Mode in { "Fill", "SpaceBetween", "SpaceAround", "SpaceEvenly" } do
		local C = Cell(`VF_{Mode}`)
		List(C, { VerticalFlex = Enum.UIFlexAlignment[Mode], Padding = UDim.new(0, 4) })
		Standard(C)
	end

	-- Cross-axis flex (the other flex property)
	for _, Mode in { "Fill", "SpaceBetween", "SpaceEvenly" } do
		local C = Cell(`HX_{Mode}`)
		List(C, { FillDirection = Enum.FillDirection.Horizontal, VerticalFlex = Enum.UIFlexAlignment[Mode], Padding = UDim.new(0, 4) })
		Standard(C)
	end

	-- Fill with overflow (shrink)
	do
		local C = Cell("FillOverflow")
		List(C, { FillDirection = Enum.FillDirection.Horizontal, HorizontalFlex = Enum.UIFlexAlignment.Fill, Padding = UDim.new(0, 4) })
		Item(C, "A", 90, 20)
		Item(C, "B", 60, 30)
		Item(C, "C", 70, 15)
	end

	-- Wraps
	for _, HA in { "Left", "Center", "Right" } do
		local C = Cell(`Wrap_{HA}`)
		List(C, { FillDirection = Enum.FillDirection.Horizontal, Wraps = true, HorizontalAlignment = Enum.HorizontalAlignment[HA], VerticalAlignment = Enum.VerticalAlignment.Center, Padding = UDim.new(0, 5) })
		for Index = 1, 7 do
			Item(C, `I{Index}`, 20 + (Index * 13) % 37, 12 + (Index * 7) % 19)
		end
	end
	do
		local C = Cell("WrapV")
		List(C, { Wraps = true, HorizontalAlignment = Enum.HorizontalAlignment.Center, VerticalAlignment = Enum.VerticalAlignment.Bottom, Padding = UDim.new(0, 3) })
		for Index = 1, 7 do
			Item(C, `I{Index}`, 20 + (Index * 11) % 29, 15 + (Index * 13) % 23)
		end
	end
	do
		local C = Cell("WrapFill")
		List(C, { FillDirection = Enum.FillDirection.Horizontal, Wraps = true, HorizontalFlex = Enum.UIFlexAlignment.Fill, Padding = UDim.new(0, 5) })
		for Index = 1, 6 do
			Item(C, `I{Index}`, 30 + (Index * 13) % 37, 14 + (Index * 7) % 19)
		end
	end
	do
		local C = Cell("WrapSpaceBetween")
		List(C, { FillDirection = Enum.FillDirection.Horizontal, Wraps = true, HorizontalFlex = Enum.UIFlexAlignment.SpaceBetween, VerticalFlex = Enum.UIFlexAlignment.SpaceEvenly, Padding = UDim.new(0, 5) })
		for Index = 1, 6 do
			Item(C, `I{Index}`, 30 + (Index * 13) % 37, 14 + (Index * 7) % 19)
		end
	end

	-- ItemLineAlignment on the layout
	for _, Alignment in { "Start", "Center", "End", "Stretch" } do
		local C = Cell(`ILA_{Alignment}`)
		List(C, { FillDirection = Enum.FillDirection.Horizontal, ItemLineAlignment = Enum.ItemLineAlignment[Alignment], Wraps = true, Padding = UDim.new(0, 4) })
		for Index = 1, 6 do
			Item(C, `I{Index}`, 25 + (Index * 13) % 31, 10 + (Index * 9) % 23)
		end
	end
	do
		local C = Cell("ILA_NoWrap")
		List(C, { FillDirection = Enum.FillDirection.Horizontal, ItemLineAlignment = Enum.ItemLineAlignment.Center, Padding = UDim.new(0, 4) })
		Standard(C)
	end

	-- UIFlexItem modes
	do
		local C = Cell("FlexGrow")
		List(C, { FillDirection = Enum.FillDirection.Horizontal, Padding = UDim.new(0, 4) })
		Item(C, "A", 30, 20)
		Flex(Item(C, "B", 20, 30), { FlexMode = Enum.UIFlexMode.Grow })
		Item(C, "C", 40, 15)
	end
	do
		local C = Cell("FlexShrink")
		List(C, { FillDirection = Enum.FillDirection.Horizontal, Padding = UDim.new(0, 4) })
		Item(C, "A", 80, 20)
		Flex(Item(C, "B", 90, 30), { FlexMode = Enum.UIFlexMode.Shrink })
		Item(C, "C", 40, 15)
	end
	do
		local C = Cell("FlexFill")
		List(C, { FillDirection = Enum.FillDirection.Horizontal, Padding = UDim.new(0, 4) })
		Flex(Item(C, "A", 30, 20), { FlexMode = Enum.UIFlexMode.Fill })
		Flex(Item(C, "B", 50, 30), { FlexMode = Enum.UIFlexMode.Fill })
		Item(C, "C", 40, 15)
	end
	do
		local C = Cell("FlexCustom")
		List(C, { FillDirection = Enum.FillDirection.Horizontal, Padding = UDim.new(0, 4) })
		Flex(Item(C, "A", 30, 20), { FlexMode = Enum.UIFlexMode.Custom, GrowRatio = 1, ShrinkRatio = 0 })
		Flex(Item(C, "B", 20, 30), { FlexMode = Enum.UIFlexMode.Custom, GrowRatio = 2.5, ShrinkRatio = 1 })
		Flex(Item(C, "C", 40, 15), { FlexMode = Enum.UIFlexMode.Custom, GrowRatio = 0.5, ShrinkRatio = 0 })
	end
	do
		local C = Cell("FlexCustomShrink")
		List(C, { FillDirection = Enum.FillDirection.Horizontal, Padding = UDim.new(0, 4) })
		Flex(Item(C, "A", 90, 20), { FlexMode = Enum.UIFlexMode.Custom, GrowRatio = 1, ShrinkRatio = 1 })
		Flex(Item(C, "B", 60, 30), { FlexMode = Enum.UIFlexMode.Custom, GrowRatio = 0, ShrinkRatio = 3 })
		Flex(Item(C, "C", 50, 15), { FlexMode = Enum.UIFlexMode.Custom, GrowRatio = 0, ShrinkRatio = 0.5 })
	end
	do
		local C = Cell("FlexVertical")
		List(C, { Padding = UDim.new(0, 4), HorizontalAlignment = Enum.HorizontalAlignment.Center })
		Item(C, "A", 30, 20)
		Flex(Item(C, "B", 50, 20), { FlexMode = Enum.UIFlexMode.Grow })
		Flex(Item(C, "C", 40, 15), { FlexMode = Enum.UIFlexMode.Custom, GrowRatio = 3 })
	end
	do
		local C = Cell("FlexItemAlign")
		List(C, { FillDirection = Enum.FillDirection.Horizontal, Padding = UDim.new(0, 4), ItemLineAlignment = Enum.ItemLineAlignment.Start })
		Flex(Item(C, "A", 30, 20), { ItemLineAlignment = Enum.ItemLineAlignment.End })
		Flex(Item(C, "B", 20, 30), { ItemLineAlignment = Enum.ItemLineAlignment.Center })
		Flex(Item(C, "C", 40, 15), { ItemLineAlignment = Enum.ItemLineAlignment.Stretch })
		Item(C, "D", 25, 25)
	end
	do
		local C = Cell("FlexGrowWrap")
		List(C, { FillDirection = Enum.FillDirection.Horizontal, Wraps = true, Padding = UDim.new(0, 4) })
		Item(C, "A", 60, 20)
		Flex(Item(C, "B", 50, 30), { FlexMode = Enum.UIFlexMode.Grow })
		Item(C, "C", 70, 15)
		Flex(Item(C, "D", 40, 25), { FlexMode = Enum.UIFlexMode.Grow })
	end
	do
		local C = Cell("FlexCenterGrow")
		List(C, { FillDirection = Enum.FillDirection.Horizontal, HorizontalAlignment = Enum.HorizontalAlignment.Center, VerticalAlignment = Enum.VerticalAlignment.Center, Padding = UDim.new(0, 4) })
		Item(C, "A", 30, 20)
		Flex(Item(C, "B", 20, 30), { FlexMode = Enum.UIFlexMode.Grow })
	end
	do
		local C = Cell("FlexScaleSize")
		List(C, { FillDirection = Enum.FillDirection.Horizontal, Padding = UDim.new(0.02, 0), HorizontalFlex = Enum.UIFlexAlignment.Fill })
		Item(C, "A", UDim2.new(0.2, 0, 0.5, 0))
		Item(C, "B", UDim2.new(0.1, 5, 1, -10))
		Item(C, "C", UDim2.new(0, 17, 0.3, 0))
	end
end
