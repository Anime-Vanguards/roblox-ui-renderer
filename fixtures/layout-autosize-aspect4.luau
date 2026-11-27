-- AutomaticSize frames with a UIAspectRatioConstraint, continued from layout-autosize-aspect3.luau: what a scale-width
-- item holding children counts for the frame's width (a prompt's 0.95 header bar holding a centred label and two
-- full-width underlays), in a list panel, a plain panel and directly in the frame, and (1, 1) frames in frames
-- automatic on both axes or on X. Each cell holds one automatic frame H with the constraint; most are 240 x 40 of
-- ratio 3, fitted to 120 x 40, so what the content counts between 120 and 240 shows in H's width.
--   node cli/rur.mjs compare --fixture fixtures/layout-autosize-aspect4.luau --viewport 1800x1260 --time 0.2 --background 20,20,26
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
	local function Case(Name, Opts)
		local Cell = Instance.new("Frame")
		Cell.Name = Name
		Cell.Position = UDim2.fromOffset((Count % 6) * 300, (Count // 6) * 180)
		Cell.Size = UDim2.fromOffset(296, 176)
		Cell.BackgroundColor3 = Color3.fromRGB(30, 30, 38)
		Cell.BorderSizePixel = 0
		Cell.Parent = Target
		Count += 1
		local H = Frame("H", Cell, Opts.size or UDim2.fromOffset(240, 40), Color3.fromRGB(200, 40, 50))
		H.AutomaticSize = Enum.AutomaticSize[Opts.auto or "Y"]
		H.Position = UDim2.fromOffset(6, 6)
		local Aspect = Instance.new("UIAspectRatioConstraint")
		Aspect.AspectRatio = Opts.ratio or 3
		Aspect.Parent = H
		Opts.build(H)
	end
	-- the item's children: "leaf" a (1, 0, 0, 10) frame, "centre" a centred 100 x 20 frame, "alert" a centred 100 x 20
	-- frame and two bottom-anchored (1, 0) underlays, "wide" a 200 x 20 frame, "centre150" a centred 150 x 20 frame
	local function Fill(Item, Kind)
		if Kind == "leaf" then
			Frame("L", Item, UDim2.new(1, 0, 0, 10), Color3.fromRGB(240, 240, 240))
		end
		if Kind == "centre" or Kind == "alert" or Kind == "centre150" then
			local C = Frame("C", Item, UDim2.fromOffset(if Kind == "centre150" then 150 else 100, 20), Color3.fromRGB(240, 240, 240))
			C.AnchorPoint = Vector2.new(0.5, 0.5)
			C.Position = UDim2.fromScale(0.5, 0.5)
		end
		if Kind == "alert" then
			for Index, Height in { 8, 12 } do
				local U = Frame(`U{Index}`, Item, UDim2.new(1, 0, 0, Height), Color3.fromRGB(20, 20, 32))
				U.AnchorPoint = Vector2.new(0, 1)
				U.Position = UDim2.fromScale(0, 1)
			end
		end
		if Kind == "wide" then
			Frame("O", Item, UDim2.fromOffset(200, 20), Color3.fromRGB(240, 240, 240))
		end
	end
	-- where: "list" in a (1, 0) AutoY panel with a centred list, "plain" in such a panel without a list, "direct" in H
	local function Item(Where, Kind)
		return function(H)
			local Parent = H
			if Where ~= "direct" then
				local M = Frame("M", H, UDim2.fromScale(1, 0), Color3.fromRGB(40, 90, 200))
				M.AutomaticSize = Enum.AutomaticSize.Y
				if Where == "list" then
					local L = Instance.new("UIListLayout")
					L.HorizontalAlignment = Enum.HorizontalAlignment.Center
					L.Parent = M
				end
				Parent = M
			end
			local I = Frame("I", Parent, UDim2.new(0.95, 0, 0, 30), Color3.fromRGB(230, 200, 40))
			if Kind then
				Fill(I, Kind)
			end
		end
	end
	for _, Where in { "list", "plain", "direct" } do
		for _, Kind in { "leaf", "centre", "alert", "wide", "centre150" } do
			Case(`{Where}_{Kind}`, { build = Item(Where, Kind) })
		end
		Case(`{Where}_none`, { build = Item(Where, nil) })
	end
	-- a prompt's panel: a centred list with 15 px of top and bottom padding and a 10 px gap, a header bar, a
	-- description box and a button row
	Case("prompt_header", { size = UDim2.fromOffset(240, 76), ratio = 2.87, build = function(H)
		local M = Frame("M", H, UDim2.fromScale(1, 0.8862), Color3.fromRGB(40, 90, 200))
		M.AutomaticSize = Enum.AutomaticSize.Y
		local L = Instance.new("UIListLayout")
		L.HorizontalAlignment = Enum.HorizontalAlignment.Center
		L.VerticalAlignment = Enum.VerticalAlignment.Center
		L.Padding = UDim.new(0, 10)
		L.SortOrder = Enum.SortOrder.LayoutOrder
		L.Parent = M
		local P = Instance.new("UIPadding")
		P.PaddingTop, P.PaddingBottom = UDim.new(0, 15), UDim.new(0, 15)
		P.Parent = M
		local A = Frame("A", M, UDim2.new(0.95, 0, 0, 20), Color3.fromRGB(230, 200, 40))
		A.LayoutOrder = -1
		Fill(A, "alert")
		local D = Frame("D", M, UDim2.fromOffset(150, 30), Color3.fromRGB(40, 160, 60))
		D.LayoutOrder = 0
		local B = Frame("B", M, UDim2.fromOffset(100, 30), Color3.fromRGB(40, 160, 60))
		B.LayoutOrder = 1
	end })
	-- (1, 1) frames
	local function Full(W, Ht)
		return function(H)
			Frame("F", H, UDim2.fromScale(1, 1), Color3.fromRGB(230, 200, 40))
			Frame("B", H, UDim2.fromOffset(W, Ht), Color3.fromRGB(40, 160, 60))
		end
	end
	Case("xy_full_b60x80", { auto = "XY", size = UDim2.fromOffset(200, 60), ratio = 2, build = Full(60, 80) })
	Case("xy_full_b60x20", { auto = "XY", size = UDim2.fromOffset(200, 60), ratio = 2, build = Full(60, 20) })
	Case("xy_full_b220x20", { auto = "XY", size = UDim2.fromOffset(200, 60), ratio = 2, build = Full(220, 20) })
	Case("xy_full_b0x0", { auto = "XY", size = UDim2.fromOffset(200, 60), ratio = 2, build = Full(0, 0) })
	Case("x_full_b40x40", { auto = "X", size = UDim2.fromOffset(60, 60), ratio = 0.5, build = Full(40, 40) })
	Case("x_full_b20x100", { auto = "X", size = UDim2.fromOffset(60, 60), ratio = 0.5, build = Full(20, 100) })
	Case("y_full_b100x30", { size = UDim2.fromOffset(240, 60), build = Full(100, 30) })
	Case("y_full_b100x70", { size = UDim2.fromOffset(240, 60), build = Full(100, 70) })
	Case("y_half_b100x70", { size = UDim2.fromOffset(240, 60), build = function(H)
		Frame("F", H, UDim2.fromScale(0.5, 0.5), Color3.fromRGB(230, 200, 40))
		Frame("B", H, UDim2.fromOffset(100, 70), Color3.fromRGB(40, 160, 60))
	end })
end
