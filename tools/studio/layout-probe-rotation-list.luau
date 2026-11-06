-- AbsoluteRotation of rotated children inside UIListLayout / UIGridLayout / plain parents.
return function(Target)
	local Results = {}
	local Nodes = {}
	for Index, Kind in { "None", "List", "Grid" } do
		for _, ParentRotation in { 0, 10 } do
			local Holder = Instance.new("Frame")
			Holder.Name = `{Kind}_{ParentRotation}`
			Holder.Position = UDim2.fromOffset(20 + (Index - 1) * 250, if ParentRotation == 0 then 20 else 300)
			Holder.Size = UDim2.fromOffset(200, 200)
			Holder.Rotation = ParentRotation
			Holder.Parent = Target
			if Kind == "List" then
				Instance.new("UIListLayout").Parent = Holder
			elseif Kind == "Grid" then
				Instance.new("UIGridLayout").Parent = Holder
			end
			for ChildIndex, ChildRotation in { 0, 45, -30 } do
				local Child = Instance.new("Frame")
				Child.Name = `C{ChildIndex}`
				Child.Size = UDim2.fromOffset(50, 30)
				Child.Rotation = ChildRotation
				Child.Parent = Holder
				local Grand = Instance.new("Frame")
				Grand.Name = "G"
				Grand.Size = UDim2.fromOffset(10, 10)
				Grand.Rotation = 5
				Grand.Parent = Child
				table.insert(Nodes, Child)
				table.insert(Nodes, Grand)
			end
		end
	end
	task.wait()
	for _, Node in Nodes do
		Results[Node.Parent.Name .. "/" .. Node.Name .. (if Node.Name == "G" then "@" .. Node.Parent.Parent.Name else "")] = { Node.AbsolutePosition.X, Node.AbsolutePosition.Y, Node.AbsoluteRotation }
	end
	return Results
end
