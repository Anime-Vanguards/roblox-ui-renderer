-- UIScale on children placed by UIGridLayout / UIListLayout: size and position within the cell
-- per AnchorPoint; UIScale on the grid container.
return function(Target)
	local Results = {}
	local Nodes = {}
	local Count = 0
	local function Holder(W, H)
		Count += 1
		local F = Instance.new("Frame")
		F.Name = `H{Count}`
		F.Position = UDim2.fromOffset(((Count - 1) % 3) * 400, ((Count - 1) // 3) * 170)
		F.Size = UDim2.fromOffset(W or 390, H or 160)
		F.BackgroundTransparency = 1
		F.Parent = Target
		return F
	end
	for _, Kind in { "Grid", "List", "ListH" } do
		for _, Spec in { { 0.885, Vector2.new(0.5, 0.5) }, { 0.885, Vector2.new(0, 0) }, { 0.885, Vector2.new(1, 1) }, { 1.2, Vector2.new(0.5, 0.5) }, { 0.6, Vector2.new(0.25, 0.75) } } do
			local H = Holder()
			if Kind == "Grid" then
				local G = Instance.new("UIGridLayout")
				G.CellSize = UDim2.fromOffset(120, 50)
				G.CellPadding = UDim2.fromOffset(-10, 3)
				G.Parent = H
			else
				local L = Instance.new("UIListLayout")
				L.FillDirection = if Kind == "ListH" then Enum.FillDirection.Horizontal else Enum.FillDirection.Vertical
				L.Padding = UDim.new(0, 4)
				L.Parent = H
			end
			for Index = 1, 3 do
				local F = Instance.new("Frame")
				F.Name = `I{Index}`
				F.Size = UDim2.fromOffset(100, 40)
				F.AnchorPoint = Spec[2]
				F.Parent = H
				if Index ~= 2 then
					local S = Instance.new("UIScale")
					S.Scale = Spec[1]
					S.Parent = F
				end
				local Inner = Instance.new("Frame")
				Inner.Name = "Inner"
				Inner.Position = UDim2.fromScale(0.5, 0.5)
				Inner.Size = UDim2.fromOffset(10, 10)
				Inner.Parent = F
				Nodes[`{Kind}_{Spec[1]}_{Spec[2].X}_{Index}`] = F
				Nodes[`{Kind}_{Spec[1]}_{Spec[2].X}_{Index}_inner`] = Inner
			end
		end
	end
	do
		local H = Holder()
		local S = Instance.new("UIScale")
		S.Scale = 0.8
		S.Parent = H
		local G = Instance.new("UIGridLayout")
		G.CellSize = UDim2.new(0.3, 0, 0, 50)
		G.Parent = H
		for Index = 1, 3 do
			local F = Instance.new("Frame")
			F.Size = UDim2.fromOffset(10, 10)
			F.Parent = H
			Nodes[`ScaledGrid_{Index}`] = F
		end
	end
	task.wait()
	for Key, Node in Nodes do
		local Origin = Node.Parent.AbsolutePosition
		Results[Key] = { Node.AbsolutePosition.X - Origin.X, Node.AbsolutePosition.Y - Origin.Y, Node.AbsoluteSize.X, Node.AbsoluteSize.Y }
	end
	return Results
end
