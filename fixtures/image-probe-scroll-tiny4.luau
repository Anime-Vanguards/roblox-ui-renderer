-- Horizontal scroll bars of narrow frames (companion of image-probe-scroll-tiny2.luau): a horizontal bar of thickness 7
-- on frames 40 tall swept over width, alone or beside a vertical bar, with and without a border, at the start and
-- scrolled to the end. End caps only (white over black), so each cap's place reads directly.
-- compare: --viewport 1200x480 --time 0.5 --background 0,0,0
return function(Target)
	local White = "rbxasset://textures/ui/InGameMenu/WhiteSquare.png"
	-- rows of { canvas width, canvas height, border, at the end }
	local Rows = {
		{ 300, 0, 0, false },
		{ 300, 0, 1, false },
		{ 300, 0, 0, true },
		{ 300, 300, 0, false },
	}
	local Widths = { 3, 5, 8, 10, 12, 14, 16, 18, 22, 30 }
	for Row, Spec in Rows do
		local CanvasWidth, CanvasHeight, Border, AtEnd = table.unpack(Spec)
		for Column, Width in Widths do
			local Holder = Instance.new("Frame")
			Holder.Name = `H{Row}W{Width}`
			Holder.Position = UDim2.fromOffset((Column - 1) * 120, (Row - 1) * 115)
			Holder.Size = UDim2.fromOffset(116, 110)
			Holder.BackgroundColor3 = Color3.fromRGB(0, 0, 0)
			Holder.BorderSizePixel = 0
			Holder.Parent = Target
			local Frame = Instance.new("ScrollingFrame")
			Frame.Name = "S"
			Frame.Position = UDim2.fromOffset(40, 35)
			Frame.Size = UDim2.fromOffset(Width, 40)
			Frame.CanvasSize = UDim2.fromOffset(CanvasWidth, CanvasHeight)
			Frame.ScrollBarThickness = 7
			Frame.BorderSizePixel = Border
			Frame.BorderColor3 = Color3.fromRGB(40, 90, 200)
			Frame.BackgroundColor3 = Color3.fromRGB(70, 30, 30)
			Frame.BackgroundTransparency = if Border > 0 then 0 else 1
			Frame.ScrollBarImageColor3 = Color3.new(1, 1, 1)
			Frame.TopImage = White
			Frame.MidImage = ""
			Frame.BottomImage = White
			Frame.Parent = Holder
			if AtEnd then
				Frame.CanvasPosition = Vector2.new(1e6, 1e6)
			end
		end
	end
end
