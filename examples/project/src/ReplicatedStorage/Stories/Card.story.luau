-- A story as a plain function of its target: a card with a gradient, an automatically sized
-- title and wrapped body text.
return function(Target: Frame)
	local Card = Instance.new("Frame")
	Card.Name = "Card"
	Card.AnchorPoint = Vector2.new(0.5, 0.5)
	Card.Position = UDim2.fromScale(0.5, 0.5)
	Card.Size = UDim2.fromOffset(320, 0)
	Card.AutomaticSize = Enum.AutomaticSize.Y
	Card.BackgroundColor3 = Color3.new(1, 1, 1)
	Card.Parent = Target

	local Corner = Instance.new("UICorner")
	Corner.CornerRadius = UDim.new(0, 14)
	Corner.Parent = Card

	local Gradient = Instance.new("UIGradient")
	Gradient.Rotation = 90
	Gradient.Color = ColorSequence.new(Color3.fromRGB(44, 52, 80), Color3.fromRGB(24, 26, 36))
	Gradient.Parent = Card

	local Padding = Instance.new("UIPadding")
	Padding.PaddingTop = UDim.new(0, 16)
	Padding.PaddingBottom = UDim.new(0, 16)
	Padding.PaddingLeft = UDim.new(0, 16)
	Padding.PaddingRight = UDim.new(0, 16)
	Padding.Parent = Card

	local List = Instance.new("UIListLayout")
	List.Padding = UDim.new(0, 8)
	List.SortOrder = Enum.SortOrder.LayoutOrder
	List.Parent = Card

	local Title = Instance.new("TextLabel")
	Title.Name = "Title"
	Title.LayoutOrder = 1
	Title.BackgroundTransparency = 1
	Title.AutomaticSize = Enum.AutomaticSize.XY
	Title.FontFace = Font.new("rbxasset://fonts/families/Montserrat.json", Enum.FontWeight.Bold)
	Title.Text = "Daily reward"
	Title.TextColor3 = Color3.new(1, 1, 1)
	Title.TextSize = 24
	Title.Parent = Card

	local Body = Instance.new("TextLabel")
	Body.Name = "Body"
	Body.LayoutOrder = 2
	Body.BackgroundTransparency = 1
	Body.Size = UDim2.fromScale(1, 0)
	Body.AutomaticSize = Enum.AutomaticSize.Y
	Body.FontFace = Font.new("rbxasset://fonts/families/SourceSansPro.json")
	Body.Text = "Come back tomorrow for another one. Text wraps inside the card, and the card grows to fit it."
	Body.TextColor3 = Color3.fromRGB(200, 204, 220)
	Body.TextSize = 18
	Body.TextWrapped = true
	Body.TextXAlignment = Enum.TextXAlignment.Left
	Body.Parent = Card

	local Stroke = Instance.new("UIStroke")
	Stroke.Color = Color3.fromRGB(110, 130, 200)
	Stroke.Thickness = 1.5
	Stroke.Parent = Card
end
