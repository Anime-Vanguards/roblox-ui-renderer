-- Rotated frames: solid fills at several angles, rounded corners, borders, nested rotation, text.
return function(Target)
	local Angles = { 5, 15, 30, 45, 60, 90, 135, -20 }
	for Index, Angle in Angles do
		local F = Instance.new("Frame")
		F.AnchorPoint = Vector2.new(0.5, 0.5)
		F.Position = UDim2.fromOffset(80 + (Index - 1) * 130, 80)
		F.Size = UDim2.fromOffset(90, 50)
		F.Rotation = Angle
		F.BackgroundColor3 = Color3.fromRGB(240, 200, 60)
		F.BorderSizePixel = 0
		F.Parent = Target

		local R = F:Clone()
		R.Position = UDim2.fromOffset(80 + (Index - 1) * 130, 200)
		R.BackgroundColor3 = Color3.fromRGB(90, 170, 255)
		local Corner = Instance.new("UICorner")
		Corner.CornerRadius = UDim.new(0, 12)
		Corner.Parent = R
		R.Parent = Target
	end

	-- nested rotation (child rotates with parent)
	local Parent = Instance.new("Frame")
	Parent.AnchorPoint = Vector2.new(0.5, 0.5)
	Parent.Position = UDim2.fromOffset(120, 340)
	Parent.Size = UDim2.fromOffset(160, 90)
	Parent.Rotation = 20
	Parent.BackgroundColor3 = Color3.fromRGB(60, 60, 80)
	Parent.BorderSizePixel = 0
	Parent.Parent = Target
	local Child = Instance.new("Frame")
	Child.Position = UDim2.fromOffset(20, 20)
	Child.Size = UDim2.fromOffset(60, 40)
	Child.Rotation = 15
	Child.BackgroundColor3 = Color3.fromRGB(255, 90, 120)
	Child.BorderSizePixel = 0
	Child.Parent = Parent

	-- rotated with border pixel
	local Bordered = Instance.new("Frame")
	Bordered.AnchorPoint = Vector2.new(0.5, 0.5)
	Bordered.Position = UDim2.fromOffset(340, 340)
	Bordered.Size = UDim2.fromOffset(100, 60)
	Bordered.Rotation = 25
	Bordered.BackgroundColor3 = Color3.fromRGB(200, 255, 200)
	Bordered.BorderSizePixel = 3
	Bordered.BorderColor3 = Color3.fromRGB(255, 0, 0)
	Bordered.Parent = Target

	-- rotated semi-transparent and a rotated text label
	local Faded = Instance.new("Frame")
	Faded.AnchorPoint = Vector2.new(0.5, 0.5)
	Faded.Position = UDim2.fromOffset(520, 340)
	Faded.Size = UDim2.fromOffset(110, 70)
	Faded.Rotation = -35
	Faded.BackgroundColor3 = Color3.new(1, 1, 1)
	Faded.BackgroundTransparency = 0.5
	Faded.BorderSizePixel = 0
	Faded.Parent = Target

	local Label = Instance.new("TextLabel")
	Label.AnchorPoint = Vector2.new(0.5, 0.5)
	Label.Position = UDim2.fromOffset(760, 340)
	Label.Size = UDim2.fromOffset(200, 50)
	Label.Rotation = 12
	Label.BackgroundTransparency = 1
	Label.Text = "Rotated text"
	Label.TextColor3 = Color3.new(1, 1, 1)
	Label.FontFace = Font.new("rbxasset://fonts/families/FredokaOne.json")
	Label.TextSize = 28
	Label.Parent = Target
end
