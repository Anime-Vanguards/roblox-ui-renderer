-- UIGradient keypoints close together: strips showing a quarter of a gradient that has two keypoints a small
-- distance apart outside the part shown, by that distance; then the same with the pair in the transparency.
-- compare: --viewport 960x520 --time 0.2 --background 0,0,0
return function(Target)
	local Black, White = Color3.new(0, 0, 0), Color3.new(1, 1, 1)
	local Count = 0
	local function Strip(Props)
		local Frame = Instance.new("Frame")
		Frame.Name = `S{Count}`
		Frame.Position = UDim2.fromOffset(20, 10 + Count * 20)
		Frame.Size = UDim2.fromOffset(900, 16)
		Frame.BackgroundColor3 = White
		Frame.BorderSizePixel = 0
		Count += 1
		local Gradient = Instance.new("UIGradient")
		for Key, Value in Props do
			(Gradient :: any)[Key] = Value
		end
		Gradient.Parent = Frame
		Frame.Parent = Target
	end
	local Gaps = { 0.0001, 0.0005, 0.001, 0.002, 0.003, 0.004, 0.005, 0.008, 0.01, 0.02, 0.05 }
	for _, Gap in Gaps do
		Strip({
			Scale = 4,
			Color = ColorSequence.new({
				ColorSequenceKeypoint.new(0, Black),
				ColorSequenceKeypoint.new(0.3, White),
				ColorSequenceKeypoint.new(0.8, Black),
				ColorSequenceKeypoint.new(0.8 + Gap, White),
				ColorSequenceKeypoint.new(1, Black),
			}),
		})
	end
	for _, Gap in Gaps do
		Strip({
			Scale = 4,
			Color = ColorSequence.new({
				ColorSequenceKeypoint.new(0, Black),
				ColorSequenceKeypoint.new(0.3, White),
				ColorSequenceKeypoint.new(1, Black),
			}),
			Transparency = NumberSequence.new({
				NumberSequenceKeypoint.new(0, 0),
				NumberSequenceKeypoint.new(0.8, 0),
				NumberSequenceKeypoint.new(0.8 + Gap, 0.5),
				NumberSequenceKeypoint.new(1, 0),
			}),
		})
	end
	-- three keypoints in all, two of them close
	Strip({ Scale = 4, Color = ColorSequence.new({ ColorSequenceKeypoint.new(0, White), ColorSequenceKeypoint.new(0.9, Black), ColorSequenceKeypoint.new(0.9001, White), ColorSequenceKeypoint.new(1, White) }) })
	Strip({ Scale = 4, Color = ColorSequence.new({ ColorSequenceKeypoint.new(0, White), ColorSequenceKeypoint.new(0.0001, Black), ColorSequenceKeypoint.new(1, White) }) })
end
