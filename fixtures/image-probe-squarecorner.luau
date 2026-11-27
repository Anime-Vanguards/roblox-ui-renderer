-- compare: --viewport 1760x600 --time 0.3 --background 0,0,0
-- UICorner per-corner radii with one square corner (radius 0) or a 1 px one beside rounded ones, on white images,
-- white Frames and CanvasGroups of a white Frame over black (each pixel reads the mask's coverage): the square corner
-- in each of the four places, each at consecutive logical places and sizes so that the snapped corners fall on every
-- parity of the 2 x 2 derivative quads, at 1.5x and 1x.
return function(Target)
	local White = "rbxasset://textures/ui/InGameMenu/WhiteSquare.png"
	local Keys = { "TopLeftRadius", "TopRightRadius", "BottomRightRadius", "BottomLeftRadius" }

	local function Rounded(Kind, Name, X, Y, W, H, Radii)
		local Object
		if Kind == "Image" then
			Object = Instance.new("ImageLabel")
			Object.BackgroundTransparency = 1
			Object.Image = White
		elseif Kind == "Frame" then
			Object = Instance.new("Frame")
			Object.BackgroundColor3 = Color3.new(1, 1, 1)
		else
			Object = Instance.new("CanvasGroup")
			Object.BackgroundTransparency = 1
			local Fill = Instance.new("Frame")
			Fill.Size = UDim2.fromScale(1, 1)
			Fill.BackgroundColor3 = Color3.new(1, 1, 1)
			Fill.BorderSizePixel = 0
			Fill.Parent = Object
		end
		Object.Name = Name
		Object.Position = UDim2.fromOffset(X, Y)
		Object.Size = UDim2.fromOffset(W, H)
		Object.BorderSizePixel = 0
		local Corner = Instance.new("UICorner")
		for Index, Key in Keys do
			(Corner :: any)[Key] = Radii[Index]
		end
		Corner.Parent = Object
		Object.Parent = Target
	end

	-- Rows: the square corner's place (4), then the same with a 1 px corner (4); a block of 8 columns per kind
	for Block, Kind in { "Image", "Frame", "Group" } do
		local Row = 0
		for _, Small in { UDim.new(0, 0), UDim.new(0, 1) } do
			for Square = 1, 4 do
				local Radii = {}
				for Index = 1, 4 do
					Radii[Index] = if Index == Square then Small else UDim.new(0, 10)
				end
				for Column = 0, 7 do
					local X = 10 + (Block - 1) * 584 + Column * 72 + Column % 4
					local Y = 10 + Row * 72 + Column // 4
					local W = 44 + Column % 2
					local H = 40 + (Column // 2) % 2
					Rounded(Kind, `{Kind}R{Row}C{Column}`, X, Y, W, H, Radii)
				end
				Row += 1
			end
		end
	end
end
