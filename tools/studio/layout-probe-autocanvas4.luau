-- AutomaticCanvasSize with a scale-sized ScrollingFrame (round 9): the earlier autocanvas probes all
-- used offset sizes. One term at a time (a bottom-anchored plain child, a scale-sized plain child, list
-- gap scale, list item scale, grid padding scale, grid cell scale), on Y and on X, with and without a
-- scale UIPadding, for three frame Sizes (scale + offset, scale only, offset only).
return function(Target)
	local Results = {}
	local Nodes = {}
	local Count = 0
	local function Holder()
		Count += 1
		local F = Instance.new("Frame")
		F.Position = UDim2.fromOffset(((Count - 1) % 10) * 118, ((Count - 1) // 10) * 150)
		F.Size = UDim2.fromOffset(110, 140)
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
	local Sizes = {
		mix = UDim2.new(0.8, 5, 0.6, 10),
		sc = UDim2.fromScale(0.9, 0.7),
		off = UDim2.fromOffset(90, 80),
	}
	for SizeName, FrameSize in Sizes do
		for _, Axis in { "Y", "X" } do
			for _, Padded in { false, true } do
				for _, Kind in { "anchor", "plain", "gap", "item", "cellpad", "cell" } do
					local Name = `{Kind}_{SizeName}_{Axis}_{if Padded then "p" else "n"}`
					local S = New("ScrollingFrame", Holder(), Name, {
						Size = FrameSize,
						AutomaticCanvasSize = Enum.AutomaticSize[Axis],
						CanvasSize = UDim2.new(),
						ScrollBarThickness = 0,
						BorderSizePixel = 0,
					})
					if Padded then
						New("UIPadding", S, nil, {
							PaddingTop = UDim.new(0.05, 2), PaddingBottom = UDim.new(0.1, 1),
							PaddingLeft = UDim.new(0.05, 2), PaddingRight = UDim.new(0.1, 1),
						})
					end
					local Y = Axis == "Y"
					local function Along(Scale, Offset, CrossScale, CrossOffset)
						if Y then
							return UDim2.new(CrossScale, CrossOffset, Scale, Offset)
						end
						return UDim2.new(Scale, Offset, CrossScale, CrossOffset)
					end
					if Kind == "anchor" then
						New("Frame", S, `{Name}_a`, { Size = UDim2.fromOffset(10, 10), Position = if Y then UDim2.fromScale(0, 1) else UDim2.fromScale(1, 0), AnchorPoint = if Y then Vector2.new(0, 1) else Vector2.new(1, 0) })
						New("Frame", S, `{Name}_b`, { Size = Along(0, 150, 0, 10) })
					elseif Kind == "plain" then
						New("Frame", S, `{Name}_a`, { Size = Along(0.5, 3, 0, 10), Position = Along(0.25, 20, 0, 0) })
						New("Frame", S, `{Name}_b`, { Size = Along(0, 150, 0, 10), Position = Along(0, 0, 0, 12) })
					elseif Kind == "gap" then
						New("UIListLayout", S, nil, { Padding = UDim.new(0.1, 1), SortOrder = Enum.SortOrder.LayoutOrder, FillDirection = if Y then Enum.FillDirection.Vertical else Enum.FillDirection.Horizontal })
						for Index = 1, 3 do
							New("Frame", S, `{Name}_{Index}`, { Size = Along(0, 60, 0, 20), LayoutOrder = Index })
						end
					elseif Kind == "item" then
						New("UIListLayout", S, nil, { SortOrder = Enum.SortOrder.LayoutOrder, FillDirection = if Y then Enum.FillDirection.Vertical else Enum.FillDirection.Horizontal })
						New("Frame", S, `{Name}_1`, { Size = Along(0.5, 2, 0.5, 0), LayoutOrder = 1 })
						New("Frame", S, `{Name}_2`, { Size = Along(0, 150, 0, 20), LayoutOrder = 2 })
					elseif Kind == "cellpad" then
						New("UIGridLayout", S, nil, { CellSize = UDim2.fromOffset(30, 30), CellPadding = UDim2.new(0.05, 1, 0.1, 1), SortOrder = Enum.SortOrder.LayoutOrder, FillDirection = if Y then Enum.FillDirection.Horizontal else Enum.FillDirection.Vertical })
						for Index = 1, 8 do
							New("Frame", S, if Index == 8 then `{Name}_8` else nil, { LayoutOrder = Index })
						end
					else
						New("UIGridLayout", S, nil, { CellSize = UDim2.new(0.3, 1, 0.3, 1), CellPadding = UDim2.fromOffset(4, 4), SortOrder = Enum.SortOrder.LayoutOrder, FillDirection = if Y then Enum.FillDirection.Horizontal else Enum.FillDirection.Vertical })
						for Index = 1, 8 do
							New("Frame", S, if Index == 8 then `{Name}_8` else nil, { LayoutOrder = Index })
						end
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
		end
		Results[Key] = Entry
	end
	return Results
end
