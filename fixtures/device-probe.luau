-- What the scripts see of the device: UserInputService's input flags and last input type,
-- GuiService's inset (--device / --topbar-inset), and where ScreenGuis start: the story's gui
-- ignores the inset, a default ScreenGui (CoreUISafeInsets) starts below it.
return function(Target)
	local Input = game:GetService("UserInputService")
	local Inset = game:GetService("GuiService"):GetGuiInset()

	local Gui = Instance.new("ScreenGui")
	Gui.Name = "BelowTopbar"
	local Fill = Instance.new("Frame")
	Fill.Size = UDim2.fromScale(1, 1)
	Fill.Parent = Gui
	Gui.Parent = game:GetService("CoreGui")

	local Label = Instance.new("TextLabel")
	Label.Name = "Probe"
	Label.Size = UDim2.fromOffset(380, 40)
	Label.Text = `touch={Input.TouchEnabled} mouse={Input.MouseEnabled} keyboard={Input.KeyboardEnabled} gamepad={Input.GamepadEnabled} last={Input:GetLastInputType().Name} inset={Inset.Y} story={Target.AbsolutePosition.Y} gui={Fill.AbsolutePosition.Y},{Fill.AbsoluteSize.Y}`
	Label.Parent = Target
end
