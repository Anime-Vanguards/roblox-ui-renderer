-- AutomaticSize: which size (base or final) scale-sized/positioned children see, per axis.
return function(Target)
	local Results = {}
	local Nodes = {}
	local Count = 0
	local function Holder()
		Count += 1
		local H = Instance.new("Frame")
		H.Position = UDim2.fromOffset(((Count - 1) % 5) * 230, ((Count - 1) // 5) * 180)
		H.Size = UDim2.fromOffset(220, 170)
		H.BackgroundTransparency = 1
		H.Parent = Target
		return H
	end
	local function Frame(Parent, Props)
		local F = Instance.new("Frame")
		for K, V in Props do
			(F :: any)[K] = V
		end
		F.Parent = Parent
		return F
	end
	local Cases = {
		-- name, axis, base, grow child (position, size)
		{ "growY", "XY", { 30, 20 }, UDim2.new(0, 0, 1, 5), { 10, 20 } },
		{ "growX", "XY", { 30, 20 }, UDim2.new(1, 5, 0, 0), { 20, 10 } },
		{ "growBoth", "XY", { 30, 20 }, UDim2.new(1, 5, 1, 5), { 20, 20 } },
		{ "growYonlyY", "Y", { 30, 20 }, UDim2.new(0, 0, 1, 5), { 10, 20 } },
		{ "growXonlyX", "X", { 30, 20 }, UDim2.new(1, 5, 0, 0), { 20, 10 } },
		{ "growOffsetY", "XY", { 30, 20 }, UDim2.new(0, 0, 0, 40), { 10, 20 } },
		{ "growOffsetX", "XY", { 30, 20 }, UDim2.new(0, 60, 0, 0), { 20, 10 } },
		{ "growOffsetBoth", "XY", { 30, 20 }, UDim2.new(0, 60, 0, 40), { 20, 20 } },
		{ "growOffsetYonlyY", "Y", { 30, 20 }, UDim2.new(0, 0, 0, 40), { 10, 20 } },
		{ "growOffsetXonlyX", "X", { 30, 20 }, UDim2.new(0, 60, 0, 0), { 20, 10 } },
	}
	for _, Case in Cases do
		local A = Frame(Holder(), { Size = UDim2.fromOffset(Case[3][1], Case[3][2]), AutomaticSize = Enum.AutomaticSize[Case[2]] })
		Nodes[Case[1] .. "_self"] = A
		Nodes[Case[1] .. "_half"] = Frame(A, { Size = UDim2.new(0.5, 0, 0.5, 0) })
		Nodes[Case[1] .. "_full"] = Frame(A, { Size = UDim2.fromScale(1, 1) })
		Nodes[Case[1] .. "_corner"] = Frame(A, { AnchorPoint = Vector2.new(1, 1), Position = UDim2.fromScale(1, 1), Size = UDim2.fromOffset(6, 6) })
		Nodes[Case[1] .. "_grow"] = Frame(A, { Position = Case[4], Size = UDim2.fromOffset(Case[5][1], Case[5][2]) })
	end
	task.wait()
	for Key, Node in Nodes do
		local Parent = Node.Parent
		Results[Key] = { Node.AbsolutePosition.X - Parent.AbsolutePosition.X, Node.AbsolutePosition.Y - Parent.AbsolutePosition.Y, Node.AbsoluteSize.X, Node.AbsoluteSize.Y }
	end
	return Results
end
