-- Float32 layout arithmetic: raw AbsolutePosition / AbsoluteSize (no rounding in the probe) of frames
-- with fractional scale sizes / positions, anchor points, UIScale, nesting, UIPadding and a list, so
-- the order of Studio's float32 operations can be fitted bit-exactly.
return function(Target)
	local Results = {}
	local Frames = {}
	local function Add(Key, Instance)
		table.insert(Frames, { Key = Key, Instance = Instance })
	end
	for ScaleIndex, Scale in { 1, 0.885, 1.3, 0.7 } do
		local Holder = Instance.new("Frame")
		Holder.Position = UDim2.new(0.013 * ScaleIndex, 3, 0.017 * ScaleIndex, 1)
		Holder.Size = UDim2.new(0.77, 0, 0.61, 0)
		Holder.BackgroundTransparency = 1
		Holder.Parent = Target
		Add(`H{ScaleIndex}`, Holder)
		if Scale ~= 1 then
			local UIScale = Instance.new("UIScale")
			UIScale.Scale = Scale
			UIScale.Parent = Holder
		end
		for Index = 1, 16 do
			local Child = Instance.new("Frame")
			Child.Size = UDim2.new(0.037 * Index, Index, 0.0123 * Index + 0.1, 0)
			Child.Position = UDim2.new(0.0417 * Index, -Index, 0.031 * Index, 2 * Index)
			if Index % 2 == 1 then
				Child.AnchorPoint = Vector2.new(0.3, 0.7)
			end
			Child.BackgroundTransparency = 1
			Child.Parent = Holder
			Add(`H{ScaleIndex}_C{Index}`, Child)
			if Index % 4 == 0 then
				local Padding = Instance.new("UIPadding")
				Padding.PaddingLeft = UDim.new(0.05, 3)
				Padding.PaddingTop = UDim.new(0.07, 1)
				Padding.PaddingRight = UDim.new(0.011, 0)
				Padding.Parent = Child
			end
			local Grand = Instance.new("Frame")
			Grand.Size = UDim2.new(0.5, 1, 0.333, 0)
			Grand.Position = UDim2.new(0.25, 0, 0.1, 1)
			Grand.AnchorPoint = Vector2.new(0.5, 0.25)
			Grand.BackgroundTransparency = 1
			Grand.Parent = Child
			Add(`H{ScaleIndex}_C{Index}_G`, Grand)
		end
		local List = Instance.new("Frame")
		List.Position = UDim2.new(0.6, 0, 0.5, 0)
		List.Size = UDim2.new(0.3, 0, 0.45, 0)
		List.BackgroundTransparency = 1
		List.Parent = Holder
		Add(`H{ScaleIndex}_L`, List)
		local Layout = Instance.new("UIListLayout")
		Layout.Padding = UDim.new(0.017, 1)
		Layout.HorizontalAlignment = Enum.HorizontalAlignment.Center
		Layout.Parent = List
		for Index = 1, 8 do
			local Item = Instance.new("Frame")
			Item.Size = UDim2.new(0.1 * Index, 0, 0.0371 * Index, 0)
			Item.LayoutOrder = Index
			Item.BackgroundTransparency = 1
			Item.Parent = List
			Add(`H{ScaleIndex}_L{Index}`, Item)
		end
	end
	task.wait()
	for _, Entry in Frames do
		local Position, Size = Entry.Instance.AbsolutePosition, Entry.Instance.AbsoluteSize
		Results[Entry.Key] = { Position.X, Position.Y, Size.X, Size.Y }
	end
	return Results
end
