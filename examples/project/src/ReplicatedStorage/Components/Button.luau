-- A small button component: a TextButton with rounded corners, a border stroke and padding.
local Button = {}

export type Props = {
	Text: string,
	Color: Color3?,
	Size: UDim2?,
}

function Button.new(Props: Props): TextButton
	local Root = Instance.new("TextButton")
	Root.Name = "Button"
	Root.AutoButtonColor = true
	Root.BackgroundColor3 = Props.Color or Color3.fromRGB(56, 132, 255)
	Root.BorderSizePixel = 0
	Root.Size = Props.Size or UDim2.fromOffset(180, 48)
	Root.FontFace = Font.new("rbxasset://fonts/families/SourceSansPro.json", Enum.FontWeight.Bold)
	Root.Text = Props.Text
	Root.TextColor3 = Color3.new(1, 1, 1)
	Root.TextSize = 22

	local Corner = Instance.new("UICorner")
	Corner.CornerRadius = UDim.new(0, 10)
	Corner.Parent = Root

	local Stroke = Instance.new("UIStroke")
	Stroke.ApplyStrokeMode = Enum.ApplyStrokeMode.Border
	Stroke.Color = Color3.new(1, 1, 1)
	Stroke.Thickness = 2
	Stroke.Transparency = 0.6
	Stroke.Parent = Root

	return Root
end

return Button
