-- compare: --viewport 1200x700 --time 0.3 --background 0,0,0
-- A UICorner with one corner of radius 0 beside 10 px ones on turned shapes, and on UIStrokes: white Frames, white
-- images and CanvasGroups of a white Frame turned 7, 20, 30 and 45 degrees, the square corner in each of the four
-- places; then white UIStrokes of 2 and 5 px around clear Frames with the same corners, unturned and turned, over
-- black, so each pixel reads the shape's coverage.
return function(Target)
	local White = "rbxasset://textures/ui/InGameMenu/WhiteSquare.png"
	local Keys = { "TopLeftRadius", "TopRightRadius", "BottomRightRadius", "BottomLeftRadius" }

	local function Shape(Kind, Name, X, Y, W, H, Square, Rotation, Thickness)
		local Object
		if Kind == "Image" then
			Object = Instance.new("ImageLabel")
			Object.BackgroundTransparency = 1
			Object.Image = White
		elseif Kind == "Frame" then
			Object = Instance.new("Frame")
			Object.BackgroundColor3 = Color3.new(1, 1, 1)
		elseif Kind == "Group" then
			Object = Instance.new("CanvasGroup")
			Object.BackgroundTransparency = 1
			local Fill = Instance.new("Frame")
			Fill.Size = UDim2.fromScale(1, 1)
			Fill.BackgroundColor3 = Color3.new(1, 1, 1)
			Fill.BorderSizePixel = 0
			Fill.Parent = Object
		else
			Object = Instance.new("Frame")
			Object.BackgroundTransparency = 1
			local Stroke = Instance.new("UIStroke")
			Stroke.Color = Color3.new(1, 1, 1)
			Stroke.Thickness = Thickness
			Stroke.ApplyStrokeMode = Enum.ApplyStrokeMode.Border
			Stroke.Parent = Object
		end
		Object.Name = Name
		Object.AnchorPoint = Vector2.new(0.5, 0.5)
		Object.Position = UDim2.fromOffset(X, Y)
		Object.Size = UDim2.fromOffset(W, H)
		Object.Rotation = Rotation
		Object.BorderSizePixel = 0
		local Corner = Instance.new("UICorner")
		for Index, Key in Keys do
			(Corner :: any)[Key] = if Index == Square then UDim.new(0, 0) else UDim.new(0, 10)
		end
		Corner.Parent = Object
		Object.Parent = Target
	end

	local Rows = {
		{ "Frame", { 7, 20, 30, 45 } },
		{ "Image", { 7, 20, 30, 45 } },
		{ "Group", { 7, 20, 30, 45 } },
		{ "Stroke2", { 0, 7, 30, 45 }, 2 },
		{ "Stroke5", { 0, 7, 30, 45 }, 5 },
	}
	for Row, Spec in Rows do
		local Kind = if string.sub(Spec[1], 1, 6) == "Stroke" then "Stroke" else Spec[1]
		for Square = 1, 4 do
			for Index, Rotation in Spec[2] do
				local Column = (Square - 1) * 4 + (Index - 1)
				-- Odd and even logical sizes and places put the corners on whole and half device pixels at 1.5x
				local W = 50 + Column % 2
				local H = 38 + (Column // 2) % 2
				local X = 40 + Column * 70 + Column % 3
				local Y = 50 + (Row - 1) * 120 + Column % 2
				Shape(Kind, `{Spec[1]}S{Square}A{Rotation}`, X, Y, W, H, Square, Rotation, Spec[3])
			end
		end
	end
end
