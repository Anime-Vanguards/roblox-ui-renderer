-- Plain children of ScrollingFrames with UIPadding: which basis Size and Position scale use per
-- axis, for AutomaticCanvasSize None / X / Y / XY (layout-probe-autocanvas: with AutomaticCanvasSize
-- Y a child's Size.X used the unpadded width while its Position.X used the padded one).
return function(Target)
	local Results = {}
	local Nodes = {}
	local Count = 0
	local function Holder()
		Count += 1
		local F = Instance.new("Frame")
		F.Position = UDim2.fromOffset(((Count - 1) % 6) * 200, ((Count - 1) // 6) * 250)
		F.Size = UDim2.fromOffset(190, 240)
		F.BackgroundTransparency = 1
		F.Parent = Target
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
	for _, Auto in { "None", "X", "Y", "XY" } do
		for _, Canvas in { { "zero", UDim2.new() }, { "big", UDim2.fromOffset(300, 300) } } do
			local Name = `{Auto}_{Canvas[1]}`
			local S = New("ScrollingFrame", Holder(), Name, {
				Size = UDim2.fromOffset(160, 120),
				AutomaticCanvasSize = Enum.AutomaticSize[Auto],
				CanvasSize = Canvas[2],
				ScrollBarThickness = 0,
				BorderSizePixel = 0,
			})
			New("UIPadding", S, nil, { PaddingTop = UDim.new(0.05, 2), PaddingLeft = UDim.new(0.1, 3), PaddingRight = UDim.new(0, 5), PaddingBottom = UDim.new(0.02, 0) })
			New("Frame", S, `{Name}_a`, { Size = UDim2.fromScale(0.5, 0.25), Position = UDim2.fromScale(0.1, 0.1) })
			New("Frame", S, `{Name}_end`, { Size = UDim2.fromOffset(10, 10), Position = UDim2.fromScale(1, 1), AnchorPoint = Vector2.new(1, 1) })
			New("Frame", S, `{Name}_big`, { Size = UDim2.fromOffset(200, 150) })
		end
	end
	do
		local F = New("Frame", Holder(), "frame", { Size = UDim2.fromOffset(160, 120) })
		New("UIPadding", F, nil, { PaddingTop = UDim.new(0.05, 2), PaddingLeft = UDim.new(0.1, 3), PaddingRight = UDim.new(0, 5), PaddingBottom = UDim.new(0.02, 0) })
		New("Frame", F, "frame_a", { Size = UDim2.fromScale(0.5, 0.25), Position = UDim2.fromScale(0.1, 0.1) })
		New("Frame", F, "frame_end", { Size = UDim2.fromOffset(10, 10), Position = UDim2.fromScale(1, 1), AnchorPoint = Vector2.new(1, 1) })
	end
	task.wait()
	task.wait()
	for Key, Node in Nodes do
		local Root = Node
		while Root.Parent ~= Target do
			Root = Root.Parent
		end
		local Entry = { Node.AbsolutePosition.X - Root.AbsolutePosition.X, Node.AbsolutePosition.Y - Root.AbsolutePosition.Y, Node.AbsoluteSize.X, Node.AbsoluteSize.Y }
		if Node:IsA("ScrollingFrame") then
			table.insert(Entry, Node.AbsoluteCanvasSize.X)
			table.insert(Entry, Node.AbsoluteCanvasSize.Y)
			table.insert(Entry, Node.AbsoluteWindowSize.X)
			table.insert(Entry, Node.AbsoluteWindowSize.Y)
		end
		Results[Key] = Entry
	end
	return Results
end
