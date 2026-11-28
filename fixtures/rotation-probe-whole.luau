-- Whole turns: does a client draw an element turned 360 degrees (or -360, 720, or 180 under a parent turned 180) as an
-- unturned one? Each row holds one kind of element (a frame, an image, a label, a frame with a UIStroke, a rounded
-- frame) at Rotation 0, 360, -360, 720, 1 (a real turn, control) and 180 inside a parent turned 180; at odd logical
-- positions, so at 1.5x an unturned element's edges snap to whole device px and a turned one's stay at the half.
-- Row 6: children turned 0 / 360 / 1 that overflow a ClipsDescendants frame. Row 7: frames, images and rounded frames
-- turned 0 and 360 under a UIScale of 1.013, so their edges fall at other fractions of a device px.
-- compare: --viewport 1000x640 --time 0.2 --background 0,0,0
return function(Target)
	local White = "rbxasset://textures/ui/InGameMenu/WhiteSquare.png"
	local Turns = { 0, 360, -360, 720, 1 }
	local function Make(Kind, Row, Name, Rotation, Parent, X, Y)
		local Object
		if Kind == "Image" then
			Object = Instance.new("ImageLabel")
			Object.Image = White
			Object.BackgroundTransparency = 1
		elseif Kind == "Label" then
			Object = Instance.new("TextLabel")
			Object.Text = "Ab"
			Object.TextSize = 24
			Object.TextColor3 = Color3.new(1, 1, 1)
			Object.BackgroundColor3 = Color3.fromRGB(60, 60, 200)
		else
			Object = Instance.new("Frame")
			Object.BackgroundColor3 = Color3.new(1, 1, 1)
		end
		Object.Name = Name
		Object.BorderSizePixel = 0
		Object.Position = UDim2.fromOffset(X, Y)
		Object.Size = UDim2.fromOffset(61, 41)
		Object.Rotation = Rotation
		if Kind == "Stroke" then
			Object.BackgroundTransparency = 1
			local Stroke = Instance.new("UIStroke")
			Stroke.Color = Color3.new(1, 1, 1)
			Stroke.Thickness = 3
			Stroke.Parent = Object
		elseif Kind == "Round" then
			local Corner = Instance.new("UICorner")
			Corner.CornerRadius = UDim.new(0, 9)
			Corner.Parent = Object
		end
		Object.Parent = Parent
		return Object
	end
	for Row, Kind in { "Frame", "Image", "Label", "Stroke", "Round" } do
		local Y = 11 + (Row - 1) * 80
		for Index, Rotation in Turns do
			Make(Kind, Row, `{Kind}{Rotation}`, Rotation, Target, 13 + (Index - 1) * 110, Y)
		end
		-- 180 inside a parent turned 180 (AbsoluteRotation 360)
		local Parent = Instance.new("Frame")
		Parent.Name = `{Kind}Parent180`
		Parent.BackgroundTransparency = 1
		Parent.Position = UDim2.fromOffset(13 + 5 * 110, Y)
		Parent.Size = UDim2.fromOffset(61, 41)
		Parent.Rotation = 180
		Parent.Parent = Target
		Make(Kind, Row, `{Kind}In180`, 180, Parent, 0, 0)
		-- 360 inside a parent turned 360
		local Parent2 = Instance.new("Frame")
		Parent2.Name = `{Kind}Parent360`
		Parent2.BackgroundTransparency = 1
		Parent2.Position = UDim2.fromOffset(13 + 6 * 110, Y)
		Parent2.Size = UDim2.fromOffset(61, 41)
		Parent2.Rotation = 360
		Parent2.Parent = Target
		Make(Kind, Row, `{Kind}In360`, 0, Parent2, 0, 0)
	end
	for Index, Rotation in { 0, 360, -360, 1 } do
		local Clip = Instance.new("Frame")
		Clip.Name = `Clip{Rotation}`
		Clip.BackgroundColor3 = Color3.fromRGB(40, 40, 40)
		Clip.BorderSizePixel = 0
		Clip.ClipsDescendants = true
		Clip.Position = UDim2.fromOffset(13 + (Index - 1) * 150, 421)
		Clip.Size = UDim2.fromOffset(101, 61)
		Clip.Parent = Target
		local Child = Make("Frame", 6, `Over{Rotation}`, Rotation, Clip, 41, 21)
		Child.Size = UDim2.fromOffset(91, 61)
	end
	local Scaled = Instance.new("Frame")
	Scaled.Name = "Scaled"
	Scaled.BackgroundTransparency = 1
	Scaled.Position = UDim2.fromOffset(13, 501)
	Scaled.Size = UDim2.fromOffset(960, 60)
	Scaled.Parent = Target
	local UIScale = Instance.new("UIScale")
	UIScale.Scale = 1.013
	UIScale.Parent = Scaled
	for Index, Spec in { { "Frame", 0 }, { "Frame", 360 }, { "Image", 0 }, { "Image", 360 }, { "Round", 0 }, { "Round", 360 }, { "Label", 0 }, { "Label", 360 } } do
		Make(Spec[1], 7, `Scaled{Spec[1]}{Spec[2]}`, Spec[2], Scaled, 7 + (Index - 1) * 110, 3)
	end
end
