-- Scroll bars of small frames drawn faded (companion of image-probe-scroll-tiny2.luau): a vertical bar on frames swept
-- over width and height, at thicknesses 7, 4 and 12 and at ScrollBarImageTransparency 0.5, end caps only (white over
-- black), so each cap's level is the bar's opacity.
-- compare: --viewport 1200x700 --time 0.5 --background 0,0,0
return function(Target)
	local White = "rbxasset://textures/ui/InGameMenu/WhiteSquare.png"
	-- rows of { thickness, height, widths, transparency }
	local Rows = {
		{ 7, 12, { 14, 15, 16, 17, 18, 19, 20, 22, 24, 28 }, 0 },
		{ 7, 16, { 14, 15, 16, 17, 18, 19, 20, 22, 24, 28 }, 0 },
		{ 7, 24, { 12, 14, 16, 18, 20, 22, 23, 24, 26, 28 }, 0 },
		{ 4, 6, { 8, 10, 12, 13, 14, 15, 16, 18, 20, 24 }, 0 },
		{ 12, 12, { 14, 16, 18, 20, 22, 24, 26, 28, 30, 34 }, 0 },
		{ 7, 12, { 14, 16, 18, 20, 22, 24, 28, 32, 36, 40 }, 0.5 },
	}
	for Row, Spec in Rows do
		local Thickness, Height, Widths, Transparency = table.unpack(Spec)
		for Column, Width in Widths do
			local Holder = Instance.new("Frame")
			Holder.Name = `R{Row}W{Width}`
			Holder.Position = UDim2.fromOffset((Column - 1) * 120, (Row - 1) * 115)
			Holder.Size = UDim2.fromOffset(116, 110)
			Holder.BackgroundColor3 = Color3.fromRGB(0, 0, 0)
			Holder.BorderSizePixel = 0
			Holder.Parent = Target
			local Frame = Instance.new("ScrollingFrame")
			Frame.Name = "S"
			Frame.Position = UDim2.fromOffset(40, 35)
			Frame.Size = UDim2.fromOffset(Width, Height)
			Frame.CanvasSize = UDim2.fromOffset(0, 300)
			Frame.ScrollBarThickness = Thickness
			Frame.ScrollBarImageTransparency = Transparency
			Frame.BorderSizePixel = 0
			Frame.BackgroundTransparency = 1
			Frame.ScrollBarImageColor3 = Color3.new(1, 1, 1)
			Frame.TopImage = White
			Frame.MidImage = ""
			Frame.BottomImage = White
			Frame.Parent = Holder
		end
	end
end
