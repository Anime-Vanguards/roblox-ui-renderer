-- A slider prompt's Holder: AutomaticSize XY, Size (500, 158), UIAspectRatioConstraint 3.5, with a
-- scale-sized AutomaticSize Y "Main" (list + padding + content) and a (1, 0, 0, 267) "Shadow";
-- Studio makes it 533 wide (the children see 533) where the offline layout keeps 500.
return function(Target)
	local Results = {}
	local Nodes = {}
	local Count = 0
	local function Holder()
		Count += 1
		local H = Instance.new("Frame")
		H.Position = UDim2.fromOffset(((Count - 1) % 3) * 400, ((Count - 1) // 3) * 440)
		H.Size = UDim2.fromOffset(1200, 900)
		H.BackgroundTransparency = 1
		H.Parent = Target
		return H
	end
	local function Main(Parent, ContentHeight)
		local M = Instance.new("Frame")
		M.Name = "Main"
		M.AutomaticSize = Enum.AutomaticSize.Y
		M.Position = UDim2.fromScale(0.002, -0.0066)
		M.Size = UDim2.fromScale(1, 1)
		M.BackgroundTransparency = 1
		M.Parent = Parent
		local List = Instance.new("UIListLayout")
		List.Padding = UDim.new(0, 11)
		List.HorizontalAlignment = Enum.HorizontalAlignment.Center
		List.Parent = M
		local Padding = Instance.new("UIPadding")
		Padding.PaddingTop = UDim.new(0, 12)
		Padding.PaddingBottom = UDim.new(0, 12)
		Padding.PaddingLeft = UDim.new(0, 5)
		Padding.PaddingRight = UDim.new(0, 5)
		Padding.Parent = M
		local C = Instance.new("Frame")
		C.Size = UDim2.fromOffset(375, ContentHeight)
		C.Parent = M
		local D = Instance.new("Frame")
		D.Size = UDim2.fromOffset(200, 65)
		D.Parent = M
		return M
	end
	local function Shadow(Parent)
		local S = Instance.new("Frame")
		S.Name = "Shadow"
		S.Size = UDim2.new(1, 0, 0, 267)
		S.BackgroundTransparency = 1
		S.Parent = Parent
		local I = Instance.new("ImageLabel")
		I.AnchorPoint = Vector2.new(0.5, 0.5)
		I.Position = UDim2.fromScale(0.5, 0.5)
		I.Size = UDim2.fromScale(1.5, 1.5)
		I.Parent = S
		return S
	end
	local Cases = {
		{ "both", true, true, 100 },
		{ "main", true, false, 100 },
		{ "shadow", false, true, 100 },
		{ "none", false, false, 100 },
		{ "bothTall", true, true, 250 },
		{ "mainTall", true, false, 250 },
	}
	for _, Case in Cases do
		local A = Instance.new("Frame")
		A.AnchorPoint = Vector2.new(0.5, 0.5)
		A.Position = UDim2.fromOffset(300, 220)
		A.AutomaticSize = Enum.AutomaticSize.XY
		A.Size = UDim2.fromOffset(500, 158)
		A.BackgroundTransparency = 1
		A.Parent = Holder()
		local C = Instance.new("UIAspectRatioConstraint")
		C.AspectRatio = 3.5
		C.Parent = A
		Nodes[Case[1] .. "_holder"] = A
		if Case[2] then
			Nodes[Case[1] .. "_main"] = Main(A, Case[4])
		end
		if Case[3] then
			Nodes[Case[1] .. "_shadow"] = Shadow(A)
		end
	end
	for _ = 1, 3 do
		task.wait()
	end
	for Key, Node in Nodes do
		local Parent = Node.Parent
		Results[Key] = { Node.AbsolutePosition.X - Parent.AbsolutePosition.X, Node.AbsolutePosition.Y - Parent.AbsolutePosition.Y, Node.AbsoluteSize.X, Node.AbsoluteSize.Y }
	end
	return Results
end
