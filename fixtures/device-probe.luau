-- What the scripts see of the device: UserInputService's input flags and last input type,
-- GuiService's inset (--device / --topbar-inset), and where ScreenGuis start: the story's gui
-- ignores the inset, a default ScreenGui (CoreUISafeInsets) starts below it. AbsolutePosition is
-- measured from below the inset, as in Roblox: the story reads -inset and the default gui 0.
-- The label draws only what belongs to the device and the story. GamepadEnabled, the last input type and the
-- default gui's height follow the Studio session (a gamepad an earlier script's VirtualInput used, the user's
-- mouse over Studio's window, the size of Studio's viewport), so they are printed, not drawn: a capture does
-- not depend on what Studio went through before it (notes/interaction.md "Device state in captures").
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
	Label.Text = `touch={Input.TouchEnabled} mouse={Input.MouseEnabled} keyboard={Input.KeyboardEnabled} inset={Inset.Y} story={Target.AbsolutePosition.Y} gui={Fill.AbsolutePosition.Y}`
	print(`gamepad={Input.GamepadEnabled} last={Input:GetLastInputType().Name} gui height={Fill.AbsoluteSize.Y}`)
	Label.Parent = Target
end
