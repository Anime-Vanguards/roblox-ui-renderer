-- AutomaticCanvasSize on an inset axis, part 2 (round 9): list item scale, a bottom-anchored plain
-- child, a CanvasSize above / below the extent, XY with both bars inset; border 0 / 3.
return function(Target)
	local Results = {}
	local Nodes = {}
	local Count = 0
	local function Holder(ScaleBy)
		Count += 1
		local F = Instance.new("Frame")
		F.Position = UDim2.fromOffset(((Count - 1) % 8) * 145, ((Count - 1) // 8) * 190)
		F.Size = UDim2.fromOffset(110, 140)
		F.BackgroundTransparency = 1
		F.Parent = Target
		if ScaleBy then
			local U = Instance.new("UIScale")
			U.Scale = ScaleBy
			U.Parent = F
		end
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
	local function Case(Name, Opts)
		local S = New("ScrollingFrame", Holder(Opts.uiscale), Name, {
			Size = Opts.size or UDim2.new(0.9, 0, 0.7, 4),
			AutomaticCanvasSize = Enum.AutomaticSize[Opts.axis or "Y"],
			CanvasSize = Opts.canvas or UDim2.new(),
			ScrollBarThickness = Opts.thickness or 0,
			BorderSizePixel = Opts.border or 0,
			VerticalScrollBarInset = Opts.vinset or Enum.ScrollBarInset.None,
			HorizontalScrollBarInset = Opts.hinset or Enum.ScrollBarInset.None,
		})
		if Opts.maxsize then
			New("UISizeConstraint", S, nil, { MaxSize = Opts.maxsize })
		end
		if Opts.aspect then
			New("UIAspectRatioConstraint", S, nil, { AspectRatio = Opts.aspect })
		end
		if Opts.padded then
			New("UIPadding", S, nil, {
				PaddingTop = UDim.new(0.05, 2), PaddingBottom = UDim.new(0.1, 1),
				PaddingLeft = UDim.new(0.05, 2), PaddingRight = UDim.new(0.1, 1),
			})
		end
		local Y = (Opts.axis or "Y") ~= "X"
		if Opts.kind == "item" then
			New("UIListLayout", S, nil, { SortOrder = Enum.SortOrder.LayoutOrder })
			New("Frame", S, `{Name}_1`, { Size = UDim2.new(0, 20, 0.5, 2), LayoutOrder = 1 })
			New("Frame", S, `{Name}_2`, { Size = UDim2.fromOffset(20, 150), LayoutOrder = 2 })
		elseif Opts.kind == "anchor" then
			New("Frame", S, `{Name}_1`, { Size = UDim2.fromOffset(10, 10), Position = UDim2.fromScale(0, 1), AnchorPoint = Vector2.new(0, 1) })
			New("Frame", S, `{Name}_2`, { Size = UDim2.fromOffset(10, 150) })
		else
			New("UIListLayout", S, nil, { Padding = Opts.pad or UDim.new(0.1, 1), SortOrder = Enum.SortOrder.LayoutOrder, FillDirection = if Y then Enum.FillDirection.Vertical else Enum.FillDirection.Horizontal })
			for Index = 1, 3 do
				New("Frame", S, `{Name}_{Index}`, { Size = if Y then UDim2.fromOffset(20 + (if Opts.axis == "XY" then 180 else 0), 60) else UDim2.fromOffset(60, 20), LayoutOrder = Index })
			end
		end
	end
	for _, Padded in { false, true } do
		local P = if Padded then "_p" else "_n"
		for _, Border in { 0, 3 } do
			local B = `_b{Border}`
			Case("item" .. B .. P, { padded = Padded, border = Border, kind = "item", thickness = 10, canvas = UDim2.fromOffset(300, 0), hinset = Enum.ScrollBarInset.ScrollBar })
			Case("anchor" .. B .. P, { padded = Padded, border = Border, kind = "anchor", thickness = 10, canvas = UDim2.fromOffset(300, 0), hinset = Enum.ScrollBarInset.ScrollBar })
			Case("canvas250" .. B .. P, { padded = Padded, border = Border, thickness = 10, canvas = UDim2.fromOffset(300, 250), hinset = Enum.ScrollBarInset.ScrollBar })
			Case("canvas195" .. B .. P, { padded = Padded, border = Border, thickness = 10, canvas = UDim2.fromOffset(300, 195), hinset = Enum.ScrollBarInset.ScrollBar })
			Case("xy" .. B .. P, { padded = Padded, border = Border, axis = "XY", thickness = 10, hinset = Enum.ScrollBarInset.ScrollBar, vinset = Enum.ScrollBarInset.ScrollBar })
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
