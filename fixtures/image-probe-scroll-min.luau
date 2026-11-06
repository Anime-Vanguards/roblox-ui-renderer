-- Scroll bar thumbs at their minimum length (companion of image-probe-scroll.luau): the thumb visible *
-- visible / canvas is shorter than twice the thickness, so the thumb is clamped. Solid white parts on black
-- (row 1: end caps only, row 2: middle only), vertical bars; scrolled ones check the offset of a clamped thumb.
-- compare: --viewport 1200x900 --time 0.5 --background 0,0,0
return function(Target)
	local White = "rbxasset://textures/ui/InGameMenu/WhiteSquare.png"
	-- { height, canvas height, thickness, CanvasPosition.Y }
	local Configs = {
		{ 150, 600, 20, 0 },
		{ 150, 600, 24, 0 },
		{ 150, 600, 30, 0 },
		{ 300, 3000, 8, 0 },
		{ 300, 3000, 12, 0 },
		{ 300, 3000, 12, 1350 },
		{ 300, 3000, 20, 2700 },
		{ 200, 2000, 7, 0 },
		{ 300, 900, 50, 300 },
	}
	for Row = 1, 2 do
		for Index, Config in Configs do
			local Height, Canvas, Thickness, Position = table.unpack(Config)
			local Frame = Instance.new("ScrollingFrame")
			Frame.Name = `M{Row}_{Index}`
			Frame.Position = UDim2.fromOffset(20 + (Index - 1) * 125, (Row - 1) * 420 + 10)
			Frame.Size = UDim2.fromOffset(80, Height)
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
