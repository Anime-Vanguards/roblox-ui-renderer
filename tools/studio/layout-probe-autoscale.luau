-- AutomaticSize X / XY plain containers (no layout) with scale-width children: frames vs text labels
-- (wrapped AutoY text measures its lines at some width, layout-probe-autoaspect fitXY_text), at two
-- positions in the holder (available width 390 / 290).
return function(Target)
	local Results = {}
	local Nodes = {}
	local Face = Font.new("rbxasset://fonts/families/SourceSansPro.json")
	local Long = "Which units would you like to unequip from the team right now?"
	local Count = 0
	local function Holder()
		Count += 1
		local H = Instance.new("Frame")
		H.Position = UDim2.fromOffset(((Count - 1) % 3) * 400, ((Count - 1) // 3) * 170)
		H.Size = UDim2.fromOffset(390, 160)
		H.BackgroundTransparency = 1
		H.Parent = Target
		return H
	end
	local Children = {
		{ "frame", function()
			local F = Instance.new("Frame")
			F.Size = UDim2.new(1, -20, 0, 30)
			return F
		end },
		{ "wrapped", function()
			local L = Instance.new("TextLabel")
			L.Size = UDim2.new(1, -20, 0, 0)
			L.AutomaticSize = Enum.AutomaticSize.Y
			L.TextWrapped = true
			L.FontFace = Face
			L.TextSize = 20
			L.Text = Long
			return L
		end },
		{ "fixed", function()
			local L = Instance.new("TextLabel")
			L.Size = UDim2.new(1, -20, 0, 30)
			L.FontFace = Face
			L.TextSize = 20
			L.Text = "Short"
			return L
		end },
		{ "half", function()
			local L = Instance.new("TextLabel")
			L.Size = UDim2.new(0.5, 0, 0, 0)
			L.AutomaticSize = Enum.AutomaticSize.Y
			L.TextWrapped = true
			L.FontFace = Face
			L.TextSize = 20
			L.Text = Long
			return L
		end },
		{ "short", function()
			local L = Instance.new("TextLabel")
			L.Size = UDim2.new(1, -20, 0, 0)
			L.AutomaticSize = Enum.AutomaticSize.Y
			L.TextWrapped = true
			L.FontFace = Face
			L.TextSize = 20
			L.Text = "Short"
			return L
		end },
	}
	for _, Auto in { "X", "XY" } do
		for _, X in { 0, 100 } do
			for _, Child in Children do
				local A = Instance.new("Frame")
				A.Position = UDim2.fromOffset(X, 0)
				A.Size = UDim2.fromOffset(0, 20)
				A.AutomaticSize = Enum.AutomaticSize[Auto]
				A.BackgroundTransparency = 0.5
				A.Parent = Holder()
				local C = Child[2]()
				C.Parent = A
				Nodes[`{Auto}_{X}_{Child[1]}_self`] = A
				Nodes[`{Auto}_{X}_{Child[1]}_child`] = C
			end
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
