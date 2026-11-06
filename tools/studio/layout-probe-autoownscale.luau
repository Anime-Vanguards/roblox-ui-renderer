-- The space an AutomaticSize object with its own UIScale measures scale-sized content against
-- (layout-probe-autolistscale noLabel: an AutoXY list frame with UIScale 0.9 and a scale-width leaf
-- item, 570 px of space, is 513 wide: the space is taken as its local size, not divided by its
-- own scale). Own scales 0.5 / 0.9 / 1.3, list and plain containers, a wrapped AutoX label and an
-- AutoY frame, anchors 0 and 1.
return function(Target)
	local Results = {}
	local Nodes = {}
	local Count = 0
	local function Holder()
		Count += 1
		local H = Instance.new("Frame")
		H.BackgroundTransparency = 1
		H.Position = UDim2.fromOffset(((Count - 1) % 3) * 400, ((Count - 1) // 3) * 75)
		H.Size = UDim2.fromOffset(300, 70)
		H.Parent = Target
		return H
	end
	local function Scaled(Object, Scale)
		local S = Instance.new("UIScale")
		S.Scale = Scale
		S.Parent = Object
	end
	for _, Scale in { 0.5, 0.9, 1.3 } do
		for _, Anchor in { 0, 1 } do
			local Position = if Anchor == 0 then UDim2.fromOffset(40, 0) else UDim2.fromOffset(260, 0)

			-- list container with a scale-width leaf
			local ListFrame = Instance.new("Frame")
			ListFrame.AutomaticSize = Enum.AutomaticSize.XY
			ListFrame.Size = UDim2.new()
			ListFrame.AnchorPoint = Vector2.new(Anchor, 0)
			ListFrame.Position = Position
			ListFrame.Parent = Holder()
			Instance.new("UIListLayout").Parent = ListFrame
			local Leaf = Instance.new("Frame")
			Leaf.Size = UDim2.new(1, 0, 0, 10)
			Leaf.Parent = ListFrame
			local Item = Instance.new("Frame")
			Item.Size = UDim2.fromOffset(50, 10)
			Item.Parent = ListFrame
			Scaled(ListFrame, Scale)
			Nodes[`list_{Scale}_{Anchor}`] = ListFrame

			-- plain container whose child holds a scale-width grandchild (descendant rule)
			local Plain = Instance.new("Frame")
			Plain.AutomaticSize = Enum.AutomaticSize.XY
			Plain.Size = UDim2.new()
			Plain.AnchorPoint = Vector2.new(Anchor, 0)
			Plain.Position = Position
			Plain.Parent = Holder()
			local Inner = Instance.new("Frame")
			Inner.Size = UDim2.fromOffset(50, 10)
			Inner.Parent = Plain
			local Glow = Instance.new("Frame")
			Glow.Size = UDim2.new(1, 20, 1, 0)
			Glow.Parent = Inner
			Scaled(Plain, Scale)
			Nodes[`plain_{Scale}_{Anchor}`] = Plain

			-- wrapped AutoX label
			local Label = Instance.new("TextLabel")
			Label.AutomaticSize = Enum.AutomaticSize.XY
			Label.Size = UDim2.new()
			Label.AnchorPoint = Vector2.new(Anchor, 0)
			Label.Position = Position
			Label.TextWrapped = true
			Label.FontFace = Font.new("rbxasset://fonts/families/SourceSansPro.json")
			Label.TextSize = 16
			Label.Text = "a few words that wrap somewhere in the available space"
			Label.Parent = Holder()
			Scaled(Label, Scale)
			Nodes[`label_{Scale}_{Anchor}`] = Label
		end
	end
	task.wait()
	for Key, Node in Nodes do
		Results[Key] = { Node.AbsolutePosition.X - Node.Parent.AbsolutePosition.X, Node.AbsoluteSize.X, Node.AbsoluteSize.Y }
		if Node:IsA("TextLabel") then
			table.insert(Results[Key], Node.TextBounds.X)
		end
	end
	return Results
end
