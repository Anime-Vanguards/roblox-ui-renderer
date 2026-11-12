-- UIGradient on turned elements: which space the gradient's position is measured in. Frames 150 x 90 turned by
-- 0, 30, -45, 90 and 180 degrees (columns) under a gradient turned by 0, 90, 45 and -30 degrees (rows 1 to 4,
-- black to white), then a four-keypoint list at gradient Rotation 90 (row 5), an uneven list (row 6) and the
-- uneven list on frames with a 3 px red border (row 7).
-- compare: --viewport 1100x1160 --time 0.2 --background 0,0,60
return function(Target)
	local Black, White = Color3.new(0, 0, 0), Color3.new(1, 1, 1)
	local Two = ColorSequence.new(Black, White)
	local Four = ColorSequence.new({
		ColorSequenceKeypoint.new(0, Color3.new(1, 0, 0)),
		ColorSequenceKeypoint.new(1 / 3, Color3.new(0, 1, 0)),
		ColorSequenceKeypoint.new(2 / 3, Color3.new(0, 0, 1)),
		ColorSequenceKeypoint.new(1, Color3.new(1, 1, 0)),
	})
	local Uneven = ColorSequence.new({
		ColorSequenceKeypoint.new(0, Color3.new(1, 0, 0)),
		ColorSequenceKeypoint.new(0.2, Color3.new(1, 1, 1)),
		ColorSequenceKeypoint.new(1, Color3.new(0, 0, 1)),
	})
	local Rows = {
		{ Two, 0 }, { Two, 90 }, { Two, 45 }, { Two, -30 }, { Four, 90 }, { Uneven, 0 }, { Uneven, 0, 3 },
	}
	for RowIndex, Row in Rows do
		for Column, Turn in { 0, 30, -45, 90, 180 } do
			local Frame = Instance.new("Frame")
			Frame.Name = `R{RowIndex}C{Column}`
			Frame.Position = UDim2.fromOffset(40 + (Column - 1) * 210, 40 + (RowIndex - 1) * 160)
			Frame.Size = UDim2.fromOffset(150, 90)
			Frame.BackgroundColor3 = White
			Frame.BorderSizePixel = Row[3] or 0
			Frame.BorderColor3 = Color3.new(1, 0, 0)
			Frame.Rotation = Turn
			local Gradient = Instance.new("UIGradient")
			Gradient.Color = Row[1]
			Gradient.Rotation = Row[2]
			Gradient.Parent = Frame
			Frame.Parent = Target
		end
	end
end
