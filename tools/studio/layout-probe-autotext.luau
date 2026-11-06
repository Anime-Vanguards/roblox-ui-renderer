-- Do text children extend an AutomaticSize container by their text (layout-probe-autoscale: a
-- non-automatic TextLabel (1, -20, 0, 30) made an AutoX frame as wide as its text)? Offset-sized
-- labels with overflowing / wrapped / short text, alignment, positions, list parents, visibility.
return function(Target)
	local Results = {}
	local Nodes = {}
	local Face = Font.new("rbxasset://fonts/families/SourceSansPro.json")
	local Long = "Which units would you like to unequip"
	local Count = 0
	local function Holder()
		Count += 1
		local H = Instance.new("Frame")
		H.Position = UDim2.fromOffset(((Count - 1) % 4) * 300, ((Count - 1) // 4) * 110)
		H.Size = UDim2.fromOffset(290, 100)
		H.BackgroundTransparency = 1
		H.Parent = Target
		return H
	end
	local function Label(Props)
		local L = Instance.new("TextLabel")
		L.FontFace = Face
		L.TextSize = 20
		for Name, Value in Props do
			L[Name] = Value
		end
		return L
	end
	local Cases = {
		{ "overflow", function() return Label({ Size = UDim2.fromOffset(50, 30), Text = Long }) end },
		{ "wrapped", function() return Label({ Size = UDim2.fromOffset(50, 30), Text = Long, TextWrapped = true }) end },
		{ "short", function() return Label({ Size = UDim2.fromOffset(50, 30), Text = "Hi" }) end },
		{ "right", function() return Label({ Size = UDim2.fromOffset(50, 30), Text = Long, TextXAlignment = Enum.TextXAlignment.Right }) end },
		{ "offsetpos", function() return Label({ Position = UDim2.fromOffset(15, 5), Size = UDim2.fromOffset(50, 30), Text = Long }) end },
		{ "scalepos", function() return Label({ Position = UDim2.fromScale(0.5, 0), Size = UDim2.fromOffset(50, 30), Text = Long }) end },
		{ "hidden", function() return Label({ Size = UDim2.fromOffset(50, 30), Text = Long, Visible = false }) end },
		{ "image", function()
			local I = Instance.new("ImageLabel")
			I.Size = UDim2.new(1, -20, 0, 30)
			return I
		end },
		{ "tall", function() return Label({ Size = UDim2.fromOffset(50, 10), Text = "Hi" }) end },
		{ "scaled", function() return Label({ Size = UDim2.new(0.5, 0, 0, 30), Text = Long, TextScaled = true }) end },
		{ "zerowrap", function() return Label({ Size = UDim2.new(1, -20, 0, 0), Text = Long, TextWrapped = true }) end },
		{ "autoYwrap", function() return Label({ Size = UDim2.new(1, -20, 0, 0), Text = Long, TextWrapped = true, AutomaticSize = Enum.AutomaticSize.Y }) end },
	}
	for _, Auto in { "X", "XY" } do
		for _, Case in Cases do
			local A = Instance.new("Frame")
			A.Size = UDim2.fromOffset(0, 20)
			A.AutomaticSize = Enum.AutomaticSize[Auto]
			A.BackgroundTransparency = 0.5
			A.Parent = Holder()
			local C = Case[2]()
			C.Parent = A
			Nodes[`{Auto}_{Case[1]}_self`] = A
			Nodes[`{Auto}_{Case[1]}_child`] = C
		end
	end
	for _ = 1, 3 do
		task.wait()
	end
	for Key, Node in Nodes do
		local Parent = Node.Parent
		local Row = { Node.AbsolutePosition.X - Parent.AbsolutePosition.X, Node.AbsolutePosition.Y - Parent.AbsolutePosition.Y, Node.AbsoluteSize.X, Node.AbsoluteSize.Y }
		if Node:IsA("TextLabel") then
			table.insert(Row, Node.TextBounds.X)
			table.insert(Row, Node.TextBounds.Y)
		end
		Results[Key] = Row
	end
	return Results
end
