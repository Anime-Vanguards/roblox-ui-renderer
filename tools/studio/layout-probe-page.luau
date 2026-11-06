-- UIPageLayout and UITableLayout basics: page positions (direction, padding, alignment, page sizes,
-- JumpTo), table cell positions (major axis, padding, fill empty space, alignments).
return function(Target)
	local Results = {}
	local Nodes = {}
	local Count = 0
	local function Holder(Name)
		Count += 1
		local H = Instance.new("Frame")
		H.Name = Name
		H.Position = UDim2.fromOffset(((Count - 1) % 5) * 235, ((Count - 1) // 5) * 190)
		H.Size = UDim2.fromOffset(200, 150)
		H.BackgroundTransparency = 1
		H.Parent = Target
		return H
	end
	local function Page(Parent, Name, Size, Order)
		local F = Instance.new("Frame")
		F.Name = Name
		F.Size = Size
		F.LayoutOrder = Order
		F.Parent = Parent
		Nodes[`{Parent.Name}/{Name}`] = F
		return F
	end

	for _, Spec in {
		{ "PageH", {} },
		{ "PageV", { FillDirection = Enum.FillDirection.Vertical } },
		{ "PagePad", { Padding = UDim.new(0, 10) } },
		{ "PagePadScale", { Padding = UDim.new(0.1, 5) } },
		{ "PageAlign", { HorizontalAlignment = Enum.HorizontalAlignment.Right, VerticalAlignment = Enum.VerticalAlignment.Bottom } },
		{ "PageSmall", { HorizontalAlignment = Enum.HorizontalAlignment.Center, VerticalAlignment = Enum.VerticalAlignment.Center } },
		{ "PageCircular", { Circular = true } },
	} do
		local H = Holder(Spec[1])
		local L = Instance.new("UIPageLayout")
		L.Animated = false
		L.SortOrder = Enum.SortOrder.LayoutOrder
		for Key, Value in Spec[2] do
			(L :: any)[Key] = Value
		end
		L.Parent = H
		local Size = if Spec[1] == "PageSmall" or Spec[1] == "PageAlign" then UDim2.fromOffset(120, 80) else UDim2.fromScale(1, 1)
		Page(H, "A", Size, 1)
		Page(H, "B", Size, 2)
		Page(H, "C", Size, 3)
		Nodes[`{Spec[1]}/Layout`] = L
	end
	do
		local H = Holder("PageJump")
		local L = Instance.new("UIPageLayout")
		L.Animated = false
		L.SortOrder = Enum.SortOrder.LayoutOrder
		L.Parent = H
		Page(H, "A", UDim2.fromScale(1, 1), 1)
		local B = Page(H, "B", UDim2.fromScale(1, 1), 2)
		Page(H, "C", UDim2.fromScale(1, 1), 3)
		L:JumpTo(B)
		Nodes["PageJump/Layout"] = L
	end

	-- UITableLayout
	local function Table(Name, Props, Rows)
		local H = Holder(Name)
		local T = Instance.new("UITableLayout")
		T.SortOrder = Enum.SortOrder.LayoutOrder
		for Key, Value in Props do
			(T :: any)[Key] = Value
		end
		T.Parent = H
		for RowIndex, Row in Rows do
			local R = Instance.new("Frame")
			R.Name = `R{RowIndex}`
			R.LayoutOrder = RowIndex
			R.Size = UDim2.fromOffset(10, 10)
			R.Parent = H
			Nodes[`{Name}/R{RowIndex}`] = R
			for CellIndex, CellSize in Row do
				local C = Instance.new("Frame")
				C.Name = `C{CellIndex}`
				C.LayoutOrder = CellIndex
				C.Size = UDim2.fromOffset(CellSize[1], CellSize[2])
				C.Parent = R
				Nodes[`{Name}/R{RowIndex}/C{CellIndex}`] = C
			end
		end
		Nodes[`{Name}/Layout`] = T
	end
	local Rows = { { { 30, 20 }, { 50, 15 }, { 20, 25 } }, { { 40, 10 }, { 25, 30 } }, { { 35, 12 }, { 30, 18 }, { 45, 22 } } }
	Table("Table", {}, Rows)
	Table("TablePad", { Padding = UDim2.fromOffset(4, 6) }, Rows)
	Table("TableColumn", { MajorAxis = Enum.TableMajorAxis.ColumnMajor }, Rows)
	Table("TableFill", { FillEmptySpaceColumns = true, FillEmptySpaceRows = true }, Rows)
	Table("TableAlign", { HorizontalAlignment = Enum.HorizontalAlignment.Center, VerticalAlignment = Enum.VerticalAlignment.Center }, Rows)

	task.wait()
	for Key, Node in Nodes do
		if Node:IsA("UIGridStyleLayout") then
			Results[Key] = { Node.AbsoluteContentSize.X, Node.AbsoluteContentSize.Y }
		else
			local Root = Node
			while Root.Parent ~= Target do
				Root = Root.Parent
			end
			local Origin = Root.AbsolutePosition
			Results[Key] = { Node.AbsolutePosition.X - Origin.X, Node.AbsolutePosition.Y - Origin.Y, Node.AbsoluteSize.X, Node.AbsoluteSize.Y }
		end
	end
	return Results
end
