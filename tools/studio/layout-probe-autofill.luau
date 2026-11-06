-- AutomaticSize frames with scale-sized children, as list items and in plain parents (the hover
-- surfaces of a hover preview grow to their list container's size in Studio).
return function(Target)
	local Results = {}
	local Nodes = {}
	local Count = 0
	local function Holder(List)
		Count += 1
		local H = Instance.new("Frame")
		H.Position = UDim2.fromOffset(((Count - 1) % 6) * 200, ((Count - 1) // 6) * 170)
		H.Size = UDim2.fromOffset(190, 160)
		H.BackgroundTransparency = 1
		H.Parent = Target
		if List then
			local Layout = Instance.new("UIListLayout")
			for Key, Value in List do
				(Layout :: any)[Key] = Value
			end
			Layout.Parent = H
		end
		return H
	end
	local function Frame(Parent, Key, Props)
		local F = Instance.new("Frame")
		for K, V in Props do
			(F :: any)[K] = V
		end
		F.Parent = Parent
		if Key then
			Nodes[Key] = F
		end
		return F
	end
	local function Auto(Parent, Key, Axes, Size)
		return Frame(Parent, Key, { AutomaticSize = Enum.AutomaticSize[Axes or "XY"], Size = Size or UDim2.fromOffset(0, 0) })
	end
	local Vertical = { SortOrder = Enum.SortOrder.LayoutOrder }
	local Centered = { SortOrder = Enum.SortOrder.LayoutOrder, HorizontalAlignment = Enum.HorizontalAlignment.Center, VerticalAlignment = Enum.VerticalAlignment.Center, Padding = UDim.new(0, 6) }
	local Horizontal = { SortOrder = Enum.SortOrder.LayoutOrder, FillDirection = Enum.FillDirection.Horizontal }

	-- name, holder list, auto axes, base size, children
	local Cases = {
		{ "plain", nil, "XY", nil, { "fill", "content" } },
		{ "plainBase", nil, "XY", UDim2.fromOffset(30, 20), { "fill", "content" } },
		{ "list", Vertical, "XY", nil, { "fill", "content" } },
		{ "listCentered", Centered, "XY", nil, { "fill", "content" } },
		{ "listContentOnly", Vertical, "XY", nil, { "content" } },
		{ "listFillOnly", Vertical, "XY", nil, { "fill" } },
		{ "listAutoX", Vertical, "X", UDim2.fromOffset(0, 40), { "fill", "content" } },
		{ "listAutoY", Vertical, "Y", UDim2.fromOffset(80, 0), { "fill", "content" } },
		{ "listHalf", Vertical, "XY", nil, { "half", "content" } },
		{ "listHorizontal", Horizontal, "XY", nil, { "fill", "content" } },
		{ "listBase", Vertical, "XY", UDim2.fromOffset(30, 20), { "fill", "content" } },
		{ "listScaleBase", Vertical, "XY", UDim2.fromScale(0.2, 0.1), { "fill", "content" } },
		{ "listUIScale", Vertical, "XY", nil, { "uiscale", "fill", "content" } },
		{ "plainInnerList", nil, "XY", nil, { "innerlist" } },
		{ "listInnerList", Vertical, "XY", nil, { "innerlist" } },
		{ "listSurface", Centered, "XY", nil, { "surface" } },
		{ "listTwo", Centered, "XY", nil, { "fill", "content" }, true },
		{ "listNested", Vertical, "XY", nil, { "nested" } },
	}
	for _, Case in Cases do
		local Name = Case[1]
		local H = Holder(Case[2])
		local F = Auto(H, `{Name}_F`, Case[3], Case[4])
		for _, Child in Case[5] do
			if Child == "fill" then
				Frame(F, `{Name}_fill`, { Size = UDim2.fromScale(1, 1) })
			elseif Child == "half" then
				Frame(F, `{Name}_half`, { Size = UDim2.fromScale(0.5, 0.5) })
			elseif Child == "content" then
				Frame(F, `{Name}_content`, { Size = UDim2.fromOffset(60, 20), Position = UDim2.fromOffset(5, 5) })
			elseif Child == "uiscale" then
				local Scale = Instance.new("UIScale")
				Scale.Scale = 1
				Scale.Parent = F
			elseif Child == "innerlist" then
				local Layout = Instance.new("UIListLayout")
				Layout.SortOrder = Enum.SortOrder.LayoutOrder
				Layout.Parent = F
				Frame(F, `{Name}_divider`, { Size = UDim2.new(1, 0, 0, 2), LayoutOrder = 2 })
				Frame(F, `{Name}_item`, { Size = UDim2.fromOffset(60, 20), LayoutOrder = 1 })
			elseif Child == "surface" then
				-- HoverSurface: Chrome (scale 1, 1) + Main (automatic, list + padding + divider + label)
				Frame(F, `{Name}_chrome`, { Size = UDim2.fromScale(1, 1) })
				local Main = Frame(F, `{Name}_main`, { AutomaticSize = Enum.AutomaticSize.XY, Size = UDim2.fromScale(0, 0) })
				local Layout = Instance.new("UIListLayout")
				Layout.SortOrder = Enum.SortOrder.LayoutOrder
				Layout.Padding = UDim.new(0, 4)
				Layout.Parent = Main
				local Padding = Instance.new("UIPadding")
				Padding.PaddingLeft = UDim.new(0, 8)
				Padding.PaddingRight = UDim.new(0, 8)
				Padding.PaddingTop = UDim.new(0, 8)
				Padding.PaddingBottom = UDim.new(0, 8)
				Padding.Parent = Main
				Frame(Main, `{Name}_label`, { Size = UDim2.fromOffset(70, 14), LayoutOrder = 1 })
				Frame(Main, `{Name}_divider`, { Size = UDim2.new(1, 0, 0, 1), LayoutOrder = 2 })
			elseif Child == "nested" then
				local G = Auto(F, `{Name}_G`, "XY", nil)
				Frame(G, `{Name}_fill`, { Size = UDim2.fromScale(1, 1) })
				Frame(G, `{Name}_content`, { Size = UDim2.fromOffset(60, 20) })
			end
		end
		if Case[6] then
			local Second = Auto(H, `{Name}_F2`, Case[3], Case[4])
			Frame(Second, `{Name}_fill2`, { Size = UDim2.fromScale(1, 1) })
			Frame(Second, `{Name}_content2`, { Size = UDim2.fromOffset(40, 10) })
		end
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
