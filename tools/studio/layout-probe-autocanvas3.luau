-- AutomaticCanvasSize Y measurement, one term at a time: a bottom-anchored plain child, list gap
-- scale only, list item scale only, grid padding scale only, grid cell scale only; with and without
-- a scale UIPadding and with CanvasSize offsets 0 / 200 / 400.
return function(Target)
	local Results = {}
	local Nodes = {}
	local Count = 0
	local function Holder()
		Count += 1
		local F = Instance.new("Frame")
		F.Position = UDim2.fromOffset(((Count - 1) % 8) * 150, ((Count - 1) // 8) * 170)
		F.Size = UDim2.fromOffset(140, 160)
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
	for _, Offset in { 0, 200, 400 } do
		for _, Padded in { false, true } do
			for _, Kind in { "anchor", "gap", "item", "cellpad", "cell" } do
				local Name = `{Kind}_o{Offset}_{if Padded then "p" else "n"}`
				local S = New("ScrollingFrame", Holder(), Name, {
					Size = UDim2.fromOffset(120, 100),
					AutomaticCanvasSize = Enum.AutomaticSize.Y,
					CanvasSize = UDim2.fromOffset(0, Offset),
					ScrollBarThickness = 0,
					BorderSizePixel = 0,
				})
				if Padded then
					New("UIPadding", S, nil, { PaddingTop = UDim.new(0.05, 2), PaddingBottom = UDim.new(0.1, 1) })
				end
				if Kind == "anchor" then
					New("Frame", S, `{Name}_a`, { Size = UDim2.fromOffset(10, 10), Position = UDim2.fromScale(0, 1), AnchorPoint = Vector2.new(0, 1) })
					New("Frame", S, `{Name}_b`, { Size = UDim2.fromOffset(10, 150) })
				elseif Kind == "gap" then
					New("UIListLayout", S, nil, { Padding = UDim.new(0.1, 0), SortOrder = Enum.SortOrder.LayoutOrder })
					for Index = 1, 3 do
						New("Frame", S, `{Name}_{Index}`, { Size = UDim2.fromOffset(20, 60), LayoutOrder = Index })
					end
				elseif Kind == "item" then
					New("UIListLayout", S, nil, { SortOrder = Enum.SortOrder.LayoutOrder })
					New("Frame", S, `{Name}_1`, { Size = UDim2.new(0, 20, 0.5, 0), LayoutOrder = 1 })
					New("Frame", S, `{Name}_2`, { Size = UDim2.fromOffset(20, 150), LayoutOrder = 2 })
				elseif Kind == "cellpad" then
					New("UIGridLayout", S, nil, { CellSize = UDim2.fromOffset(50, 40), CellPadding = UDim2.fromScale(0.05, 0.1), SortOrder = Enum.SortOrder.LayoutOrder })
					for Index = 1, 6 do
						New("Frame", S, if Index == 5 then `{Name}_5` else nil, { LayoutOrder = Index })
					end
				else
					New("UIGridLayout", S, nil, { CellSize = UDim2.new(0, 50, 0.3, 0), CellPadding = UDim2.fromOffset(4, 4), SortOrder = Enum.SortOrder.LayoutOrder })
					for Index = 1, 6 do
						New("Frame", S, if Index == 5 then `{Name}_5` else nil, { LayoutOrder = Index })
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
			table.insert(Entry, Node.AbsoluteCanvasSize.Y)
		end
		Results[Key] = Entry
	end
	return Results
end
