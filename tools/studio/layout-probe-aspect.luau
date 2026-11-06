-- UIAspectRatioConstraint: AspectType x DominantAxis over element/parent sizes, inside a list, and
-- the constraint on a UIGridLayout (cell aspect).
return function(Target)
	local Results = {}
	local Nodes = {}
	local Count = 0
	local function Holder(W, H)
		Count += 1
		local F = Instance.new("Frame")
		F.Position = UDim2.fromOffset(((Count - 1) % 8) * 150, ((Count - 1) // 8) * 110)
		F.Size = UDim2.fromOffset(W, H)
		F.BackgroundTransparency = 1
		F.Parent = Target
		return F
	end
	local Sizes = { UDim2.fromOffset(100, 60), UDim2.fromOffset(60, 100), UDim2.new(1, 0, 1, 0), UDim2.new(0.5, 0, 0.8, 0), UDim2.fromOffset(200, 30) }
	for SizeIndex, Size in Sizes do
		for _, Type in { "FitWithinMaxSize", "ScaleWithParentSize" } do
			for _, Axis in { "Width", "Height" } do
				for _, Ratio in { 2, 0.5 } do
					local H = Holder(140, 100)
					local F = Instance.new("Frame")
					F.Size = Size
					F.Position = UDim2.fromOffset(3, 4)
					F.Parent = H
					local C = Instance.new("UIAspectRatioConstraint")
					C.AspectRatio = Ratio
					C.AspectType = Enum.AspectType[Type]
					C.DominantAxis = Enum.DominantAxis[Axis]
					C.Parent = F
					Nodes[`S{SizeIndex}_{Type}_{Axis}_{Ratio}`] = F
				end
			end
		end
	end
	-- In a list
	do
		local H = Holder(140, 100)
		local L = Instance.new("UIListLayout")
		L.Parent = H
		for Index, Type in { "FitWithinMaxSize", "ScaleWithParentSize" } do
			local F = Instance.new("Frame")
			F.Size = UDim2.fromOffset(100, 60)
			F.Parent = H
			local C = Instance.new("UIAspectRatioConstraint")
			C.AspectRatio = 3
			C.AspectType = Enum.AspectType[Type]
			C.Parent = F
			Nodes[`List_{Index}`] = F
		end
	end
	-- On a grid layout
	for Index, Spec in { { UDim2.fromOffset(60, 40), 1 }, { UDim2.fromOffset(60, 40), 2 }, { UDim2.new(0.3, 0, 0.5, 0), 1.5 } } do
		local H = Holder(140, 100)
		local G = Instance.new("UIGridLayout")
		G.CellSize = Spec[1]
		G.CellPadding = UDim2.fromOffset(2, 2)
		G.Parent = H
		local C = Instance.new("UIAspectRatioConstraint")
		C.AspectRatio = Spec[2]
		C.Parent = G
		for Item = 1, 3 do
			local F = Instance.new("Frame")
			F.Parent = H
			Nodes[`Grid{Index}_{Item}`] = F
		end
		Nodes[`Grid{Index}_cell`] = G
	end
	task.wait()
	for Key, Node in Nodes do
		if Node:IsA("UIGridLayout") then
			Results[Key] = { Node.AbsoluteCellSize.X, Node.AbsoluteCellSize.Y, Node.AbsoluteContentSize.X, Node.AbsoluteContentSize.Y, Node.AbsoluteCellCount.X, Node.AbsoluteCellCount.Y }
		else
			local Origin = Node.Parent.AbsolutePosition
			Results[Key] = { Node.AbsolutePosition.X - Origin.X, Node.AbsolutePosition.Y - Origin.Y, Node.AbsoluteSize.X, Node.AbsoluteSize.Y }
		end
	end
	return Results
end
