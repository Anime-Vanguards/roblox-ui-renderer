-- Content boxes narrower than their padding (layout-probe-autotextpos2: a child of a 0 px frame with
-- 8 + 8 padding sits at x - 8, not x + 8, as if the content box were [x + W - right, x + left];
-- layout-probe-autolistchild: a zero-width list container centres its items in a negative width).
-- Parent widths 0 / 10 / 30 with paddings (8, 8), (20, 5), (0, 8), (8, 0); children at scale
-- positions, with scale sizes and anchors; a centred / right-aligned UIListLayout; both axes.
return function(Target)
	local Results = {}
	local Nodes = {}
	local Count = 0
	local Paddings = { { 8, 8 }, { 20, 5 }, { 0, 8 }, { 8, 0 } }
	for _, Width in { 0, 10, 30 } do
		for PadIndex, Pad in Paddings do
			for _, Mode in { "plain", "center", "right" } do
				Count += 1
				local Parent = Instance.new("Frame")
				Parent.BackgroundTransparency = 1
				Parent.Position = UDim2.fromOffset(((Count - 1) % 6) * 200 + 60, ((Count - 1) // 6) * 70 + 20)
				Parent.Size = UDim2.fromOffset(Width, Width)
				Parent.Parent = Target
				local Padding = Instance.new("UIPadding")
				Padding.PaddingLeft = UDim.new(0, Pad[1])
				Padding.PaddingRight = UDim.new(0, Pad[2])
				Padding.PaddingTop = UDim.new(0, Pad[1])
				Padding.PaddingBottom = UDim.new(0, Pad[2])
				Padding.Parent = Parent
				local Key = `{Width}_{PadIndex}_{Mode}`
				if Mode == "plain" then
					for _, Spec in {
						{ "origin", UDim2.new(), Vector2.zero, UDim2.fromOffset(4, 4) },
						{ "far", UDim2.fromScale(1, 1), Vector2.zero, UDim2.fromOffset(4, 4) },
						{ "farAnchored", UDim2.fromScale(1, 1), Vector2.one, UDim2.fromOffset(4, 4) },
						{ "middle", UDim2.fromScale(0.5, 0.5), Vector2.new(0.5, 0.5), UDim2.fromOffset(4, 4) },
						{ "fill", UDim2.new(), Vector2.zero, UDim2.fromScale(1, 1) },
						{ "fillOffset", UDim2.new(), Vector2.zero, UDim2.new(1, 6, 1, 6) },
					} do
						local Child = Instance.new("Frame")
						Child.Position = Spec[2]
						Child.AnchorPoint = Spec[3]
						Child.Size = Spec[4]
						Child.Parent = Parent
						Nodes[`{Key}_{Spec[1]}`] = Child
					end
				else
					local List = Instance.new("UIListLayout")
					List.HorizontalAlignment = if Mode == "center" then Enum.HorizontalAlignment.Center else Enum.HorizontalAlignment.Right
					List.VerticalAlignment = if Mode == "center" then Enum.VerticalAlignment.Center else Enum.VerticalAlignment.Bottom
					List.Parent = Parent
					local Item = Instance.new("Frame")
					Item.Size = UDim2.fromOffset(12, 6)
					Item.Parent = Parent
					Nodes[`{Key}_item`] = Item
					local Scaled = Instance.new("Frame")
					Scaled.Size = UDim2.new(1, 2, 0, 4)
					Scaled.Parent = Parent
					Nodes[`{Key}_scaled`] = Scaled
				end
			end
		end
	end
	task.wait()
	for Key, Node in Nodes do
		local Parent = Node.Parent
		Results[Key] = { Node.AbsolutePosition.X - Parent.AbsolutePosition.X, Node.AbsolutePosition.Y - Parent.AbsolutePosition.Y, Node.AbsoluteSize.X, Node.AbsoluteSize.Y }
	end
	return Results
end
