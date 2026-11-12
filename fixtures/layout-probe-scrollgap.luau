-- ScrollingFrames whose scroll bar thickness leaves a gap between the window and the bar (notes/layout.md
-- "Scroll bar gap"): thickness 4 to 13 with both bars, with a border, without one, over a parent of another
-- colour, with a bar on the left, and with only the vertical bar.
-- rur compare --fixture fixtures/layout-probe-scrollgap.luau --viewport 1200x700 --time 0.2 --background 0,60,0
return function(Target)
	local Count = 0
	local function Scroller(Name, Thickness, Props)
		local Holder = Instance.new("Frame")
		Holder.Name = Name
		Holder.BorderSizePixel = 0
		Holder.BackgroundColor3 = Color3.fromRGB(200, 40, 200)
		Holder.Size = UDim2.fromOffset(110, 100)
		Holder.Position = UDim2.fromOffset(6 + (Count % 10) * 118, 6 + (Count // 10) * 108)
		Holder.Parent = Target
		Count += 1
		local Frame = Instance.new("ScrollingFrame")
		Frame.Name = "Scroll"
		Frame.Position = UDim2.fromOffset(7, 7)
		Frame.Size = UDim2.fromOffset(95, 85)
		Frame.BackgroundColor3 = Color3.fromRGB(30, 30, 30)
		Frame.BorderColor3 = Color3.fromRGB(255, 220, 0)
		Frame.BorderSizePixel = 1
		Frame.ScrollBarThickness = Thickness
		Frame.CanvasSize = UDim2.fromOffset(300, 400)
		Frame.CanvasPosition = Vector2.new(30 + Count * 3, 60 + Count * 7)
		for Key, Value in Props do
			(Frame :: any)[Key] = Value
		end
		Frame.Parent = Holder
		local Content = Instance.new("Frame")
		Content.Name = "Content"
		Content.BorderSizePixel = 0
		Content.BackgroundColor3 = Color3.fromRGB(60, 160, 220)
		Content.Size = UDim2.fromOffset(280, 380)
		Content.Position = UDim2.fromOffset(10, 10)
		Content.Parent = Frame
	end
	for Thickness = 4, 13 do
		Scroller(`Both{Thickness}`, Thickness, {})
	end
	for Thickness = 4, 13 do
		Scroller(`NoBorder{Thickness}`, Thickness, { BorderSizePixel = 0 })
	end
	for Thickness = 4, 13 do
		Scroller(`Clear{Thickness}`, Thickness, { BackgroundTransparency = 1, BorderSizePixel = 0 })
	end
	for Thickness = 4, 13 do
		Scroller(`Left{Thickness}`, Thickness, { VerticalScrollBarPosition = Enum.VerticalScrollBarPosition.Left })
	end
	for Thickness = 4, 13 do
		Scroller(`Vertical{Thickness}`, Thickness, { CanvasSize = UDim2.fromOffset(0, 400) })
	end
	for Thickness = 4, 13 do
		Scroller(`Odd{Thickness}`, Thickness, { Size = UDim2.fromOffset(94, 86), Position = UDim2.fromOffset(8, 6), BorderSizePixel = 3 })
	end
end
