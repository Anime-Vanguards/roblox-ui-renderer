-- AutomaticSize frames with a UIAspectRatioConstraint, continued from layout-autosize-aspect4.luau: what a scale-width
-- item holding children counts for the frame's width. Each cell's frame H is 240 x 30 of ratio 3 (fitted to 90 x 30)
-- and holds a 10 x 80 bar, so H is as wide as its content counts between 90 and 240.
--   node cli/rur.mjs compare --fixture fixtures/layout-autosize-aspect5.luau --viewport 1800x1260 --time 0.2 --background 20,20,26
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
	local function Case(Name, Build)
		local Cell = Instance.new("Frame")
		Cell.Name = Name
		Cell.Position = UDim2.fromOffset((Count % 6) * 300, (Count // 6) * 140)
		Cell.Size = UDim2.fromOffset(296, 136)
		Cell.BackgroundColor3 = Color3.fromRGB(30, 30, 38)
		Cell.BorderSizePixel = 0
		Cell.Parent = Target
		Count += 1
		local H = Frame("H", Cell, UDim2.fromOffset(240, 30), Color3.fromRGB(200, 40, 50))
		H.AutomaticSize = Enum.AutomaticSize.Y
		H.Position = UDim2.fromOffset(6, 6)
		local Aspect = Instance.new("UIAspectRatioConstraint")
		Aspect.AspectRatio = 3
		Aspect.Parent = H
		Frame("Bar", H, UDim2.fromOffset(10, 80), Color3.fromRGB(40, 160, 60))
		Build(H)
	end
	local function Fill(Item, Kind)
		local function Underlay(Name, Height)
			local U = Frame(Name, Item, UDim2.new(1, 0, 0, Height), Color3.fromRGB(20, 20, 32))
			U.AnchorPoint = Vector2.new(0, 1)
			U.Position = UDim2.fromScale(0, 1)
		end
		local function Centre(Width)
			local C = Frame("C", Item, UDim2.fromOffset(Width, 10), Color3.fromRGB(240, 240, 240))
			C.AnchorPoint = Vector2.new(0.5, 0.5)
			C.Position = UDim2.fromScale(0.5, 0.5)
		end
		if Kind == "leaf" then
			Frame("L", Item, UDim2.new(1, 0, 0, 6), Color3.fromRGB(240, 240, 240))
		elseif Kind == "halfleaf" then
			Frame("L", Item, UDim2.new(0.5, 0, 0, 6), Color3.fromRGB(240, 240, 240))
		elseif Kind == "underlay" then
			Underlay("U", 6)
		elseif Kind == "centre" then
			Centre(60)
		elseif Kind == "alert" then
			Centre(60)
			Underlay("U1", 4)
			Underlay("U2", 6)
		elseif Kind == "offset" then
			Frame("O", Item, UDim2.fromOffset(150, 6), Color3.fromRGB(240, 240, 240))
		elseif Kind == "offset_leaf" then
			Frame("O", Item, UDim2.fromOffset(150, 6), Color3.fromRGB(240, 240, 240))
			Frame("L", Item, UDim2.new(1, 0, 0, 6), Color3.fromRGB(240, 240, 240))
		elseif Kind == "centre_leaf" then
			Centre(60)
			Frame("L", Item, UDim2.new(1, 0, 0, 6), Color3.fromRGB(240, 240, 240))
		end
	end
	local function Item(Where, Kind, Width)
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
			local I = Frame("I", Parent, UDim2.new(Width or 0.8, 0, 0, 20), Color3.fromRGB(230, 200, 40))
			if Kind then
				Fill(I, Kind)
			end
		end
	end
	for _, Where in { "list", "plain", "direct" } do
		for _, Kind in { "none", "leaf", "halfleaf", "underlay", "centre", "alert", "offset", "offset_leaf", "centre_leaf" } do
			Case(`{Where}_{Kind}`, Item(Where, if Kind == "none" then nil else Kind))
		end
	end
	Case("list_alert_w05", Item("list", "alert", 0.5))
	Case("plain_alert_w05", Item("plain", "alert", 0.5))
	Case("direct_alert_w05", Item("direct", "alert", 0.5))
end
