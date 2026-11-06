-- Long route-like edges with centers outside a clipped scrolling window.
return function(Target)
	local function Box(Parent, Name, X, Y, W, H, Rotation)
		local F = Instance.new("Frame")
		F.Name = Name
		F.BorderSizePixel = 0
		F.BackgroundColor3 = Color3.fromRGB(230, 150, 80)
		F.Position = UDim2.fromOffset(X, Y)
		F.Size = UDim2.fromOffset(W, H)
		F.Rotation = Rotation or 0
		F.Parent = Parent
		return F
	end
	for Index, Kind in { "Frame", "ScrollingFrame", "CanvasGroup" } do
		local Clip = Instance.new(Kind)
		Clip.Name = Kind
		Clip.Position = UDim2.fromOffset(70 + (Index - 1) * 290, 180)
		Clip.Size = UDim2.fromOffset(220, 180)
		Clip.BackgroundColor3 = Color3.fromRGB(40, 45, 60)
		Clip.BorderSizePixel = 0
		Clip.ClipsDescendants = true
		if Kind == "ScrollingFrame" then
			Clip.CanvasSize = UDim2.fromOffset(220, 800)
			Clip.ScrollBarThickness = 0
		end
		Clip.Parent = Target
		Box(Clip, "CenterOutside", -40, -100, 300, 8, 25)
		local Entering = Box(Clip, "RotatesIntoClip", 20, -45, 180, 8, 45)
		Entering.BackgroundColor3 = Color3.fromRGB(70, 170, 240)
		Box(Clip, "CrossingTop", -40, -5, 300, 8, -25)
		Box(Clip, "CrossingBottom", -40, 160, 300, 8, 25)
		local Outside = Box(Clip, "OutsideAncestor", 0, -150, 200, 20)
		Outside.BackgroundTransparency = 1
		Box(Outside, "Edge", 10, 10, 250, 8, -20)
		local RotatedClip = Box(Clip, "RotatedClip", 30, 40, 140, 70, 10)
		RotatedClip.BackgroundColor3 = Color3.fromRGB(70, 80, 100)
		RotatedClip.ClipsDescendants = true
		Box(RotatedClip, "Child", -80, 0, 300, 7, -10)
	end
end
