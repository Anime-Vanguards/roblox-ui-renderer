-- AutomaticSize frames whose UIListLayout items have scale sizes: which size the scale resolves
-- against (layout-probe-autofill: a scale-width item makes the frame as wide as its parent).
return function(Target)
	local Results = {}
	local Nodes = {}
	local Count = 0
	local function Holder()
		Count += 1
		local H = Instance.new("Frame")
		H.Position = UDim2.fromOffset(((Count - 1) % 6) * 200, ((Count - 1) // 6) * 170)
		H.Size = UDim2.fromOffset(190, 160)
		H.BackgroundTransparency = 1
		H.Parent = Target
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
	local function List(Parent, Props)
		local Layout = Instance.new("UIListLayout")
		Layout.SortOrder = Enum.SortOrder.LayoutOrder
		for Key, Value in Props or {} do
			(Layout :: any)[Key] = Value
		end
		Layout.Parent = Parent
		return Layout
	end
	local function Auto(Parent, Key, Axes, Size, Position)
		return Frame(Parent, Key, { AutomaticSize = Enum.AutomaticSize[Axes], Size = Size or UDim2.fromOffset(0, 0), Position = Position or UDim2.new() })
	end
	local function Items(F, Name, ScaleSize)
		Frame(F, `{Name}_item`, { Size = UDim2.fromOffset(60, 20), LayoutOrder = 1 })
		Frame(F, `{Name}_scaled`, { Size = ScaleSize, LayoutOrder = 2 })
	end

	do
		local F = Auto(Holder(), "tall_F", "XY")
		List(F)
		Items(F, "tall", UDim2.new(0, 30, 1, 0))
	end
	do
		local F = Auto(Holder(), "both_F", "XY")
		List(F)
		Items(F, "both", UDim2.fromScale(1, 1))
	end
	do
		local F = Auto(Holder(), "half_F", "XY")
		List(F)
		Items(F, "half", UDim2.new(0.5, 0, 0, 2))
	end
	do
		local F = Auto(Holder(), "pos_F", "XY", nil, UDim2.fromOffset(30, 10))
		List(F)
		Items(F, "pos", UDim2.new(1, 0, 0, 2))
	end
	do
		local F = Auto(Holder(), "posScale_F", "XY", nil, UDim2.new(0.2, 0, 0, 0))
		List(F)
		Items(F, "posScale", UDim2.new(1, 0, 0, 2))
	end
	do
		local F = Auto(Holder(), "pad_F", "XY")
		List(F)
		local Padding = Instance.new("UIPadding")
		Padding.PaddingLeft = UDim.new(0, 10)
		Padding.PaddingRight = UDim.new(0, 6)
		Padding.PaddingTop = UDim.new(0, 4)
		Padding.Parent = F
		Items(F, "pad", UDim2.new(1, 0, 0, 2))
	end
	do
		local F = Auto(Holder(), "autoX_F", "X", UDim2.fromOffset(0, 50))
		List(F)
		Items(F, "autoX", UDim2.new(1, 0, 0, 2))
	end
	do
		local F = Auto(Holder(), "autoY_F", "Y", UDim2.fromOffset(80, 0))
		List(F)
		Items(F, "autoY", UDim2.new(1, 0, 0, 2))
	end
	do
		local F = Auto(Holder(), "horizontal_F", "XY")
		List(F, { FillDirection = Enum.FillDirection.Horizontal })
		Items(F, "horizontal", UDim2.new(1, 0, 0, 10))
	end
	do
		local F = Auto(Holder(), "horizontalHalf_F", "XY")
		List(F, { FillDirection = Enum.FillDirection.Horizontal })
		Items(F, "horizontalHalf", UDim2.new(0.25, 0, 0, 10))
	end
	do
		local F = Auto(Holder(), "base_F", "XY", UDim2.fromOffset(100, 0))
		List(F)
		Items(F, "base", UDim2.new(1, 0, 0, 2))
	end
	do
		local F = Auto(Holder(), "offset_F", "XY")
		List(F)
		Items(F, "offset", UDim2.new(1, -20, 0, 2))
	end
	do
		local G = Frame(Holder(), "fixed_G", { Size = UDim2.fromOffset(120, 100) })
		local F = Auto(G, "fixed_F", "XY")
		List(F)
		Items(F, "fixed", UDim2.new(1, 0, 0, 2))
	end
	do
		local P = Auto(Holder(), "autoParent_P", "XY")
		local F = Auto(P, "autoParent_F", "XY")
		List(F)
		Items(F, "autoParent", UDim2.new(1, 0, 0, 2))
	end
	do
		local F = Auto(Holder(), "nested_F", "XY")
		List(F)
		local G = Auto(F, "nested_G", "XY")
		G.LayoutOrder = 3
		List(G)
		Items(G, "nested", UDim2.new(1, 0, 0, 2))
		Frame(F, "nested_item", { Size = UDim2.fromOffset(40, 10), LayoutOrder = 1 })
	end
	do
		local F = Auto(Holder(), "text_F", "XY")
		List(F)
		local Label = Instance.new("TextLabel")
		Label.Size = UDim2.new(1, 0, 0, 0)
		Label.AutomaticSize = Enum.AutomaticSize.Y
		Label.TextWrapped = true
		Label.TextSize = 14
		Label.FontFace = Font.new("rbxasset://fonts/families/SourceSansPro.json")
		Label.Text = "a label long enough to wrap inside the available width of the holder frame"
		Label.Parent = F
		Nodes["text_label"] = Label
	end
	do
		local F = Auto(Holder(), "grid_F", "XY")
		local Grid = Instance.new("UIGridLayout")
		Grid.CellSize = UDim2.new(0.5, 0, 0, 20)
		Grid.Parent = F
		Frame(F, "grid_a", { Size = UDim2.fromOffset(10, 10) })
		Frame(F, "grid_b", { Size = UDim2.fromOffset(10, 10) })
		Frame(F, "grid_c", { Size = UDim2.fromOffset(10, 10) })
	end
	do
		local F = Auto(Holder(), "flex_F", "XY")
		List(F, { FillDirection = Enum.FillDirection.Horizontal, HorizontalFlex = Enum.UIFlexAlignment.Fill })
		Frame(F, "flex_a", { Size = UDim2.fromOffset(30, 10) })
		Frame(F, "flex_b", { Size = UDim2.fromOffset(20, 10) })
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
		end
		Results[Key] = Entry
	end
	return Results
end
