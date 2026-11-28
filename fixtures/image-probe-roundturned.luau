-- compare: --viewport 1200x700 --time 0.3 --background 0,0,0
-- The arcs of turned rounded images and CanvasGroups: white images and groups with all four corners rounded (6, 12,
-- 20 px and a per-corner mix), turned 3, 10, 25, 40, 60 and 80 degrees, over black, so each pixel reads the coverage.
return function(Target)
	local White = "rbxasset://textures/ui/InGameMenu/WhiteSquare.png"
	local Radii = { { 6, 6, 6, 6 }, { 12, 12, 12, 12 }, { 20, 20, 20, 20 }, { 4, 16, 8, 24 } }
	local Angles = { 3, 10, 25, 40, 60, 80 }
	local Keys = { "TopLeftRadius", "TopRightRadius", "BottomRightRadius", "BottomLeftRadius" }
	for Block, Kind in { "Image", "Group" } do
		for Row, R in Radii do
			for Column, Angle in Angles do
				local Object
				if Kind == "Image" then
					Object = Instance.new("ImageLabel")
					Object.BackgroundTransparency = 1
					Object.Image = White
				else
					Object = Instance.new("CanvasGroup")
					Object.BackgroundTransparency = 1
					local Fill = Instance.new("Frame")
					Fill.Size = UDim2.fromScale(1, 1)
					Fill.BackgroundColor3 = Color3.new(1, 1, 1)
					Fill.BorderSizePixel = 0
					Fill.Parent = Object
				end
				Object.Name = `{Kind}R{Row}A{Angle}`
				Object.AnchorPoint = Vector2.new(0.5, 0.5)
				Object.Position = UDim2.fromOffset(50 + (Column - 1) * 95 + (Block - 1) * 590 + Row % 2, 50 + (Row - 1) * 160 + Column % 2)
				Object.Size = UDim2.fromOffset(61 + Column % 2, 47 + Row % 2)
				Object.Rotation = Angle
				Object.BorderSizePixel = 0
				local Corner = Instance.new("UICorner")
				for Index, Key in Keys do
					(Corner :: any)[Key] = UDim.new(0, R[Index])
				end
				Corner.Parent = Object
				Object.Parent = Target
			end
		end
	end
end
