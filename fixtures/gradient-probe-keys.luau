-- UIGradient under a magnifier, by the number of keypoints: strips whose Scale shows a quarter of a gradient of two
-- to six keypoints, so the run of the colours from the strip's ends to the keypoints inside is read pixel by pixel.
-- compare: --viewport 960x420 --time 0.2 --background 0,0,0
return function(Target)
	local Black, White = Color3.new(0, 0, 0), Color3.new(1, 1, 1)
	local function Keys(...)
		local List = {}
		for _, Pair in { ... } do
			table.insert(List, ColorSequenceKeypoint.new(Pair[1], Pair[2]))
		end
		return ColorSequence.new(List)
	end
	local K2 = Keys({ 0, White }, { 1, Black })
	local K3 = Keys({ 0, Black }, { 0.3, White }, { 1, Black })
	local K4 = Keys({ 0, Black }, { 0.3, White }, { 0.8, Black }, { 1, White })
	local K5 = Keys({ 0, Black }, { 0.3, White }, { 0.8, Black }, { 0.9, White }, { 1, Black })
	local K6 = Keys({ 0, Black }, { 0.1, White }, { 0.3, Black }, { 0.8, White }, { 0.9, Black }, { 1, White })
	local Count = 0
	local function Strip(Props, FrameProps)
		local Frame = Instance.new("Frame")
		Frame.Name = `S{Count}`
		Frame.Position = UDim2.fromOffset(20, 10 + Count * 20)
		Frame.Size = UDim2.fromOffset(900, 16)
		Frame.BackgroundColor3 = White
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
	Strip({ Color = K2, Scale = 4 })
	Strip({ Color = K3, Scale = 4 })
	Strip({ Color = K4, Scale = 4 })
	Strip({ Color = K5, Scale = 4 })
	Strip({ Color = K6, Scale = 4 })
	Strip({ Color = K3, Scale = 2 })
	Strip({ Color = K3, Offset = Vector2.new(0.3, 0) })
	Strip({ Color = K3, Scale = 4, Rotation = 180 })
	Strip({ Color = K2, Scale = 4, Transparency = NumberSequence.new({ NumberSequenceKeypoint.new(0, 0), NumberSequenceKeypoint.new(0.3, 0.5), NumberSequenceKeypoint.new(1, 0) }) })
	Strip({ Color = K3, Scale = 0.25, TileMode = Enum.GradientTileMode.Repeat })
	Strip({ Color = K3, Scale = 0.25, TileMode = Enum.GradientTileMode.Mirror })
	Strip({ Color = K3, Scale = 4, TileMode = Enum.GradientTileMode.Repeat })
	Strip({ Color = K3, Scale = 0.5 })
	Strip({ Color = K3 })
	Strip({ Color = K3, Scale = 4, Type = Enum.GradientType.Radial })
	Strip({ Color = K3, Scale = 0.1, Type = Enum.GradientType.Radial })
	Strip({ Color = K2, Scale = 4, Rotation = 180 })
	Strip({ Color = K2, Scale = 4, Offset = Vector2.new(1, 0) })
	Strip({ Color = K3, Scale = 4, Offset = Vector2.new(-0.6, 0) })
	Strip({ Color = K3, Scale = 4, Offset = Vector2.new(0.4, 0) })
end
