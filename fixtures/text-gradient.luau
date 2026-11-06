-- UIGradient on text: colour and transparency sequences, rotation, with and without backgrounds.
return function(Target)
	local function Label(X, Y, GradientProps, Props)
		local L = Instance.new("TextLabel")
		L.Position = UDim2.fromOffset(X, Y)
		L.Size = UDim2.fromOffset(300, 60)
		L.BackgroundTransparency = 1
		L.TextColor3 = Color3.new(1, 1, 1)
		L.FontFace = Font.new("rbxasset://fonts/families/FredokaOne.json")
		L.TextSize = 40
		L.Text = "Gradient Text"
		for Key, Value in Props or {} do
			(L :: any)[Key] = Value
		end
		local G = Instance.new("UIGradient")
		for Key, Value in GradientProps do
			(G :: any)[Key] = Value
		end
		G.Parent = L
		L.Parent = Target
		return L
	end
	Label(20, 20, { Color = ColorSequence.new(Color3.fromRGB(255, 80, 80), Color3.fromRGB(80, 120, 255)) })
	Label(340, 20, { Color = ColorSequence.new(Color3.fromRGB(255, 220, 60), Color3.fromRGB(255, 80, 0)), Rotation = 90 })
	Label(20, 100, { Transparency = NumberSequence.new(0, 1) })
	Label(340, 100, { Color = ColorSequence.new(Color3.fromRGB(0, 255, 160), Color3.fromRGB(0, 80, 255)), Rotation = 45 }, { BackgroundTransparency = 0, BackgroundColor3 = Color3.fromRGB(60, 60, 70) })
	Label(20, 180, { Color = ColorSequence.new(Color3.fromRGB(255, 255, 255), Color3.fromRGB(120, 120, 120)), Rotation = 90, Offset = Vector2.new(0, 0.3) }, { TextColor3 = Color3.fromRGB(255, 200, 100) })
	Label(340, 180, { Transparency = NumberSequence.new({ NumberSequenceKeypoint.new(0, 0), NumberSequenceKeypoint.new(0.5, 0.8), NumberSequenceKeypoint.new(1, 0) }) }, { TextTransparency = 0.3 })
end
