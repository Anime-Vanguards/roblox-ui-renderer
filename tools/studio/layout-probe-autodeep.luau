-- When do grandchildren extend an AutomaticSize XY frame (layout-probe-autonested: F (0, 0) ->
-- Chrome (1, 1) -> (1, 1) fills the holder) but not a slider prompt's Holder (500 x 158, aspect 3.5,
-- Shadow (1, 0, 0, 267) -> image (1.5, 1.5) centred)? Varies base size, aspect, the intermediate's size
-- and the grandchild's size / anchor.
return function(Target)
	local Results = {}
	local Nodes = {}
	local Count = 0
	local function Case(Name, Base, Aspect, MidSize, LeafSize, Centred)
		Count += 1
		local Holder = Instance.new("Frame")
		Holder.Position = UDim2.fromOffset(((Count - 1) % 4) * 300, ((Count - 1) // 4) * 230)
		Holder.Size = UDim2.fromOffset(290, 220)
		Holder.BackgroundTransparency = 1
		Holder.Parent = Target
		local F = Instance.new("Frame")
		F.AutomaticSize = Enum.AutomaticSize.XY
		F.Size = Base
		F.BackgroundTransparency = 1
		F.Parent = Holder
		if Aspect then
			local C = Instance.new("UIAspectRatioConstraint")
			C.AspectRatio = Aspect
			C.Parent = F
		end
		local Mid = Instance.new("Frame")
		Mid.Size = MidSize
		Mid.BackgroundTransparency = 1
		Mid.Parent = F
		local Leaf = Instance.new("Frame")
		Leaf.Size = LeafSize
		if Centred then
			Leaf.AnchorPoint = Vector2.new(0.5, 0.5)
			Leaf.Position = UDim2.fromScale(0.5, 0.5)
		end
		Leaf.Parent = Mid
		Nodes[Name] = F
		Nodes[Name .. "_mid"] = Mid
	end
	local Zero, Base = UDim2.fromOffset(0, 0), UDim2.fromOffset(100, 40)
	local Fill, Wide = UDim2.fromScale(1, 1), UDim2.new(1, 0, 0, 90)
	local Big, Small = UDim2.fromScale(1.5, 1.5), UDim2.fromScale(0.5, 0.5)
	Case("z_fill_fill", Zero, nil, Fill, Fill, false)
	Case("z_wide_fill", Zero, nil, Wide, Fill, false)
	Case("z_wide_big_c", Zero, nil, Wide, Big, true)
	Case("z_fill_big_c", Zero, nil, Fill, Big, true)
	Case("b_fill_fill", Base, nil, Fill, Fill, false)
	Case("b_wide_fill", Base, nil, Wide, Fill, false)
	Case("b_wide_big_c", Base, nil, Wide, Big, true)
	Case("b_fill_big_c", Base, nil, Fill, Big, true)
	Case("ba_fill_fill", Base, 2.5, Fill, Fill, false)
	Case("ba_wide_big_c", Base, 2.5, Wide, Big, true)
	Case("za_fill_fill", Zero, 2.5, Fill, Fill, false)
	Case("za_wide_big_c", Zero, 2.5, Wide, Big, true)
	Case("b_fill_small", Base, nil, Fill, Small, false)
	Case("z_fill_small", Zero, nil, Fill, Small, false)
	Case("b_wide_small_c", Base, nil, Wide, Small, true)
	Case("z_wide_small_c", Zero, nil, Wide, Small, true)
	for _ = 1, 3 do
		task.wait()
	end
	for Key, Node in Nodes do
		local Parent = Node.Parent
		Results[Key] = { Node.AbsolutePosition.X - Parent.AbsolutePosition.X, Node.AbsolutePosition.Y - Parent.AbsolutePosition.Y, Node.AbsoluteSize.X, Node.AbsoluteSize.Y }
	end
	return Results
end
