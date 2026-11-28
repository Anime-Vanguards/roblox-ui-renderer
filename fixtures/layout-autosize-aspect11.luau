-- AutomaticSize XY frames with a UIAspectRatioConstraint whose ratio box is shorter than their top and bottom padding,
-- continued from layout-autosize-aspect10.luau: where the children start and what a scale child sizes against, over
-- the padding (equal or not), the ratio, the box B's size, the base height, side padding and scale children of other
-- sizes and positions (F). Each cell holds one automatic frame H.
--   node cli/rur.mjs compare --fixture fixtures/layout-autosize-aspect11.luau --viewport 2400x660 --time 0.2 --background 20,20,26
return function(Target)
	local Count = 0
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
	-- Opts: size, ratio, pad ({ left, right, top, bottom }), full (F's Size), at (F's Position), box ({ w, h })
	local function Case(Name, Opts)
		local Cell = Instance.new("Frame")
		Cell.Name = Name
		Cell.Position = UDim2.fromOffset((Count % 8) * 300, (Count // 8) * 220)
		Cell.Size = UDim2.fromOffset(296, 216)
		Cell.BackgroundColor3 = Color3.fromRGB(30, 30, 38)
		Cell.BorderSizePixel = 0
		Cell.Parent = Target
		Count += 1
		local H = Frame("H", Cell, Opts.size or UDim2.fromOffset(0, 0), Color3.fromRGB(200, 40, 50))
		H.AutomaticSize = Enum.AutomaticSize.XY
		H.Position = UDim2.fromOffset(6, 6)
		local Aspect = Instance.new("UIAspectRatioConstraint")
		Aspect.AspectRatio = Opts.ratio or 2
		Aspect.Parent = H
		local P = Opts.pad
		local Pad = Instance.new("UIPadding")
		Pad.PaddingLeft, Pad.PaddingRight = UDim.new(0, P[1]), UDim.new(0, P[2])
		Pad.PaddingTop, Pad.PaddingBottom = UDim.new(0, P[3]), UDim.new(0, P[4])
		Pad.Parent = H
		if Opts.full then
			local F = Frame("F", H, Opts.full, Color3.fromRGB(230, 200, 40))
			if Opts.at then
				F.Position = Opts.at
			end
		end
		Frame("B", H, UDim2.fromOffset(Opts.box[1], Opts.box[2]), Color3.fromRGB(40, 120, 160))
	end
	local Full = UDim2.fromScale(1, 1)
	Case("r2_t20b20_b60x80", { pad = { 0, 0, 20, 20 }, full = Full, box = { 60, 80 } })
	Case("r2_t30b10_b60x80", { pad = { 0, 0, 30, 10 }, full = Full, box = { 60, 80 } })
	Case("r2_t10b30_b60x80", { pad = { 0, 0, 10, 30 }, full = Full, box = { 60, 80 } })
	Case("r2_t20b20_b60x80_half", { pad = { 0, 0, 20, 20 }, full = UDim2.fromScale(0.5, 0.5), box = { 60, 80 } })
	Case("b0x120_r2_t20b20_b60x80", { size = UDim2.fromOffset(0, 120), pad = { 0, 0, 20, 20 }, full = Full, box = { 60, 80 } })
	Case("b0x100_r2_t20b20_b60x80_half", { size = UDim2.fromOffset(0, 100), pad = { 0, 0, 20, 20 }, full = UDim2.fromScale(0.5, 0.5), box = { 60, 80 } })
	Case("r3_t20b20_b60x80", { ratio = 3, pad = { 0, 0, 20, 20 }, full = Full, box = { 60, 80 } })
	Case("r3_t25b5_b60x80", { ratio = 3, pad = { 0, 0, 25, 5 }, full = Full, box = { 60, 80 } })
	Case("r2_t15b15_b40x80", { pad = { 0, 0, 15, 15 }, full = Full, box = { 40, 80 } })
	Case("r2_l10r10t20b20_b60x80", { pad = { 10, 10, 20, 20 }, full = Full, box = { 60, 80 } })
	Case("r2_t20b20_b60x120", { pad = { 0, 0, 20, 20 }, full = Full, box = { 60, 120 } })
	Case("r2_t20b20_b60x80_mid", { pad = { 0, 0, 20, 20 }, full = UDim2.fromScale(1, 0.5), at = UDim2.fromScale(0, 0.5), box = { 60, 80 } })
	Case("r1_t40b40_b60x80", { ratio = 1, pad = { 0, 0, 40, 40 }, full = Full, box = { 60, 80 } })
	Case("r2_t20b20_b60x80_offset", { pad = { 0, 0, 20, 20 }, full = UDim2.new(0, 30, 1, -10), box = { 60, 80 } })
	Case("r2_t20b20_b100x80", { pad = { 0, 0, 20, 20 }, full = Full, box = { 100, 80 } })
	Case("r2_t20b20_b60x80_box", { pad = { 0, 0, 20, 20 }, box = { 60, 80 } })
	Case("r2_t20b20_b80x50", { pad = { 0, 0, 20, 20 }, full = Full, box = { 80, 50 } })
	Case("b0x40_r2_t30b30_b60x80", { size = UDim2.fromOffset(0, 40), pad = { 0, 0, 30, 30 }, full = Full, box = { 60, 80 } })
	Case("r2_t20b20_b60x80_end", { pad = { 0, 0, 20, 20 }, full = UDim2.fromScale(1, 0.25), at = UDim2.fromScale(0, 1), box = { 60, 80 } })
	Case("b30x0_r2_t20b20_b60x80", { size = UDim2.fromOffset(30, 0), pad = { 0, 0, 20, 20 }, full = Full, box = { 60, 80 } })
end
