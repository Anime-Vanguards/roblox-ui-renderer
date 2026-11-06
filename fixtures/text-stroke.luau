-- Text UIStroke probe: white FredokaOne text, black strokes of several thicknesses on a gray backdrop.
return function(Target)
	local Thicknesses = { 0.5, 1, 1.5, 2, 2.2125, 3, 4, 6 }
	for Index, Thickness in Thicknesses do
		local Label = Instance.new("TextLabel")
		Label.Name = `Stroke{Thickness}`
		Label.BackgroundTransparency = 1
		Label.TextColor3 = Color3.new(1, 1, 1)
		Label.FontFace = Font.new("rbxasset://fonts/families/FredokaOne.json")
		Label.TextSize = 32
		Label.Text = "Hill Oxo"
		Label.TextXAlignment = Enum.TextXAlignment.Left
		Label.Size = UDim2.fromOffset(240, 48)
		Label.Position = UDim2.fromOffset(20 + ((Index - 1) % 4) * 260, 20 + ((Index - 1) // 4) * 70)
		Label.Parent = Target
		local Stroke = Instance.new("UIStroke")
		Stroke.Thickness = Thickness
		Stroke.Color = Color3.new(0, 0, 0)
		Stroke.Parent = Label
	end
	local Big = Instance.new("TextLabel")
	Big.Name = "Big"
	Big.BackgroundTransparency = 1
	Big.TextColor3 = Color3.new(1, 1, 1)
	Big.FontFace = Font.new("rbxasset://fonts/families/FredokaOne.json")
	Big.TextSize = 64
	Big.Text = "Il"
	Big.Size = UDim2.fromOffset(200, 90)
	Big.Position = UDim2.fromOffset(20, 170)
	Big.Parent = Target
	local BigStroke = Instance.new("UIStroke")
	BigStroke.Thickness = 5
	BigStroke.Color = Color3.fromRGB(255, 0, 0)
	BigStroke.Parent = Big
	local Clear = Big:Clone()
	Clear.Name = "Transparent"
	Clear.Position = UDim2.fromOffset(260, 170)
	Clear.TextTransparency = 0.5
	Clear.Parent = Target
	local Joined = Big:Clone()
	Joined.Name = "Miter"
	Joined.Text = "VW"
	Joined.Position = UDim2.fromOffset(500, 170)
	Joined:FindFirstChildOfClass("UIStroke").LineJoinMode = Enum.LineJoinMode.Miter
	Joined.Parent = Target
end
