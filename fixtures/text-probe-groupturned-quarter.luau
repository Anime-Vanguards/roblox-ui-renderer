-- Text in CanvasGroups turned 0, 360, 180, 90 and 270 degrees, and the same labels turned on their own outside groups.
-- A group turned by a multiple of 90 degrees at whole device pixels with an even device size maps its texture 1:1
-- onto the screen, so the capture shows the texture's text unresampled; the four turned groups draw the same
-- texture (notes/text.md "Text in a turned CanvasGroup").
-- rur compare --fixture fixtures/text-probe-groupturned-quarter.luau --viewport 1200x900 --time 0.2 --background 0,0,0
return function(Target)
	local Angles = { 0, 360, 180, 90, 270 }
	for I, Angle in Angles do
		local G = Instance.new("CanvasGroup")
		G.Name = string.format("G%d", Angle)
		G.Size = UDim2.fromOffset(228, 228)
		G.Position = UDim2.fromOffset(4 + (I - 1) * 240, 4)
		G.Rotation = Angle
		G.BackgroundColor3 = Color3.fromRGB(30, 60, 110)
		G.BorderSizePixel = 0
		G.Parent = Target
		local Y = 4
		for Size = 14, 28, 2 do
			local L = Instance.new("TextLabel")
			L.Name = string.format("S%d", Size)
			L.BackgroundTransparency = 1
			L.TextColor3 = Color3.fromRGB(255, 255, 255)
			L.Text = "Hg"
			L.TextSize = Size
			L.TextXAlignment = Enum.TextXAlignment.Left
			L.Position = UDim2.fromOffset(8, Y)
			L.Size = UDim2.fromOffset(80, Size + 4)
			L.Parent = G
			local R = L:Clone()
			R.Name = string.format("C%d", Size)
			R.TextXAlignment = Enum.TextXAlignment.Center
			R.Position = UDim2.fromOffset(100, Y)
			R.Size = UDim2.fromOffset(Size * 2 + 1, Size + 3)
			R.Parent = G
			Y += Size + 6
		end
		local Bar = Instance.new("Frame")
		Bar.Name = "Bar"
		Bar.BackgroundColor3 = Color3.new(1, 1, 1)
		Bar.BorderSizePixel = 0
		Bar.Position = UDim2.fromOffset(216, 4)
		Bar.Size = UDim2.fromOffset(2, 216)
		Bar.Parent = G
	end
	-- the same labels turned on their own, outside any group
	for I, Angle in Angles do
		local Y = 260
		for Size = 14, 30, 4 do
			local L = Instance.new("TextLabel")
			L.Name = string.format("R%d_%d", Angle, Size)
			L.BackgroundTransparency = 1
			L.TextColor3 = Color3.fromRGB(255, 255, 255)
			L.Text = "Hg"
			L.TextSize = Size
			L.TextXAlignment = Enum.TextXAlignment.Left
			L.Position = UDim2.fromOffset(18 + (I - 1) * 240, Y)
			L.Size = UDim2.fromOffset(80, Size + 4)
			L.Rotation = Angle
			L.Parent = Target
			Y += Size + 16
		end
	end
end
