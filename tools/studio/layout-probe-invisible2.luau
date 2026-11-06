-- Invisible AutomaticSize items of a wrapping UIListLayout (a mode window's Chips row: an AutoXY
-- TextButton chip created with Visible = false in a horizontal list with Wraps = true reads 0 x 0 in
-- Studio; layout-probe-invisible, without Wraps, gives hidden automatic objects their full size).
-- Wraps on / off, horizontal / vertical, flex, AutoY row container, fixed-size items, a wrapping row
-- in an AutoX container and a vertical wrapping column of fixed height.
return function(Target)
	local Results = {}
	local Nodes = {}
	local Count = 0
	local function Row(Name, Options)
		Count += 1
		local H = Instance.new("Frame")
		H.BackgroundTransparency = 1
		H.Position = UDim2.fromOffset(((Count - 1) % 3) * 400, ((Count - 1) // 3) * 100)
		H.Size = UDim2.fromOffset(390, 90)
		H.Parent = Target
		local Chips = Instance.new("Frame")
		Chips.BackgroundTransparency = 1
		Chips.AutomaticSize = if Options.AutoX then Enum.AutomaticSize.X elseif Options.AutoY == false then Enum.AutomaticSize.None else Enum.AutomaticSize.Y
		Chips.Size = if Options.AutoX then UDim2.fromOffset(0, 70) else UDim2.new(1, -15, 0, if Options.AutoY == false then 60 else 0)
		Chips.Parent = H
		local List = Instance.new("UIListLayout")
		List.FillDirection = if Options.Vertical then Enum.FillDirection.Vertical else Enum.FillDirection.Horizontal
		List.Padding = UDim.new(0, 6)
		List.SortOrder = Enum.SortOrder.LayoutOrder
		List.Wraps = Options.Wraps == true
		if Options.Flex then
			List.HorizontalFlex = Enum.UIFlexAlignment.Fill
		end
		List.Parent = Chips
		for Index, Visible in { true, true, false } do
			local Chip = Instance.new("TextButton")
			Chip.AutomaticSize = if Options.Fixed then Enum.AutomaticSize.None else Enum.AutomaticSize.XY
			Chip.Size = if Options.Fixed then UDim2.fromOffset(150, 20) else UDim2.new()
			Chip.Text = ""
			Chip.LayoutOrder = Index
			Chip.Visible = Visible
			local Pad = Instance.new("UIPadding")
			Pad.PaddingLeft = UDim.new(0, 8)
			Pad.PaddingRight = UDim.new(0, 8)
			Pad.PaddingTop = UDim.new(0, 5)
			Pad.PaddingBottom = UDim.new(0, 5)
			Pad.Parent = Chip
			local Label = Instance.new("TextLabel")
			Label.AutomaticSize = Enum.AutomaticSize.XY
			Label.Size = UDim2.new()
			Label.Text = `Chip number {Index}`
			Label.TextSize = 15
			Label.Parent = Chip
			Chip.Parent = Chips
			Nodes[`{Name}_{Index}`] = Chip
		end
		Nodes[`{Name}_row`] = Chips
	end
	Row("wraps", { Wraps = true })
	Row("nowrap", { Wraps = false })
	Row("wrapsFixed", { Wraps = true, AutoY = false })
	Row("wrapsVertical", { Wraps = true, Vertical = true })
	Row("flex", { Flex = true })
	Row("wrapsFlex", { Wraps = true, Flex = true })
	Row("wrapsFixedItems", { Wraps = true, Fixed = true })
	Row("flexFixedItems", { Flex = true, Fixed = true })
	Row("wrapsAutoX", { Wraps = true, AutoX = true })
	Row("wrapsVerticalFixed", { Wraps = true, Vertical = true, AutoY = false })
	task.wait()
	for Key, Node in Nodes do
		Results[Key] = { Node.AbsolutePosition.X - Node.Parent.AbsolutePosition.X, Node.AbsolutePosition.Y - Node.Parent.AbsolutePosition.Y, Node.AbsoluteSize.X, Node.AbsoluteSize.Y }
	end
	return Results
end
