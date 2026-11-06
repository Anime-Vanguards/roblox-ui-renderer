-- ScrollingFrame scroll bars: thickness, colours, transparency, both axes, canvas positions, insets.
return function(Target)
	local function Scroller(X, Y, Props, Canvas)
		local S = Instance.new("ScrollingFrame")
		S.Position = UDim2.fromOffset(X, Y)
		S.Size = UDim2.fromOffset(200, 150)
		S.BackgroundColor3 = Color3.fromRGB(40, 44, 56)
		S.BorderSizePixel = 0
		S.CanvasSize = Canvas or UDim2.fromOffset(200, 600)
		for Key, Value in Props do
			(S :: any)[Key] = Value
		end
		local Fill = Instance.new("Frame")
		Fill.Size = UDim2.new(1, -20, 0, 40)
		Fill.Position = UDim2.fromOffset(10, 10)
		Fill.BackgroundColor3 = Color3.fromRGB(90, 140, 220)
		Fill.BorderSizePixel = 0
		Fill.Parent = S
		S.Parent = Target
		return S
	end
	Scroller(20, 20, {})
	Scroller(240, 20, { ScrollBarThickness = 6 })
	Scroller(460, 20, { ScrollBarThickness = 20, ScrollBarImageColor3 = Color3.fromRGB(255, 120, 0) })
	Scroller(680, 20, { ScrollBarImageTransparency = 0.5, ScrollBarThickness = 10 })
	Scroller(20, 200, { CanvasPosition = Vector2.new(0, 225) })
	Scroller(240, 200, { CanvasPosition = Vector2.new(0, 450), ScrollBarThickness = 8 })
	Scroller(460, 200, { ScrollBarThickness = 8, VerticalScrollBarInset = Enum.ScrollBarInset.ScrollBar }, UDim2.fromOffset(500, 600))
	Scroller(680, 200, { ScrollBarThickness = 8, VerticalScrollBarPosition = Enum.VerticalScrollBarPosition.Left })
	Scroller(20, 380, { ScrollBarThickness = 8, ScrollingDirection = Enum.ScrollingDirection.X }, UDim2.fromOffset(600, 150))
	Scroller(240, 380, { ScrollBarThickness = 12, CanvasPosition = Vector2.new(150, 200) }, UDim2.fromOffset(500, 500))
	Scroller(460, 380, { ScrollBarThickness = 8, TopImage = "", MidImage = "rbxasset://textures/ui/Scroll/scroll-middle.png", BottomImage = "" })
	Scroller(680, 380, { ScrollBarThickness = 0 })
end
