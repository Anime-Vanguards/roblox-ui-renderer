-- How UIListLayout orders children whose sort keys tie: by child order (the order they were
-- parented), by creation order, or by name. Libraries such as Vide's `values()` parent rows in the
-- iteration order of a table keyed by the row values, which follows pointer hashes (arbitrary, and
-- different in every process), so only a tie-break that ignores child order is reproducible.
-- Each case creates frames in one order, parents them in another and reports GetChildren() order
-- and the visual (AbsolutePosition) order, for SortOrder.LayoutOrder (equal LayoutOrder) and Name.
return function(Target)
	local Results = {}

	local function Case(Label, CreationOrder, ParentOrder, SortOrder, Names, Reparent)
		local Holder = Instance.new("Frame")
		Holder.Size = UDim2.fromOffset(100, 400)
		Holder.BackgroundTransparency = 1
		Holder.Parent = Target
		local List = Instance.new("UIListLayout")
		List.SortOrder = SortOrder
		List.Parent = Holder

		local Frames = {}
		for _, Key in CreationOrder do
			local Frame = Instance.new("Frame")
			Frame.Name = if Names then Names[Key] else "Row"
			Frame.Size = UDim2.fromOffset(100, 20)
			Frame.LayoutOrder = 0
			Frame:SetAttribute("Key", Key)
			Frames[Key] = Frame
		end
		for _, Key in ParentOrder do
			Frames[Key].Parent = Holder
		end
		if Reparent then
			for _, Key in Reparent do
				Frames[Key].Parent = nil
				Frames[Key].Parent = Holder
			end
		end
		task.wait()

		local Children = {}
		for _, Child in Holder:GetChildren() do
			if Child:IsA("Frame") then
				table.insert(Children, Child:GetAttribute("Key"))
			end
		end
		local Visual = table.clone(Children)
		table.sort(Visual, function(A, B)
			return Frames[A].AbsolutePosition.Y < Frames[B].AbsolutePosition.Y
		end)
		table.insert(Results, { case = Label, children = table.concat(Children, ""), visual = table.concat(Visual, "") })
		Holder:Destroy()
	end

	local LayoutOrder = Enum.SortOrder.LayoutOrder
	local NameOrder = Enum.SortOrder.Name
	Case("same name, created ABC, parented CAB", { "A", "B", "C" }, { "C", "A", "B" }, LayoutOrder)
	Case("same name, created CAB, parented ABC", { "C", "A", "B" }, { "A", "B", "C" }, LayoutOrder)
	Case("same name, created BCA, parented BCA", { "B", "C", "A" }, { "B", "C", "A" }, LayoutOrder)
	Case("same name, created ABC, parented ABC, A re-parented", { "A", "B", "C" }, { "A", "B", "C" }, LayoutOrder, nil, { "A" })
	Case("same name, created ABCDEF, parented FDBECA", { "A", "B", "C", "D", "E", "F" }, { "F", "D", "B", "E", "C", "A" }, LayoutOrder)
	local Names = { A = "Zed", B = "Mid", C = "Alpha" }
	Case("names A=Zed B=Mid C=Alpha, created ABC, parented CAB", { "A", "B", "C" }, { "C", "A", "B" }, LayoutOrder, Names)
	Case("names A=Zed B=Mid C=Alpha, created ABC, parented BCA", { "A", "B", "C" }, { "B", "C", "A" }, LayoutOrder, Names)
	local Same = { A = "Row", B = "Row", C = "Row" }
	Case("SortOrder.Name, same name, created ABC, parented CAB", { "A", "B", "C" }, { "C", "A", "B" }, NameOrder, Same)
	Case("SortOrder.Name, same name, created CAB, parented ABC", { "C", "A", "B" }, { "A", "B", "C" }, NameOrder, Same)

	return { Results = Results }
end
