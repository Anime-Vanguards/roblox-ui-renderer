-- UIGradient ends, by the shape of the keypoint list: gradients ending in black, drawn half as wide as their strip
-- (or moved off it), so the colour beyond the last keypoint is read where the gradient does not reach.
-- compare: --viewport 960x420 --time 0.2 --background 0,0,0
return function(Target)
	local Black, White, Grey = Color3.new(0, 0, 0), Color3.new(1, 1, 1), Color3.fromRGB(128, 128, 128)
	local function Keys(...)
		local List = {}
		for _, Pair in { ... } do
			table.insert(List, ColorSequenceKeypoint.new(Pair[1], Pair[2]))
		end
		return ColorSequence.new(List)
	end
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
	local K3 = Keys({ 0, Black }, { 0.3, White }, { 1, Black })
	Strip({ Scale = 0.5, Color = Keys({ 0, White }, { 0.5, Grey }, { 1, Black }) })
	Strip({ Scale = 0.5, Color = Keys({ 0, Black }, { 1 / 3, White }, { 2 / 3, Grey }, { 1, Black }) })
	Strip({ Scale = 0.5, Color = Keys({ 0, Black }, { 0.25, White }, { 0.5, Grey }, { 0.75, White }, { 1, Black }) })
	Strip({ Scale = 0.5, Color = Keys({ 0, Black }, { 0.5, White }, { 1, Black }), Transparency = NumberSequence.new({ NumberSequenceKeypoint.new(0, 0), NumberSequenceKeypoint.new(0.3, 0), NumberSequenceKeypoint.new(1, 0) }) })
	Strip({ Scale = 0.5, Color = Keys({ 0, White }, { 1, Black }) })
	Strip({ Scale = 0.5, Color = K3, TileMode = Enum.GradientTileMode.Mirror })
	Strip({ Scale = 0.5, Color = K3, TileMode = Enum.GradientTileMode.Repeat })
	Strip({ Scale = 0.5, Color = Keys({ 0, Black }, { 0.3, White }, { 0.3001, Grey }, { 1, Black }) })
	Strip({ Scale = 0.5, Color = Keys({ 0, White }, { 0.3, Black }, { 1, White }) })
	Strip({ Scale = 0.5, Color = K3, Type = Enum.GradientType.Radial })
	Strip({ Scale = 0.25, Color = K3 })
	Strip({ Color = K3, Offset = Vector2.new(-0.3, 0) })
	Strip({ Scale = 2, Color = K3, Offset = Vector2.new(-0.9, 0) })
	Strip({ Scale = 0.5, Color = K3, Rotation = 180 })
	Strip({ Scale = 0.5, Color = Keys({ 0, Black }, { 0.3, White }, { 0.6, Grey }, { 1, Black }) })
	Strip({ Scale = 0.5, Color = Keys({ 0, Black }, { 0.5, White }, { 0.75, Grey }, { 1, Black }) })
	Strip({ Scale = 0.5, Color = Keys({ 0, Black }, { 0.25, White }, { 0.5, Grey }, { 1, Black }) })
	Strip({ Scale = 0.5, Color = Keys({ 0, White }, { 0.3, Black }, { 0.6, White }, { 1, Black }) })
	Strip({ Scale = 0.5, Color = Keys({ 0, Black }, { 0.3, White }, { 1, Black }), Transparency = NumberSequence.new({ NumberSequenceKeypoint.new(0, 0), NumberSequenceKeypoint.new(0.5, 0), NumberSequenceKeypoint.new(1, 0) }) })
end
