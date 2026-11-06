-- Size bases inside AutomaticCanvasSize ScrollingFrames: UIPadding scale vs children's scale sizes
-- vs list / grid scale padding, for default ({0,0},{2,0}), zero and explicit CanvasSize, with and
-- without a visible bar (an emote window: padding scale uses the canvas, grid CellPadding the window).
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
	local Canvases = {
		{ "def", nil },
		{ "zero", UDim2.new() },
		{ "off", UDim2.fromOffset(0, 400) },
		{ "sc", UDim2.fromScale(0, 1.5) },
	}
	for _, Canvas in Canvases do
		for _, Kind in { "plain", "list", "grid" } do
			for _, Bar in { 0, 10 } do
				local Name = `{Canvas[1]}_{Kind}_t{Bar}`
				local S = New("ScrollingFrame", Holder(), Name, {
					Size = UDim2.fromOffset(160, 120),
					AutomaticCanvasSize = Enum.AutomaticSize.Y,
					ScrollBarThickness = Bar,
					BorderSizePixel = 0,
				})
				if Canvas[2] then
					S.CanvasSize = Canvas[2]
				end
				New("UIPadding", S, nil, { PaddingTop = UDim.new(0.05, 2), PaddingLeft = UDim.new(0.1, 0) })
				if Kind == "plain" then
					New("Frame", S, `{Name}_a`, { Size = UDim2.fromScale(0.5, 0.25), Position = UDim2.fromScale(0.1, 0.1) })
					New("Frame", S, `{Name}_b`, { Size = UDim2.fromOffset(20, 300) })
				elseif Kind == "list" then
					New("UIListLayout", S, nil, { Padding = UDim.new(0.05, 0), SortOrder = Enum.SortOrder.LayoutOrder })
					New("Frame", S, `{Name}_a`, { Size = UDim2.fromScale(0.5, 0.25), LayoutOrder = 1 })
					New("Frame", S, `{Name}_b`, { Size = UDim2.fromOffset(20, 300), LayoutOrder = 2 })
					New("Frame", S, `{Name}_c`, { Size = UDim2.fromOffset(20, 10), LayoutOrder = 3 })
				else
					New("UIGridLayout", S, nil, { CellSize = UDim2.fromScale(0.4, 0.3), CellPadding = UDim2.fromScale(0.05, 0.1), SortOrder = Enum.SortOrder.LayoutOrder })
					for Index = 1, 7 do
						New("Frame", S, if Index <= 3 or Index == 7 then `{Name}_{Index}` else nil, { LayoutOrder = Index })
					end
				end
			end
		end
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
