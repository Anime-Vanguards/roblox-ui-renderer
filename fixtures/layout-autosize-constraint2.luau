-- AutomaticSize with UIPadding and UISizeConstraint, continued from layout-autosize-constraint.luau: MinSize above
-- MaxSize (property reads in R), a MaxSize the children push past (uneven padding, an offset child, scale children,
-- scale padding), Size beside MaxSize, and AutomaticSize parents (P, through a list, with padding, two levels up
-- through Q) measuring constrained children with and without AutomaticSize. Each cell holds one object E (a Frame
-- holding C, a TextLabel or a ScrollingFrame).
--   node cli/rur.mjs compare --fixture fixtures/layout-autosize-constraint2.luau --viewport 1200x900 --time 0.2 --background 20,20,26
return function(Target)
	local Face = Font.new("rbxasset://fonts/families/Montserrat.json", Enum.FontWeight.Heavy)
	local Count = 0
	local function Cell(Name)
		local Column = Count % 8
		local Row = Count // 8
		Count += 1
		local Holder = Instance.new("Frame")
		Holder.Name = Name
		Holder.Position = UDim2.fromOffset(10 + Column * 148, 10 + Row * 88)
		Holder.Size = UDim2.fromOffset(140, 80)
		Holder.BackgroundColor3 = Color3.fromRGB(34, 34, 44)
		Holder.BorderSizePixel = 0
		Holder.Parent = Target
		return Holder
	end
	local function Padding(Parent, Sides)
		local Pad = Instance.new("UIPadding")
		Pad.PaddingLeft, Pad.PaddingRight, Pad.PaddingTop, Pad.PaddingBottom = Sides[1], Sides[2], Sides[3], Sides[4]
		Pad.Parent = Parent
	end
	local OFFSET = { UDim.new(0, 6), UDim.new(0, 6), UDim.new(0, 4), UDim.new(0, 4) }
	local SCALE = { UDim.new(0.1, 0), UDim.new(0.1, 0), UDim.new(0.1, 0), UDim.new(0.1, 0) }
	local MIXED = { UDim.new(0.1, 6), UDim.new(0, 6), UDim.new(0.1, 4), UDim.new(0, 4) }
	local UNEVEN = { UDim.new(0, 2), UDim.new(0, 10), UDim.new(0, 1), UDim.new(0, 7) }

	-- Opts: kind ("frame" | "label"), auto, size, pad, min, max, child (Vector2), text, textSize, textMax,
	-- border, uiscale, list, center (child anchored at the middle), scaleChild, parent (P's AutomaticSize)
	local function Case(Name, Opts)
		local Holder = Cell(Name)
		local Parent = Holder
		if Opts.parent then
			local P = Instance.new("Frame")
			P.Name = "P"
			P.Position = UDim2.fromOffset(4, 4)
			P.Size = Opts.parentSize or UDim2.fromOffset(0, 0)
			P.AutomaticSize = Enum.AutomaticSize[Opts.parent]
			P.BackgroundColor3 = Color3.fromRGB(200, 40, 50)
			P.BorderSizePixel = 0
			P.Parent = Holder
			Parent = P
			if Opts.nest then
				local Q = Instance.new("Frame")
				Q.Name = "Q"
				Q.Size = UDim2.fromOffset(10, 10)
				Q.BackgroundTransparency = 0.5
				Q.BorderSizePixel = 0
				Q.Parent = P
				Parent = Q
			end
			if Opts.parentList then
				local List = Instance.new("UIListLayout")
				List.Padding = UDim.new(0, 2)
				List.Parent = P
				local Other = Instance.new("Frame")
				Other.Name = "O"
				Other.Size = UDim2.fromOffset(10, 10)
				Other.LayoutOrder = 2
				Other.BorderSizePixel = 0
				Other.Parent = P
			end
			if Opts.parentPad then
				Padding(P, { UDim.new(0, 3), UDim.new(0, 5), UDim.new(), UDim.new() })
			end
		end
		local E
		if Opts.kind == "label" then
			E = Instance.new("TextLabel")
			E.FontFace = Face
			E.TextSize = Opts.textSize or 15
			E.TextColor3 = Color3.new(1, 1, 1)
			E.Text = Opts.text or "3"
			E.TextWrapped = Opts.wrap or false
			if Opts.truncate then
				E.TextTruncate = Enum.TextTruncate.AtEnd
			end
			E.BackgroundColor3 = Color3.fromRGB(60, 110, 200)
		elseif Opts.scrolling then
			E = Instance.new("ScrollingFrame")
			E.ScrollBarThickness = 0
			E.CanvasSize = UDim2.new()
			E.BackgroundColor3 = Color3.fromRGB(60, 110, 200)
		else
			E = Instance.new("Frame")
			E.BackgroundColor3 = Color3.fromRGB(60, 110, 200)
		end
		E.Name = "E"
		E.Position = Opts.position or if Opts.parent then UDim2.new() else UDim2.fromOffset(4, 4)
		E.Size = Opts.size or UDim2.fromOffset(0, 0)
		E.AutomaticSize = Enum.AutomaticSize[Opts.auto or "None"]
		E.BorderSizePixel = Opts.border or 0
		if Opts.border then
			E.BorderMode = Enum.BorderMode.Inset
			E.BorderColor3 = Color3.fromRGB(230, 200, 90)
		end
		E.Parent = Parent
		if Opts.pad then
			Padding(E, Opts.pad)
		end
		if Opts.min or Opts.max then
			local Constraint = Instance.new("UISizeConstraint")
			if Opts.maxFirst and Opts.max then
				Constraint.MaxSize = Opts.max
			end
			if Opts.min then
				Constraint.MinSize = Opts.min
			end
			if Opts.max and not Opts.maxFirst then
				Constraint.MaxSize = Opts.max
			end
			Constraint.Parent = E
			if Opts.readback then
				local Read = Instance.new("TextLabel")
				Read.Name = "R"
				Read.Position = UDim2.fromOffset(4, 60)
				Read.Size = UDim2.fromOffset(130, 16)
				Read.BackgroundTransparency = 1
				Read.TextColor3 = Color3.new(1, 1, 1)
				Read.TextSize = 10
				Read.Text = `{Constraint.MinSize.X},{Constraint.MinSize.Y}|{Constraint.MaxSize.X},{Constraint.MaxSize.Y}`
				Read.Parent = Holder
			end
		end
		if Opts.textMax or Opts.textMin then
			local Constraint = Instance.new("UITextSizeConstraint")
			if Opts.textMax then
				Constraint.MaxTextSize = Opts.textMax
			end
			if Opts.textMin then
				Constraint.MinTextSize = Opts.textMin
			end
			Constraint.Parent = E
		end
		if Opts.uiscale then
			local Scale = Instance.new("UIScale")
			Scale.Scale = Opts.uiscale
			Scale.Parent = E
		end
		if Opts.list then
			local List = Instance.new("UIListLayout")
			List.FillDirection = Enum.FillDirection.Horizontal
			List.Padding = UDim.new(0, 2)
			List.Parent = E
			for Index = 1, Opts.list do
				local Item = Instance.new("Frame")
				Item.Name = `I{Index}`
				Item.Size = UDim2.fromOffset(10, 10)
				Item.BackgroundColor3 = Color3.fromRGB(120, 230, 120)
				Item.BorderSizePixel = 0
				Item.LayoutOrder = Index
				Item.Parent = E
			end
		end
		if Opts.child then
			local C = Instance.new("Frame")
			C.Name = "C"
			C.Size = if Opts.scaleChild then UDim2.new(0.5, 0, 0.5, 0) else UDim2.fromOffset(Opts.child.X, Opts.child.Y)
			if Opts.center then
				C.AnchorPoint = Vector2.new(0.5, 0.5)
				C.Position = UDim2.fromScale(0.5, 0.5)
			end
			if Opts.childPos then
				C.Position = Opts.childPos
			end
			C.BackgroundColor3 = Color3.fromRGB(120, 230, 120)
			C.BorderSizePixel = 0
			C.Parent = E
			if Opts.extraScale then
				local S = Instance.new("Frame")
				S.Name = "S"
				S.Size = UDim2.new(0.5, 0, 0, 10)
				S.Position = UDim2.fromOffset(0, 12)
				S.BackgroundColor3 = Color3.fromRGB(230, 200, 90)
				S.BorderSizePixel = 0
				S.Parent = E
			end
		end
	end

	local INF = math.huge
	local C10 = Vector2.new(10, 10)
	local C40 = Vector2.new(40, 10)
	local SIDES = { UDim.new(0, 6), UDim.new(0, 6), UDim.new(), UDim.new() }
	local UNEVEN_X = { UDim.new(0, 2), UDim.new(0, 10), UDim.new(), UDim.new() }
	local MIXED_X = { UDim.new(0.1, 6), UDim.new(0, 6), UDim.new(), UDim.new() }
	Case("fx_min22max30_c10", { auto = "X", pad = SIDES, min = Vector2.new(22, 0), max = Vector2.new(30, INF), child = C10 })
	Case("fx_min40max30_c10", { auto = "X", pad = SIDES, min = Vector2.new(40, 0), max = Vector2.new(30, INF), child = C10 })
	Case("fx_min22max30_nopad", { auto = "X", min = Vector2.new(22, 0), max = Vector2.new(30, INF), child = C10 })
	Case("fx_max30_c40_uneven", { auto = "X", pad = UNEVEN_X, max = Vector2.new(30, INF), child = C40 })
	Case("fx_max30_c40_nopad", { auto = "X", max = Vector2.new(30, INF), child = C40 })
	Case("fx_max30_c34", { auto = "X", pad = SIDES, max = Vector2.new(30, INF), child = Vector2.new(34, 10) })
	Case("fx_max30_c40_pos5", { auto = "X", pad = SIDES, max = Vector2.new(30, INF), child = C40, childPos = UDim2.fromOffset(5, 0) })
	Case("fx_max30_c40_scale", { auto = "X", pad = SIDES, max = Vector2.new(30, INF), child = C40, extraScale = true })
	Case("fx_scalesize_min22", { auto = "X", size = UDim2.new(0.1, 0, 0, 10), pad = SIDES, min = Vector2.new(22, 0), child = C10 })
	Case("px_fx_max30_c40", { parent = "X", parentSize = UDim2.fromOffset(0, 24), auto = "X", pad = SIDES, max = Vector2.new(30, INF), child = C40 })
	Case("px_fx_max30_c40_uneven", { parent = "X", parentSize = UDim2.fromOffset(0, 24), auto = "X", pad = UNEVEN_X, max = Vector2.new(30, INF), child = C40 })
	Case("px_fx_max30_c40_nopad", { parent = "X", parentSize = UDim2.fromOffset(0, 24), auto = "X", max = Vector2.new(30, INF), child = C40 })
	Case("px_fx_max30_c24", { parent = "X", parentSize = UDim2.fromOffset(0, 24), auto = "X", pad = SIDES, max = Vector2.new(30, INF), child = Vector2.new(24, 10) })
	Case("px_fx_max30_c34", { parent = "X", parentSize = UDim2.fromOffset(0, 24), auto = "X", pad = SIDES, max = Vector2.new(30, INF), child = Vector2.new(34, 10) })
	Case("px_lx_max30", { parent = "X", parentSize = UDim2.fromOffset(0, 24), kind = "label", auto = "X", size = UDim2.fromOffset(0, 22), pad = SIDES, max = Vector2.new(30, INF), text = "33333" })
	Case("px_fn_max30_c40", { parent = "X", parentSize = UDim2.fromOffset(0, 24), size = UDim2.fromOffset(50, 10), pad = SIDES, max = Vector2.new(30, INF), child = C40 })
	Case("px_fx_min22max30_c10", { parent = "X", parentSize = UDim2.fromOffset(0, 24), auto = "X", pad = SIDES, min = Vector2.new(22, 0), max = Vector2.new(30, INF), child = C10 })
	Case("py_fy_max20_c40", { parent = "Y", parentSize = UDim2.fromOffset(24, 0), auto = "Y", size = UDim2.fromOffset(10, 0), pad = OFFSET, max = Vector2.new(INF, 20), child = Vector2.new(10, 40) })
	Case("py_fy_max20_c16", { parent = "Y", parentSize = UDim2.fromOffset(24, 0), auto = "Y", size = UDim2.fromOffset(10, 0), pad = OFFSET, max = Vector2.new(INF, 20), child = Vector2.new(10, 16) })
	Case("plist_fx_min22", { parent = "X", parentSize = UDim2.fromOffset(0, 40), parentList = true, auto = "X", pad = SIDES, min = Vector2.new(22, 0), child = C10 })
	Case("plist_fx_max30_c40", { parent = "X", parentSize = UDim2.fromOffset(0, 40), parentList = true, auto = "X", pad = SIDES, max = Vector2.new(30, INF), child = C40 })
	Case("sx_min22", { scrolling = true, auto = "X", size = UDim2.fromOffset(0, 20), pad = SIDES, min = Vector2.new(22, 0), child = C10 })
	Case("pp_fx_min22", { parent = "X", parentSize = UDim2.fromOffset(0, 24), parentPad = true, auto = "X", pad = SIDES, min = Vector2.new(22, 0), child = C10 })
	Case("px_fn_max30_s35", { parent = "X", parentSize = UDim2.fromOffset(0, 24), size = UDim2.fromOffset(35, 10), pad = SIDES, max = Vector2.new(30, INF), child = C10 })
	Case("px_fn_min22_s25", { parent = "X", parentSize = UDim2.fromOffset(0, 24), size = UDim2.fromOffset(25, 10), pad = SIDES, min = Vector2.new(22, 0), child = C10 })
	Case("px_fn_min22_s10", { parent = "X", parentSize = UDim2.fromOffset(0, 24), size = UDim2.fromOffset(10, 10), pad = SIDES, min = Vector2.new(22, 0), child = Vector2.new(4, 4) })
	Case("px_fn_pad_s10", { parent = "X", parentSize = UDim2.fromOffset(0, 24), size = UDim2.fromOffset(10, 10), pad = SIDES })
	Case("px_fn_max30_s35_nopad", { parent = "X", parentSize = UDim2.fromOffset(0, 24), size = UDim2.fromOffset(35, 10), max = Vector2.new(30, INF) })
	Case("px_fn_max30_s35_text", { parent = "X", parentSize = UDim2.fromOffset(0, 24), kind = "label", size = UDim2.fromOffset(35, 20), pad = SIDES, max = Vector2.new(30, INF) })
	Case("px_fx_min22_s25", { parent = "X", parentSize = UDim2.fromOffset(0, 24), auto = "X", size = UDim2.fromOffset(25, 10), pad = SIDES, min = Vector2.new(22, 0), child = C10 })
	Case("fx_mixedpad_max30_c40", { auto = "X", pad = MIXED_X, max = Vector2.new(30, INF), child = C40, extraScale = true })
	Case("fx_mixedpad_max30_c20", { auto = "X", pad = MIXED_X, max = Vector2.new(30, INF), child = Vector2.new(20, 10), extraScale = true })
	Case("fx_max30_c24_scale", { auto = "X", pad = SIDES, max = Vector2.new(30, INF), child = Vector2.new(24, 10), extraScale = true })
	Case("fx_min22_scale", { auto = "X", pad = SIDES, min = Vector2.new(22, 0), child = C10, extraScale = true })
	Case("lx_max30_wrap", { kind = "label", auto = "X", size = UDim2.fromOffset(0, 60), pad = SIDES, max = Vector2.new(30, INF), text = "3 3 3 3 3 3", wrap = true })
	Case("lx_max30_trunc", { kind = "label", auto = "X", size = UDim2.fromOffset(0, 22), pad = SIDES, max = Vector2.new(30, INF), text = "333333", truncate = true })
	Case("lx_max30_trunc_nopad", { kind = "label", auto = "X", size = UDim2.fromOffset(0, 22), max = Vector2.new(30, INF), text = "333333", truncate = true })
	Case("lx_max42_trunc", { kind = "label", auto = "X", size = UDim2.fromOffset(0, 22), pad = SIDES, max = Vector2.new(42, INF), text = "333333", truncate = true })
	Case("fx_min40max30", { auto = "X", pad = SIDES, min = Vector2.new(40, 0), max = Vector2.new(30, INF), child = C10, readback = true })
	Case("fx_max30min40", { auto = "X", pad = SIDES, min = Vector2.new(40, 0), max = Vector2.new(30, INF), child = C10, readback = true, maxFirst = true })
	Case("fx_min40max30_nopad", { auto = "X", min = Vector2.new(40, 0), max = Vector2.new(30, INF), child = C10, readback = true })
	Case("fn_min40max30_s10", { size = UDim2.fromOffset(10, 10), min = Vector2.new(40, 0), max = Vector2.new(30, INF), readback = true })
	Case("fn_min40max30_s35", { size = UDim2.fromOffset(35, 10), min = Vector2.new(40, 0), max = Vector2.new(30, INF), readback = true })
	Case("fn_min40max30_s50", { size = UDim2.fromOffset(50, 10), min = Vector2.new(40, 0), max = Vector2.new(30, INF), readback = true })
	Case("fn_max30min40_s50", { size = UDim2.fromOffset(50, 10), min = Vector2.new(40, 0), max = Vector2.new(30, INF), readback = true, maxFirst = true })
	Case("fx_min40max30_c50_nopad", { auto = "X", min = Vector2.new(40, 0), max = Vector2.new(30, INF), child = Vector2.new(50, 10), readback = true })
	Case("fx_min40max30_c35_nopad", { auto = "X", min = Vector2.new(40, 0), max = Vector2.new(30, INF), child = Vector2.new(35, 10), readback = true })
	Case("px_fx_size50_max30_c10", { parent = "X", parentSize = UDim2.fromOffset(0, 24), auto = "X", size = UDim2.fromOffset(50, 10), pad = SIDES, max = Vector2.new(30, INF), child = C10 })
	Case("fx_size50_max30_scale", { auto = "X", size = UDim2.fromOffset(50, 10), pad = SIDES, max = Vector2.new(30, INF), child = C10, extraScale = true })
	Case("px_fx_size40_max30_c10", { parent = "X", parentSize = UDim2.fromOffset(0, 24), auto = "X", size = UDim2.fromOffset(40, 10), pad = SIDES, max = Vector2.new(30, INF), child = C10 })
	Case("px_fn_max30_s50_c40_nopad", { parent = "X", parentSize = UDim2.fromOffset(0, 24), size = UDim2.fromOffset(50, 10), max = Vector2.new(30, INF), child = C40 })
	Case("px_fn_max30_s50_c20", { parent = "X", parentSize = UDim2.fromOffset(0, 24), size = UDim2.fromOffset(50, 10), pad = SIDES, max = Vector2.new(30, INF), child = Vector2.new(20, 10) })
	Case("px_fn_min22_s10_empty", { parent = "X", parentSize = UDim2.fromOffset(0, 24), size = UDim2.fromOffset(10, 10), pad = SIDES, min = Vector2.new(22, 0) })
	Case("px_fn_min22_s20_c4", { parent = "X", parentSize = UDim2.fromOffset(0, 24), size = UDim2.fromOffset(20, 10), pad = SIDES, min = Vector2.new(22, 0), child = Vector2.new(4, 4) })
	Case("px_fn_min22_s20_c4_nopad", { parent = "X", parentSize = UDim2.fromOffset(0, 24), size = UDim2.fromOffset(20, 10), min = Vector2.new(22, 0), child = Vector2.new(4, 4) })
	Case("px_fn_min40_s50_c4", { parent = "X", parentSize = UDim2.fromOffset(0, 24), size = UDim2.fromOffset(50, 10), pad = SIDES, min = Vector2.new(40, 0), child = Vector2.new(4, 4) })
	Case("fn_minmax_peraxis", { size = UDim2.fromOffset(10, 10), min = Vector2.new(40, 25), max = Vector2.new(30, 100) })
	Case("fn_minmax_peraxis2", { size = UDim2.fromOffset(50, 50), min = Vector2.new(40, 0), max = Vector2.new(30, 20) })
	Case("fn_max30_s50_scale", { size = UDim2.fromOffset(50, 10), pad = SIDES, max = Vector2.new(30, INF), child = C10, extraScale = true })
	Case("px_q_fn_max30", { parent = "X", parentSize = UDim2.fromOffset(0, 24), nest = true, size = UDim2.fromOffset(50, 10), pad = SIDES, max = Vector2.new(30, INF), child = C40 })
	Case("px_fx_min22_c10_pos", { parent = "X", parentSize = UDim2.fromOffset(0, 24), auto = "X", pad = SIDES, min = Vector2.new(22, 0), child = C10, position = UDim2.fromOffset(7, 0) })
	Case("px_fx_max30_c24_pos", { parent = "X", parentSize = UDim2.fromOffset(0, 24), auto = "X", pad = SIDES, max = Vector2.new(30, INF), child = Vector2.new(24, 10), position = UDim2.fromOffset(7, 0) })
	Case("py_fn_min30_s10_c4", { parent = "Y", parentSize = UDim2.fromOffset(24, 0), size = UDim2.fromOffset(10, 10), pad = { UDim.new(), UDim.new(), UDim.new(0, 4), UDim.new(0, 4) }, min = Vector2.new(0, 30), child = Vector2.new(4, 4) })

	-- Two wider scenes on the last row: a row of AutomaticSize X buttons with MinSize (125, 35), 16 + 16 px of
	-- padding and a centring list around an automatic label, and AutomaticSize Y rows with MinSize.Y 76 and
	-- 10 + 10 px of padding holding a 56 px well beside 60 or 44 px of content
	Count = 64
	do
		local Row = Cell("buttons")
		Row.Size = UDim2.fromOffset(430, 80)
		local P = Instance.new("Frame")
		P.Name = "P"
		P.AutomaticSize = Enum.AutomaticSize.XY
		P.Position = UDim2.fromOffset(4, 4)
		P.BackgroundTransparency = 1
		P.Parent = Row
		local List = Instance.new("UIListLayout")
		List.FillDirection = Enum.FillDirection.Horizontal
		List.HorizontalAlignment = Enum.HorizontalAlignment.Center
		List.VerticalAlignment = Enum.VerticalAlignment.Center
		List.Padding = UDim.new(0, 10)
		List.SortOrder = Enum.SortOrder.LayoutOrder
		List.Parent = P
		for Index, Text in { "Open", "Change mode" } do
			local Button = Instance.new("TextButton")
			Button.Name = `B{Index}`
			Button.AutomaticSize = Enum.AutomaticSize.X
			Button.Size = UDim2.fromOffset(0, 35)
			Button.Text = ""
			Button.LayoutOrder = Index
			Button.BackgroundColor3 = Color3.fromRGB(60, 110, 200)
			Button.Parent = P
			local Constraint = Instance.new("UISizeConstraint")
			Constraint.MinSize = Vector2.new(125, 35)
			Constraint.Parent = Button
			Padding(Button, { UDim.new(0, 16), UDim.new(0, 16), UDim.new(), UDim.new() })
			local Inner = Instance.new("UIListLayout")
			Inner.FillDirection = Enum.FillDirection.Horizontal
			Inner.HorizontalAlignment = Enum.HorizontalAlignment.Center
			Inner.VerticalAlignment = Enum.VerticalAlignment.Center
			Inner.Parent = Button
			local Label = Instance.new("TextLabel")
			Label.Name = "L"
			Label.AutomaticSize = Enum.AutomaticSize.XY
			Label.BackgroundTransparency = 1
			Label.FontFace = Font.new("rbxasset://fonts/families/FredokaOne.json")
			Label.TextSize = 17
			Label.TextColor3 = Color3.new(1, 1, 1)
			Label.Text = Text
			Label.Parent = Button
		end
	end
	Count = 67
	for Index, Held in { 60, 44 } do
		local Holder = Cell(`rows{Index}`)
		Holder.Size = UDim2.fromOffset(140, 120)
		local Row = Instance.new("Frame")
		Row.Name = "E"
		Row.AutomaticSize = Enum.AutomaticSize.Y
		Row.Position = UDim2.fromOffset(4, 4)
		Row.Size = UDim2.fromOffset(130, 0)
		Row.BackgroundColor3 = Color3.fromRGB(60, 110, 200)
		Row.BorderSizePixel = 0
		Row.Parent = Holder
		local Constraint = Instance.new("UISizeConstraint")
		Constraint.MinSize = Vector2.new(0, 76)
		Constraint.Parent = Row
		Padding(Row, { UDim.new(0, 12), UDim.new(0, 12), UDim.new(0, 10), UDim.new(0, 10) })
		local Well = Instance.new("Frame")
		Well.Name = "C"
		Well.Size = UDim2.fromOffset(56, 56)
		Well.BorderSizePixel = 0
		Well.Parent = Row
		local Content = Instance.new("Frame")
		Content.Name = "S"
		Content.Position = UDim2.fromOffset(62, 0)
		Content.Size = UDim2.fromOffset(40, Held)
		Content.BackgroundColor3 = Color3.fromRGB(230, 200, 90)
		Content.BorderSizePixel = 0
		Content.Parent = Row
	end
end
