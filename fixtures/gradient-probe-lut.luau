-- UIGradient under a magnifier: wide strips whose Scale stretches a small part of the gradient over the strip, so
-- how the colours run between and around the keypoints (a step, corners, the two ends) is read pixel by pixel.
-- compare: --viewport 960x300 --time 0.2 --background 0,0,0
return function(Target)
	local Black, White = Color3.new(0, 0, 0), Color3.new(1, 1, 1)
	local Corners = ColorSequence.new({
		ColorSequenceKeypoint.new(0, Black),
		ColorSequenceKeypoint.new(0.2, White),
		ColorSequenceKeypoint.new(0.5, Black),
		ColorSequenceKeypoint.new(0.5001, White),
		ColorSequenceKeypoint.new(0.9, Black),
		ColorSequenceKeypoint.new(1, White),
	})
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
	-- the step, the corners and the two ends, each a quarter of the gradient wide
	Strip({ Color = Corners, Scale = 4 })
	Strip({ Color = Corners, Scale = 4, Offset = Vector2.new(1.2, 0) })
	Strip({ Color = Corners, Scale = 4, Offset = Vector2.new(-1.6, 0) })
	Strip({ Color = Corners, Scale = 4, Offset = Vector2.new(2, 0) })
	-- closer
	Strip({ Color = Corners, Scale = 16 })
	Strip({ Color = Corners, Scale = 16, Offset = Vector2.new(4.8, 0) })
	Strip({ Color = Corners, Scale = 16, Offset = Vector2.new(8, 0) })
	Strip({ Color = Corners, Scale = 16, Offset = Vector2.new(-8, 0) })
	-- dark ends
	Strip({ Color = ColorSequence.new(Color3.fromRGB(10, 10, 10), Color3.fromRGB(250, 250, 250)), Scale = 4, Offset = Vector2.new(2, 0) })
	Strip({ Color = ColorSequence.new(Color3.fromRGB(20, 20, 20), Color3.fromRGB(24, 24, 24)) })
	Strip({ Color = ColorSequence.new(Color3.fromRGB(10, 10, 10), Color3.fromRGB(14, 14, 14)) })
	Strip({ Color = ColorSequence.new(Color3.fromRGB(0, 0, 0), Color3.fromRGB(40, 40, 40)) })
	-- the frame's own colour and transparency under the gradient
	Strip({ Color = ColorSequence.new(White, Color3.fromRGB(100, 100, 100)) }, { BackgroundColor3 = Color3.fromRGB(128, 200, 60) })
	Strip({ Color = ColorSequence.new(Color3.fromRGB(255, 40, 40), Color3.fromRGB(40, 80, 255)) }, { BackgroundTransparency = 0.5 })
end
