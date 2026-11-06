-- A UI Labs story table without Vide: `story` gets { target, controls } and returns the UI it built.
-- Controls are plain values here; `rur render --controls '{"Text": "Go"}'` and the desktop storybook
-- change them.
local ReplicatedStorage = game:GetService("ReplicatedStorage")
local Button = require(ReplicatedStorage.Components.Button)

return {
	summary = "Buttons in a centred list",
	controls = {
		Text = "Play",
		Wide = false,
	},
	story = function(Props)
		local Stack = Instance.new("Frame")
		Stack.Name = "Stack"
		Stack.BackgroundTransparency = 1
		Stack.Size = UDim2.fromScale(1, 1)

		local List = Instance.new("UIListLayout")
		List.Padding = UDim.new(0, 12)
		List.HorizontalAlignment = Enum.HorizontalAlignment.Center
		List.VerticalAlignment = Enum.VerticalAlignment.Center
		List.SortOrder = Enum.SortOrder.LayoutOrder
		List.Parent = Stack

		local Primary = Button.new({
			Text = Props.controls.Text,
			Size = if Props.controls.Wide then UDim2.fromOffset(320, 48) else nil,
		})
		Primary.Name = "Primary"
		Primary.LayoutOrder = 1
		Primary.Parent = Stack

		local Secondary = Button.new({ Text = "Settings", Color = Color3.fromRGB(70, 74, 86) })
		Secondary.Name = "Secondary"
		Secondary.LayoutOrder = 2
		Secondary.Parent = Stack

		return Stack
	end,
}
