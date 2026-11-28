-- Scroll bars of frames shorter or narrower than their bar (companion of image-probe-scroll-min.luau): the window a bar
-- runs along is shorter than the minimum thumb (2 x thickness + 1), zero or negative, or the frame is narrower than the
-- bar itself, at the top and scrolled to the end, with and without a border. Row groups: end caps only (white), middle
-- only (white), the default images; each frame sits in a cell with room around it for what a bar draws outside it.
-- compare: --viewport 1200x700 --time 0.5 --background 0,0,0
return function(Target)
	local White = "rbxasset://textures/ui/InGameMenu/WhiteSquare.png"
	-- { width, height, canvas width, canvas height, thickness, border, CanvasPosition (0 or the end) }
	local Configs = {
		{ 40, 9, 0, 300, 4, 0, false },
		{ 40, 8, 0, 300, 4, 0, false },
		{ 40, 5, 0, 300, 4, 0, true },
		{ 40, 3, 0, 300, 4, 0, false },
		{ 40, 14, 0, 300, 7, 0, false },
		{ 40, 10, 0, 300, 7, 0, true },
		{ 40, 4, 0, 300, 7, 0, false },
		{ 40, 20, 0, 300, 12, 0, false },
		{ 40, 6, 0, 300, 12, 0, true },
		{ 3, 40, 0, 300, 7, 0, false },
		{ 30, 10, 60, 30, 4, 1, false },
		{ 30, 10, 60, 30, 4, 0, false },
		{ 30, 10, 60, 30, 4, 1, true },
		{ 3, 1, 11, 7, 7, 1, false },
		{ 3, 1, 11, 7, 7, 0, false },
		{ 10, 12, 0, 300, 7, 1, false },
		{ 40, 10, 300, 0, 4, 0, false },
		{ 40, 8, 300, 0, 4, 0, true },
		{ 12, 30, 300, 0, 7, 0, false },
		{ 40, 30, 0, 300, 7, 2, true },
	}
	for Group = 1, 3 do
		for Index, Config in Configs do
			local Width, Height, CanvasWidth, CanvasHeight, Thickness, Border, AtEnd = table.unpack(Config)
			local Cell = Index - 1
			local Holder = Instance.new("Frame")
			Holder.Name = `G{Group}_{Index}`
			Holder.Position = UDim2.fromOffset((Cell % 10) * 120, (Group - 1) * 230 + (Cell // 10) * 115)
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
			if Group == 1 then
				Frame.ScrollBarImageColor3 = Color3.new(1, 1, 1)
				Frame.TopImage = White
				Frame.MidImage = ""
				Frame.BottomImage = White
			elseif Group == 2 then
				Frame.ScrollBarImageColor3 = Color3.new(1, 1, 1)
				Frame.TopImage = ""
				Frame.MidImage = White
				Frame.BottomImage = ""
			end
			Frame.Parent = Holder
			if AtEnd then
				Frame.CanvasPosition = Vector2.new(1e6, 1e6)
			end
		end
	end
end
