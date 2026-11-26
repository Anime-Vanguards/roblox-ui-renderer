-- The gamepad selection highlight around GuiService.SelectedObject: a rounded button 3 px inside a clipping Frame's
-- corner (the highlight is not clipped and keeps square corners), with a higher-ZIndex sibling over its right half
-- and a child crossing its bottom edge (the highlight is drawn over both, last in the ScreenGui). Measured in a 0.747
-- sim client (notes/interaction.md "Gamepad selection").
-- compare: --viewport 480x300 --time 0.2 --background 20,20,26
return function(Target)
	local Clip = Instance.new("Frame")
	Clip.Name = "Clip"
	Clip.Position = UDim2.fromOffset(60, 60)
	Clip.Size = UDim2.fromOffset(300, 170)
	Clip.ClipsDescendants = true
	Clip.BackgroundColor3 = Color3.fromRGB(50, 50, 60)
	Clip.BorderSizePixel = 0
	Clip.Parent = Target
	local Button = Instance.new("TextButton")
	Button.Name = "Selected"
	Button.Position = UDim2.fromOffset(3, 3)
	Button.Size = UDim2.fromOffset(160, 80)
	Button.BackgroundColor3 = Color3.fromRGB(200, 200, 200)
	Button.BorderSizePixel = 0
	Button.AutoButtonColor = false
	Button.Text = ""
	Button.Parent = Clip
	local Corner = Instance.new("UICorner")
	Corner.CornerRadius = UDim.new(0, 12)
	Corner.Parent = Button
	local Child = Instance.new("Frame")
	Child.Name = "Child"
	Child.Position = UDim2.fromOffset(-10, 50)
	Child.Size = UDim2.fromOffset(70, 60)
	Child.BackgroundColor3 = Color3.fromRGB(160, 90, 70)
	Child.BorderSizePixel = 0
	Child.Parent = Button
	local Over = Instance.new("Frame")
	Over.Name = "Over"
	Over.Position = UDim2.fromOffset(150, 20)
	Over.Size = UDim2.fromOffset(120, 120)
	Over.ZIndex = 3
	Over.BackgroundColor3 = Color3.fromRGB(70, 90, 160)
	Over.BorderSizePixel = 0
	Over.Parent = Target
	local Other = Instance.new("TextButton")
	Other.Name = "Other"
	Other.Position = UDim2.fromOffset(300, 240)
	Other.Size = UDim2.fromOffset(120, 40)
	Other.BorderSizePixel = 0
	Other.AutoButtonColor = false
	Other.Text = "Other"
	Other.Parent = Target
	game:GetService("GuiService").SelectedObject = Button
end
