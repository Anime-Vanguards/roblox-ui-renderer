-- Scroll bars under a UIScale (companion of image-probe-scroll.luau): a stage list's modes row (UIScale
-- 0.8392, ScrollBarThickness 3, a horizontal bar on a canvas as wide as the frame) draws its bar 1 px higher than
-- the scaled thickness puts it. Vertical and horizontal bars with solid white parts on black under UIScale 0.8392
-- and 1.25, thickness 3 and 6, scrolled and not.
-- compare: --viewport 1200x900 --time 0.5 --background 0,0,0
-- analysis: python tools/analysis/image_scroll.py image-probe-scroll-scale
return function(Target)
	local White = "rbxasset://textures/ui/InGameMenu/WhiteSquare.png"
	for Group, Scale in { 0.8391608595848083, 1.25 } do
		local Holder = Instance.new("Frame")
		Holder.Name = `Scale{Group}`
		Holder.Position = UDim2.fromOffset((Group - 1) * 600, 0)
		Holder.Size = UDim2.fromOffset(600 / Scale, 900 / Scale)
		Holder.BackgroundTransparency = 1
		Holder.Parent = Target
		local UIScale = Instance.new("UIScale")
		UIScale.Scale = Scale
		UIScale.Parent = Holder
		-- { x, y, width, height, canvas width, canvas height, thickness, CanvasPosition, direction }
		local Configs = {
			{ 10, 10, 60, 300, 0, 500, 3, Vector2.new(0, 0), Enum.ScrollingDirection.Y },
			{ 90, 10, 60, 300, 0, 500, 6, Vector2.new(0, 120), Enum.ScrollingDirection.Y },
			{ 170, 10, 250, 52, 250, 52, 3, Vector2.new(0, 0), Enum.ScrollingDirection.X },
			{ 170, 90, 250, 60, 400, 0, 6, Vector2.new(50, 0), Enum.ScrollingDirection.X },
		}
		for Index, Config in Configs do
			local X, Y, Width, Height, CanvasWidth, CanvasHeight, Thickness, Position, Direction = table.unpack(Config)
			for Variant = 1, 2 do
				local Frame = Instance.new("ScrollingFrame")
				Frame.Name = `G{Group}_{Index}_{Variant}`
				Frame.Position = UDim2.fromOffset(X, Y + (Variant - 1) * 330)
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
				Frame.Parent = Holder
				Frame.CanvasPosition = Position
			end
		end
	end
end
