-- UIScale'd items of flex lists (a mode window's PlayButton: a 385 x 40 frame with UIScale 0.885 in a
-- HorizontalFlex Fill row is 40 tall (times the ancestors' scale), not 40 x 0.885; with Wraps the
-- same quirk was measured before). Flex modes, both fill directions, item scales, an item with
-- children.
return function(Target)
	local Results = {}
	local Nodes = {}
	local Count = 0
	local Modes = {
		{ "none", {} },
		{ "fillH", { HorizontalFlex = Enum.UIFlexAlignment.Fill } },
		{ "fillV", { VerticalFlex = Enum.UIFlexAlignment.Fill } },
		{ "around", { HorizontalFlex = Enum.UIFlexAlignment.SpaceAround } },
		{ "wraps", { Wraps = true } },
		{ "lineStretch", { ItemLineAlignment = Enum.ItemLineAlignment.Stretch } },
	}
	for _, Direction in { "Horizontal", "Vertical" } do
		for _, Mode in Modes do
			for _, Scale in { 0.885, 1.3 } do
				Count += 1
				local Row = Instance.new("Frame")
				Row.BackgroundTransparency = 1
				Row.Position = UDim2.fromOffset(((Count - 1) % 4) * 300, ((Count - 1) // 4) * 150)
				Row.Size = UDim2.fromOffset(280, 140)
				Row.Parent = Target
				local List = Instance.new("UIListLayout")
				List.FillDirection = Enum.FillDirection[Direction]
				for Key, Value in Mode[2] do
					(List :: any)[Key] = Value
				end
				List.Parent = Row
				local Item = Instance.new("Frame")
				Item.Size = UDim2.fromOffset(120, 40)
				Item.Parent = Row
				local ItemScale = Instance.new("UIScale")
				ItemScale.Scale = Scale
				ItemScale.Parent = Item
				local Child = Instance.new("Frame")
				Child.Size = UDim2.fromScale(1, 1)
				Child.Parent = Item
				local Other = Instance.new("Frame")
				Other.Size = UDim2.fromOffset(60, 30)
				Other.Parent = Row
				local Key = `{Direction}_{Mode[1]}_{Scale}`
				Nodes[`{Key}_item`] = Item
				Nodes[`{Key}_child`] = Child
				Nodes[`{Key}_other`] = Other
			end
		end
	end
	task.wait()
	for Key, Node in Nodes do
		local Row = Node.Parent
		while Row.Parent ~= Target do
			Row = Row.Parent
		end
		Results[Key] = { Node.AbsolutePosition.X - Row.AbsolutePosition.X, Node.AbsolutePosition.Y - Row.AbsolutePosition.Y, Node.AbsoluteSize.X, Node.AbsoluteSize.Y }
	end
	return Results
end
