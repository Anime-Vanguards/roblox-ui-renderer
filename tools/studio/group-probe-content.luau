-- Where a CanvasGroup's content lands in its texture: groups, unturned and turned (0.5, 2, 20, 45 and 90 degrees),
-- at whole and fractional places and sizes, each holding hard-edged white Frames (no anti-aliasing under a
-- UIGradient is not used here: plain Frames at whole logical px) against a dark fill, with a 1 px red border on
-- the group itself. A shift of the content relative to the group's own edges, or a softening, shows the
-- texture's origin and size rounding.
-- compare: --viewport 1200x900 --time 0.5 --background 0,0,0
return function(Target)
	local Places = { { 0, 0, 120, 90 }, { 0.5, 0.5, 120, 90 }, { 0.25, 0.75, 120.5, 90.5 }, { 0.33, 0.67, 121, 91 } }
	local Angles = { 0, 0.5, 2, 20, 45, 90 }
	local Y = 20
	for Row, Place in Places do
		local X = 20
		for Column, Angle in Angles do
			local G = Instance.new("CanvasGroup")
			G.Name = `G{Row}_{Angle}`
			G.Position = UDim2.fromOffset(X + Place[1], Y + Place[2])
			G.Size = UDim2.fromOffset(Place[3], Place[4])
			G.Rotation = Angle
			G.BackgroundColor3 = Color3.fromRGB(30, 40, 60)
			G.BorderColor3 = Color3.fromRGB(255, 0, 0)
			G.BorderSizePixel = 1
			for Index, Box in { { 10, 10, 30, 20 }, { 60, 15, 1, 50 }, { 15, 50, 50, 1 }, { 80, 40, 20, 30 } } do
				local F = Instance.new("Frame")
				F.Name = `Box{Index}`
				F.Position = UDim2.fromOffset(Box[1], Box[2])
				F.Size = UDim2.fromOffset(Box[3], Box[4])
				F.BackgroundColor3 = Color3.new(1, 1, 1)
				F.BorderSizePixel = 0
				F.Parent = G
			end
			G.Parent = Target
			X += 190
		end
		Y += 200
	end
end
