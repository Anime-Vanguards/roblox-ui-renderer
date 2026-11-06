-- Border UIStroke on frames at fractional positions (a stage list's Design/Stroke: x 96.5035 at
-- 1x, Outer 1.3846 px with a vertical UIGradient and UICorner 8.39, Inner 2.52 px black). Columns:
-- x fractions 0, .25, .5, .5035, .75; rows: plain Outer, + UICorner, + UIGradient, + both, the story's
-- Outer + Inner pair.
-- compare: --viewport 1200x520 --time 0.2 --background 0,0,0
return function(Target)
	local Fractions = { 0, 0.25, 0.5, 0.5035, 0.75 }
	local Rows = {
		{ Corner = false, Gradient = false, Pair = false },
		{ Corner = true, Gradient = false, Pair = false },
		{ Corner = false, Gradient = true, Pair = false },
		{ Corner = true, Gradient = true, Pair = false },
		{ Corner = true, Gradient = true, Pair = true },
	}
	for RowIndex, Row in Rows do
		for Column, Fraction in Fractions do
			local Frame = Instance.new("Frame")
			Frame.Name = `R{RowIndex}C{Column}`
			Frame.Position = UDim2.fromOffset(30 + (Column - 1) * 200 + Fraction, 30 + (RowIndex - 1) * 96 + Fraction)
			Frame.Size = UDim2.fromOffset(160, 70)
			Frame.BackgroundTransparency = 1
			Frame.BorderSizePixel = 0
			if Row.Corner then
				local Corner = Instance.new("UICorner")
				Corner.CornerRadius = UDim.new(0, 8.3916)
				Corner.Parent = Frame
			end
			local Stroke = Instance.new("UIStroke")
			Stroke.Color = Color3.new(1, 1, 1)
			Stroke.Thickness = 1.384615421295166
			Stroke.BorderStrokePosition = Enum.BorderStrokePosition.Outer
			Stroke.Parent = Frame
			if Row.Gradient then
				local Gradient = Instance.new("UIGradient")
				Gradient.Color = ColorSequence.new(Color3.new(0.0784, 0.9843, 1), Color3.new(0.1333, 0.3373, 1))
				Gradient.Rotation = 90
				Gradient.Parent = Stroke
			end
			if Row.Pair then
				local Inner = Instance.new("UIStroke")
				Inner.Color = Color3.new(0, 0, 0)
				Inner.Thickness = 2.5174825191497803
				Inner.BorderStrokePosition = Enum.BorderStrokePosition.Inner
				Inner.Parent = Frame
				Frame.BackgroundTransparency = 0
				Frame.BackgroundColor3 = Color3.fromRGB(21, 37, 52)
			end
			Frame.Parent = Target
		end
	end
end
