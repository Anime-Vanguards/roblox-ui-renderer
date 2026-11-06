-- Why the second-pass list gap counts for AutomaticSize in layout-probe-autolistgap but not in
-- layout-probe-listpad's "auto" case: the listpad setup (vertical, Size (1, 0, 0, 0), UIPadding with
-- a scale top, a scale-width item) changed one ingredient at a time.
return function(Target)
	local Results = {}
	local Nodes = {}
	local Count = 0
	local function New(ClassName, Parent, Props)
		local Object = Instance.new(ClassName)
		for K, V in Props or {} do
			(Object :: any)[K] = V
		end
		Object.Parent = Parent
		return Object
	end
	local function Case(Name, Options)
		Count += 1
		local Holder = New("Frame", Target, {
			Position = UDim2.fromOffset(((Count - 1) % 6) * 200, ((Count - 1) // 6) * 170),
			Size = UDim2.fromOffset(190, 150),
			BackgroundTransparency = 1,
		})
		local F = New("Frame", Holder, { AutomaticSize = Enum.AutomaticSize.Y, Size = Options.size or UDim2.new(1, 0, 0, 0) })
		Nodes[Name] = F
		if Options.pad then
			New("UIPadding", F, { PaddingTop = Options.pad[1], PaddingBottom = Options.pad[2] })
		end
		New("UIListLayout", F, { Padding = Options.gap or UDim.new(0.1, 0), SortOrder = Enum.SortOrder.LayoutOrder })
		Nodes[`{Name}_a`] = New("Frame", F, { Size = Options.first or UDim2.new(0.5, 0, 0, 30), LayoutOrder = 1 })
		Nodes[`{Name}_b`] = New("Frame", F, { Size = UDim2.fromOffset(20, 20), LayoutOrder = 2 })
	end
	Case("listpad", { pad = { UDim.new(0.1, 4), UDim.new(0, 6) } })
	Case("noPad", {})
	Case("offsetPad", { pad = { UDim.new(0, 4), UDim.new(0, 6) } })
	Case("scalePadOnly", { pad = { UDim.new(0.1, 0), UDim.new(0, 0) } })
	Case("offsetItem", { pad = { UDim.new(0.1, 4), UDim.new(0, 6) }, first = UDim2.fromOffset(75, 30) })
	Case("noPadOffsetItem", { first = UDim2.fromOffset(75, 30) })
	Case("base20", { size = UDim2.new(1, 0, 0, 20) })
	Case("base20Pad", { size = UDim2.new(1, 0, 0, 20), pad = { UDim.new(0.1, 4), UDim.new(0, 6) } })
	Case("scaleBase", { size = UDim2.new(1, 0, 0.2, 0) })
	Case("scaleBasePad", { size = UDim2.new(1, 0, 0.2, 0), pad = { UDim.new(0.1, 4), UDim.new(0, 6) } })
	Case("offsetGap", { gap = UDim.new(0, 8) })
	Case("widthOffset", { size = UDim2.new(0, 150, 0, 0) })
	task.wait()
	task.wait()
	for Key, Node in Nodes do
		local Root = Node
		while Root.Parent ~= Target do
			Root = Root.Parent
		end
		local Entry = { Node.AbsolutePosition.X - Root.AbsolutePosition.X, Node.AbsolutePosition.Y - Root.AbsolutePosition.Y, Node.AbsoluteSize.X, Node.AbsoluteSize.Y }
		local Layout = Node:FindFirstChildOfClass("UIListLayout")
		if Layout then
			table.insert(Entry, Layout.AbsoluteContentSize.Y)
		end
		Results[Key] = Entry
	end
	return Results
end
