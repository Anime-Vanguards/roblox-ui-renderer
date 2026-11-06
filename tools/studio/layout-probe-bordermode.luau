-- Does BorderMode / BorderSizePixel move or shrink the children of plain frames (it does for
-- ScrollingFrames: layout-probe-scrollborder), of list containers and of ScrollingFrames with
-- AutomaticCanvasSize / UIPadding?
return function(Target)
	local Results = {}
	local Nodes = {}
	local Count = 0
	local function Next()
		Count += 1
		return UDim2.fromOffset(10 + ((Count - 1) % 8) * 140, 10 + ((Count - 1) // 8) * 120)
	end
	for _, Border in { 0, 3 } do
		for _, Mode in { "Outline", "Middle", "Inset" } do
			local F = Instance.new("Frame")
			F.Position = Next()
			F.Size = UDim2.fromOffset(100, 80)
			F.BorderSizePixel = Border
			F.BorderMode = Enum.BorderMode[Mode]
			F.Parent = Target
			local Child = Instance.new("Frame")
			Child.Size = UDim2.fromScale(0.5, 0.5)
			Child.Position = UDim2.fromScale(0.5, 0.5)
			Child.Parent = F
			Nodes[`frame_b{Border}_{Mode}`] = { F, Child }

			local L = Instance.new("Frame")
			L.Position = Next()
			L.Size = UDim2.fromOffset(100, 80)
			L.BorderSizePixel = Border
			L.BorderMode = Enum.BorderMode[Mode]
			L.Parent = Target
			local List = Instance.new("UIListLayout")
			List.HorizontalAlignment = Enum.HorizontalAlignment.Right
			List.Parent = L
			local Item = Instance.new("Frame")
			Item.Size = UDim2.new(0.5, 0, 0, 20)
			Item.Parent = L
			Nodes[`list_b{Border}_{Mode}`] = { L, Item }

			local S = Instance.new("ScrollingFrame")
			S.Position = Next()
			S.Size = UDim2.fromOffset(100, 80)
			S.BorderSizePixel = Border
			S.BorderMode = Enum.BorderMode[Mode]
			S.CanvasSize = UDim2.new()
			S.AutomaticCanvasSize = Enum.AutomaticSize.Y
			S.ScrollBarThickness = 6
			S.Parent = Target
			local Padding = Instance.new("UIPadding")
			Padding.PaddingLeft = UDim.new(0, 4)
			Padding.PaddingTop = UDim.new(0, 2)
			Padding.Parent = S
			local SList = Instance.new("UIListLayout")
			SList.Parent = S
			for Index = 1, 3 do
				local Row = Instance.new("Frame")
				Row.Size = UDim2.new(1, 0, 0, 40)
				Row.Parent = S
				if Index == 1 then
					Nodes[`auto_b{Border}_{Mode}`] = { S, Row }
				end
			end
		end
	end
	task.wait()
	task.wait()
	for Key, Pair in Nodes do
		local Parent, Child = Pair[1], Pair[2]
		local Entry = { Child.AbsolutePosition.X - Parent.AbsolutePosition.X, Child.AbsolutePosition.Y - Parent.AbsolutePosition.Y, Child.AbsoluteSize.X, Child.AbsoluteSize.Y }
		if Parent:IsA("ScrollingFrame") then
			table.insert(Entry, Parent.AbsoluteCanvasSize.X)
			table.insert(Entry, Parent.AbsoluteCanvasSize.Y)
			table.insert(Entry, Parent.AbsoluteWindowSize.X)
			table.insert(Entry, Parent.AbsoluteWindowSize.Y)
		end
		Results[Key] = Entry
	end
	return Results
end
