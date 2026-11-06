-- AutomaticCanvasSize list Padding basis (round 9): what the gap's scale resolves against when the
-- frame's rect is not its Size UDim (UISizeConstraint, UIAspectRatioConstraint), under a UIScale, and
-- with the other axis' bar visible and inset.
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
			BorderSizePixel = 0,
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
		local Y = (Opts.axis or "Y") == "Y"
		New("UIListLayout", S, nil, { Padding = UDim.new(0.1, 1), SortOrder = Enum.SortOrder.LayoutOrder, FillDirection = if Y then Enum.FillDirection.Vertical else Enum.FillDirection.Horizontal })
		for Index = 1, 3 do
			New("Frame", S, `{Name}_{Index}`, { Size = if Y then UDim2.fromOffset(20, 60) else UDim2.fromOffset(60, 20), LayoutOrder = Index })
		end
	end
	for _, Padded in { false, true } do
		local P = if Padded then "_p" else "_n"
		Case("maxsize" .. P, { padded = Padded, maxsize = Vector2.new(1000, 70) })
		Case("minsize" .. P, { padded = Padded, size = UDim2.new(0.9, 0, 0.4, 2), maxsize = Vector2.new(1000, 1000), })
		Case("aspect" .. P, { padded = Padded, aspect = 1.5 })
		Case("uiscale" .. P, { padded = Padded, uiscale = 1.25 })
		Case("uiscale2" .. P, { padded = Padded, uiscale = 0.8, size = UDim2.new(0.8, 3, 0.6, 7) })
		Case("hbarinset" .. P, { padded = Padded, thickness = 10, canvas = UDim2.fromOffset(300, 0), hinset = Enum.ScrollBarInset.ScrollBar })
		Case("hbarnone" .. P, { padded = Padded, thickness = 10, canvas = UDim2.fromOffset(300, 0) })
		Case("xvbarinset" .. P, { padded = Padded, axis = "X", thickness = 10, canvas = UDim2.fromOffset(0, 300), vinset = Enum.ScrollBarInset.ScrollBar })
		Case("xvbarnone" .. P, { padded = Padded, axis = "X", thickness = 10, canvas = UDim2.fromOffset(0, 300) })
		Case("canvasscale" .. P, { padded = Padded, canvas = UDim2.fromScale(0, 0.5) })
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
