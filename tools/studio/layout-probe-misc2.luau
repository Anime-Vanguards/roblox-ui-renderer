-- Border insets with AutomaticSize / text, the ScrollingFrame need boundary with a border, the
-- size basis inside AutomaticCanvasSize ScrollingFrames (CanvasSize default {0,0},{2,0}), and where
-- invisible UIListLayout / UIGridLayout children sit.
return function(Target)
	local Results = {}
	local Nodes = {}
	local Count = 0
	local function Holder(W, H)
		Count += 1
		local F = Instance.new("Frame")
		F.Position = UDim2.fromOffset(((Count - 1) % 6) * 200, ((Count - 1) // 6) * 230)
		F.Size = UDim2.fromOffset(W or 190, H or 220)
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
	local SSP = Font.new("rbxasset://fonts/families/SourceSansPro.json")

	-- AutomaticSize + border inset
	for _, Mode in { "Inset", "Middle", "Outline" } do
		local A = New("Frame", Holder(), `auto_{Mode}`, { AutomaticSize = Enum.AutomaticSize.XY, Size = UDim2.new(), BorderSizePixel = 3, BorderMode = Enum.BorderMode[Mode] })
		New("Frame", A, `auto_{Mode}_child`, { Size = UDim2.fromOffset(50, 20) })
	end
	-- text inside a border inset: wrap width and alignment
	for _, Mode in { "Inset", "Outline" } do
		New("TextLabel", Holder(), `text_{Mode}`, {
			Size = UDim2.fromOffset(100, 60),
			BorderSizePixel = 10,
			BorderMode = Enum.BorderMode[Mode],
			FontFace = SSP,
			TextSize = 16,
			TextWrapped = true,
			Text = "wrap this text inside the border",
		})
		New("TextLabel", Holder(), `textauto_{Mode}`, {
			Size = UDim2.new(),
			AutomaticSize = Enum.AutomaticSize.XY,
			BorderSizePixel = 10,
			BorderMode = Enum.BorderMode[Mode],
			FontFace = SSP,
			TextSize = 16,
			Text = "auto",
		})
	end
	-- primary bar need with a border: canvas height vs frame height 80
	for _, Border in { 1, 3 } do
		for _, Mode in { "Outline", "Inset" } do
			for _, Height in { 74, 77, 79, 80, 81 } do
				local S = New("ScrollingFrame", Holder(120, 100), `need_b{Border}_{Mode}_h{Height}`, {
					Size = UDim2.fromOffset(100, 80),
					BorderSizePixel = Border,
					BorderMode = Enum.BorderMode[Mode],
					CanvasSize = UDim2.fromOffset(60, Height),
					ScrollBarThickness = 8,
				})
				local _ = S
			end
		end
	end
	-- size basis inside AutomaticCanvasSize scrolling frames
	for _, Case in { { "default", nil }, { "zero", UDim2.new() }, { "big", UDim2.fromOffset(0, 1000) } } do
		local H = Holder(190, 220)
		local Props = {
			Size = UDim2.fromOffset(160, 120),
			AutomaticCanvasSize = Enum.AutomaticSize.Y,
			ScrollBarThickness = 0,
			BorderSizePixel = 0,
		}
		if Case[2] then
			Props.CanvasSize = Case[2]
		end
		local S = New("ScrollingFrame", H, `basis_{Case[1]}`, Props)
		New("UIListLayout", S, nil, { Padding = UDim.new(0.1, 0), SortOrder = Enum.SortOrder.LayoutOrder })
		New("Frame", S, `basis_{Case[1]}_a`, { Size = UDim2.new(0.5, 0, 0.1, 0), LayoutOrder = 1 })
		New("Frame", S, `basis_{Case[1]}_b`, { Size = UDim2.new(0.5, 0, 0, 30), LayoutOrder = 2 })
		New("Frame", S, `basis_{Case[1]}_c`, { Size = UDim2.new(0.5, 0, 0, 30), LayoutOrder = 3 })
		local G = New("ScrollingFrame", H, `gbasis_{Case[1]}`, {
			Position = UDim2.fromOffset(0, 125),
			Size = UDim2.fromOffset(160, 90),
			AutomaticCanvasSize = Enum.AutomaticSize.Y,
			ScrollBarThickness = 0,
			BorderSizePixel = 0,
			CanvasSize = Case[2] or nil,
		})
		if not Case[2] then
			G.CanvasSize = UDim2.new(0, 0, 2, 0)
		end
		New("UIGridLayout", G, nil, { CellSize = UDim2.new(0.3, 0, 0.2, 0), CellPadding = UDim2.fromScale(0.05, 0.1), SortOrder = Enum.SortOrder.LayoutOrder })
		for Index = 1, 5 do
			New("Frame", G, `gbasis_{Case[1]}_{Index}`, { LayoutOrder = Index })
		end
	end
	-- invisible list / grid children
	for _, Align in { "Top", "Center", "Bottom" } do
		local H = Holder(190, 220)
		New("UIListLayout", H, nil, { VerticalAlignment = Enum.VerticalAlignment[Align], HorizontalAlignment = Enum.HorizontalAlignment.Center, Padding = UDim.new(0, 5), SortOrder = Enum.SortOrder.LayoutOrder })
		New("Frame", H, `hidden_{Align}_a`, { Size = UDim2.fromOffset(40, 20), LayoutOrder = 1 })
		New("Frame", H, `hidden_{Align}_b`, { Size = UDim2.fromOffset(60, 30), LayoutOrder = 2, Visible = false, Position = UDim2.fromOffset(7, 9) })
		New("Frame", H, `hidden_{Align}_c`, { Size = UDim2.fromOffset(50, 25), LayoutOrder = 3 })
		New("Frame", H, `hidden_{Align}_d`, { Size = UDim2.new(0.5, 0, 0, 12), LayoutOrder = 4, Visible = false, AnchorPoint = Vector2.new(1, 1) })
	end
	do
		local H = Holder(190, 220)
		New("UIListLayout", H, nil, { FillDirection = Enum.FillDirection.Horizontal, HorizontalAlignment = Enum.HorizontalAlignment.Right, VerticalAlignment = Enum.VerticalAlignment.Center, SortOrder = Enum.SortOrder.LayoutOrder })
		New("Frame", H, "hiddenH_a", { Size = UDim2.fromOffset(40, 20), LayoutOrder = 1 })
		New("Frame", H, "hiddenH_b", { Size = UDim2.fromOffset(30, 30), LayoutOrder = 2, Visible = false })
		New("Frame", H, "hiddenH_c", { Size = UDim2.fromOffset(20, 40), LayoutOrder = 3 })
	end
	do
		local H = Holder(190, 220)
		New("UIGridLayout", H, nil, { CellSize = UDim2.fromOffset(50, 40), HorizontalAlignment = Enum.HorizontalAlignment.Center, SortOrder = Enum.SortOrder.LayoutOrder })
		for Index = 1, 4 do
			New("Frame", H, `hiddenG_{Index}`, { LayoutOrder = Index, Visible = Index ~= 2, Position = UDim2.fromOffset(Index, Index) })
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
		if Node:IsA("TextLabel") then
			table.insert(Entry, Node.TextBounds.X)
			table.insert(Entry, Node.TextBounds.Y)
		elseif Node:IsA("ScrollingFrame") then
			table.insert(Entry, Node.AbsoluteCanvasSize.X)
			table.insert(Entry, Node.AbsoluteCanvasSize.Y)
			table.insert(Entry, Node.AbsoluteWindowSize.X)
			table.insert(Entry, Node.AbsoluteWindowSize.Y)
		end
		Results[Key] = Entry
	end
	return Results
end
