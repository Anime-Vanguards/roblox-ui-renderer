-- Horizontal scroll bar probe (companion of image-probe-scroll.luau): ScrollingFrames scrolling on X whose thumb
-- parts are drawn with a solid white texture on black, so each part's snapped quad shows. Per config one frame
-- with only the end caps and one with only the middle. Configs: wider canvases (scrolled and not), a canvas as
-- wide as the frame (a stage list's modes row: the bar still reserves its thickness), fractional
-- thicknesses, and an XY frame with both bars.
-- compare: --viewport 1200x900 --time 0.5 --background 0,0,0
-- analysis: python tools/analysis/image_scroll.py image-probe-hscroll
return function(Target)
	local White = "rbxasset://textures/ui/InGameMenu/WhiteSquare.png"
	-- { width, height, canvas width, canvas height, thickness, CanvasPosition.X, direction }
	local Configs = {
		{ 500, 60, 800, 0, 6, 0, Enum.ScrollingDirection.X },
		{ 500, 60, 800, 0, 6, 150, Enum.ScrollingDirection.X },
		{ 500, 60, 500, 0, 4, 0, Enum.ScrollingDirection.X },
		{ 500, 52, 500, 52, 2.517482578754425, 0, Enum.ScrollingDirection.X },
		{ 500, 60, 1337, 0, 5.359801769256592, 300, Enum.ScrollingDirection.X },
		{ 500, 80, 900, 200, 8, 100, Enum.ScrollingDirection.XY },
	}
	for Index, Config in Configs do
		local Width, Height, CanvasWidth, CanvasHeight, Thickness, Position, Direction = table.unpack(Config)
		for Variant = 1, 2 do
			local Frame = Instance.new("ScrollingFrame")
			Frame.Name = `H{Variant}_{Index}`
			Frame.Position = UDim2.fromOffset(20 + (Variant - 1) * 560, 10 + (Index - 1) * 140)
			Frame.Size = UDim2.fromOffset(Width, Height)
			Frame.CanvasSize = UDim2.fromOffset(CanvasWidth, CanvasHeight)
			Frame.ScrollBarThickness = Thickness
			Frame.ScrollBarImageColor3 = Color3.new(1, 1, 1)
			Frame.ScrollBarImageTransparency = 0
			Frame.BackgroundTransparency = 1
			Frame.BorderSizePixel = 0
			Frame.ScrollingDirection = Direction
			if Variant == 1 then
				Frame.TopImage = White
				Frame.MidImage = ""
				Frame.BottomImage = White
			else
				Frame.TopImage = ""
				Frame.MidImage = White
				Frame.BottomImage = ""
			end
			Frame.Parent = Target
			Frame.CanvasPosition = Vector2.new(Position, 0)
		end
	end
end
