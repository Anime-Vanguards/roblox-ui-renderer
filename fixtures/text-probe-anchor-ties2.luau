-- Follows fixtures/text-probe-anchor-ties.luau: centred labels whose centre lands at N + 0.5 - k device px for
-- k = (0.00090 .. 0.00110 in steps of 0.00001) x the device scale (columns 1-4: 1x, 1.5x, 2x, 3x), near the left
-- edge of the screen so the client's float32 positions stay fine. Where the client starts rounding the anchor up
-- tells the margin it rounds with.
-- compare: --viewport 1200x900 --time 0.2 --background 0,0,0
return function(Target)
	local Face = Font.new("rbxasset://fonts/families/SourceSansPro.json")
	for ScaleIndex, Scale in { 1, 1.5, 2, 3 } do
		for Step = 0, 20 do
			local K = (0.0009 + Step * 0.00001) * Scale
			local Anchor = 30
			local N = math.floor(Anchor * Scale)
			local UIScale = (N + 0.5 - K) / (Anchor * Scale)
			local Holder = Instance.new("Frame")
			Holder.Name = string.format("d%d_k%02d", ScaleIndex, Step)
			Holder.BackgroundTransparency = 1
			Holder.Position = UDim2.fromOffset((ScaleIndex - 1) * 4, 4 + ((ScaleIndex - 1) * 21 + Step) * 10)
			Holder.Size = UDim2.fromOffset(60, 10)
			Holder.Parent = Target
			local Scaler = Instance.new("UIScale")
			Scaler.Scale = UIScale
			Scaler.Parent = Holder
			local Label = Instance.new("TextLabel")
			Label.Name = "L"
			Label.BackgroundTransparency = 1
			Label.FontFace = Face
			Label.TextSize = 9
			Label.Text = "Hd"
			Label.TextColor3 = Color3.new(1, 1, 1)
			Label.TextXAlignment = Enum.TextXAlignment.Center
			Label.Position = UDim2.fromOffset(10, 0)
			Label.Size = UDim2.fromOffset(40, 9)
			Label.Parent = Holder
		end
	end
end
