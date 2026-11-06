-- Scroll bar thumb geometry probe: ScrollingFrames whose thumbs are drawn with a solid white texture, so every
-- part's snapped quad shows as hard white edges on black. Row 1 draws only the end caps (TopImage and
-- BottomImage, MidImage empty), row 2 only the middle (MidImage). Heights, canvas sizes, thicknesses and
-- CanvasPositions from the parity stories whose thumbs ended a pixel off (six list and
-- window stories) plus scrolled ones; y positions with fractions.
-- compare: --viewport 1200x900 --time 0.5 --background 0,0,0
-- analysis: python tools/analysis/image_scroll.py image-probe-scroll
return function(Target)
	local White = "rbxasset://textures/ui/InGameMenu/WhiteSquare.png"
	-- { height, canvas height, thickness, CanvasPosition.Y, y fraction }
	local Configs = {
		{ 338, 374, 4, 0, 0.246 },
		{ 350, 449, 12, 0, 0 },
		{ 323.3747, 1337.2705, 5.359801769256592, 0, 0.228 },
		{ 207, 261, 5, 0, 0.587 },
		{ 344, 717.6533, 6, 0, 0 },
		{ 348, 410, 5, 0, 0 },
		{ 300, 800, 10, 100, 0.3 },
		{ 300, 800, 10, 500, 0.7 },
		{ 250.5, 300.25, 3, 0, 0.1 },
		{ 380, 3000, 8, 1000, 0.45 },
	}
	for Row = 1, 2 do
		for Index, Config in Configs do
			local Height, Canvas, Thickness, Position, Fraction = table.unpack(Config)
			local Frame = Instance.new("ScrollingFrame")
			Frame.Name = `S{Row}_{Index}`
			Frame.Position = UDim2.fromOffset(20 + (Index - 1) * 115 + Fraction, (Row - 1) * 440 + 10 + Fraction)
			Frame.Size = UDim2.fromOffset(60, Height)
			Frame.CanvasSize = UDim2.fromOffset(0, Canvas)
			Frame.ScrollBarThickness = Thickness
			Frame.ScrollBarImageColor3 = Color3.new(1, 1, 1)
			Frame.ScrollBarImageTransparency = 0
			Frame.BackgroundTransparency = 1
			Frame.BorderSizePixel = 0
			Frame.ScrollingDirection = Enum.ScrollingDirection.Y
			if Row == 1 then
				Frame.TopImage = White
				Frame.MidImage = ""
				Frame.BottomImage = White
			else
				Frame.TopImage = ""
				Frame.MidImage = White
				Frame.BottomImage = ""
			end
			Frame.Parent = Target
			Frame.CanvasPosition = Vector2.new(0, Position)
		end
	end
end
