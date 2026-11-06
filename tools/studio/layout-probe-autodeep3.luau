-- Why did layout-probe-autoaspect2's Shadow leaf (1.5 x 1.5, centred) not extend its AutoXY holder
-- (anchor 0.5 at (300, 220) in 1200 x 900, base 500 x 158, aspect 3.5) while autodeep's did? Varies
-- the anchor, the aspect, the base and the position.
return function(Target)
	local Results = {}
	local Nodes = {}
	local Count = 0
	local function Case(Name, Anchor, Aspect, Base, Position)
		Count += 1
		local Holder = Instance.new("Frame")
		Holder.Position = UDim2.fromOffset(((Count - 1) % 2) * 600, ((Count - 1) // 2) * 450)
		Holder.Size = UDim2.fromOffset(1200, 900)
		Holder.BackgroundTransparency = 1
		Holder.Parent = Target
		local F = Instance.new("Frame")
		F.AutomaticSize = Enum.AutomaticSize.XY
		F.Size = Base
		F.AnchorPoint = Anchor
		F.Position = Position
		F.BackgroundTransparency = 1
		F.Parent = Holder
		if Aspect then
			local C = Instance.new("UIAspectRatioConstraint")
			C.AspectRatio = Aspect
			C.Parent = F
		end
		local Mid = Instance.new("Frame")
		Mid.Size = UDim2.new(1, 0, 0, 267)
		Mid.BackgroundTransparency = 1
		Mid.Parent = F
		local Leaf = Instance.new("Frame")
		Leaf.Size = UDim2.fromScale(1.5, 1.5)
		Leaf.AnchorPoint = Vector2.new(0.5, 0.5)
		Leaf.Position = UDim2.fromScale(0.5, 0.5)
		Leaf.Parent = Mid
		Nodes[Name] = F
	end
	local Half, Zero = Vector2.new(0.5, 0.5), Vector2.zero
	local Big, Small = UDim2.fromOffset(500, 158), UDim2.fromOffset(100, 40)
	local At = UDim2.fromOffset(300, 220)
	Case("half_aspect_big", Half, 3.5, Big, At)
	Case("half_noaspect_big", Half, nil, Big, At)
	Case("zero_aspect_big", Zero, 3.5, Big, At)
	Case("zero_noaspect_big", Zero, nil, Big, At)
	Case("half_aspect_small", Half, 3.5, Small, At)
	Case("half_noaspect_small", Half, nil, Small, At)
	Case("zero_noaspect_small", Zero, nil, Small, At)
	Case("half_noaspect_small_origin", Half, nil, Small, UDim2.fromOffset(0, 0))
	for _ = 1, 3 do
		task.wait()
	end
	for Key, Node in Nodes do
		local Parent = Node.Parent
		Results[Key] = { Node.AbsolutePosition.X - Parent.AbsolutePosition.X, Node.AbsolutePosition.Y - Parent.AbsolutePosition.Y, Node.AbsoluteSize.X, Node.AbsoluteSize.Y }
	end
	return Results
end
