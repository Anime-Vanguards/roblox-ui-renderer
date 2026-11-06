-- Basis of UIListLayout.Padding scale / UIGridLayout.CellPadding scale in containers with a
-- UIPadding: the padded content size or the container size?
return function(Target)
	local Results = {}
	local Nodes = {}
	local Count = 0
	local function Holder()
		Count += 1
		local F = Instance.new("Frame")
		F.Position = UDim2.fromOffset(((Count - 1) % 5) * 230, ((Count - 1) // 5) * 180)
		F.Size = UDim2.fromOffset(200, 150)
		F.BackgroundTransparency = 1
		F.Parent = Target
		local Pad = Instance.new("UIPadding")
		Pad.PaddingTop = UDim.new(0, 20)
		Pad.PaddingBottom = UDim.new(0, 10)
		Pad.PaddingLeft = UDim.new(0.1, 0)
		Pad.PaddingRight = UDim.new(0, 30)
		Pad.Parent = F
		return F
	end
	local function New(ClassName, Parent, Key, Props)
		local Object = Instance.new(ClassName)
		for K, V in Props or {} do
			(Object :: any)[K] = V
		end
		Object.Parent = Parent
		if Key then
			Nodes[Key] = Object
		end
		return Object
	end
	do
		local H = Holder()
		New("UIListLayout", H, nil, { Padding = UDim.new(0.1, 0), SortOrder = Enum.SortOrder.LayoutOrder })
		New("Frame", H, "v_a", { Size = UDim2.new(0.5, 0, 0.2, 0), LayoutOrder = 1 })
		New("Frame", H, "v_b", { Size = UDim2.fromOffset(20, 20), LayoutOrder = 2 })
	end
	do
		local H = Holder()
		New("UIListLayout", H, nil, { FillDirection = Enum.FillDirection.Horizontal, Padding = UDim.new(0.1, 0), SortOrder = Enum.SortOrder.LayoutOrder })
		New("Frame", H, "h_a", { Size = UDim2.new(0.2, 0, 0.5, 0), LayoutOrder = 1 })
		New("Frame", H, "h_b", { Size = UDim2.fromOffset(20, 20), LayoutOrder = 2 })
	end
	do
		local H = Holder()
		New("UIGridLayout", H, nil, { CellSize = UDim2.fromScale(0.3, 0.3), CellPadding = UDim2.fromScale(0.1, 0.1), SortOrder = Enum.SortOrder.LayoutOrder })
		for Index = 1, 4 do
			New("Frame", H, `g_{Index}`, { LayoutOrder = Index })
		end
	end
	do
		local H = Holder()
		local A = New("Frame", H, "auto", { AutomaticSize = Enum.AutomaticSize.Y, Size = UDim2.new(1, 0, 0, 0) })
		New("UIPadding", A, nil, { PaddingTop = UDim.new(0.1, 4), PaddingBottom = UDim.new(0, 6) })
		New("UIListLayout", A, nil, { Padding = UDim.new(0.1, 0), SortOrder = Enum.SortOrder.LayoutOrder })
		New("Frame", A, "auto_a", { Size = UDim2.new(0.5, 0, 0, 30), LayoutOrder = 1 })
		New("Frame", A, "auto_b", { Size = UDim2.fromOffset(20, 20), LayoutOrder = 2 })
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
