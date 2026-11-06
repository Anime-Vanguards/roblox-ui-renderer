-- AutomaticSize containers whose UIListLayout has a scale Padding and offset-sized items
-- (a reroll window's chip row: Size (0.231, 0, 0, 0), AutomaticSize, horizontal list, Padding 0.135,
-- two 60 px chips, centred, under UIScale 0.8139): which basis the gap and the alignment use.
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
			Position = UDim2.fromOffset(((Count - 1) % 4) * 300, ((Count - 1) // 4) * 110),
			Size = UDim2.fromOffset(Options.parent or 280, 100),
			BackgroundTransparency = 1,
		})
		if Options.uiscale then
			New("UIScale", Holder, { Scale = Options.uiscale })
		end
		local F = New("Frame", Holder, {
			AutomaticSize = Enum.AutomaticSize[Options.auto or "XY"],
			Size = Options.size or UDim2.new(0.231, 0, 0, 0),
			Position = Options.position or UDim2.fromOffset(0, 10),
			AnchorPoint = Options.anchor or Vector2.zero,
		})
		Nodes[Name] = F
		New("UIListLayout", F, {
			FillDirection = Enum.FillDirection[Options.fill or "Horizontal"],
			Padding = Options.padding or UDim.new(0.135, 0),
			HorizontalAlignment = Enum.HorizontalAlignment[Options.halign or "Center"],
			VerticalAlignment = Enum.VerticalAlignment.Top,
			SortOrder = Enum.SortOrder.LayoutOrder,
		})
		for Index = 1, Options.items or 2 do
			Nodes[`{Name}_{Index}`] = New("Frame", F, { Size = UDim2.fromOffset(Options.item or 60, Options.item or 60), LayoutOrder = Index })
		end
	end
	Case("story", { uiscale = 0.8139, position = UDim2.new(1, -20, 0, 10), anchor = Vector2.new(1, 0), parent = 280 })
	Case("storyNoScale", { position = UDim2.new(1, -20, 0, 10), anchor = Vector2.new(1, 0), parent = 280 })
	Case("left", {})
	Case("leftScaled", { uiscale = 0.8139 })
	Case("autoX", { auto = "X", size = UDim2.new(0.231, 0, 0, 60) })
	Case("base0", { size = UDim2.new() })
	Case("base150", { size = UDim2.fromOffset(150, 0) })
	Case("offsetPad", { padding = UDim.new(0, 16) })
	Case("mixedPad", { padding = UDim.new(0.1, 5) })
	Case("three", { items = 3 })
	Case("leftAlign", { halign = "Left" })
	Case("rightAlign", { halign = "Right" })
	Case("vertical", { fill = "Vertical", auto = "Y", size = UDim2.new(0, 60, 0.2, 0), items = 2, item = 30 })
	Case("wide", { parent = 600 })
	Case("small", { item = 30 })
	Case("scaledBig", { uiscale = 1.3 })
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
			table.insert(Entry, Layout.AbsoluteContentSize.X)
			table.insert(Entry, Layout.AbsoluteContentSize.Y)
		end
		Results[Key] = Entry
	end
	return Results
end
