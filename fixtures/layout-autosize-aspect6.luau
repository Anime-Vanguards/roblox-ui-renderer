-- AutomaticSize frames with a UIAspectRatioConstraint holding a (1, 1) frame, continued from
-- layout-autosize-aspect3.luau: what the (1, 1) frame F is sized against when the frame H has top and bottom padding,
-- swept over the box B's height, the padding on each side and H's base height. Each cell holds one automatic frame H
-- (ratio 3 unless named) with F and usually B.
--   node cli/rur.mjs compare --fixture fixtures/layout-autosize-aspect6.luau --viewport 2400x1320 --time 0.2 --background 20,20,26
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
	-- Opts: size, ratio, pad ({ left, right, top, bottom }), box (B's height, 100 wide), full (F's Size)
	local function Case(Name, Opts)
		local Cell = Instance.new("Frame")
		Cell.Name = Name
		Cell.Position = UDim2.fromOffset((Count % 8) * 300, (Count // 8) * 220)
		Cell.Size = UDim2.fromOffset(296, 216)
		Cell.BackgroundColor3 = Color3.fromRGB(30, 30, 38)
		Cell.BorderSizePixel = 0
		Cell.Parent = Target
		Count += 1
		local H = Frame("H", Cell, Opts.size or UDim2.fromOffset(240, 60), Color3.fromRGB(200, 40, 50))
		H.AutomaticSize = Enum.AutomaticSize.Y
		H.Position = UDim2.fromOffset(6, 6)
		local Aspect = Instance.new("UIAspectRatioConstraint")
		Aspect.AspectRatio = Opts.ratio or 3
		Aspect.Parent = H
		local P = Opts.pad or { 0, 0, 0, 0 }
		if P[1] ~= 0 or P[2] ~= 0 or P[3] ~= 0 or P[4] ~= 0 then
			local Pad = Instance.new("UIPadding")
			Pad.PaddingLeft, Pad.PaddingRight = UDim.new(0, P[1]), UDim.new(0, P[2])
			Pad.PaddingTop, Pad.PaddingBottom = UDim.new(0, P[3]), UDim.new(0, P[4])
			Pad.Parent = H
		end
		Frame("F", H, Opts.full or UDim2.fromScale(1, 1), Color3.fromRGB(230, 200, 40))
		if Opts.box then
			Frame("B", H, UDim2.fromOffset(100, Opts.box), Color3.fromRGB(40, 160, 60))
		end
	end
	-- top and bottom padding x B's height (base 240 x 60, ratio 3: fitted 180 x 60)
	for _, Pads in { { 0, 0 }, { 10, 10 }, { 30, 30 }, { 20, 0 }, { 0, 20 } } do
		for _, Box in { 0, 20, 50, 80, 110 } do
			Case(`t{Pads[1]}b{Pads[2]}_b{Box}`, { pad = { 0, 0, Pads[1], Pads[2] }, box = if Box > 0 then Box else nil })
		end
	end
	-- left and right padding too
	for _, Pads in { { 10, 10 }, { 30, 30 }, { 20, 0 } } do
		for _, Box in { 20, 80 } do
			Case(`lr10_t{Pads[1]}b{Pads[2]}_b{Box}`, { pad = { 10, 10, Pads[1], Pads[2] }, box = Box })
		end
	end
	-- other base heights
	for _, Base in { 0, 30, 90 } do
		for _, Pads in { { 10, 10 }, { 30, 0 } } do
			for _, Box in { 20, 80 } do
				Case(`h{Base}_t{Pads[1]}b{Pads[2]}_b{Box}`, { size = UDim2.fromOffset(240, Base), pad = { 0, 0, Pads[1], Pads[2] }, box = Box })
			end
		end
	end
	-- ratio 2 (fitted 120 x 60), and F of other sizes
	for _, Box in { 20, 80 } do
		Case(`r2_t10b10_b{Box}`, { ratio = 2, pad = { 0, 0, 10, 10 }, box = Box })
	end
	Case("half_t10b10_b80", { pad = { 0, 0, 10, 10 }, box = 80, full = UDim2.fromScale(1, 0.5) })
	Case("off_t10b10_b80", { pad = { 0, 0, 10, 10 }, box = 80, full = UDim2.new(1, 0, 1, -10) })
	Case("half_t30b30_b20", { pad = { 0, 0, 30, 30 }, box = 20, full = UDim2.fromScale(0.5, 0.5) })
end
