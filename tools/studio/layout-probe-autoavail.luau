-- The size an AutomaticSize XY frame's grandchildren resolve their scale sizes against (the frame
-- fills it through Mid (1, 0, 0, 20) -> Leaf (1, 0, 1, 0)): AnchorPoint x Position in a 1200 x 900
-- parent (layout-probe-autodeep3: anchor 0.5 gave 600 at x = 0 and x = 300, anchor 0 gave 1200 - x).
return function(Target)
	local Results = {}
	local Nodes = {}
	for AnchorIndex, Anchor in { 0, 0.25, 0.5, 0.75, 1 } do
		for PositionIndex, Position in { 0, 300, 600, 900 } do
			local Holder = Instance.new("Frame")
			Holder.Size = UDim2.fromOffset(1200, 900)
			Holder.BackgroundTransparency = 1
			Holder.Parent = Target
			local F = Instance.new("Frame")
			F.AutomaticSize = Enum.AutomaticSize.XY
			F.Size = UDim2.fromOffset(0, 0)
			F.AnchorPoint = Vector2.new(Anchor, Anchor)
			F.Position = UDim2.fromOffset(Position, Position * 0.75)
			F.BackgroundTransparency = 1
			F.Parent = Holder
			local Mid = Instance.new("Frame")
			Mid.Size = UDim2.new(1, 0, 1, 0)
			Mid.BackgroundTransparency = 1
			Mid.Parent = F
			local Leaf = Instance.new("Frame")
			Leaf.Size = UDim2.fromScale(1, 1)
			Leaf.Parent = Mid
			Nodes[`a{AnchorIndex}_p{PositionIndex}`] = F
		end
	end
	for _ = 1, 3 do
		task.wait()
	end
	for Key, Node in Nodes do
		local Parent = Node.Parent
		Results[Key] = { Node.AbsolutePosition.X - Parent.AbsolutePosition.X, Node.AbsolutePosition.Y - Parent.AbsolutePosition.Y, Node.AbsoluteSize.X, Node.AbsoluteSize.Y }
	end
	return Results
end
