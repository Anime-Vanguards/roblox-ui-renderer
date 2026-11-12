-- UIGradient under a stronger magnifier: strips showing 1 / 16 or 1 / 64 of a gradient around its dark places
-- (a black first keypoint, a black keypoint between two white ones, a black last keypoint), where one level is a
-- small step of the squared colour, so the steps the colours are stored in are read as runs of equal pixels.
-- compare: --viewport 960x520 --time 0.2 --background 0,0,0
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
	local Uneven = Keys({ 0, Black }, { 0.3, White }, { 1, Black })
	local Valley = Keys({ 0, White }, { 0.3, Black }, { 1, White })
	local Even3 = Keys({ 0, Black }, { 0.5, White }, { 1, Black })
	local Even4 = Keys({ 0, Black }, { 1 / 3, White }, { 2 / 3, Black }, { 1, White })
	local Two = Keys({ 0, Black }, { 1, White })
	-- 0-3: an uneven list's start and end
	Strip(Uneven, 16, 0.03)
	Strip(Uneven, 16, 0.97)
	Strip(Uneven, 64, 0.007)
	Strip(Uneven, 64, 0.993)
	-- 4-6: a black keypoint inside an uneven list
	Strip(Valley, 16, 0.3)
	Strip(Valley, 64, 0.3)
	Strip(Valley, 256, 0.3)
	-- 7-11: evenly spaced lists of three and four keypoints
	Strip(Even3, 16, 0.03)
	Strip(Even3, 16, 0.97)
	Strip(Even3, 64, 0.007)
	Strip(Even4, 16, 0.03)
	Strip(Even4, 16, 2 / 3)
	-- 12-14: two keypoints
	Strip(Two, 16, 0.03)
	Strip(Two, 64, 0.007)
	Strip(Keys({ 0, White }, { 1, Black }), 64, 0.993)
	-- 15-18: uneven lists with other gaps
	Strip(Keys({ 0, Black }, { 0.9, White }, { 1, Black }), 16, 0.03)
	Strip(Keys({ 0, Black }, { 0.9, White }, { 1, Black }), 16, 0.97)
	Strip(Keys({ 0, Black }, { 0.05, White }, { 1, Black }), 64, 0.007)
	Strip(Keys({ 0, Black }, { 0.01, White }, { 1, Black }), 64, 0.007)
	-- 19-21: a transparency list merged into a two-keypoint colour list, and tiling
	Strip(Two, 16, 0.03, { Transparency = NumberSequence.new({ NumberSequenceKeypoint.new(0, 0), NumberSequenceKeypoint.new(0.3, 0), NumberSequenceKeypoint.new(1, 0) }) })
	Strip(Uneven, 16, 0, { TileMode = Enum.GradientTileMode.Repeat })
	Strip(Uneven, 16, 0, { TileMode = Enum.GradientTileMode.Mirror })
	-- 22-24: five evenly spaced keypoints, and Rotation 180
	Strip(Keys({ 0, Black }, { 0.25, White }, { 0.5, Black }, { 0.75, White }, { 1, Black }), 16, 0.5)
	Strip(Uneven, 16, 0.03, { Rotation = 180, Offset = Vector2.new(-(0.5 - 0.03) * 16, 0) })
	Strip(Two, 256, 0.002)
end
