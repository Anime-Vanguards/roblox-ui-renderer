-- Corner coverage of rounded images and Frames: white over black, radii 4 to 40 logical px, at whole and
-- fractional places, so each corner's pixel values read its coverage directly
-- compare: --viewport 1200x900 --time 0.5 --background 0,0,0
return function(Target)
	local Radii = { 4, 6, 8, 12, 16, 18, 24, 30, 40 }
	for Row, Kind in { "ImageLabel", "Frame", "ImageLabel" } do
		local X = 10
		for Index, Radius in Radii do
			local Size = Radius * 2 + 20
			local E = Instance.new(Kind)
			E.Name = `{Kind}_{Row}_{Radius}`
			local Shift = if Row == 3 then 0.37 else 0
			E.Position = UDim2.fromOffset(X + Shift, 10 + (Row - 1) * 110 + Shift)
			E.Size = UDim2.fromOffset(Size, Size)
			E.BorderSizePixel = 0
			E.BackgroundColor3 = Color3.new(1, 1, 1)
			if Kind == "ImageLabel" then
				E.BackgroundTransparency = 1
				E.Image = "rbxasset://textures/ui/InGameMenu/WhiteSquare.png"
			end
			local C = Instance.new("UICorner")
			C.CornerRadius = UDim.new(0, Radius)
			C.Parent = E
			E.Parent = Target
			X += Size + 10
		end
	end
end
