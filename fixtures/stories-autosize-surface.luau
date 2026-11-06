-- Repro: which part of a UI kit's hover surface makes Studio size it to its whole parent
-- (a hover preview story: both surfaces 320x320 in a 320x320 Holder with a
-- vertical UIListLayout, although their content is 320x158 and 98x30). Variants of an
-- AutomaticSize XY surface (Size 0) holding a scale(1,1) "Chrome" and an auto-sized "Main":
--   A plain parent, B parent with a UIListLayout, C with the invisible arrows (one scale-positioned),
--   D with a UIScale child, E everything together inside a UIListLayout (the story's structure),
--   F-J pairs with the Glow (a centred scale(1,1) + 48 px ImageLabel inside Chrome).
-- Studio: A-D and H are 116x56 (the content); every variant with the glow is 184x184 (glow
-- 232x232): the glow's scale size resolves against the holder (0.5 x (320 + 48)). In the stories
-- the glow is invisible (no effect) and Chrome's SurfaceBackground (scale 1,1 at 0,0) gives 320.
local function Surface(Parent: Instance, Name: string, Options: { Arrows: boolean?, Scale: boolean?, Glow: boolean? })
	local Frame = Instance.new("Frame")
	Frame.Name = Name
	Frame.AutomaticSize = Enum.AutomaticSize.XY
	Frame.Size = UDim2.fromOffset(0, 0)
	Frame.BackgroundTransparency = 1

	if Options.Scale then
		local Scale = Instance.new("UIScale")
		Scale.Scale = 1
		Scale.Parent = Frame
	end

	if Options.Arrows then
		for _, Side in { "Left", "Right" } do
			local Arrow = Instance.new("ImageLabel")
			Arrow.Name = `Arrow{Side}`
			Arrow.AnchorPoint = if Side == "Right" then Vector2.zero else Vector2.new(1, 0)
			Arrow.BackgroundTransparency = 1
			Arrow.Position = if Side == "Right" then UDim2.new(1, -24, 0, 14) else UDim2.fromOffset(24, 14)
			Arrow.Size = UDim2.fromOffset(32, 32)
			Arrow.Visible = false
			Arrow.Parent = Frame
		end
	end

	local Chrome = Instance.new("Frame")
	Chrome.Name = "Chrome"
	Chrome.Size = UDim2.fromScale(1, 1)
	Chrome.BackgroundColor3 = Color3.fromRGB(90, 60, 160)
	Chrome.BorderSizePixel = 0
	Chrome.Parent = Frame

	if Options.Glow then
		local Glow = Instance.new("ImageLabel")
		Glow.Name = "Glow"
		Glow.AnchorPoint = Vector2.new(0.5, 0.5)
		Glow.BackgroundTransparency = 1
		Glow.Position = UDim2.fromScale(0.5, 0.5)
		Glow.Size = UDim2.new(1, 48, 1, 48)
		Glow.Parent = Chrome
	end

	local Main = Instance.new("Frame")
	Main.Name = "Main"
	Main.AutomaticSize = Enum.AutomaticSize.XY
	Main.Size = UDim2.fromScale(0, 0)
	Main.BackgroundTransparency = 1
	Main.Parent = Frame

	local Padding = Instance.new("UIPadding")
	for _, Side in { "PaddingTop", "PaddingBottom", "PaddingLeft", "PaddingRight" } do
		(Padding :: any)[Side] = UDim.new(0, 8)
	end
	Padding.Parent = Main

	local Content = Instance.new("Frame")
	Content.Name = "Content"
	Content.Size = UDim2.fromOffset(100, 40)
	Content.BackgroundColor3 = Color3.fromRGB(230, 200, 80)
	Content.BorderSizePixel = 0
	Content.Parent = Main

	Frame.Parent = Parent
	return Frame
end

return function(Target)
	local Variants = {
		{ Name = "A", List = false, Options = {} },
		{ Name = "B", List = true, Options = {} },
		{ Name = "C", List = false, Options = { Arrows = true } },
		{ Name = "D", List = false, Options = { Scale = true } },
		{ Name = "E", List = true, Options = { Arrows = true, Scale = true, Glow = true } },
		{ Name = "F", List = false, Options = { Glow = true } },
		{ Name = "G", List = true, Options = { Glow = true } },
		{ Name = "H", List = true, Options = { Arrows = true, Scale = true } },
		{ Name = "I", List = false, Options = { Arrows = true, Glow = true } },
		{ Name = "J", List = false, Options = { Scale = true, Glow = true } },
	}

	for Index, Variant in Variants do
		local Holder = Instance.new("Frame")
		Holder.Name = `Holder{Variant.Name}`
		Holder.Position = UDim2.fromOffset(20 + ((Index - 1) % 5) * 340, 40 + (Index - 1) // 5 * 360)
		Holder.Size = UDim2.fromOffset(320, 320)
		Holder.BackgroundColor3 = Color3.fromRGB(40, 40, 40)
		Holder.BorderSizePixel = 0
		Holder.Parent = Target

		if Variant.List then
			local List = Instance.new("UIListLayout")
			List.HorizontalAlignment = Enum.HorizontalAlignment.Center
			List.VerticalAlignment = Enum.VerticalAlignment.Center
			List.Padding = UDim.new(0, 24)
			List.SortOrder = Enum.SortOrder.LayoutOrder
			List.Parent = Holder
		end

		Surface(Holder, "Surface", Variant.Options)
	end
end
