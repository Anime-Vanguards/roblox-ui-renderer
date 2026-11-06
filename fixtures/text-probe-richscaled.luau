-- Rich TextScaled under UIScale (a UI kit's button labels: FredokaOne, RichText, TextScaled, UIScale 0.885):
-- Studio's size search tries whole unscaled pixel sizes N and draws floor(N x UIScale) px glyphs; which N
-- (two can give the same floor) decides the quad scale unfloored / floor. 133 centred labels, pseudo-random
-- text, box, TextSize and UIScale; white text on flat holders.
-- rur compare --fixture fixtures/text-probe-richscaled.luau --viewport 2000x1150 --time 0.2 --background 0,0,0
return function(Target)
	local Texts = { "Mystery", "Dark neutral", "Interact", "Review & craft", "Purple", "Hide Prompt", "Waxo 42", "Small" }
	local Sizes = { 14, 24, 36, 50 }
	local Scales = { 0.885, 0.885, 0.75, 1.25 }
	for I = 1, 133 do
		local Column = (I - 1) % 7
		local Row = (I - 1) // 7
		local Scale = Scales[((I - 1) // 4) % 4 + 1]
		local W = 40 + (I * 37) % 150
		local H = 16 + (I * 13) % 30
		local Holder = Instance.new("Frame")
		Holder.Name = string.format("H%03d", I)
		Holder.BorderSizePixel = 0
		Holder.BackgroundColor3 = Color3.fromRGB(20, 24, 40)
		Holder.Size = UDim2.fromOffset(W, H)
		Holder.Position = UDim2.fromOffset(6 + Column * 256, 4 + Row * 60)
		local L = Instance.new("TextLabel")
		L.Name = "Label"
		L.BackgroundTransparency = 1
		L.TextColor3 = Color3.fromRGB(255, 255, 255)
		L.FontFace = Font.new("rbxasset://fonts/families/FredokaOne.json")
		L.RichText = true
		L.TextScaled = true
		L.TextWrapped = I % 3 ~= 0
		L.TextSize = Sizes[I % 4 + 1]
		L.Text = Texts[I % 8 + 1]
		L.Size = UDim2.fromScale(1, 1)
		L.Parent = Holder
		local U = Instance.new("UIScale")
		U.Scale = Scale
		U.Parent = Holder
		Holder.Parent = Target
	end
end
