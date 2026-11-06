-- Text in a zero-height / zero-width box with a UIGradient whose span is that zero axis (a mana bar's MANA
-- labels: height 0, gradient rotation 90). Row 1: height 0, rotation 90 / 0; row 2: width 0, rotation 0 / 90;
-- row 3: normal boxes for reference.
--   node cli/rur.mjs compare --fixture fixtures/text-gradient-degenerate.luau --viewport 800x480 --time 0.2 --background 30,30,40
return function(Target)
	local function Label(Name, X, Y, W, H, Rotation)
		local Label = Instance.new("TextLabel")
		Label.Name = Name
		Label.BackgroundTransparency = 1
		Label.Position = UDim2.fromOffset(X, Y)
		Label.Size = UDim2.fromOffset(W, H)
		Label.Font = Enum.Font.FredokaOne
		Label.Text = "MANA"
		Label.TextSize = 36
		Label.TextColor3 = Color3.new(1, 1, 1)
		Label.Parent = Target
		local Gradient = Instance.new("UIGradient")
		Gradient.Color = ColorSequence.new(Color3.fromRGB(80, 170, 255), Color3.fromRGB(255, 90, 200))
		Gradient.Rotation = Rotation
		Gradient.Parent = Label
	end
	Label("ZeroH90", 150, 80, 200, 0, 90)
	Label("ZeroH0", 450, 80, 200, 0, 0)
	Label("ZeroW0", 250, 200, 0, 60, 0)
	Label("ZeroW90", 550, 200, 0, 60, 90)
	Label("Normal90", 150, 330, 200, 60, 90)
	Label("Normal0", 450, 330, 200, 60, 0)
end
