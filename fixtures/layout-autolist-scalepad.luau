-- AutomaticSize Y vertical lists with scale UIPadding and a scale list Padding (tools/studio/layout-probe-autolistgap2
-- .luau as a fixture, plus a sweep): F (AutomaticSize Y, in a 190 x 150 holder) holds a list, a (0.5, 0, 0, 30) item
-- and a 20 x 20 item. Swept: F's Size (base 0, 20 px, 0.2 scale, 0.2 scale + 10 px), the padding (scale and offset
-- tops, scale bottoms) and the gap (0.1 scale, 8 px). Holders overlap (rows 75 px apart); only the layout is compared.
--   node cli/rur.mjs compare --fixture fixtures/layout-autolist-scalepad.luau --viewport 1200x900 --time 0.2 --background 20,20,26
return function(Target)
	local Count = 0
	local function Case(Name, Options)
		Count += 1
		local Holder = Instance.new("Frame")
		Holder.Name = Name
		Holder.Position = UDim2.fromOffset(((Count - 1) % 6) * 200, ((Count - 1) // 6) * 75)
		Holder.Size = UDim2.fromOffset(190, 150)
		Holder.BackgroundTransparency = 1
		Holder.Parent = Target
		local H = Options.horizontal
		local F = Instance.new("Frame")
		F.Name = "F"
		F.AutomaticSize = if H then Enum.AutomaticSize.X else Enum.AutomaticSize.Y
		F.Size = Options.size or (if H then UDim2.new(0, 0, 1, 0) else UDim2.new(1, 0, 0, 0))
		F.BackgroundTransparency = 0.6
		F.BorderSizePixel = 0
		F.Parent = Holder
		if Options.pad then
			local Pad = Instance.new("UIPadding")
			if H then
				Pad.PaddingLeft = Options.pad[1]
				Pad.PaddingRight = Options.pad[2]
			else
				Pad.PaddingTop = Options.pad[1]
				Pad.PaddingBottom = Options.pad[2]
			end
			Pad.Parent = F
		end
		if Options.max then
			local Constraint = Instance.new("UISizeConstraint")
			Constraint.MaxSize = if H then Vector2.new(Options.max, math.huge) else Vector2.new(math.huge, Options.max)
			Constraint.Parent = F
		end
		local List = Instance.new("UIListLayout")
		List.Padding = Options.gap or UDim.new(0.1, 0)
		List.SortOrder = Enum.SortOrder.LayoutOrder
		if H then
			List.FillDirection = Enum.FillDirection.Horizontal
		end
		List.Parent = F
		local A = Instance.new("Frame")
		A.Name = "a"
		A.Size = Options.first or (if H then UDim2.new(0, 30, 0.5, 0) else UDim2.new(0.5, 0, 0, 30))
		A.LayoutOrder = 1
		A.BorderSizePixel = 0
		A.Parent = F
		local B = Instance.new("Frame")
		B.Name = "b"
		B.Size = UDim2.fromOffset(20, 20)
		B.LayoutOrder = 2
		B.BorderSizePixel = 0
		B.Parent = F
	end
	-- layout-probe-autolistgap2
	Case("listpad", { pad = { UDim.new(0.1, 4), UDim.new(0, 6) } })
	Case("noPad", {})
	Case("offsetPad", { pad = { UDim.new(0, 4), UDim.new(0, 6) } })
	Case("scalePadOnly", { pad = { UDim.new(0.1, 0), UDim.new(0, 0) } })
	Case("offsetItem", { pad = { UDim.new(0.1, 4), UDim.new(0, 6) }, first = UDim2.fromOffset(75, 30) })
	Case("noPadOffsetItem", { first = UDim2.fromOffset(75, 30) })
	Case("base20", { size = UDim2.new(1, 0, 0, 20) })
	Case("base20Pad", { size = UDim2.new(1, 0, 0, 20), pad = { UDim.new(0.1, 4), UDim.new(0, 6) } })
	Case("scaleBase", { size = UDim2.new(1, 0, 0.2, 0) })
	Case("scaleBasePad", { size = UDim2.new(1, 0, 0.2, 0), pad = { UDim.new(0.1, 4), UDim.new(0, 6) } })
	Case("offsetGap", { gap = UDim.new(0, 8) })
	Case("widthOffset", { size = UDim2.new(0, 150, 0, 0) })
	-- the sweep
	local Bases = {
		{ "b0", UDim2.new(1, 0, 0, 0) },
		{ "b20", UDim2.new(1, 0, 0, 20) },
		{ "k2", UDim2.new(1, 0, 0.2, 0) },
		{ "k2o10", UDim2.new(1, 0, 0.2, 10) },
	}
	local Pads = {
		{ "t10", { UDim.new(0.1, 0), UDim.new() } },
		{ "t10o4b6", { UDim.new(0.1, 4), UDim.new(0, 6) } },
		{ "t20", { UDim.new(0.2, 0), UDim.new() } },
		{ "b10", { UDim.new(), UDim.new(0.1, 0) } },
		{ "t10b10", { UDim.new(0.1, 0), UDim.new(0.1, 0) } },
		{ "o4b6", { UDim.new(0, 4), UDim.new(0, 6) } },
	}
	local Gaps = { { "g10", UDim.new(0.1, 0) }, { "g8", UDim.new(0, 8) } }
	for _, Base in Bases do
		for _, Pad in Pads do
			for _, Gap in Gaps do
				Case(`{Base[1]}_{Pad[1]}_{Gap[1]}`, { size = Base[2], pad = Pad[2], gap = Gap[2] })
			end
		end
	end
	-- small offset paddings (the second pass's gap), a MaxSize, horizontal lists
	Case("b0_o2b2_g10", { pad = { UDim.new(0, 2), UDim.new(0, 2) } })
	Case("b0_o1b1_g10", { pad = { UDim.new(0, 1), UDim.new(0, 1) } })
	Case("b20_o2b2_g10", { size = UDim2.new(1, 0, 0, 20), pad = { UDim.new(0, 2), UDim.new(0, 2) } })
	Case("k2_max40_g10", { size = UDim2.new(1, 0, 0.2, 0), max = 40 })
	Case("k2_t10o4b6_max40_g10", { size = UDim2.new(1, 0, 0.2, 0), pad = { UDim.new(0.1, 4), UDim.new(0, 6) }, max = 40 })
	Case("k5_max20_g10", { size = UDim2.new(1, 0, 0.5, 0), max = 20 })
	Case("hx_b0_l10_g10", { horizontal = true, pad = { UDim.new(0.1, 0), UDim.new() } })
	Case("hx_k2_l10o4r6_g10", { horizontal = true, size = UDim2.new(0.2, 0, 1, 0), pad = { UDim.new(0.1, 4), UDim.new(0, 6) } })
	Case("hx_b0_o2r2_g10", { horizontal = true, pad = { UDim.new(0, 2), UDim.new(0, 2) } })
	Case("hx_k2_g10", { horizontal = true, size = UDim2.new(0.2, 0, 1, 0) })
end
