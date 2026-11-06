-- AutomaticSize combined with UIAspectRatioConstraint: when the constraint applies and what the
-- children see.
return function(Target)
	local Results = {}
	local Nodes = {}
	local Face = Font.new("rbxasset://fonts/families/SourceSansPro.json")
	local Count = 0
	local function Holder()
		Count += 1
		local H = Instance.new("Frame")
		H.Position = UDim2.fromOffset(((Count - 1) % 3) * 400, ((Count - 1) // 3) * 300)
		H.Size = UDim2.fromOffset(390, 290)
		H.BackgroundTransparency = 1
		H.Parent = Target
		return H
	end
	local Cases = {
		{ "fitW_text", "Y", { 300, 0 }, 4, "FitWithinMaxSize", "Width", "text" },
		{ "fitW_box", "Y", { 300, 0 }, 4, "FitWithinMaxSize", "Width", "box" },
		{ "fitW_tall", "Y", { 300, 0 }, 4, "FitWithinMaxSize", "Width", "tall" },
		{ "fitH_box", "Y", { 300, 0 }, 4, "FitWithinMaxSize", "Height", "box" },
		{ "scaleW_box", "Y", { 300, 0 }, 4, "ScaleWithParentSize", "Width", "box" },
		{ "fitX_box", "X", { 0, 60 }, 2, "FitWithinMaxSize", "Width", "box" },
		{ "fitXY_text", "XY", { 0, 0 }, 3, "FitWithinMaxSize", "Width", "text" },
		{ "noauto", "None", { 300, 100 }, 4, "FitWithinMaxSize", "Width", "box" },
	}
	for _, Case in Cases do
		local A = Instance.new("Frame")
		A.Size = UDim2.fromOffset(Case[3][1], Case[3][2])
		A.AutomaticSize = Enum.AutomaticSize[Case[2]]
		A.Parent = Holder()
		local C = Instance.new("UIAspectRatioConstraint")
		C.AspectRatio = Case[4]
		C.AspectType = Enum.AspectType[Case[5]]
		C.DominantAxis = Enum.DominantAxis[Case[6]]
		C.Parent = A
		local Full = Instance.new("Frame")
		Full.Size = UDim2.fromScale(1, 1)
		Full.BackgroundTransparency = 0.5
		Full.Parent = A
		Nodes[Case[1] .. "_self"] = A
		Nodes[Case[1] .. "_full"] = Full
		if Case[7] == "text" then
			local L = Instance.new("TextLabel")
			L.Size = UDim2.new(1, -20, 0, 0)
			L.AutomaticSize = Enum.AutomaticSize.Y
			L.TextWrapped = true
			L.FontFace = Face
			L.TextSize = 20
			L.Text = "Which units would you like to unequip from the team right now?"
			L.Parent = A
			Nodes[Case[1] .. "_label"] = L
		elseif Case[7] == "box" then
			local B = Instance.new("Frame")
			B.Size = UDim2.fromOffset(100, 50)
			B.Parent = A
			Nodes[Case[1] .. "_box"] = B
		else
			local B = Instance.new("Frame")
			B.Size = UDim2.fromOffset(100, 150)
			B.Parent = A
			Nodes[Case[1] .. "_box"] = B
		end
	end
	for _ = 1, 20 do
		task.wait()
	end
	for Key, Node in Nodes do
		local Parent = Node.Parent
		Results[Key] = { Node.AbsolutePosition.X - Parent.AbsolutePosition.X, Node.AbsolutePosition.Y - Parent.AbsolutePosition.Y, Node.AbsoluteSize.X, Node.AbsoluteSize.Y }
	end
	return Results
end
