-- Scroll bars of narrow frames (companion of image-probe-scroll-tiny.luau): frames about as wide as their bar or
-- narrower, with one bar (vertical or horizontal) or both, with and without a border, over a sweep of widths and
-- heights. End caps only (white), so each cap's place reads directly.
-- compare: --viewport 1200x700 --time 0.5 --background 0,0,0
return function(Target)
	local White = "rbxasset://textures/ui/InGameMenu/WhiteSquare.png"
	-- { width, height, canvas width, canvas height, thickness, border }
	local Configs = {}
	for _, Border in { 0, 1 } do
		for _, Width in { 7, 8, 9, 10, 11, 12, 14, 16, 20 } do
			table.insert(Configs, { Width, 12, 0, 300, 7, Border })
		end
	end
	for _, Border in { 0, 1 } do
		for _, Width in { 4, 7, 8, 9, 12 } do
			table.insert(Configs, { Width, 40, 0, 300, 7, Border })
		end
	end
	for _, Height in { 7, 8, 9, 12 } do
		table.insert(Configs, { 40, Height, 300, 0, 7, 1 })
	end
	for _, Size in { { 3, 40 }, { 5, 40 }, { 8, 40 }, { 12, 40 }, { 40, 3 }, { 40, 8 }, { 20, 20 }, { 12, 12 } } do
		table.insert(Configs, { Size[1], Size[2], 300, 300, 7, 0 })
	end
	for Index, Config in Configs do
		local Width, Height, CanvasWidth, CanvasHeight, Thickness, Border = table.unpack(Config)
		local Cell = Index - 1
		local Holder = Instance.new("Frame")
		Holder.Name = `N{Index}`
		Holder.Position = UDim2.fromOffset((Cell % 10) * 120, (Cell // 10) * 115)
		Holder.Size = UDim2.fromOffset(116, 110)
		Holder.BackgroundColor3 = Color3.fromRGB(0, 0, 0)
		Holder.BorderSizePixel = 0
		Holder.Parent = Target
		local Frame = Instance.new("ScrollingFrame")
		Frame.Name = "S"
		Frame.Position = UDim2.fromOffset(40, 35)
		Frame.Size = UDim2.fromOffset(Width, Height)
		Frame.CanvasSize = UDim2.fromOffset(CanvasWidth, CanvasHeight)
		Frame.ScrollBarThickness = Thickness
		Frame.BorderSizePixel = Border
		Frame.BorderColor3 = Color3.fromRGB(40, 90, 200)
		Frame.BackgroundColor3 = Color3.fromRGB(70, 30, 30)
		Frame.BackgroundTransparency = if Border > 0 then 0 else 1
		Frame.ScrollBarImageColor3 = Color3.new(1, 1, 1)
		Frame.TopImage = White
		Frame.MidImage = ""
		Frame.BottomImage = White
		Frame.Parent = Holder
	end
end
