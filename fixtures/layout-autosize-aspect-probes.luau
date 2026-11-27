-- The Studio probes behind rur's AutomaticSize + UIAspectRatioConstraint rule, rebuilt for a client capture:
-- tools/studio/layout-probe-autoaspect.luau (cells a_*: Size (300, 0) AutoY frames of ratio 4 around a (1, 1) frame and
-- a label or a box, AutoX, AutoXY and ScaleWithParentSize variants) and layout-probe-autoaspect4.luau (cells w_*: which
-- children decide the width of a Size (300, 0) AutoY frame of ratio 4). Each cell is the probe's 390 x 290 holder;
-- the holders overlap (transparent), the frames they hold do not.
--   node cli/rur.mjs compare --fixture fixtures/layout-autosize-aspect-probes.luau --viewport 1800x1260 --time 0.2 --background 20,20,26
return function(Target)
	local Face = Font.new("rbxasset://fonts/families/SourceSansPro.json")
	local function Frame(Name, Parent, Size, Props)
		local F = Instance.new("Frame")
		F.Name = Name
		F.Size = Size
		F.BackgroundColor3 = Color3.fromRGB(60, 60, 80)
		F.BackgroundTransparency = 0.5
		F.BorderSizePixel = 0
		for Key, Value in Props or {} do
			F[Key] = Value
		end
		F.Parent = Parent
		return F
	end
	local function Holder(Name, X, Y)
		local H = Instance.new("Frame")
		H.Name = Name
		H.Position = UDim2.fromOffset(X, Y)
		H.Size = UDim2.fromOffset(390, 290)
		H.BackgroundTransparency = 1
		H.Parent = Target
		return H
	end

	-- layout-probe-autoaspect
	local Cases = {
		{ "a_fitW_text", "Y", { 300, 0 }, 4, "FitWithinMaxSize", "Width", "text" },
		{ "a_fitW_box", "Y", { 300, 0 }, 4, "FitWithinMaxSize", "Width", "box" },
		{ "a_fitW_tall", "Y", { 300, 0 }, 4, "FitWithinMaxSize", "Width", "tall" },
		{ "a_fitH_box", "Y", { 300, 0 }, 4, "FitWithinMaxSize", "Height", "box" },
		{ "a_scaleW_box", "Y", { 300, 0 }, 4, "ScaleWithParentSize", "Width", "box" },
		{ "a_fitX_box", "X", { 0, 60 }, 2, "FitWithinMaxSize", "Width", "box" },
		{ "a_fitXY_text", "XY", { 0, 0 }, 3, "FitWithinMaxSize", "Width", "text" },
		{ "a_noauto", "None", { 300, 100 }, 4, "FitWithinMaxSize", "Width", "box" },
	}
	for Index, Case in Cases do
		local Cell = Holder(Case[1], ((Index - 1) % 4) * 450, ((Index - 1) // 4) * 220)
		local A = Frame("A", Cell, UDim2.fromOffset(Case[3][1], Case[3][2]), { BackgroundColor3 = Color3.fromRGB(200, 40, 50) })
		A.AutomaticSize = Enum.AutomaticSize[Case[2]]
		local C = Instance.new("UIAspectRatioConstraint")
		C.AspectRatio = Case[4]
		C.AspectType = Enum.AspectType[Case[5]]
		C.DominantAxis = Enum.DominantAxis[Case[6]]
		C.Parent = A
		Frame("Full", A, UDim2.fromScale(1, 1), { BackgroundColor3 = Color3.fromRGB(230, 200, 40) })
		if Case[7] == "text" then
			local L = Instance.new("TextLabel")
			L.Name = "Label"
			L.Size = UDim2.new(1, -20, 0, 0)
			L.AutomaticSize = Enum.AutomaticSize.Y
			L.BackgroundTransparency = 1
			L.TextColor3 = Color3.new(1, 1, 1)
			L.TextWrapped = true
			L.FontFace = Face
			L.TextSize = 20
			L.Text = "Which units would you like to unequip from the team right now?"
			L.Parent = A
		else
			Frame("Box", A, UDim2.fromOffset(100, if Case[7] == "box" then 50 else 150), { BackgroundColor3 = Color3.fromRGB(40, 160, 60) })
		end
	end

	-- layout-probe-autoaspect4
	local Count = 0
	local function Case(Name, Build)
		local Cell = Holder(Name, (Count % 4) * 450, 470 + (Count // 4) * 260)
		Count += 1
		local A = Frame("A", Cell, UDim2.fromOffset(300, 0), { AutomaticSize = Enum.AutomaticSize.Y, BackgroundColor3 = Color3.fromRGB(200, 40, 50) })
		local C = Instance.new("UIAspectRatioConstraint")
		C.AspectRatio = 4
		C.Parent = A
		Build(A)
	end
	local function List(Parent)
		local L = Instance.new("UIListLayout")
		L.Parent = Parent
	end
	Case("w_scale_inner100", function(A)
		local S = Frame("S", A, UDim2.new(1, 0, 0, 30))
		Frame("I", S, UDim2.fromOffset(100, 20))
	end)
	Case("w_scale_inner100_offset200", function(A)
		local S = Frame("S", A, UDim2.new(1, 0, 0, 30))
		Frame("I", S, UDim2.fromOffset(100, 20))
		Frame("O", A, UDim2.fromOffset(200, 20))
	end)
	Case("w_offset100", function(A)
		Frame("O", A, UDim2.fromOffset(100, 50))
	end)
	Case("w_scale_empty", function(A)
		Frame("S", A, UDim2.new(1, 0, 0, 30))
	end)
	Case("w_scale_list100", function(A)
		local S = Frame("S", A, UDim2.new(1, 0, 0, 0), { AutomaticSize = Enum.AutomaticSize.Y })
		List(S)
		Frame("I", S, UDim2.fromOffset(100, 20))
	end)
	Case("w_scale_list100_tall", function(A)
		local S = Frame("S", A, UDim2.new(1, 0, 0, 0), { AutomaticSize = Enum.AutomaticSize.Y })
		List(S)
		Frame("I", S, UDim2.fromOffset(100, 80))
	end)
	Case("w_scale_list400", function(A)
		local S = Frame("S", A, UDim2.new(1, 0, 0, 0), { AutomaticSize = Enum.AutomaticSize.Y })
		List(S)
		Frame("I", S, UDim2.fromOffset(400, 20))
	end)
	Case("w_half_list100", function(A)
		local S = Frame("S", A, UDim2.new(0.5, 0, 0, 0), { AutomaticSize = Enum.AutomaticSize.Y })
		List(S)
		Frame("I", S, UDim2.fromOffset(100, 20))
	end)
	Case("w_scale_list100_noauto", function(A)
		local S = Frame("S", A, UDim2.new(1, 0, 0, 40))
		List(S)
		Frame("I", S, UDim2.fromOffset(100, 20))
	end)
end
