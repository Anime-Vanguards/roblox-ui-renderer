-- Edge snapping of UIGradient frames: odd logical coordinates (x.5 physical ties at 1.5x), with
-- BorderSizePixel 0/1/2 and each BorderMode, next to the same frames without a gradient.
return function(Target)
	local Modes = { Enum.BorderMode.Outline, Enum.BorderMode.Middle, Enum.BorderMode.Inset }
	local Index = 0
	for _, Gradient in { false, true } do
		for _, Border in { 0, 1, 2 } do
			for ModeIndex, Mode in Modes do
				if Border == 0 and ModeIndex > 1 then
					continue
				end
				for Offset = 0, 1 do
					local Frame = Instance.new("Frame")
					Frame.BackgroundColor3 = Color3.fromRGB(230, 230, 240)
					Frame.BorderColor3 = Color3.fromRGB(255, 120, 40)
					Frame.BorderSizePixel = Border
					Frame.BorderMode = Mode
					Frame.Position = UDim2.fromOffset(21 + (Index % 7) * 70 + Offset, 21 + (Index // 7) * 60 + Offset)
					Frame.Size = UDim2.fromOffset(47 + Offset, 33 + Offset)
					if Gradient then
						local UIGradient = Instance.new("UIGradient")
						UIGradient.Color = ColorSequence.new(Color3.fromRGB(80, 200, 255), Color3.fromRGB(255, 80, 160))
						UIGradient.Rotation = 30
						UIGradient.Parent = Frame
					end
					Frame.Parent = Target
					Index += 1
				end
			end
		end
	end
end
