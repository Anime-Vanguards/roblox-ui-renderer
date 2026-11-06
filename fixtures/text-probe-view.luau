-- A list row's "VIEW" (Round 8, notes/text.md): the button label (FredokaOne, RichText, TextScaled,
-- TextWrapped, TextSize 17, 80 x 30, UIStroke 0.75, an identity UIGradient) draws its "W" stroke band 1 px wider
-- than our 27 px quad. The label and variants: without the gradient, not rich, other TextSizes, other texts.
-- rur compare --fixture fixtures/text-probe-view.luau --viewport 1200x900 --time 0.2 --background 0,0,0
return function(Target)
	local Texts = { "VIEW", "PLAY", "WWW", "VIEW W", "WAVE" }
	local Variants = {
		{ Rich = true, Gradient = true, Size = 17 },
		{ Rich = true, Gradient = false, Size = 17 },
		{ Rich = false, Gradient = true, Size = 17 },
		{ Rich = false, Gradient = false, Size = 17 },
		{ Rich = true, Gradient = true, Size = 16 },
		{ Rich = true, Gradient = true, Size = 18 },
		{ Rich = true, Gradient = false, Size = 25 },
		{ Rich = false, Gradient = false, Size = 25 },
	}
	for V, Variant in Variants do
		for T, Text in Texts do
			local Holder = Instance.new("Frame")
			Holder.Name = string.format("V%dT%d", V, T)
			Holder.BorderSizePixel = 0
			Holder.BackgroundColor3 = Color3.fromRGB(19, 19, 19)
			Holder.Size = UDim2.fromOffset(80, 30)
			Holder.Position = UDim2.fromOffset(8 + (T - 1) * 110, 8 + (V - 1) * 44)
			Holder.Parent = Target
			local L = Instance.new("TextLabel")
			L.Name = "Label"
			L.BackgroundTransparency = 1
			L.TextColor3 = Color3.fromRGB(255, 255, 255)
			L.FontFace = Font.new("rbxasset://fonts/families/FredokaOne.json")
			L.RichText = Variant.Rich
			L.TextScaled = true
			L.TextWrapped = true
			L.TextSize = Variant.Size
			L.Text = Text
			L.Size = UDim2.fromScale(1, 1)
			L.Parent = Holder
			local Stroke = Instance.new("UIStroke")
			Stroke.Thickness = 0.75
			Stroke.Color = Color3.fromRGB(0, 0, 0)
			Stroke.Parent = L
			if Variant.Gradient then
				local Gradient = Instance.new("UIGradient")
				Gradient.Rotation = 90
				Gradient.Parent = L
			end
		end
	end
end
