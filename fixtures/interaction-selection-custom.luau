-- A custom selection highlight: a TextButton's SelectionImageObject, a half-transparent Frame with rounded corners
-- placed below the button's middle by its own Position, Size and AnchorPoint within the button's rect, drawn over
-- the button and a higher-ZIndex sibling. Measured in a 0.747 sim client (notes/interaction.md "Gamepad selection").
-- compare: --viewport 420x260 --time 0.2 --background 20,20,26
return function(Target)
	local Button = Instance.new("TextButton")
	Button.Name = "Selected"
	Button.Position = UDim2.fromOffset(80, 70)
	Button.Size = UDim2.fromOffset(180, 70)
	Button.BackgroundColor3 = Color3.fromRGB(200, 200, 200)
	Button.BorderSizePixel = 0
	Button.AutoButtonColor = false
	Button.Text = ""
	Button.Parent = Target
	local Over = Instance.new("Frame")
	Over.Name = "Over"
	Over.Position = UDim2.fromOffset(220, 40)
	Over.Size = UDim2.fromOffset(100, 150)
	Over.ZIndex = 4
	Over.BackgroundColor3 = Color3.fromRGB(70, 90, 160)
	Over.BorderSizePixel = 0
	Over.Parent = Target
	local Image = Instance.new("Frame")
	Image.Name = "Highlight"
	Image.AnchorPoint = Vector2.new(0.5, 0)
	Image.Position = UDim2.new(0.5, 0, 0.6, 0)
	Image.Size = UDim2.new(1, 24, 0.5, 20)
	Image.BackgroundColor3 = Color3.fromRGB(255, 200, 0)
	Image.BackgroundTransparency = 0.35
	Image.BorderSizePixel = 0
	local Corner = Instance.new("UICorner")
	Corner.CornerRadius = UDim.new(0, 10)
	Corner.Parent = Image
	Button.SelectionImageObject = Image
	game:GetService("GuiService").SelectedObject = Button
end
