-- UIListLayout flex rules: Space* padding behaviour, overflow, cross-axis flex with several lines,
-- ItemLineAlignment inside stretched lines, UIFlexItem with size constraints, AbsoluteContentSize.
return function(Target)
	local Cases = {}
	local Results = {}

	local function Case(Name, W, H, ListProps, Items)
		table.insert(Cases, { Name = Name, W = W, H = H, ListProps = ListProps, Items = Items })
	end

	local Row3 = { { 30, 20 }, { 23, 31 }, { 41, 15 } }
	local Wrap6 = { { 43, 21 }, { 56, 28 }, { 32, 16 }, { 45, 23 }, { 58, 30 }, { 34, 18 } }

	-- Space* with a padding larger than the distributed gap, and with overflow
	for _, Mode in { "SpaceBetween", "SpaceAround", "SpaceEvenly" } do
		Case(`BigPad_{Mode}`, 120, 60, { FillDirection = "Horizontal", HorizontalFlex = Mode, Padding = UDim.new(0, 12) }, Row3)
		Case(`HugePad_{Mode}`, 120, 60, { FillDirection = "Horizontal", HorizontalFlex = Mode, Padding = UDim.new(0, 40) }, Row3)
		Case(`Over_{Mode}`, 80, 60, { FillDirection = "Horizontal", HorizontalFlex = Mode, Padding = UDim.new(0, 4) }, Row3)
		Case(`One_{Mode}`, 120, 60, { FillDirection = "Horizontal", HorizontalFlex = Mode, Padding = UDim.new(0, 4) }, { { 30, 20 } })
		Case(`AlignRight_{Mode}`, 181, 60, { FillDirection = "Horizontal", HorizontalFlex = Mode, HorizontalAlignment = "Right", Padding = UDim.new(0, 4) }, Row3)
	end

	-- Cross-axis flex with several lines
	for _, Mode in { "None", "Fill", "SpaceBetween", "SpaceAround", "SpaceEvenly" } do
		for _, VA in { "Top", "Center", "Bottom" } do
			Case(`Cross_{Mode}_{VA}`, 181, 121, { FillDirection = "Horizontal", Wraps = true, VerticalFlex = Mode, VerticalAlignment = VA, Padding = UDim.new(0, 5) }, Wrap6)
		end
	end
	for _, Mode in { "Fill", "SpaceEvenly" } do
		for _, ILA in { "Start", "Center", "End", "Stretch" } do
			Case(`CrossILA_{Mode}_{ILA}`, 181, 121, { FillDirection = "Horizontal", Wraps = true, VerticalFlex = Mode, ItemLineAlignment = ILA, Padding = UDim.new(0, 5) }, Wrap6)
		end
	end
	Case("CrossFill_NoWrap_Center", 181, 121, { FillDirection = "Horizontal", VerticalFlex = "Fill", VerticalAlignment = "Center", Padding = UDim.new(0, 5) }, Row3)
	Case("CrossFill_NoWrap_ILACenter", 181, 121, { FillDirection = "Horizontal", VerticalFlex = "Fill", ItemLineAlignment = "Center", Padding = UDim.new(0, 5) }, Row3)
	Case("CrossFill_Vertical", 121, 181, { Wraps = true, HorizontalFlex = "Fill", Padding = UDim.new(0, 5) }, Wrap6)
	Case("CrossEvenly_Vertical", 121, 181, { Wraps = true, HorizontalFlex = "SpaceEvenly", Padding = UDim.new(0, 5) }, Wrap6)
	Case("CrossFill_Overflow", 181, 40, { FillDirection = "Horizontal", Wraps = true, VerticalFlex = "Fill", Padding = UDim.new(0, 5) }, Wrap6)

	-- ItemLineAlignment without flex, several lines, with vertical alignment
	for _, ILA in { "Start", "Center", "End", "Stretch" } do
		Case(`ILA_{ILA}_Center`, 181, 121, { FillDirection = "Horizontal", Wraps = true, ItemLineAlignment = ILA, VerticalAlignment = "Center", Padding = UDim.new(0, 5) }, Wrap6)
	end
	Case("ILA_Stretch_NoWrap", 181, 121, { FillDirection = "Horizontal", ItemLineAlignment = "Stretch", Padding = UDim.new(0, 5) }, Row3)
	Case("ILA_Stretch_Vertical", 121, 181, { ItemLineAlignment = "Stretch", Padding = UDim.new(0, 5) }, Row3)

	-- UIFlexItem with size constraints and odd ratios
	Case("ShrinkToZero", 60, 60, { FillDirection = "Horizontal", Padding = UDim.new(0, 4) }, {
		{ 20, 20, Flex = { FlexMode = "Shrink" } },
		{ 80, 31 },
	})
	Case("GrowRatios", 300, 60, { FillDirection = "Horizontal", Padding = UDim.new(0, 3) }, {
		{ 10, 20, Flex = { FlexMode = "Custom", GrowRatio = 1 } },
		{ 20, 20, Flex = { FlexMode = "Custom", GrowRatio = 2 } },
		{ 30, 20, Flex = { FlexMode = "Custom", GrowRatio = 3 } },
		{ 40, 20, Flex = { FlexMode = "Custom", GrowRatio = 0 } },
	})
	Case("FillPlusGrowItem", 300, 60, { FillDirection = "Horizontal", HorizontalFlex = "Fill", Padding = UDim.new(0, 3) }, {
		{ 10, 20, Flex = { FlexMode = "Custom", GrowRatio = 3 } },
		{ 20, 20 },
		{ 30, 20, Flex = { FlexMode = "None" } },
	})
	Case("FillPlusSpace", 300, 60, { FillDirection = "Horizontal", HorizontalFlex = "SpaceEvenly", Padding = UDim.new(0, 3) }, {
		{ 10, 20, Flex = { FlexMode = "Grow" } },
		{ 20, 20 },
		{ 30, 20 },
	})
	Case("GrowWrapLines", 150, 100, { FillDirection = "Horizontal", Wraps = true, Padding = UDim.new(0, 4) }, {
		{ 60, 20 },
		{ 50, 30, Flex = { FlexMode = "Grow" } },
		{ 70, 15 },
		{ 40, 25, Flex = { FlexMode = "Grow" } },
		{ 30, 25, Flex = { FlexMode = "Custom", GrowRatio = 2 } },
	})
	Case("ShrinkWrapLines", 150, 100, { FillDirection = "Horizontal", Wraps = true, Padding = UDim.new(0, 4) }, {
		{ 160, 20, Flex = { FlexMode = "Shrink" } },
		{ 50, 30 },
	})
	Case("ScalePadFill", 181, 60, { FillDirection = "Horizontal", HorizontalFlex = "Fill", Padding = UDim.new(0.05, 1) }, Row3)
	Case("ScalePadEvenly", 181, 60, { FillDirection = "Horizontal", HorizontalFlex = "SpaceEvenly", Padding = UDim.new(0.05, 1) }, Row3)
	Case("VerticalFill", 60, 181, { VerticalFlex = "Fill", Padding = UDim.new(0, 4) }, Row3)
	Case("VerticalShrink", 60, 50, { VerticalFlex = "Fill", Padding = UDim.new(0, 4) }, Row3)

	-- Build
	local Built = {}
	local Column, Y, RowHeight, X = 0, 0, 0, 0
	for _, Spec in Cases do
		if X + Spec.W > 1180 then
			X = 0
			Y += RowHeight + 10
			RowHeight = 0
		end
		local Holder = Instance.new("Frame")
		Holder.Name = Spec.Name
		Holder.Position = UDim2.fromOffset(X + 10, Y + 10)
		Holder.Size = UDim2.fromOffset(Spec.W, Spec.H)
		Holder.BorderSizePixel = 0
		Holder.Parent = Target
		X += Spec.W + 10
		RowHeight = math.max(RowHeight, Spec.H)
		Column += 1

		local List = Instance.new("UIListLayout")
		List.SortOrder = Enum.SortOrder.LayoutOrder
		for Key, Value in Spec.ListProps do
			if Key == "FillDirection" then
				List.FillDirection = Enum.FillDirection[Value]
			elseif Key == "HorizontalFlex" or Key == "VerticalFlex" then
				(List :: any)[Key] = Enum.UIFlexAlignment[Value]
			elseif Key == "HorizontalAlignment" then
				List.HorizontalAlignment = Enum.HorizontalAlignment[Value]
			elseif Key == "VerticalAlignment" then
				List.VerticalAlignment = Enum.VerticalAlignment[Value]
			elseif Key == "ItemLineAlignment" then
				List.ItemLineAlignment = Enum.ItemLineAlignment[Value]
			else
				(List :: any)[Key] = Value
			end
		end
		List.Parent = Holder

		local Items = {}
		for Index, ItemSpec in Spec.Items do
			local Item = Instance.new("Frame")
			Item.Name = `I{Index}`
			Item.LayoutOrder = Index
			Item.Size = UDim2.fromOffset(ItemSpec[1], ItemSpec[2])
			Item.BorderSizePixel = 0
			if ItemSpec.Flex then
				local Flex = Instance.new("UIFlexItem")
				for Key, Value in ItemSpec.Flex do
					if Key == "FlexMode" then
						Flex.FlexMode = Enum.UIFlexMode[Value]
					else
						(Flex :: any)[Key] = Value
					end
				end
				Flex.Parent = Item
			end
			if ItemSpec.Min or ItemSpec.Max then
				local Constraint = Instance.new("UISizeConstraint")
				Constraint.MinSize = ItemSpec.Min or Vector2.zero
				Constraint.MaxSize = ItemSpec.Max or Vector2.new(math.huge, math.huge)
				Constraint.Parent = Item
			end
			Item.Parent = Holder
			table.insert(Items, Item)
		end
		table.insert(Built, { Spec = Spec, Holder = Holder, List = List, Items = Items })
	end

	task.wait()
	for _, Entry in Built do
		local Origin = Entry.Holder.AbsolutePosition
		local Rects = {}
		for _, Item in Entry.Items do
			table.insert(Rects, { Item.AbsolutePosition.X - Origin.X, Item.AbsolutePosition.Y - Origin.Y, Item.AbsoluteSize.X, Item.AbsoluteSize.Y })
		end
		Results[Entry.Spec.Name] = { items = Rects, content = { Entry.List.AbsoluteContentSize.X, Entry.List.AbsoluteContentSize.Y } }
	end
	return Results
end
