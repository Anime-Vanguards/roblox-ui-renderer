-- Plain frames: borders (Outline/Middle/Inset, 1-4px), transparency, odd offsets.
return function(Target)
	local function Frame(X, Y, W, H, Color, Props)
		local F = Instance.new("Frame")
		F.Position = UDim2.fromOffset(X, Y)
		F.Size = UDim2.fromOffset(W, H)
		F.BackgroundColor3 = Color
		F.BorderSizePixel = 0
		for Key, Value in Props or {} do
			(F :: any)[Key] = Value
		end
		F.Parent = Target
		return F
	end

	local Modes = { Enum.BorderMode.Outline, Enum.BorderMode.Middle, Enum.BorderMode.Inset }
	for ModeIndex, Mode in Modes do
		for Size = 1, 4 do
			Frame(20 + (Size - 1) * 90, 20 + (ModeIndex - 1) * 90, 61, 61, Color3.fromRGB(80, 160, 255), {
				BorderSizePixel = Size,
				BorderMode = Mode,
				BorderColor3 = Color3.fromRGB(255, 200, 40),
			})
		end
	end

	for Index, Transparency in { 0, 0.25, 0.5, 0.75, 0.9 } do
		Frame(400 + (Index - 1) * 70, 20, 60, 60, Color3.fromRGB(255, 60, 90), { BackgroundTransparency = Transparency, BorderSizePixel = 2, BorderColor3 = Color3.new(1, 1, 1) })
	end

	for Index = 0, 7 do
		Frame(401 + Index * 45, 111 + Index, 31 + Index, 21 + Index, Color3.fromRGB(120, 255, 140))
	end

	Frame(400, 200, 200, 80, Color3.fromRGB(40, 40, 60), { BorderSizePixel = 1, BorderColor3 = Color3.fromRGB(255, 255, 255), BackgroundTransparency = 1 })
end
