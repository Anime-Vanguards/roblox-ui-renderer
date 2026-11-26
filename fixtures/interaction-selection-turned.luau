-- The gamepad selection highlight of a turned button under a UIScale, inside a CanvasGroup at GroupTransparency 0.4:
-- the highlight turns with the button, wraps its scaled rect with the same 14 px of texture (the UIScale does not grow
-- it) and is not faded by the group. Measured in a 0.747 sim client (notes/interaction.md "Gamepad selection").
-- compare: --viewport 480x320 --time 0.2 --background 20,20,26
return function(Target)
	local Group = Instance.new("CanvasGroup")
	Group.Name = "Group"
	Group.Position = UDim2.fromOffset(40, 30)
	Group.Size = UDim2.fromOffset(400, 260)
	Group.GroupTransparency = 0.4
	Group.BackgroundColor3 = Color3.fromRGB(50, 50, 60)
	Group.BorderSizePixel = 0
	Group.Parent = Target
	local Holder = Instance.new("Frame")
	Holder.Name = "Holder"
	Holder.Position = UDim2.fromOffset(60, 40)
	Holder.Size = UDim2.fromOffset(140, 100)
	Holder.BackgroundTransparency = 1
	Holder.Parent = Group
	local Scale = Instance.new("UIScale")
	Scale.Scale = 1.5
	Scale.Parent = Holder
	local Button = Instance.new("TextButton")
	Button.Name = "Selected"
	Button.Position = UDim2.fromOffset(30, 30)
	Button.Size = UDim2.fromOffset(90, 50)
	Button.Rotation = 20
	Button.BackgroundColor3 = Color3.fromRGB(220, 180, 90)
	Button.BorderSizePixel = 0
	Button.AutoButtonColor = false
	Button.Text = ""
	Button.Parent = Holder
	game:GetService("GuiService").SelectedObject = Button
end
