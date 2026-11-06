-- A scale-sized child with a UIListLayout inside a plain AutomaticSize frame (a slider prompt: Holder
-- AutoXY <- Main (1, 0, 1, 0) AutoY with a list whose widest item is 523 + padding 10 -> 533 wide).
-- Varies the child's AutomaticSize, Size, padding, list direction / alignment and the items.
return function(Target)
	local Results = {}
	local Nodes = {}
	local Count = 0
	local function Case(Name, Auto, ChildAuto, ChildSize, Padding, Horizontal, Align, ItemSizes)
		Count += 1
		local Holder = Instance.new("Frame")
		Holder.Position = UDim2.fromOffset(((Count - 1) % 4) * 300, ((Count - 1) // 4) * 150)
		Holder.Size = UDim2.fromOffset(290, 140)
		Holder.BackgroundTransparency = 1
		Holder.Parent = Target
		local A = Instance.new("Frame")
		A.Size = UDim2.fromOffset(0, 20)
		A.AutomaticSize = Enum.AutomaticSize[Auto]
		A.BackgroundTransparency = 1
		A.Parent = Holder
		local M = Instance.new("Frame")
		M.Size = ChildSize
		M.AutomaticSize = Enum.AutomaticSize[ChildAuto]
		M.BackgroundTransparency = 1
		M.Parent = A
		local List = Instance.new("UIListLayout")
		List.FillDirection = if Horizontal then Enum.FillDirection.Horizontal else Enum.FillDirection.Vertical
		List.HorizontalAlignment = Enum.HorizontalAlignment[Align]
		List.Padding = UDim.new(0, 4)
		List.Parent = M
		if Padding then
			local P = Instance.new("UIPadding")
			P.PaddingLeft = UDim.new(0, 5)
			P.PaddingRight = UDim.new(0, 7)
			P.PaddingTop = UDim.new(0, 3)
			P.PaddingBottom = UDim.new(0, 2)
			P.Parent = M
		end
		for Index, Size in ItemSizes do
			local Item = Instance.new("Frame")
			Item.Size = Size
			Item.LayoutOrder = Index
			Item.Parent = M
			Nodes[`{Name}_item{Index}`] = Item
		end
		Nodes[Name] = A
		Nodes[Name .. "_child"] = M
	end
	local Items = { UDim2.fromOffset(123, 20), UDim2.fromOffset(80, 20) }
	local Fill, Half = UDim2.fromScale(1, 1), UDim2.new(0.5, 0, 1, 0)
	Case("xy_y_pad", "XY", "Y", Fill, true, false, "Center", Items)
	Case("xy_none_pad", "XY", "None", Fill, true, false, "Center", Items)
	Case("xy_x_pad", "XY", "X", Fill, true, false, "Center", Items)
	Case("xy_y_nopad", "XY", "Y", Fill, false, false, "Center", Items)
	Case("xy_y_left", "XY", "Y", Fill, true, false, "Left", Items)
	Case("xy_y_right", "XY", "Y", Fill, true, false, "Right", Items)
	Case("xy_y_horiz", "XY", "Y", Fill, true, true, "Left", Items)
	Case("x_y_pad", "X", "Y", Fill, true, false, "Center", Items)
	Case("x_none_pad", "X", "None", Fill, true, false, "Center", Items)
	Case("xy_y_half", "XY", "Y", Half, true, false, "Center", Items)
	Case("xy_y_minus", "XY", "Y", UDim2.new(1, -20, 1, 0), true, false, "Center", Items)
	Case("xy_y_scaleitem", "XY", "Y", Fill, true, false, "Center", { UDim2.fromOffset(123, 20), UDim2.new(0.5, 0, 0, 20) })
	Case("y_none_pad", "Y", "None", Fill, true, false, "Center", Items)
	Case("y_x_pad", "Y", "X", Fill, true, false, "Center", Items)
	Case("xy_none_nolist", "XY", "None", Fill, true, false, "Center", {})
	for _ = 1, 3 do
		task.wait()
	end
	for Key, Node in Nodes do
		local Parent = Node.Parent
		Results[Key] = { Node.AbsolutePosition.X - Parent.AbsolutePosition.X, Node.AbsolutePosition.Y - Parent.AbsolutePosition.Y, Node.AbsoluteSize.X, Node.AbsoluteSize.Y }
	end
	return Results
end
