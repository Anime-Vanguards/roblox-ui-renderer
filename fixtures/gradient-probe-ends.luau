-- UIGradient ends: black to white at a keypoint and back to black, half as wide as its strip, by where the white
-- keypoint is, so the colour beyond the last keypoint is read from the part of the strip the gradient does not reach.
-- compare: --viewport 960x420 --time 0.2 --background 0,0,0
return function(Target)
	local Black, White = Color3.new(0, 0, 0), Color3.new(1, 1, 1)
	local Count = 0
	for Step = 1, 19 do
		local Frame = Instance.new("Frame")
		Frame.Name = `S{Count}`
		Frame.Position = UDim2.fromOffset(20, 10 + Count * 20)
		Frame.Size = UDim2.fromOffset(900, 16)
		Frame.BackgroundColor3 = White
		Frame.BorderSizePixel = 0
		Count += 1
		local Gradient = Instance.new("UIGradient")
		Gradient.Scale = 0.5
		Gradient.Color = ColorSequence.new({
			ColorSequenceKeypoint.new(0, Black),
			ColorSequenceKeypoint.new(Step / 20, White),
			ColorSequenceKeypoint.new(1, Black),
		})
		Gradient.Parent = Frame
		Frame.Parent = Target
	end
end
