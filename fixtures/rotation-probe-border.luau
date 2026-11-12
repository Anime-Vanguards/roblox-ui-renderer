-- Borders of turned frames, wide enough to tell their corners from their sides: a filled frame and one with a
-- clear background at a few angles, and a thin border beside them.
-- compare: --viewport 1200x520 --time 0.2 --background 0,0,0
return function(Target)
	local WHITE, RED = Color3.new(1, 1, 1), Color3.fromRGB(255, 60, 60)
	for Index, Angle in { 5, 20, 45, 70, 135 } do
		local X = 50 + (Index - 1) * 230
		for Row, Props in { { BorderSizePixel = 20 }, { BorderSizePixel = 20, BackgroundTransparency = 1 }, { BorderSizePixel = 2, BackgroundTransparency = 0.5 } } do
			local Frame = Instance.new("Frame")
			Frame.Name = `B{Index}_{Row}`
			Frame.Position = UDim2.fromOffset(X, 50 + (Row - 1) * 165)
			Frame.Size = UDim2.fromOffset(120, 60)
			Frame.BackgroundColor3 = WHITE
			Frame.BorderColor3 = RED
			Frame.Rotation = Angle
			for Key, Value in Props do
				(Frame :: any)[Key] = Value
			end
			Frame.Parent = Target
		end
	end
end
