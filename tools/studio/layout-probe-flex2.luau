-- UIListLayout flex rules (second pass): Space* with alignments/overflow, single-line cross flex,
-- per-item alignment in filled lines, shrink clamping.
return function(Target)
	local Cases = {}
	local Results = {}

	local function Case(Name, W, H, ListProps, Items)
		table.insert(Cases, { Name = Name, W = W, H = H, ListProps = ListProps, Items = Items })
	end

	local Row3 = { { 30, 20 }, { 23, 31 }, { 41, 15 } }
	for _, HA in { "Left", "Center", "Right" } do
		for _, Mode in { "SpaceBetween", "SpaceAround", "SpaceEvenly" } do
			Case(`One_{Mode}_{HA}`, 120, 60, { FillDirection = "Horizontal", HorizontalFlex = Mode, HorizontalAlignment = HA, Padding = UDim.new(0, 4) }, { { 30, 20 } })
			Case(`Over_{Mode}_{HA}`, 80, 60, { FillDirection = "Horizontal", HorizontalFlex = Mode, HorizontalAlignment = HA, Padding = UDim.new(0, 4) }, Row3)
			Case(`BigPad_{Mode}_{HA}`, 120, 60, { FillDirection = "Horizontal", HorizontalFlex = Mode, HorizontalAlignment = HA, Padding = UDim.new(0, 12) }, Row3)
		end
	end
	for _, VA in { "Top", "Center", "Bottom" } do
		for _, Mode in { "SpaceBetween", "SpaceAround", "Fill" } do
			Case(`Line_{Mode}_{VA}`, 120, 60, { FillDirection = "Horizontal", VerticalFlex = Mode, VerticalAlignment = VA, Padding = UDim.new(0, 4) }, Row3)
			Case(`LineOver_{Mode}_{VA}`, 120, 20, { FillDirection = "Horizontal", VerticalFlex = Mode, VerticalAlignment = VA, Padding = UDim.new(0, 4) }, Row3)
			Case(`LinesOver_{Mode}_{VA}`, 60, 50, { FillDirection = "Horizontal", Wraps = true, VerticalFlex = Mode, VerticalAlignment = VA, Padding = UDim.new(0, 4) }, Row3)
		end
	end
	Case("CrossFillItemOverride", 181, 121, { FillDirection = "Horizontal", Wraps = true, VerticalFlex = "Fill", Padding = UDim.new(0, 5) }, {
		{ 43, 21, Flex = { ItemLineAlignment = "End" } },
		{ 56, 28, Flex = { ItemLineAlignment = "Center" } },
		{ 32, 16 },
		{ 45, 23, Flex = { ItemLineAlignment = "Start" } },
		{ 58, 30 },
		{ 34, 18, Flex = { ItemLineAlignment = "Stretch" } },
	})
	Case("ShrinkFreeze", 80, 60, { FillDirection = "Horizontal", Padding = UDim.new(0, 0) }, {
		{ 10, 20, Flex = { FlexMode = "Custom", ShrinkRatio = 10 } },
		{ 100, 20, Flex = { FlexMode = "Custom", ShrinkRatio = 1 } },
	})
	Case("ShrinkFreeze3", 60, 60, { FillDirection = "Horizontal", Padding = UDim.new(0, 2) }, {
		{ 10, 20, Flex = { FlexMode = "Custom", ShrinkRatio = 20 } },
		{ 50, 20, Flex = { FlexMode = "Shrink" } },
		{ 40, 20, Flex = { FlexMode = "Shrink" } },
	})
	Case("ShrinkAllZero", 20, 60, { FillDirection = "Horizontal", Padding = UDim.new(0, 5) }, {
		{ 30, 20, Flex = { FlexMode = "Shrink" } },
		{ 40, 20, Flex = { FlexMode = "Shrink" } },
	})
	Case("FillNoneItems", 150, 60, { FillDirection = "Horizontal", HorizontalFlex = "Fill", HorizontalAlignment = "Center", Padding = UDim.new(0, 4) }, {
		{ 30, 20, Flex = { FlexMode = "None" } },
		{ 40, 20, Flex = { FlexMode = "None" } },
	})
	Case("GrowOnlyWithEvenly", 150, 60, { FillDirection = "Horizontal", HorizontalFlex = "SpaceEvenly", Padding = UDim.new(0, 4) }, {
		{ 30, 20, Flex = { FlexMode = "Shrink" } },
		{ 40, 20 },
	})
	Case("VerticalWrapEvenly", 100, 90, { Wraps = true, VerticalFlex = "SpaceEvenly", HorizontalFlex = "SpaceAround", Padding = UDim.new(0, 3) }, {
		{ 20, 30 }, { 25, 40 }, { 30, 25 }, { 22, 35 }, { 28, 20 },
	})
	Case("WrapCenterFill", 160, 100, { FillDirection = "Horizontal", Wraps = true, HorizontalFlex = "Fill", VerticalAlignment = "Center", HorizontalAlignment = "Center", Padding = UDim.new(0, 4) }, {
		{ 60, 20 }, { 50, 30 }, { 70, 15 }, { 40, 25 },
	})

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
					elseif Key == "ItemLineAlignment" then
						Flex.ItemLineAlignment = Enum.ItemLineAlignment[Value]
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
