-- AutomaticSize with UIPadding and UISizeConstraint: whether MinSize / MaxSize bound the content inside the padding
-- or the padded size, on each axis, with offset, scale and mixed padding, a border inset, a UIScale, a list, a
-- UITextSizeConstraint beside the size constraint, and for comparison objects without AutomaticSize. Each cell holds
-- one object E (a Frame holding C, or a TextLabel), some inside an AutomaticSize parent P.
--   node cli/rur.mjs compare --fixture fixtures/layout-autosize-constraint.luau --viewport 1200x900 --time 0.2 --background 20,20,26
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
		end
		local E
		if Opts.kind == "label" then
			E = Instance.new("TextLabel")
			E.FontFace = Face
			E.TextSize = Opts.textSize or 15
			E.TextColor3 = Color3.new(1, 1, 1)
			E.Text = Opts.text or "3"
			E.BackgroundColor3 = Color3.fromRGB(60, 110, 200)
		else
			E = Instance.new("Frame")
			E.BackgroundColor3 = Color3.fromRGB(60, 110, 200)
		end
		E.Name = "E"
		E.Position = if Opts.parent then UDim2.new() else UDim2.fromOffset(4, 4)
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
			if Opts.min then
				Constraint.MinSize = Opts.min
			end
			if Opts.max then
				Constraint.MaxSize = Opts.max
			end
			Constraint.Parent = E
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
			C.BackgroundColor3 = Color3.fromRGB(120, 230, 120)
			C.BorderSizePixel = 0
			C.Parent = E
		end
	end

	local INF = math.huge
	local C10 = Vector2.new(10, 10)
	-- AutomaticSize X frames, 6 + 6 px padding
	Case("fx_min22_c10", { auto = "X", pad = OFFSET, min = Vector2.new(22, 0), child = C10 })
	Case("fx_min22_c16", { auto = "X", pad = OFFSET, min = Vector2.new(22, 0), child = Vector2.new(16, 10) })
	Case("fx_min22_c30", { auto = "X", pad = OFFSET, min = Vector2.new(22, 0), child = Vector2.new(30, 10) })
	Case("fx_max30_c40", { auto = "X", pad = OFFSET, max = Vector2.new(30, INF), child = Vector2.new(40, 10) })
	Case("fx_max30_c24", { auto = "X", pad = OFFSET, max = Vector2.new(30, INF), child = Vector2.new(24, 10) })
	Case("fx_max30_c10", { auto = "X", pad = OFFSET, max = Vector2.new(30, INF), child = C10 })
	Case("fx_min22max30_c40", { auto = "X", pad = OFFSET, min = Vector2.new(22, 0), max = Vector2.new(30, INF), child = Vector2.new(40, 10) })
	Case("fx_size30_min40_c10", { auto = "X", size = UDim2.fromOffset(30, 10), pad = OFFSET, min = Vector2.new(40, 0), child = C10 })
	Case("fx_size50_max30_c10", { auto = "X", size = UDim2.fromOffset(50, 10), pad = OFFSET, max = Vector2.new(30, INF), child = C10 })
	Case("fx_size40_min22_c10", { auto = "X", size = UDim2.fromOffset(40, 10), pad = OFFSET, min = Vector2.new(22, 0), child = C10 })
	Case("fx_min22_uneven", { auto = "X", pad = UNEVEN, min = Vector2.new(22, 0), child = C10 })
	Case("fx_min22_nopad", { auto = "X", min = Vector2.new(22, 0), child = C10 })
	Case("fx_minY_c10", { auto = "X", size = UDim2.fromOffset(0, 10), pad = OFFSET, min = Vector2.new(22, 30), child = C10 })
	Case("fx_maxY_c10", { auto = "X", size = UDim2.fromOffset(0, 40), pad = OFFSET, max = Vector2.new(30, 20), child = C10 })
	-- AutomaticSize Y frames, 4 + 4 px padding
	Case("fy_min30_c10", { auto = "Y", size = UDim2.fromOffset(10, 0), pad = OFFSET, min = Vector2.new(0, 30), child = C10 })
	Case("fy_min30_c26", { auto = "Y", size = UDim2.fromOffset(10, 0), pad = OFFSET, min = Vector2.new(0, 30), child = Vector2.new(10, 26) })
	Case("fy_max20_c40", { auto = "Y", size = UDim2.fromOffset(10, 0), pad = OFFSET, max = Vector2.new(INF, 20), child = Vector2.new(10, 40) })
	Case("fy_max20_c16", { auto = "Y", size = UDim2.fromOffset(10, 0), pad = OFFSET, max = Vector2.new(INF, 20), child = Vector2.new(10, 16) })
	Case("fy_minX_c10", { auto = "Y", size = UDim2.fromOffset(10, 0), pad = OFFSET, min = Vector2.new(22, 30), child = C10 })
	-- AutomaticSize XY frames
	Case("fxy_min", { auto = "XY", pad = OFFSET, min = Vector2.new(22, 30), child = C10 })
	Case("fxy_max", { auto = "XY", pad = OFFSET, max = Vector2.new(30, 20), child = Vector2.new(40, 40) })
	-- No AutomaticSize
	Case("fn_min", { size = UDim2.fromOffset(10, 10), pad = OFFSET, min = Vector2.new(22, 30), child = C10 })
	Case("fn_max", { size = UDim2.fromOffset(50, 50), pad = OFFSET, max = Vector2.new(30, 20), child = C10 })
	-- Scale and mixed padding, a border inset, a UIScale
	Case("fx_scalepad_min22", { auto = "X", pad = SCALE, min = Vector2.new(22, 0), child = C10 })
	Case("fx_scalepad_max30", { auto = "X", pad = SCALE, max = Vector2.new(30, INF), child = Vector2.new(40, 10) })
	Case("fx_mixedpad_min22", { auto = "X", pad = MIXED, min = Vector2.new(22, 0), child = C10 })
	Case("fy_scalepad_min30", { auto = "Y", size = UDim2.fromOffset(10, 0), pad = SCALE, min = Vector2.new(0, 30), child = C10 })
	Case("fxy_mixedpad_min", { auto = "XY", pad = MIXED, min = Vector2.new(22, 30), child = C10 })
	Case("fn_scalepad_min", { size = UDim2.fromOffset(10, 10), pad = SCALE, min = Vector2.new(22, 30), child = C10 })
	Case("fx_border_min22", { auto = "X", border = 4, min = Vector2.new(22, 0), child = C10 })
	Case("fx_uiscale_min22", { auto = "X", pad = OFFSET, min = Vector2.new(22, 0), child = C10, uiscale = 1.5 })
	-- Where children sit in the widened object, and a list
	Case("fx_center_min40", { auto = "X", size = UDim2.fromOffset(0, 30), pad = OFFSET, min = Vector2.new(40, 0), child = C10, center = true })
	Case("fx_scalechild_min40", { auto = "X", size = UDim2.fromOffset(0, 30), pad = OFFSET, min = Vector2.new(40, 0), child = C10, scaleChild = true })
	Case("fx_list_min40", { auto = "X", size = UDim2.fromOffset(0, 14), pad = OFFSET, min = Vector2.new(40, 0), list = 2 })
	Case("fx_list_max15", { auto = "X", size = UDim2.fromOffset(0, 14), pad = OFFSET, max = Vector2.new(15, INF), list = 2 })
	-- AutomaticSize parents
	Case("px_fx_min22", { parent = "X", parentSize = UDim2.fromOffset(0, 24), auto = "X", pad = OFFSET, min = Vector2.new(22, 0), child = C10 })
	Case("py_fy_min30", { parent = "Y", parentSize = UDim2.fromOffset(24, 0), auto = "Y", size = UDim2.fromOffset(10, 0), pad = OFFSET, min = Vector2.new(0, 30), child = C10 })
	Case("pxy_fxy_max", { parent = "XY", auto = "XY", pad = OFFSET, max = Vector2.new(30, 20), child = Vector2.new(40, 40) })
	-- Text labels: AutomaticSize X, 6 + 6 px padding (lx_min22 is the reported badge without its parent)
	Case("lx_min22", { kind = "label", auto = "X", size = UDim2.fromOffset(0, 22), pad = OFFSET, min = Vector2.new(22, 0) })
	Case("lx_min22_33", { kind = "label", auto = "X", size = UDim2.fromOffset(0, 22), pad = OFFSET, min = Vector2.new(22, 0), text = "33" })
	Case("lx_min22_333", { kind = "label", auto = "X", size = UDim2.fromOffset(0, 22), pad = OFFSET, min = Vector2.new(22, 0), text = "333" })
	Case("lx_max30_33", { kind = "label", auto = "X", size = UDim2.fromOffset(0, 22), pad = OFFSET, max = Vector2.new(30, INF), text = "33" })
	Case("lx_max30_33333", { kind = "label", auto = "X", size = UDim2.fromOffset(0, 22), pad = OFFSET, max = Vector2.new(30, INF), text = "33333" })
	Case("lx_scalepad_min22", { kind = "label", auto = "X", size = UDim2.fromOffset(0, 22), pad = SCALE, min = Vector2.new(22, 0) })
	Case("lx_scalepad_max30", { kind = "label", auto = "X", size = UDim2.fromOffset(0, 22), pad = SCALE, max = Vector2.new(30, INF), text = "33333" })
	Case("ly_min30", { kind = "label", auto = "Y", size = UDim2.fromOffset(60, 0), pad = OFFSET, min = Vector2.new(0, 30) })
	Case("ly_max10", { kind = "label", auto = "Y", size = UDim2.fromOffset(60, 0), pad = OFFSET, max = Vector2.new(INF, 10) })
	Case("lxy_min", { kind = "label", auto = "XY", pad = OFFSET, min = Vector2.new(22, 30) })
	Case("lxy_max", { kind = "label", auto = "XY", pad = OFFSET, max = Vector2.new(30, 10), text = "33333" })
	Case("ln_min", { kind = "label", size = UDim2.fromOffset(10, 10), pad = OFFSET, min = Vector2.new(22, 30) })
	Case("ln_max", { kind = "label", size = UDim2.fromOffset(60, 40), pad = OFFSET, max = Vector2.new(30, 20), text = "33333" })
	-- UITextSizeConstraint beside the size constraint
	Case("lx_text_min22", { kind = "label", auto = "X", size = UDim2.fromOffset(0, 22), pad = OFFSET, min = Vector2.new(22, 0), textSize = 30, textMax = 15 })
	Case("lx_text_max30", { kind = "label", auto = "X", size = UDim2.fromOffset(0, 22), pad = OFFSET, max = Vector2.new(30, INF), textSize = 30, textMax = 15, text = "33333" })
	Case("lxy_textmin_min", { kind = "label", auto = "XY", pad = OFFSET, min = Vector2.new(22, 30), textSize = 8, textMin = 15 })
	-- The reported badge: an AutomaticSize X frame around the label
	Case("badge", { parent = "X", parentSize = UDim2.fromOffset(0, 24), kind = "label", auto = "X", size = UDim2.fromOffset(0, 22), pad = { UDim.new(0, 6), UDim.new(0, 6), UDim.new(), UDim.new() }, min = Vector2.new(22, 0) })
	Case("badge_333", { parent = "X", parentSize = UDim2.fromOffset(0, 24), kind = "label", auto = "X", size = UDim2.fromOffset(0, 22), pad = { UDim.new(0, 6), UDim.new(0, 6), UDim.new(), UDim.new() }, min = Vector2.new(22, 0), text = "333" })
end
