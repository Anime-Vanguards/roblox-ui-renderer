-- AutomaticSize frames with a UIAspectRatioConstraint, continued from layout-autosize-aspect2.luau: the frame's own
-- padding (offsets on either axis, with and without a base height) and children at offset positions. Each cell holds
-- one automatic frame H with the constraint, a box B and sometimes a (1, 1) frame F.
--   node cli/rur.mjs compare --fixture fixtures/layout-autosize-aspect3.luau --viewport 1800x1260 --time 0.2 --background 20,20,26
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
	-- Opts: auto, size, ratio, pad ({ left, right, top, bottom }), box ({ w, h }), at (B's offset position), full
	local function Case(Name, Opts)
		local Cell = Instance.new("Frame")
		Cell.Name = Name
		Cell.Position = UDim2.fromOffset((Count % 6) * 300, (Count // 6) * 180)
		Cell.Size = UDim2.fromOffset(296, 176)
		Cell.BackgroundColor3 = Color3.fromRGB(30, 30, 38)
		Cell.BorderSizePixel = 0
		Cell.Parent = Target
		Count += 1
		local H = Frame("H", Cell, Opts.size or UDim2.fromOffset(240, 60), Color3.fromRGB(200, 40, 50))
		H.AutomaticSize = Enum.AutomaticSize[Opts.auto or "Y"]
		H.Position = UDim2.fromOffset(6, 6)
		local Aspect = Instance.new("UIAspectRatioConstraint")
		Aspect.AspectRatio = Opts.ratio or 3
		Aspect.Parent = H
		if Opts.pad then
			local Pad = Instance.new("UIPadding")
			Pad.PaddingLeft, Pad.PaddingRight = UDim.new(0, Opts.pad[1]), UDim.new(0, Opts.pad[2])
			Pad.PaddingTop, Pad.PaddingBottom = UDim.new(0, Opts.pad[3]), UDim.new(0, Opts.pad[4])
			Pad.Parent = H
		end
		if Opts.full then
			Frame("F", H, UDim2.fromScale(1, 1), Color3.fromRGB(230, 200, 40))
		end
		if Opts.box then
			local B = Frame("B", H, UDim2.fromOffset(Opts.box[1], Opts.box[2]), Color3.fromRGB(40, 160, 60))
			if Opts.at then
				B.Position = UDim2.fromOffset(Opts.at[1], Opts.at[2])
			end
		end
	end
	local H0 = UDim2.fromOffset(240, 0)

	-- vertical padding with a base height of 60
	Case("pady30_b100x10", { pad = { 0, 0, 30, 30 }, box = { 100, 10 } })
	Case("pady30_b100x80", { pad = { 0, 0, 30, 30 }, box = { 100, 80 } })
	Case("pady20_b100x40", { pad = { 0, 0, 20, 20 }, box = { 100, 40 } })
	Case("pady25_b100x40", { pad = { 0, 0, 25, 25 }, box = { 100, 40 } })
	Case("pady30_b200x40", { pad = { 0, 0, 30, 30 }, box = { 200, 40 } })
	Case("padtop60_b100x40", { pad = { 0, 0, 60, 0 }, box = { 100, 40 } })
	Case("padbottom60_b100x40", { pad = { 0, 0, 0, 60 }, box = { 100, 40 } })
	Case("pady30_empty", { pad = { 0, 0, 30, 30 } })
	Case("pady30_padx20_b100x40", { pad = { 20, 20, 30, 30 }, box = { 100, 40 } })
	Case("padx30_pady10_b100x40", { pad = { 30, 30, 10, 10 }, box = { 100, 40 } })
	Case("pady10_b100x50", { pad = { 0, 0, 10, 10 }, box = { 100, 50 } })
	Case("pady10_b100x70", { pad = { 0, 0, 10, 10 }, box = { 100, 70 } })
	-- padding with no base height
	Case("h0_pad10_b100x40", { size = H0, pad = { 10, 10, 10, 10 }, box = { 100, 40 } })
	Case("h0_padx10_b100x40", { size = H0, pad = { 10, 10, 0, 0 }, box = { 100, 40 } })
	Case("h0_pady10_b100x40", { size = H0, pad = { 0, 0, 10, 10 }, box = { 100, 40 } })
	Case("h0_pad10_b200x40", { size = H0, pad = { 10, 10, 10, 10 }, box = { 200, 40 } })
	Case("h0_pad10_b100x80", { size = H0, pad = { 10, 10, 10, 10 }, box = { 100, 80 } })
	Case("h0_pad20_b100x40", { size = H0, pad = { 20, 20, 20, 20 }, box = { 100, 40 } })
	-- children at offset positions
	Case("b100x100_at0x20", { box = { 100, 100 }, at = { 0, 20 } })
	Case("b100x40_at0x50", { box = { 100, 40 }, at = { 0, 50 } })
	Case("b100x100_at30x0", { box = { 100, 100 }, at = { 30, 0 } })
	Case("b200x40_at60x0", { box = { 200, 40 }, at = { 60, 0 } })
	Case("b200x100_at60x0", { box = { 200, 100 }, at = { 60, 0 } })
	Case("h0_b100x40_at20x10", { size = H0, box = { 100, 40 }, at = { 20, 10 } })
	-- AutomaticSize X
	Case("x_padx30_b40x40", { auto = "X", size = UDim2.fromOffset(60, 120), ratio = 0.5, pad = { 30, 30, 0, 0 }, box = { 40, 40 } })
	Case("x_pad10_b100x40", { auto = "X", size = UDim2.fromOffset(60, 120), ratio = 0.5, pad = { 10, 10, 10, 10 }, box = { 100, 40 } })
	Case("x_b40x40_at0x100", { auto = "X", size = UDim2.fromOffset(60, 120), ratio = 0.5, box = { 40, 40 }, at = { 30, 100 } })
	-- with a (1, 1) frame
	Case("pad10_full_b100x100", { pad = { 10, 10, 10, 10 }, full = true, box = { 100, 100 } })
	Case("pady30_full_b100x40", { pad = { 0, 0, 30, 30 }, full = true, box = { 100, 40 } })
	Case("h0_pad10_full_b100x40", { size = H0, pad = { 10, 10, 10, 10 }, full = true, box = { 100, 40 } })
end
