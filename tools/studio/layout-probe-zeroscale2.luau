-- The effective factor of tiny, zero and negative UIScales: UIScale.Scale read back, and a 1e6 px
-- child's AbsoluteSize / a child at offset 1e6's AbsolutePosition under it.
return function(Target)
	local Results = {}
	local Rows = {}
	for Index, Scale in { 0, 1e-9, 1e-8, 5e-8, 9e-8, 1e-7, 1.5e-7, 2e-7, 1e-6, 1e-4, -1, -0.5, -1e-9 } do
		local Holder = Instance.new("Frame")
		Holder.BackgroundTransparency = 1
		Holder.Position = UDim2.fromOffset(10, 10)
		Holder.Size = UDim2.fromOffset(100, 100)
		Holder.Parent = Target
		local UIScale = Instance.new("UIScale")
		UIScale.Scale = Scale
		UIScale.Parent = Holder
		local Big = Instance.new("Frame")
		Big.Size = UDim2.fromOffset(1000000, 1000000)
		Big.Position = UDim2.fromOffset(1000000, 1000000)
		Big.Parent = Holder
		table.insert(Rows, { Index, UIScale, Holder, Big })
	end
	task.wait()
	for _, Row in Rows do
		local _, UIScale, Holder, Big = table.unpack(Row)
		Results[`r{Row[1]}`] = { UIScale.Scale, Holder.AbsoluteSize.X, Big.AbsoluteSize.X, Big.AbsolutePosition.X }
	end
	return Results
end
