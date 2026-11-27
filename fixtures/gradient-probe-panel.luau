-- UIGradient between two saturated colours: the rounded, stroked banner panel of a 600x300 board (a white frame under
-- orange (255, 80, 40) to blue (40, 120, 255)), then the same two colours on bare strips, turned 180 degrees, over a
-- coloured frame, and orange to blue against red to blue and white to black, so the colour halfway along is read
-- pixel by pixel. A plain mix of the colours is grey-purple (147.5, 100, 147.5) halfway; a client mixes them in
-- squared space, (182, 102, 182) (docs/images.md, "UIGradient").
-- compare: --viewport 960x620 --time 0.2 --background 0,0,0
return function(Target)
	local Orange, Blue = Color3.fromRGB(255, 80, 40), Color3.fromRGB(40, 120, 255)
	local function Keys(...)
		local List = {}
		for _, Pair in { ... } do
			table.insert(List, ColorSequenceKeypoint.new(Pair[1], Pair[2]))
		end
		return ColorSequence.new(List)
	end
	local Banner = Keys({ 0, Orange }, { 1, Blue })

	local Board = Instance.new("Frame")
	Board.Name = "Board"
	Board.Position = UDim2.fromOffset(10, 10)
	Board.Size = UDim2.fromOffset(600, 300)
	Board.BackgroundColor3 = Color3.fromRGB(40, 40, 50)
	Board.BorderSizePixel = 0
	Board.Parent = Target
	local Panel = Instance.new("Frame")
	Panel.Name = "Panel"
	Panel.Position = UDim2.fromScale(0.36, 0.05)
	Panel.Size = UDim2.fromScale(0.6, 0.4)
	Panel.BackgroundColor3 = Color3.new(1, 1, 1)
	Panel.BorderSizePixel = 0
	local Corner = Instance.new("UICorner")
	Corner.CornerRadius = UDim.new(0, 16)
	Corner.Parent = Panel
	local Stroke = Instance.new("UIStroke")
	Stroke.Thickness = 4
	Stroke.Color = Color3.fromRGB(20, 20, 60)
	Stroke.Parent = Panel
	local PanelGradient = Instance.new("UIGradient")
	PanelGradient.Color = Banner
	PanelGradient.Parent = Panel
	Panel.Parent = Board

	local Count = 0
	local function Strip(Props, FrameProps)
		local Frame = Instance.new("Frame")
		Frame.Name = `S{Count}`
		Frame.Position = UDim2.fromOffset(20, 330 + Count * 40)
		Frame.Size = UDim2.fromOffset(900, 32)
		Frame.BackgroundColor3 = Color3.new(1, 1, 1)
		Frame.BorderSizePixel = 0
		for Key, Value in FrameProps or {} do
			(Frame :: any)[Key] = Value
		end
		Count += 1
		local Gradient = Instance.new("UIGradient")
		for Key, Value in Props do
			(Gradient :: any)[Key] = Value
		end
		Gradient.Parent = Frame
		Frame.Parent = Target
	end
	Strip({ Color = Banner })
	Strip({ Color = Banner, Rotation = 180 })
	Strip({ Color = Banner }, { BackgroundColor3 = Color3.fromRGB(200, 230, 120) })
	Strip({ Color = Keys({ 0, Color3.fromRGB(255, 0, 0) }, { 1, Color3.fromRGB(0, 0, 255) }) })
	Strip({ Color = Keys({ 0, Color3.new(1, 1, 1) }, { 1, Color3.new(0, 0, 0) }) })
	Strip({ Color = Keys({ 0, Orange }, { 0.4, Color3.fromRGB(128, 128, 128) }, { 1, Blue }) })
	Strip({ Color = Banner, Scale = 4 })
end
