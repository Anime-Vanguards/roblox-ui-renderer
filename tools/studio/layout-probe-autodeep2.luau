-- Grandchild extents of AutomaticSize XY frames (layout-probe-autodeep): Frame vs ImageLabel leaf,
-- the frame anchored at the centre vs the corner, and the leaf with a UIGradient (a slider prompt's
-- Shadow/ShadowImage did not extend its Holder).
return function(Target)
	local Results = {}
	local Nodes = {}
	local Count = 0
	local function Case(Name, LeafClass, Centred, Gradient, HolderAnchor)
		Count += 1
		local Holder = Instance.new("Frame")
		Holder.Position = UDim2.fromOffset(((Count - 1) % 4) * 300, ((Count - 1) // 4) * 230)
		Holder.Size = UDim2.fromOffset(290, 220)
		Holder.BackgroundTransparency = 1
		Holder.Parent = Target
		local F = Instance.new("Frame")
		F.AutomaticSize = Enum.AutomaticSize.XY
		F.Size = UDim2.fromOffset(100, 40)
		if HolderAnchor then
			F.AnchorPoint = Vector2.new(0.5, 0.5)
			F.Position = UDim2.fromScale(0.5, 0.5)
		end
		F.BackgroundTransparency = 1
		F.Parent = Holder
		local Mid = Instance.new("Frame")
		Mid.Size = UDim2.new(1, 0, 0, 90)
		Mid.BackgroundTransparency = 1
		Mid.Parent = F
		local Leaf = Instance.new(LeafClass)
		Leaf.Size = UDim2.fromScale(1.5, 1.5)
		if Centred then
			Leaf.AnchorPoint = Vector2.new(0.5, 0.5)
			Leaf.Position = UDim2.fromScale(0.5, 0.5)
		end
		if Gradient then
			Instance.new("UIGradient").Parent = Leaf
		end
		Leaf.Parent = Mid
		Nodes[Name] = F
	end
	Case("frame_c", "Frame", true, false, false)
	Case("image_c", "ImageLabel", true, false, false)
	Case("image_c_grad", "ImageLabel", true, true, false)
	Case("frame_c_anch", "Frame", true, false, true)
	Case("image_c_anch", "ImageLabel", true, false, true)
	Case("frame_anch", "Frame", false, false, true)
	Case("image", "ImageLabel", false, false, false)
	Case("text_c", "TextLabel", true, false, false)
	for _ = 1, 3 do
		task.wait()
	end
	for Key, Node in Nodes do
		local Parent = Node.Parent
		Results[Key] = { Node.AbsolutePosition.X - Parent.AbsolutePosition.X, Node.AbsolutePosition.Y - Parent.AbsolutePosition.Y, Node.AbsoluteSize.X, Node.AbsoluteSize.Y }
	end
	return Results
end
