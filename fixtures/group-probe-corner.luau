-- CanvasGroup probe: rounded images in CanvasGroups whose texture is resampled (its texel count differs from the
-- group's device width or height: odd logical sizes at fractional device places at 1.5x, and under a UIScale at
-- both scales). Each group holds a white rounded image (its pixels read the corner's coverage directly over the
-- black background), a white circle, a rounded Motor icon and a rounded Fit photo; the last row draws the same
-- content in plain Frames.
-- compare: --viewport 1000x620 --time 0.3 --background 0,0,0
return function(Target)
	local White = "rbxasset://textures/ui/InGameMenu/WhiteSquare.png"
	local Motor = "rbxasset://textures/ui/Motor.png"
	local Photo = "rbxasset://textures/ui/LuaApp/graphic/Auth/FPSBackground.png"

	local function Rounded(Parent, Name, Uri, X, Y, W, H, Radius, Props)
		local Label = Instance.new("ImageLabel")
		Label.Name = Name
		Label.Position = UDim2.fromOffset(X, Y)
		Label.Size = UDim2.fromOffset(W, H)
		Label.BackgroundTransparency = 1
		Label.BorderSizePixel = 0
		Label.Image = Uri
		for Key, Value in Props or {} do
			(Label :: any)[Key] = Value
		end
		local Corner = Instance.new("UICorner")
		Corner.CornerRadius = Radius
		Corner.Parent = Label
		Label.Parent = Parent
	end

	local function Fill(Parent)
		Rounded(Parent, "WhiteRect", White, 5, 5, 51, 33, UDim.new(0, 9))
		Rounded(Parent, "WhiteCircle", White, 63, 4, 37, 37, UDim.new(0.5, 0))
		Rounded(Parent, "Motor", Motor, 5, 45, 41, 41, UDim.new(0, 11))
		Rounded(Parent, "Photo", Photo, 51, 47, 63, 39, UDim.new(0, 13), { ScaleType = Enum.ScaleType.Fit })
	end

	-- (width, height, x shift): odd sizes start on a half pixel at 1.5x with a shift of 1
	local Cells = { { 121, 91, 0 }, { 121, 91, 1 }, { 117, 89, 0 }, { 117, 89, 1 }, { 120, 90, 0 }, { 123, 93, 1 } }
	for Row, Scale in { 1, 0.9, 1.13, 0.87 } do
		local Holder = Instance.new("Frame")
		Holder.Name = `Row{Row}`
		Holder.Position = UDim2.fromOffset(0, 10 + (Row - 1) * 125)
		Holder.Size = UDim2.fromOffset(1000, 120)
		Holder.BackgroundTransparency = 1
		Holder.Parent = Target
		if Scale ~= 1 then
			local UIScale = Instance.new("UIScale")
			UIScale.Scale = Scale
			UIScale.Parent = Holder
		end
		for Col, Cell in Cells do
			local Group = Instance.new("CanvasGroup")
			Group.Name = `G{Col}`
			Group.Position = UDim2.fromOffset(10 + (Col - 1) * 140 + Cell[3], 0)
			Group.Size = UDim2.fromOffset(Cell[1], Cell[2])
			Group.BackgroundTransparency = 1
			Group.BorderSizePixel = 0
			Group.Parent = Holder
			Fill(Group)
		end
	end
	for Col, Cell in Cells do
		local Frame = Instance.new("Frame")
		Frame.Name = `F{Col}`
		Frame.Position = UDim2.fromOffset(10 + (Col - 1) * 140 + Cell[3], 510)
		Frame.Size = UDim2.fromOffset(Cell[1], Cell[2])
		Frame.BackgroundTransparency = 1
		Frame.Parent = Target
		Fill(Frame)
	end
end
