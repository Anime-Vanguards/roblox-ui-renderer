-- AutomaticSize frames whose scale-sized child has children of its own (layout-probe-hoversurface: a
-- HoverSurface fills its list container in Studio even stripped down; the difference to
-- layout-probe-autofill's listSurface is Chrome's own scale-sized child).
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
	local function Case(Name, List, Build)
		Count += 1
		local Holder = New("Frame", Target, {
			Position = UDim2.fromOffset(((Count - 1) % 5) * 220, ((Count - 1) // 5) * 180),
			Size = UDim2.fromOffset(200, 160),
			BackgroundTransparency = 1,
		})
		if List then
			New("UIListLayout", Holder, { SortOrder = Enum.SortOrder.LayoutOrder })
		end
		local F = New("Frame", Holder, { AutomaticSize = Enum.AutomaticSize.XY, Size = UDim2.fromOffset(0, 0) })
		Nodes[Name] = F
		Build(F, Name)
	end
	local function Content(F)
		New("Frame", F, { Size = UDim2.fromOffset(60, 20) })
	end
	for _, List in { true, false } do
		local Prefix = if List then "list" else "plain"
		Case(`{Prefix}_fillchild`, List, function(F, Name)
			local Chrome = New("Frame", F, { Size = UDim2.fromScale(1, 1) })
			Nodes[Name .. "_chrome"] = Chrome
			New("Frame", Chrome, { Size = UDim2.fromScale(1, 1) })
			Content(F)
		end)
		Case(`{Prefix}_halfchild`, List, function(F, Name)
			local Chrome = New("Frame", F, { Size = UDim2.fromScale(1, 1) })
			Nodes[Name .. "_chrome"] = Chrome
			New("Frame", Chrome, { Size = UDim2.fromScale(0.5, 0.5) })
			Content(F)
		end)
		Case(`{Prefix}_fixedchild`, List, function(F, Name)
			local Chrome = New("Frame", F, { Size = UDim2.fromScale(1, 1) })
			Nodes[Name .. "_chrome"] = Chrome
			New("Frame", Chrome, { Size = UDim2.fromOffset(10, 10) })
			Content(F)
		end)
		Case(`{Prefix}_nochild`, List, function(F, Name)
			local Chrome = New("Frame", F, { Size = UDim2.fromScale(1, 1) })
			Nodes[Name .. "_chrome"] = Chrome
			Content(F)
		end)
		Case(`{Prefix}_offsetchrome`, List, function(F, Name)
			local Chrome = New("Frame", F, { Size = UDim2.fromOffset(30, 30) })
			Nodes[Name .. "_chrome"] = Chrome
			New("Frame", Chrome, { Size = UDim2.fromScale(1, 1) })
			Content(F)
		end)
		Case(`{Prefix}_deep`, List, function(F, Name)
			local Chrome = New("Frame", F, { Size = UDim2.fromScale(1, 0.5) })
			Nodes[Name .. "_chrome"] = Chrome
			local Mid = New("Frame", Chrome, { Size = UDim2.fromScale(1, 1) })
			New("Frame", Mid, { Size = UDim2.fromScale(1, 1) })
			Content(F)
		end)
	end
	task.wait()
	task.wait()
	for Key, Node in Nodes do
		local Root = Node
		while Root.Parent ~= Target do
			Root = Root.Parent
		end
		Results[Key] = { Node.AbsolutePosition.X - Root.AbsolutePosition.X, Node.AbsolutePosition.Y - Root.AbsolutePosition.Y, Node.AbsoluteSize.X, Node.AbsoluteSize.Y }
	end
	return Results
end
