-- The Studio probes behind rur's AutomaticSize + UIAspectRatioConstraint rule for prompt holders, rebuilt for a
-- client capture: tools/studio/layout-probe-autoaspect3.luau (cells o_*: an options prompt's Holder, Size (540, 0)
-- AutoY of ratio 4 around a scale-width AutoY panel holding a wrapping row of buttons, and its variations) and
-- layout-probe-autoaspect2.luau (cells s_*: a slider prompt's Holder, AutoXY Size (500, 158) of ratio 3.5 around a
-- scale-sized AutoY panel and a (1, 0, 0, 267) shadow). Each cell is the probe's 1200 x 900 holder, centred on its
-- slot; the holders overlap (transparent).
--   node cli/rur.mjs compare --fixture fixtures/layout-autosize-aspect-panels.luau --viewport 1800x1260 --time 0.2 --background 20,20,26
return function(Target)
	local function Frame(Name, Parent, Size, Props)
		local F = Instance.new("Frame")
		F.Name = Name
		F.Size = Size
		F.BackgroundColor3 = Color3.fromRGB(60, 60, 80)
		F.BackgroundTransparency = 0.6
		F.BorderSizePixel = 0
		for Key, Value in Props or {} do
			F[Key] = Value
		end
		F.Parent = Parent
		return F
	end
	local Count = 0
	-- a 1200 x 900 holder whose centre is the slot's
	local function Slot(Name)
		local X = (Count % 3) * 600 + 300
		local Y = (Count // 3) * 210 + 120
		Count += 1
		local Holder = Instance.new("Frame")
		Holder.Name = Name
		Holder.Position = UDim2.fromOffset(X - 600, Y - 450)
		Holder.Size = UDim2.fromOffset(1200, 900)
		Holder.BackgroundTransparency = 1
		Holder.Parent = Target
		return Holder
	end

	-- layout-probe-autoaspect3
	local function Options(Name, Options)
		local Holder = Slot(Name)
		local H = Frame("H", Holder, Options.size or UDim2.fromOffset(540, 0), { BackgroundColor3 = Color3.fromRGB(200, 40, 50) })
		H.AutomaticSize = Enum.AutomaticSize[Options.auto or "Y"]
		if Options.corner then
			H.Position = UDim2.fromOffset(330, 360)
		else
			H.AnchorPoint = Vector2.new(0.5, 0.5)
			H.Position = UDim2.fromScale(0.5, 0.5)
		end
		if Options.aspect ~= false then
			local C = Instance.new("UIAspectRatioConstraint")
			C.AspectRatio = Options.ratio or 4
			C.Parent = H
		end
		local Main = Frame("Main", H, Options.mainSize or UDim2.new(1, 0, 0, 0), { AutomaticSize = Enum.AutomaticSize.Y })
		local P = Instance.new("UIPadding")
		P.PaddingLeft, P.PaddingRight = UDim.new(0, 14), UDim.new(0, 14)
		P.PaddingTop, P.PaddingBottom = UDim.new(0, 16), UDim.new(0, 16)
		P.Parent = Main
		local L = Instance.new("UIListLayout")
		L.Padding = UDim.new(0, 12)
		L.HorizontalAlignment = Enum.HorizontalAlignment.Center
		L.Parent = Main
		local Buttons = Frame("Buttons", Main, UDim2.new(1, 0, 0, 0), { AutomaticSize = Enum.AutomaticSize.Y })
		local BL = Instance.new("UIListLayout")
		BL.FillDirection = Enum.FillDirection.Horizontal
		BL.Wraps = Options.wraps ~= false
		BL.HorizontalAlignment = Enum.HorizontalAlignment.Center
		BL.Padding = UDim.new(0, 10)
		BL.Parent = Buttons
		for Index = 1, Options.items or 3 do
			Frame(`B{Index}`, Buttons, UDim2.fromOffset(Options.itemWidth or 240, 56), { BackgroundColor3 = Color3.fromRGB(40, 160, 60), BackgroundTransparency = 0.3 })
		end
	end
	Options("o_base", {})
	Options("o_items100", { itemWidth = 100 })
	Options("o_items200x2", { itemWidth = 200, items = 2 })
	Options("o_small", { size = UDim2.fromOffset(300, 0) })
	Options("o_noaspect", { aspect = false })
	Options("o_corner", { corner = true })
	Options("o_offsetmain", { mainSize = UDim2.new(0, 540, 0, 0) })
	Options("o_nowrap", { wraps = false })
	Options("o_ratio2", { ratio = 2 })
	Options("o_autoXY", { auto = "XY", size = UDim2.fromOffset(0, 0) })

	-- layout-probe-autoaspect2
	local function Slider(Name, WithMain, WithShadow, ContentHeight)
		local Holder = Slot(Name)
		local A = Frame("H", Holder, UDim2.fromOffset(500, 158), { BackgroundColor3 = Color3.fromRGB(200, 40, 50) })
		A.AnchorPoint = Vector2.new(0.5, 0.5)
		A.Position = UDim2.fromOffset(600, 450)
		A.AutomaticSize = Enum.AutomaticSize.XY
		local C = Instance.new("UIAspectRatioConstraint")
		C.AspectRatio = 3.5
		C.Parent = A
		if WithMain then
			local M = Frame("Main", A, UDim2.fromScale(1, 1), { AutomaticSize = Enum.AutomaticSize.Y, Position = UDim2.fromScale(0.002, -0.0066) })
			local List = Instance.new("UIListLayout")
			List.Padding = UDim.new(0, 11)
			List.HorizontalAlignment = Enum.HorizontalAlignment.Center
			List.Parent = M
			local Padding = Instance.new("UIPadding")
			Padding.PaddingTop = UDim.new(0, 12)
			Padding.PaddingBottom = UDim.new(0, 12)
			Padding.PaddingLeft = UDim.new(0, 5)
			Padding.PaddingRight = UDim.new(0, 5)
			Padding.Parent = M
			Frame("C", M, UDim2.fromOffset(375, ContentHeight), { BackgroundColor3 = Color3.fromRGB(40, 160, 60) })
			Frame("D", M, UDim2.fromOffset(200, 65), { BackgroundColor3 = Color3.fromRGB(40, 90, 200) })
		end
		if WithShadow then
			local S = Frame("Shadow", A, UDim2.new(1, 0, 0, 267), { BackgroundTransparency = 1 })
			Frame("Image", S, UDim2.fromScale(1.5, 1.5), { AnchorPoint = Vector2.new(0.5, 0.5), Position = UDim2.fromScale(0.5, 0.5), BackgroundTransparency = 0.85 })
		end
	end
	Slider("s_both", true, true, 100)
	Slider("s_main", true, false, 100)
	Slider("s_shadow", false, true, 100)
	Slider("s_none", false, false, 100)
	Slider("s_bothTall", true, true, 250)
	Slider("s_mainTall", true, false, 250)
end
