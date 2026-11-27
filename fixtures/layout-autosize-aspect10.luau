-- AutomaticSize XY frames with a UIAspectRatioConstraint whose top and bottom padding push the ratio's width past what
-- they hold, continued from layout-autosize-aspect9.luau: a (1, 1) frame F beside a box B wider or narrower than the
-- base, boxes alone, side padding, bases wider than their fitted size and bases of 0 x 0 or 100 x 0 with a wrapped
-- label T. Each cell holds one automatic frame H.
--   node cli/rur.mjs compare --fixture fixtures/layout-autosize-aspect10.luau --viewport 2400x880 --time 0.2 --background 20,20,26
return function(Target)
	local Count = 0
	local Face = Font.new("rbxasset://fonts/families/SourceSansPro.json")
	local function Frame(Name, Parent, Size, Colour)
		local F = Instance.new("Frame")
		F.Name = Name
		F.Size = Size
		F.BackgroundColor3 = Colour or Color3.fromRGB(60, 60, 80)
		F.BackgroundTransparency = 0.4
		F.BorderSizePixel = 0
		F.Parent = Parent
		return F
	end
	-- Opts: size, ratio, pad ({ left, right, top, bottom }), full (F's Size), box ({ w, h }), text (T's Size)
	local function Case(Name, Opts)
		local Cell = Instance.new("Frame")
		Cell.Name = Name
		Cell.Position = UDim2.fromOffset((Count % 8) * 300, (Count // 8) * 220)
		Cell.Size = UDim2.fromOffset(296, 216)
		Cell.BackgroundColor3 = Color3.fromRGB(30, 30, 38)
		Cell.BorderSizePixel = 0
		Cell.Parent = Target
		Count += 1
		local H = Frame("H", Cell, Opts.size or UDim2.fromOffset(200, 60), Color3.fromRGB(200, 40, 50))
		H.AutomaticSize = Enum.AutomaticSize.XY
		H.Position = UDim2.fromOffset(6, 6)
		local Aspect = Instance.new("UIAspectRatioConstraint")
		Aspect.AspectRatio = Opts.ratio or 2
		Aspect.Parent = H
		local P = Opts.pad or { 0, 0, 0, 0 }
		if P[1] ~= 0 or P[2] ~= 0 or P[3] ~= 0 or P[4] ~= 0 then
			local Pad = Instance.new("UIPadding")
			Pad.PaddingLeft, Pad.PaddingRight = UDim.new(0, P[1]), UDim.new(0, P[2])
			Pad.PaddingTop, Pad.PaddingBottom = UDim.new(0, P[3]), UDim.new(0, P[4])
			Pad.Parent = H
		end
		if Opts.full then
			Frame("F", H, Opts.full, Color3.fromRGB(230, 200, 40))
		end
		if Opts.box then
			Frame("B", H, UDim2.fromOffset(Opts.box[1], Opts.box[2]), Color3.fromRGB(40, 120, 160))
		end
		if Opts.text then
			local T = Instance.new("TextLabel")
			T.Name = "T"
			T.AutomaticSize = Enum.AutomaticSize.Y
			T.BackgroundTransparency = 1
			T.FontFace = Face
			T.Size = Opts.text
			T.Text = "Which units would you like to unequip from the team right now?"
			T.TextColor3 = Color3.new(1, 1, 1)
			T.TextSize = 18
			T.TextWrapped = true
			T.Parent = H
		end
	end
	local Full = UDim2.fromScale(1, 1)
	-- 200 x 60 of ratio 2: F beside a box, top and bottom padding past the base width
	Case("f_t20b20_b250x80", { pad = { 0, 0, 20, 20 }, full = Full, box = { 250, 80 } })
	Case("f_l10r10t20b20_b250x80", { pad = { 10, 10, 20, 20 }, full = Full, box = { 250, 80 } })
	Case("f_l10r10t20b20_b60x80", { pad = { 10, 10, 20, 20 }, full = Full, box = { 60, 80 } })
	Case("f_t30b30_b60x80", { pad = { 0, 0, 30, 30 }, full = Full, box = { 60, 80 } })
	Case("f_t20b20_b60x120", { pad = { 0, 0, 20, 20 }, full = Full, box = { 60, 120 } })
	Case("f_l30r30t20b20_b60x80", { pad = { 30, 30, 20, 20 }, full = Full, box = { 60, 80 } })
	Case("f_t40b40", { pad = { 0, 0, 40, 40 }, full = Full })
	Case("f_l20r20t40b40", { pad = { 20, 20, 40, 40 }, full = Full })
	-- boxes alone
	Case("b_t30b30_b100x80", { pad = { 0, 0, 30, 30 }, box = { 100, 80 } })
	Case("b_l10r10t30b30_b100x80", { pad = { 10, 10, 30, 30 }, box = { 100, 80 } })
	Case("b_t20b20_b100x80", { pad = { 0, 0, 20, 20 }, box = { 100, 80 } })
	Case("b_t30b30_b180x40", { pad = { 0, 0, 30, 30 }, box = { 180, 40 } })
	Case("b100x60_t30b30_b150x80", { size = UDim2.fromOffset(100, 60), pad = { 0, 0, 30, 30 }, box = { 150, 80 } })
	Case("b300x60_t30b30_b100x80", { size = UDim2.fromOffset(300, 60), pad = { 0, 0, 30, 30 }, box = { 100, 80 } })
	Case("b300x60_t10b10_b100x80", { size = UDim2.fromOffset(300, 60), pad = { 0, 0, 10, 10 }, box = { 100, 80 } })
	Case("b300x60_l20r20t30b30_b100x80", { size = UDim2.fromOffset(300, 60), pad = { 20, 20, 30, 30 }, box = { 100, 80 } })
	-- bases wider than their fitted size, with F
	Case("b300x60_t30b30_full_b100x80", { size = UDim2.fromOffset(300, 60), pad = { 0, 0, 30, 30 }, full = Full, box = { 100, 80 } })
	Case("b300x60_t30b30_full", { size = UDim2.fromOffset(300, 60), pad = { 0, 0, 30, 30 }, full = Full })
	Case("b260x100_t20b20_full_b60x120", { size = UDim2.fromOffset(260, 100), pad = { 0, 0, 20, 20 }, full = Full, box = { 60, 120 } })
	Case("b120x100_t20b20_full_b60x80", { size = UDim2.fromOffset(120, 100), pad = { 0, 0, 20, 20 }, full = Full, box = { 60, 80 } })
	-- 0 x 0 and 100 x 0 bases with a wrapped label (ratio 3)
	Case("b0x0_r3_text", { size = UDim2.fromOffset(0, 0), ratio = 3, text = UDim2.new(1, -20, 0, 0) })
	Case("b0x0_r3_t10b10_text", { size = UDim2.fromOffset(0, 0), ratio = 3, pad = { 0, 0, 10, 10 }, text = UDim2.new(1, -20, 0, 0) })
	Case("b100x0_r3_text", { size = UDim2.fromOffset(100, 0), ratio = 3, text = UDim2.new(1, -20, 0, 0) })
	Case("b100x0_r3_t10b10_text", { size = UDim2.fromOffset(100, 0), ratio = 3, pad = { 0, 0, 10, 10 }, text = UDim2.new(1, -20, 0, 0) })
	Case("b200x60_r3_t10b10_text", { size = UDim2.fromOffset(200, 60), ratio = 3, pad = { 0, 0, 10, 10 }, text = UDim2.new(1, -20, 0, 0) })
	Case("b0x0_r3_t10b10_full_text", { size = UDim2.fromOffset(0, 0), ratio = 3, pad = { 0, 0, 10, 10 }, full = Full, text = UDim2.new(1, -20, 0, 0) })
	Case("b0x0_r2_t20b20_b60x80", { size = UDim2.fromOffset(0, 0), pad = { 0, 0, 20, 20 }, box = { 60, 80 } })
	Case("b0x0_r2_t20b20_full_b60x80", { size = UDim2.fromOffset(0, 0), pad = { 0, 0, 20, 20 }, full = Full, box = { 60, 80 } })
	Case("b100x0_r2_t20b20_full_b60x80", { size = UDim2.fromOffset(100, 0), pad = { 0, 0, 20, 20 }, full = Full, box = { 60, 80 } })
	Case("b100x0_r2_t20b20_b150x80", { size = UDim2.fromOffset(100, 0), pad = { 0, 0, 20, 20 }, box = { 150, 80 } })
	Case("b0x60_r2_t20b20_full_b60x80", { size = UDim2.fromOffset(0, 60), pad = { 0, 0, 20, 20 }, full = Full, box = { 60, 80 } })
	Case("b200x0_r2_t20b20_full_b60x80", { size = UDim2.fromOffset(200, 0), pad = { 0, 0, 20, 20 }, full = Full, box = { 60, 80 } })
end
