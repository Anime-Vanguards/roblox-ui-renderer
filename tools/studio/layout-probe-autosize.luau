-- AutomaticSize rules: children against the base size, UIPadding scale in the automatic size,
-- TextWrapped + AutomaticSize X wrap width.
return function(Target)
	local Results = {}
	local Nodes = {}
	local Face = Font.new("rbxasset://fonts/families/SourceSansPro.json")
	local Count = 0
	local function Holder(W)
		Count += 1
		local H = Instance.new("Frame")
		H.Name = `H{Count}`
		H.Position = UDim2.fromOffset(((Count - 1) % 4) * 300, ((Count - 1) // 4) * 120)
		H.Size = UDim2.fromOffset(W or 290, 110)
		H.BackgroundTransparency = 1
		H.Parent = Target
		return H
	end
	local function Track(Key, Node)
		Nodes[Key] = Node
		return Node
	end
	local function Frame(Parent, Props)
		local F = Instance.new("Frame")
		for K, V in Props do
			(F :: any)[K] = V
		end
		F.Parent = Parent
		return F
	end
	local function Pad(Parent, L, R, T, B)
		local P = Instance.new("UIPadding")
		P.PaddingLeft, P.PaddingRight, P.PaddingTop, P.PaddingBottom = L, R, T, B
		P.Parent = Parent
	end

	-- A: scale children against the base size
	for _, Base in { { 60, 40 }, { 30, 20 }, { 100, 80 } } do
		local A = Track(`A{Base[1]}_self`, Frame(Holder(), { Size = UDim2.fromOffset(Base[1], Base[2]), AutomaticSize = Enum.AutomaticSize.XY }))
		Track(`A{Base[1]}_half`, Frame(A, { Size = UDim2.new(0.5, 10, 0.5, 5) }))
		Track(`A{Base[1]}_far`, Frame(A, { Position = UDim2.new(1, 5, 0, 10), Size = UDim2.fromOffset(20, 20) }))
		Track(`A{Base[1]}_anchor`, Frame(A, { AnchorPoint = Vector2.new(1, 1), Position = UDim2.fromScale(1, 1), Size = UDim2.fromOffset(15, 15) }))
	end

	-- B: UIPadding scale in the automatic size
	for Index, Pads in {
		{ UDim.new(0, 0), UDim.new(0, 0), UDim.new(0.2, 0), UDim.new(0, 5) },
		{ UDim.new(0, 0), UDim.new(0, 0), UDim.new(0.2, 3), UDim.new(0.1, 0) },
		{ UDim.new(0.1, 2), UDim.new(0.25, 0), UDim.new(0, 0), UDim.new(0, 0) },
		{ UDim.new(0.1, 2), UDim.new(0.25, 0), UDim.new(0.1, 1), UDim.new(0.1, 1) },
	} do
		local B = Track(`B{Index}_self`, Frame(Holder(), { Size = UDim2.fromOffset(40, 10), AutomaticSize = Enum.AutomaticSize.XY }))
		Pad(B, Pads[1], Pads[2], Pads[3], Pads[4])
		Track(`B{Index}_child`, Frame(B, { Size = UDim2.fromOffset(50, 40) }))
	end

	-- C: wrapped text with AutomaticSize X / XY
	local Long = "Wrapped automatic label with several words in it"
	for _, Spec in { { 290, 5, 0 }, { 290, 60, 0 }, { 290, 5, 120 }, { 200, 5, 0 }, { 290, 5, 350 }, { 120, 10, 30 } } do
		for _, Axis in { "X", "XY" } do
			local L = Instance.new("TextLabel")
			L.Position = UDim2.fromOffset(Spec[2], 5)
			L.Size = UDim2.fromOffset(Spec[3], 20)
			L.AutomaticSize = Enum.AutomaticSize[Axis]
			L.TextWrapped = true
			L.FontFace = Face
			L.TextSize = 16
			L.Text = Long
			L.Parent = Holder(Spec[1])
			Track(`C{Spec[1]}_{Spec[2]}_{Spec[3]}_{Axis}`, L)
		end
	end
	do
		local H = Holder(250)
		Pad(H, UDim.new(0, 20), UDim.new(0, 30), UDim.new(0, 0), UDim.new(0, 0))
		local L = Instance.new("TextLabel")
		L.Size = UDim2.fromOffset(0, 0)
		L.AutomaticSize = Enum.AutomaticSize.XY
		L.TextWrapped = true
		L.FontFace = Face
		L.TextSize = 16
		L.Text = Long
		L.Parent = H
		Track("C_padded", L)
	end
	do
		local H = Holder(250)
		local List = Instance.new("UIListLayout")
		List.FillDirection = Enum.FillDirection.Horizontal
		List.Parent = H
		Frame(H, { Size = UDim2.fromOffset(40, 20) })
		local L = Instance.new("TextLabel")
		L.Size = UDim2.fromOffset(0, 0)
		L.AutomaticSize = Enum.AutomaticSize.XY
		L.TextWrapped = true
		L.FontFace = Face
		L.TextSize = 16
		L.Text = Long
		L.Parent = H
		Track("C_list", L)
	end
	do
		local Outer = Frame(Holder(260), { Size = UDim2.fromOffset(180, 100), AutomaticSize = Enum.AutomaticSize.X })
		local L = Instance.new("TextLabel")
		L.Size = UDim2.fromOffset(0, 0)
		L.AutomaticSize = Enum.AutomaticSize.XY
		L.TextWrapped = true
		L.FontFace = Face
		L.TextSize = 16
		L.Text = Long
		L.Parent = Outer
		Track("C_autoParent", L)
		Track("C_autoParentOuter", Outer)
	end

	task.wait()
	for Key, Node in Nodes do
		local Parent = Node.Parent
		local Entry = { Node.AbsolutePosition.X - Parent.AbsolutePosition.X, Node.AbsolutePosition.Y - Parent.AbsolutePosition.Y, Node.AbsoluteSize.X, Node.AbsoluteSize.Y }
		if Node:IsA("TextLabel") then
			table.insert(Entry, Node.TextBounds.X)
			table.insert(Entry, Node.TextBounds.Y)
		end
		Results[Key] = Entry
	end
	return Results
end
