-- AutomaticSize objects whose Size is above their UISizeConstraint's MaxSize, continued from
-- layout-autosize-constraint2.luau (cell fx_size50_max30_scale): what box their scale children are sized against.
-- Each cell holds one automatic Frame E holding an offset child C and a scale child S (or SY on the Y axis), swept
-- over E's Size, MaxSize, MinSize, the padding, C's size and S's scale.
--   node cli/rur.mjs compare --fixture fixtures/layout-autosize-constraint3.luau --viewport 1200x900 --time 0.2 --background 20,20,26
return function(Target)
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
	local INF = math.huge
	-- Opts: axis ("X" | "Y"), size (offset on the axis), max, min, pad ({ near, far } px), child (px on the axis),
	-- scale (S's scale on the axis), scaleSize (E's Size scale on the axis, against the 140 x 80 cell), childPos (C's
-- Position scale on the axis), noScale (no S)
	local function Case(Name, Opts)
		local Holder = Cell(Name)
		local X = Opts.axis ~= "Y"
		local E = Instance.new("Frame")
		E.Name = "E"
		E.Position = UDim2.fromOffset(4, 4)
		local Along = Opts.size or 0
		local AlongScale = Opts.scaleSize or 0
		E.Size = if X then UDim2.new(AlongScale, Along, 0, 10) else UDim2.new(0, 10, AlongScale, Along)
		E.AutomaticSize = if X then Enum.AutomaticSize.X else Enum.AutomaticSize.Y
		E.BackgroundColor3 = Color3.fromRGB(200, 40, 50)
		E.BorderSizePixel = 0
		E.Parent = Holder
		if Opts.pad then
			local Pad = Instance.new("UIPadding")
			if X then
				Pad.PaddingLeft, Pad.PaddingRight = UDim.new(0, Opts.pad[1]), UDim.new(0, Opts.pad[2])
			else
				Pad.PaddingTop, Pad.PaddingBottom = UDim.new(0, Opts.pad[1]), UDim.new(0, Opts.pad[2])
			end
			Pad.Parent = E
		end
		if Opts.max or Opts.min then
			local Constraint = Instance.new("UISizeConstraint")
			if X then
				Constraint.MinSize = Vector2.new(Opts.min or 0, 0)
				Constraint.MaxSize = Vector2.new(Opts.max or INF, INF)
			else
				Constraint.MinSize = Vector2.new(0, Opts.min or 0)
				Constraint.MaxSize = Vector2.new(INF, Opts.max or INF)
			end
			Constraint.Parent = E
		end
		local C = Instance.new("Frame")
		C.Name = "C"
		local ChildSize = Opts.child or 10
		C.Size = if X then UDim2.fromOffset(ChildSize, 10) else UDim2.fromOffset(10, ChildSize)
		if Opts.childPos then
			C.Position = if X then UDim2.fromScale(Opts.childPos, 0) else UDim2.fromScale(0, Opts.childPos)
		end
		C.BackgroundColor3 = Color3.fromRGB(120, 230, 120)
		C.BorderSizePixel = 0
		C.Parent = E
		if Opts.noScale then
			return
		end
		local S = Instance.new("Frame")
		S.Name = "S"
		local Scale = Opts.scale or 0.5
		S.Size = if X then UDim2.new(Scale, 0, 0, 10) else UDim2.new(0, 10, Scale, 0)
		S.Position = if X then UDim2.fromOffset(0, 12) else UDim2.fromOffset(12, 0)
		S.BackgroundColor3 = Color3.fromRGB(230, 200, 90)
		S.BorderSizePixel = 0
		S.Parent = E
	end
	local SIDES = { 6, 6 }
	-- Size swept past MaxSize 30 with 6 + 6 px of padding, three child sizes
	for _, Size in { 20, 30, 40, 50, 60, 80 } do
		for _, Child in { 10, 24, 40 } do
			Case(`x_s{Size}_max30_c{Child}`, { size = Size, max = 30, pad = SIDES, child = Child })
		end
	end
	-- no padding, uneven padding
	for _, Size in { 40, 50, 80 } do
		Case(`x_s{Size}_max30_c10_nopad`, { size = Size, max = 30, child = 10 })
		Case(`x_s{Size}_max30_c10_uneven`, { size = Size, max = 30, pad = { 2, 10 }, child = 10 })
	end
	-- S's scale
	for _, Scale in { 0.25, 1, 0.1 } do
		Case(`x_s50_max30_c10_k{Scale * 100}`, { size = 50, max = 30, pad = SIDES, child = 10, scale = Scale })
		Case(`x_s80_max30_c10_k{Scale * 100}`, { size = 80, max = 30, pad = SIDES, child = 10, scale = Scale })
	end
	-- other caps and a MinSize beside the cap
	for _, Size in { 50, 60, 80 } do
		Case(`x_s{Size}_max40_c10`, { size = Size, max = 40, pad = SIDES, child = 10 })
		Case(`x_s{Size}_min22max30_c10`, { size = Size, min = 22, max = 30, pad = SIDES, child = 10 })
	end
	-- E's Size with a scale part (0.5 of the 140 px cell = 70)
	Case("x_k50_max30_c10", { scaleSize = 0.5, max = 30, pad = SIDES, child = 10 })
	Case("x_k50_max30_c40", { scaleSize = 0.5, max = 30, pad = SIDES, child = 40 })
	-- the same on Y (4 + 4 px)
	for _, Size in { 30, 50, 80 } do
		for _, Child in { 10, 40 } do
			Case(`y_s{Size}_max30_c{Child}`, { axis = "Y", size = Size, max = 30, pad = { 4, 4 }, child = Child })
		end
	end
	-- no constraint: a scale child whose first-pass size plus the padding passes the Size
	for _, Size in { 20, 30, 40 } do
		Case(`n_s{Size}_k100_c4`, { size = Size, pad = SIDES, child = 4, scale = 1 })
	end
	Case("n_s20_k75_c4", { size = 20, pad = SIDES, child = 4, scale = 0.75 })
	Case("n_s20_k100_c4_nopad", { size = 20, child = 4, scale = 1 })
	Case("n_s20_k100_c4_uneven", { size = 20, pad = { 2, 10 }, child = 4, scale = 1 })
	Case("n_k10_k100_c4", { scaleSize = 0.1, pad = SIDES, child = 4, scale = 1 })
	Case("n_k10o10_k100_c4", { scaleSize = 0.1, size = 10, pad = SIDES, child = 4, scale = 1 })
	Case("m_k25o20_max30_c10", { scaleSize = 0.25, size = 20, max = 30, pad = SIDES, child = 10 })
	Case("m_k25o20_max30_c10_k100", { scaleSize = 0.25, size = 20, max = 30, pad = SIDES, child = 10, scale = 1 })
	Case("y_n_s20_k100_c4", { axis = "Y", size = 20, pad = { 4, 4 }, child = 4, scale = 1 })
	Case("y_n_k25_k100_c4", { axis = "Y", scaleSize = 0.25, pad = { 4, 4 }, child = 4, scale = 1 })
	-- a scale-positioned child (no S): does the first pass place it against the same basis?
	Case("p_s20_pos50_c10", { size = 20, pad = SIDES, child = 10, childPos = 0.5, noScale = true })
	Case("p_s40_pos50_c10", { size = 40, pad = SIDES, child = 10, childPos = 0.5, noScale = true })
	Case("p_s40_pos100_c10", { size = 40, pad = SIDES, child = 10, childPos = 1, noScale = true })
	Case("p_s40_pos50_c10_nopad", { size = 40, child = 10, childPos = 0.5, noScale = true })
	Case("p_k10_pos50_c10", { scaleSize = 0.1, pad = SIDES, child = 10, childPos = 0.5, noScale = true })
	Case("p_s50_max30_pos50_c10", { size = 50, max = 30, pad = SIDES, child = 10, childPos = 0.5, noScale = true })
	Case("p_s20_pos50_c10_k100", { size = 20, pad = SIDES, child = 10, childPos = 0.5, scale = 1 })
	Case("y_p_s20_pos50_c10", { axis = "Y", size = 20, pad = { 4, 4 }, child = 10, childPos = 0.5, noScale = true })
end
