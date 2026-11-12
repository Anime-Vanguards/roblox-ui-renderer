-- UIGradient under a magnifier, at a dark keypoint between two brighter ones in an unevenly spaced list: strips
-- showing 1 / 64 of the gradient around that keypoint, by the lengths of the two segments, the colours beside it
-- and the keypoint's own colour; and the same for a transparency keypoint.
-- compare: --viewport 960x300 --time 0.2 --background 0,0,0
return function(Target)
	local Black, White = Color3.new(0, 0, 0), Color3.new(1, 1, 1)
	local function Keys(...)
		local List = {}
		for _, Pair in { ... } do
			table.insert(List, ColorSequenceKeypoint.new(Pair[1], Pair[2]))
		end
		return ColorSequence.new(List)
	end
	local Count = 0
	-- A strip whose middle shows the gradient at At, 1 / Scale of the gradient wide
	local function Strip(Color, Scale, At, Props)
		local Frame = Instance.new("Frame")
		Frame.Name = `S{Count}`
		Frame.Position = UDim2.fromOffset(20, 10 + Count * 20)
		Frame.Size = UDim2.fromOffset(900, 16)
		Frame.BackgroundColor3 = White
		Frame.BorderSizePixel = 0
		Count += 1
		local Gradient = Instance.new("UIGradient")
		Gradient.Color = Color
		Gradient.Scale = Scale
		Gradient.Offset = Vector2.new((0.5 - At) * Scale, 0)
		for Key, Value in Props or {} do
			(Gradient :: any)[Key] = Value
		end
		Gradient.Parent = Frame
		Frame.Parent = Target
	end
	local Grey = function(Level) return Color3.fromRGB(Level, Level, Level) end
	Strip(Keys({ 0, White }, { 0.3, Black }, { 1, White }), 64, 0.3)
	Strip(Keys({ 0, White }, { 0.6, Black }, { 1, White }), 64, 0.6)
	Strip(Keys({ 0, White }, { 0.1, Black }, { 1, White }), 64, 0.1)
	Strip(Keys({ 0, White }, { 0.3, Black }, { 0.6, White }, { 1, Black }), 64, 0.3)
	Strip(Keys({ 0, White }, { 0.3, Grey(40) }, { 1, White }), 64, 0.3)
	Strip(Keys({ 0, Black }, { 0.3, White }, { 1, Black }), 64, 0.3)
	Strip(Keys({ 0, Grey(100) }, { 0.3, Black }, { 1, Grey(100) }), 64, 0.3)
	Strip(Keys({ 0, White }, { 0.3, Black }, { 0.31, Black }, { 1, White }), 64, 0.305)
	Strip(ColorSequence.new(White), 64, 0.3, { Transparency = NumberSequence.new({ NumberSequenceKeypoint.new(0, 0), NumberSequenceKeypoint.new(0.3, 1), NumberSequenceKeypoint.new(1, 0) }) })
	Strip(Keys({ 0, White }, { 0.3, Black }, { 1, White }), 64, 0.3, { Rotation = 180, Offset = Vector2.new(-(0.5 - 0.3) * 64, 0) })
	Strip(Keys({ 0, Color3.new(1, 0, 0) }, { 0.3, Color3.new(0, 0, 1) }, { 1, Color3.new(1, 0, 0) }), 64, 0.3)
	Strip(Keys({ 0, White }, { 0.3, Black }, { 1, White }), 8, 0.3)
	Strip(Keys({ 0, White }, { 0.25, Black }, { 1, White }), 64, 0.25)
end
