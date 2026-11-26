-- Which descendants count for AutomaticSize XY (tools/studio/layout-probe-autonested2.luau as a fixture), and a
-- sweep of its scalepos case: F (AutomaticSize XY, at (10, 5) in a 200 x 160 holder) holds the case's child C, then a
-- 60 x 20 frame (holders overlap a little). scalepos: C of Size (0.5, 0.5) at Position (0.5, 0) holding a (1, 1) frame G; swept over C's
-- Position and Size, G's Size, F's Size and a childless C.
--   node cli/rur.mjs compare --fixture fixtures/layout-autonested-scalepos.luau --viewport 1200x900 --time 0.2 --background 20,20,26
return function(Target)
	local Count = 0
	local function New(ClassName, Parent, Props)
		local Object = Instance.new(ClassName)
		for K, V in Props or {} do
			(Object :: any)[K] = V
		end
		if Object:IsA("GuiObject") then
			Object.BorderSizePixel = 0
			Object.BackgroundTransparency = 0.5
		end
		Object.Parent = Parent
		return Object
	end
	local function Case(Name, Build, FSize)
		Count += 1
		local Holder = New("Frame", Target, {
			Name = Name,
			Position = UDim2.fromOffset(((Count - 1) % 6) * 190, ((Count - 1) // 6) * 105),
			Size = UDim2.fromOffset(200, 160),
		})
		Holder.BackgroundTransparency = 1
		local F = New("Frame", Holder, { Name = "F", AutomaticSize = Enum.AutomaticSize.XY, Size = FSize or UDim2.fromOffset(0, 0), Position = UDim2.fromOffset(10, 5) })
		Build(F)
		New("Frame", F, { Name = "Z", Size = UDim2.fromOffset(60, 20) })
	end
	-- layout-probe-autonested2
	Case("oversize", function(F)
		local C = New("Frame", F, { Name = "C", Size = UDim2.fromOffset(40, 20) })
		New("Frame", C, { Name = "G", Size = UDim2.fromOffset(100, 90) })
	end)
	Case("oversizeScale", function(F)
		local C = New("Frame", F, { Name = "C", Size = UDim2.fromScale(0.2, 0.2) })
		New("Frame", C, { Name = "G", Size = UDim2.fromOffset(100, 90) })
	end)
	Case("scalepos", function(F)
		local C = New("Frame", F, { Name = "C", Size = UDim2.fromScale(0.5, 0.5), Position = UDim2.fromScale(0.5, 0) })
		New("Frame", C, { Name = "G", Size = UDim2.fromScale(1, 1) })
	end)
	Case("offsetgrand", function(F)
		local C = New("Frame", F, { Name = "C", Size = UDim2.fromScale(1, 1) })
		New("Frame", C, { Name = "G", Size = UDim2.fromOffset(10, 10), Position = UDim2.fromOffset(120, 100) })
	end)
	Case("scalegrandpos", function(F)
		local C = New("Frame", F, { Name = "C", Size = UDim2.fromScale(1, 1) })
		New("Frame", C, { Name = "G", Size = UDim2.fromOffset(10, 10), Position = UDim2.fromScale(0.5, 0.5) })
	end)
	Case("hiddengrand", function(F)
		local C = New("Frame", F, { Name = "C", Size = UDim2.fromScale(1, 1) })
		New("Frame", C, { Name = "G", Size = UDim2.fromScale(1, 1), Visible = false })
	end)
	Case("hiddenchild", function(F)
		local C = New("Frame", F, { Name = "C", Size = UDim2.fromScale(1, 1), Visible = false })
		New("Frame", C, { Name = "G", Size = UDim2.fromScale(1, 1) })
	end)
	Case("greatgrand", function(F)
		local C = New("Frame", F, { Name = "C", Size = UDim2.fromScale(1, 1) })
		local G = New("Frame", C, { Name = "G", Size = UDim2.fromOffset(50, 50) })
		New("Frame", G, { Name = "H", Size = UDim2.fromScale(2, 2) })
	end)
	Case("autogrand", function(F)
		local C = New("Frame", F, { Name = "C", Size = UDim2.fromScale(1, 1) })
		local G = New("Frame", C, { Name = "G", Size = UDim2.new(), AutomaticSize = Enum.AutomaticSize.XY })
		New("Frame", G, { Name = "H", Size = UDim2.fromOffset(30, 70) })
	end)
	Case("listchild", function(F)
		local C = New("Frame", F, { Name = "C", Size = UDim2.fromScale(1, 1) })
		New("UIListLayout", C, {})
		New("Frame", C, { Name = "G", Size = UDim2.new(0.5, 0, 0, 30) })
	end)
	Case("uiscale", function(F)
		local C = New("Frame", F, { Name = "C", Size = UDim2.fromScale(1, 1) })
		New("UIScale", C, { Scale = 0.5 })
		New("Frame", C, { Name = "G", Size = UDim2.fromScale(1, 1) })
	end)
	-- scalepos, swept
	local function ScalePos(Name, CSize, CPos, GSize, FSize, Anchor)
		Case(Name, function(F)
			local C = New("Frame", F, { Name = "C", Size = CSize, Position = CPos, AnchorPoint = Anchor or Vector2.zero })
			if GSize then
				New("Frame", C, { Name = "G", Size = GSize })
			end
		end, FSize)
	end
	ScalePos("sp_x25", UDim2.fromScale(0.5, 0.5), UDim2.fromScale(0.25, 0), UDim2.fromScale(1, 1))
	ScalePos("sp_x100", UDim2.fromScale(0.5, 0.5), UDim2.fromScale(1, 0), UDim2.fromScale(1, 1))
	ScalePos("sp_y50", UDim2.fromScale(0.5, 0.5), UDim2.fromScale(0, 0.5), UDim2.fromScale(1, 1))
	ScalePos("sp_xy50", UDim2.fromScale(0.5, 0.5), UDim2.fromScale(0.5, 0.5), UDim2.fromScale(1, 1))
	ScalePos("sp_x50_o10", UDim2.fromScale(0.5, 0.5), UDim2.new(0.5, 10, 0, 0), UDim2.fromScale(1, 1))
	ScalePos("sp_c30", UDim2.fromScale(0.3, 0.3), UDim2.fromScale(0.5, 0), UDim2.fromScale(1, 1))
	ScalePos("sp_c100", UDim2.fromScale(1, 1), UDim2.fromScale(0.5, 0), UDim2.fromScale(1, 1))
	ScalePos("sp_g50", UDim2.fromScale(0.5, 0.5), UDim2.fromScale(0.5, 0), UDim2.fromScale(0.5, 0.5))
	ScalePos("sp_goffset", UDim2.fromScale(0.5, 0.5), UDim2.fromScale(0.5, 0), UDim2.fromOffset(20, 20))
	ScalePos("sp_nog", UDim2.fromScale(0.5, 0.5), UDim2.fromScale(0.5, 0), nil)
	ScalePos("sp_f50x30", UDim2.fromScale(0.5, 0.5), UDim2.fromScale(0.5, 0), UDim2.fromScale(1, 1), UDim2.fromOffset(50, 30))
	ScalePos("sp_cmixed", UDim2.new(0.5, 10, 0.5, 0), UDim2.fromScale(0.5, 0), UDim2.fromScale(1, 1))
	ScalePos("sp_anchor", UDim2.fromScale(0.5, 0.5), UDim2.fromScale(0.5, 0), UDim2.fromScale(1, 1), nil, Vector2.new(0.5, 0))
	-- scale containers two levels deep, and padded scale containers
	Case("chain_1_05", function(F)
		local C = New("Frame", F, { Name = "C", Size = UDim2.fromScale(1, 1) })
		local G = New("Frame", C, { Name = "G", Size = UDim2.fromScale(0.5, 0.5) })
		New("Frame", G, { Name = "H", Size = UDim2.fromScale(1, 1) })
	end)
	Case("chain_08_08", function(F)
		local C = New("Frame", F, { Name = "C", Size = UDim2.fromScale(0.8, 0.8) })
		local G = New("Frame", C, { Name = "G", Size = UDim2.fromScale(0.8, 0.8) })
		New("Frame", G, { Name = "H", Size = UDim2.fromScale(1, 1) })
	end)
	Case("sp_cpad", function(F)
		local C = New("Frame", F, { Name = "C", Size = UDim2.fromScale(0.5, 0.5), Position = UDim2.fromScale(0.5, 0) })
		New("UIPadding", C, { PaddingLeft = UDim.new(0, 10), PaddingRight = UDim.new(0, 10), PaddingTop = UDim.new(0, 10), PaddingBottom = UDim.new(0, 10) })
		New("Frame", C, { Name = "G", Size = UDim2.fromScale(1, 1) })
	end)
	Case("sp_cpadscale", function(F)
		local C = New("Frame", F, { Name = "C", Size = UDim2.fromScale(0.5, 0.5), Position = UDim2.fromScale(0.5, 0) })
		New("UIPadding", C, { PaddingLeft = UDim.new(0.1, 0), PaddingRight = UDim.new(0.1, 0), PaddingTop = UDim.new(0.1, 0), PaddingBottom = UDim.new(0.1, 0) })
		New("Frame", C, { Name = "G", Size = UDim2.fromScale(1, 1) })
	end)
	Case("sp_gpos", function(F)
		local C = New("Frame", F, { Name = "C", Size = UDim2.fromScale(0.5, 0.5), Position = UDim2.fromScale(0.5, 0) })
		New("Frame", C, { Name = "G", Size = UDim2.fromScale(1, 1), Position = UDim2.new(0, 10, 0.2, 0) })
	end)
	local function Pad(C, Sides)
		New("UIPadding", C, { PaddingTop = Sides[1], PaddingBottom = Sides[2], PaddingLeft = Sides[1], PaddingRight = Sides[2] })
	end
	for _, Entry in {
		{ "cpad_t10", { UDim.new(0, 10), UDim.new() } },
		{ "cpad_b10", { UDim.new(), UDim.new(0, 10) } },
		{ "cpad_st", { UDim.new(0.1, 0), UDim.new() } },
		{ "cpad_sb", { UDim.new(), UDim.new(0.1, 0) } },
		{ "cpad_mixed", { UDim.new(0.1, 10), UDim.new() } },
	} do
		Case(Entry[1], function(F)
			local C = New("Frame", F, { Name = "C", Size = UDim2.fromScale(0.5, 0.5) })
			Pad(C, Entry[2])
			New("Frame", C, { Name = "G", Size = UDim2.fromScale(1, 1) })
		end)
	end
	local function Chain(Name, Scales)
		Case(Name, function(F)
			local Parent = F
			for Index, Scale in Scales do
				Parent = New("Frame", Parent, { Name = `L{Index}`, Size = UDim2.fromScale(Scale, Scale) })
			end
			New("Frame", Parent, { Name = "H", Size = UDim2.fromScale(1, 1) })
		end)
	end
	Chain("chain_05_08", { 0.5, 0.8 })
	Chain("chain_08_05", { 0.8, 0.5 })
	Chain("chain_08_08_08", { 0.8, 0.8, 0.8 })
	Case("chain_08_leaf", function(F)
		local C = New("Frame", F, { Name = "C", Size = UDim2.fromScale(0.8, 0.8) })
		New("Frame", C, { Name = "L", Size = UDim2.fromScale(1, 1) })
		local G = New("Frame", C, { Name = "G", Size = UDim2.fromScale(0.8, 0.8) })
		New("Frame", G, { Name = "H", Size = UDim2.fromScale(1, 1) })
	end)
	-- offset-sized and mixed containers with padding, holding scale leaves
	Case("off_pad", function(F)
		local C = New("Frame", F, { Name = "C", Size = UDim2.fromOffset(60, 60) })
		Pad(C, { UDim.new(0, 10), UDim.new(0, 10) })
		New("Frame", C, { Name = "G", Size = UDim2.fromScale(1, 1) })
	end)
	Case("mix_pad", function(F)
		local C = New("Frame", F, { Name = "C", Size = UDim2.new(0.25, 20, 0.25, 20) })
		Pad(C, { UDim.new(0, 10), UDim.new(0, 10) })
		New("Frame", C, { Name = "G", Size = UDim2.fromScale(1, 1) })
	end)
	Case("off_leaf05", function(F)
		local C = New("Frame", F, { Name = "C", Size = UDim2.fromOffset(60, 60) })
		New("Frame", C, { Name = "G", Size = UDim2.fromScale(0.5, 0.5) })
	end)
end
