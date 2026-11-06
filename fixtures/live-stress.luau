-- Live-preview stress test: 48 cards animating every frame with repeating tweens (position, size,
-- colour, rotation, transparency) plus corners, strokes, gradients, shadows and a per-frame counter.
-- Open it in the storybook browser (fixtures) to watch engine/draw cost per frame.
local TweenService = game:GetService("TweenService")
local RunService = game:GetService("RunService")

return function(Target)
	local Columns = 8
	for Index = 0, 47 do
		local Column, Row = Index % Columns, Index // Columns
		local Card = Instance.new("Frame")
		Card.Name = `Card{Index}`
		Card.Size = UDim2.fromOffset(110, 70)
		Card.Position = UDim2.fromOffset(20 + Column * 140, 20 + Row * 95)
		Card.BackgroundColor3 = Color3.fromHSV(Index / 48, 0.6, 0.9)
		Card.Parent = Target

		local Corner = Instance.new("UICorner")
		Corner.CornerRadius = UDim.new(0, 10 + Index % 3 * 4)
		Corner.Parent = Card

		local Stroke = Instance.new("UIStroke")
		Stroke.Thickness = 2
		Stroke.Color = Color3.fromRGB(20, 20, 30)
		Stroke.ApplyStrokeMode = Enum.ApplyStrokeMode.Border
		Stroke.Parent = Card

		if Index % 2 == 0 then
			local Gradient = Instance.new("UIGradient")
			Gradient.Color = ColorSequence.new(Color3.new(1, 1, 1), Color3.fromRGB(150, 150, 170))
			Gradient.Rotation = 90
			Gradient.Parent = Card
		end

		if Index % 6 == 0 then
			local Shadow = Instance.new("UIShadow")
			Shadow.BlurRadius = UDim.new(0, 12)
			Shadow.Transparency = 0.5
			Shadow.Parent = Card
		end

		local Label = Instance.new("TextLabel")
		Label.BackgroundTransparency = 1
		Label.Size = UDim2.fromScale(1, 1)
		Label.FontFace = Font.fromEnum(Enum.Font.GothamBold)
		Label.TextSize = 16
		Label.TextColor3 = Color3.new(1, 1, 1)
		Label.Text = `#{Index}`
		Label.Parent = Card
		local TextStroke = Instance.new("UIStroke")
		TextStroke.Thickness = 1.5
		TextStroke.Parent = Label

		local Info = TweenInfo.new(0.6 + (Index % 5) * 0.15, Enum.EasingStyle.Sine, Enum.EasingDirection.InOut, -1, true)
		local Goals = { Position = Card.Position + UDim2.fromOffset(0, 12) }
		if Index % 3 == 0 then
			Goals.Size = UDim2.fromOffset(120, 76)
		end
		if Index % 4 == 1 then
			Goals.BackgroundColor3 = Color3.fromHSV((Index / 48 + 0.5) % 1, 0.6, 0.9)
		end
		if Index % 5 == 2 then
			Goals.Rotation = 8
		end
		TweenService:Create(Card, Info, Goals):Play()
	end

	local Counter = Instance.new("TextLabel")
	Counter.Name = "Counter"
	Counter.BackgroundColor3 = Color3.fromRGB(20, 22, 30)
	Counter.Size = UDim2.fromOffset(220, 36)
	Counter.Position = UDim2.fromOffset(20, 600)
	Counter.FontFace = Font.fromEnum(Enum.Font.BuilderSansBold)
	Counter.TextSize = 20
	Counter.TextColor3 = Color3.new(1, 1, 1)
	Counter.Parent = Target
	local Frames = 0
	RunService.Heartbeat:Connect(function()
		Frames += 1
		Counter.Text = `frame {Frames}`
	end)
end
