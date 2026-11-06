-- Rich TextScaled at UIScale 1 with odd TextSizes (Round 8, notes/text.md): a list row's "VIEW" (button,
-- FredokaOne, RichText, TextScaled, TextSize 17, 80 x 30 box, UIStroke 0.75) lays out 25 px glyphs but Studio
-- draws its quads scaled like a 25.5 px label (the "W" band 27 -> 28 px). Labels of odd and even TextSizes in
-- boxes from too small (the start size does not fit) to roomy, rich and plain, with a thin UIStroke.
-- rur compare --fixture fixtures/text-probe-richscaled1.luau --viewport 1200x1000 --time 0.2 --background 0,0,0
return function(Target)
	local Texts = { "VIEW", "PLAY", "Wow WW", "Review & craft" }
	local Sizes = { 17, 13, 19, 21, 16, 25 }
	local Boxes = { { 80, 30 }, { 60, 22 }, { 110, 40 }, { 140, 60 }, { 50, 30 } }
	local Index = 0
	for R = 0, 1 do
		for S, Size in Sizes do
			for B, Box in Boxes do
				for T, Text in Texts do
					if (S + B + T) % 2 == R then
						local Column = Index % 8
						local Row = Index // 8
						Index += 1
						local Holder = Instance.new("Frame")
						Holder.Name = string.format("R%dS%dB%dT%d", R, Size, B, T)
						Holder.BorderSizePixel = 0
						Holder.BackgroundColor3 = Color3.fromRGB(30, 30, 36)
						Holder.Size = UDim2.fromOffset(Box[1], Box[2])
						Holder.Position = UDim2.fromOffset(4 + Column * 149, 4 + Row * 64)
						Holder.Parent = Target
						local L = Instance.new("TextLabel")
						L.Name = "Label"
						L.BackgroundTransparency = 1
						L.TextColor3 = Color3.fromRGB(255, 255, 255)
						L.FontFace = Font.new("rbxasset://fonts/families/FredokaOne.json")
						L.RichText = R == 0
						L.TextScaled = true
						L.TextWrapped = true
						L.TextSize = Size
						L.Text = Text
						L.Size = UDim2.fromScale(1, 1)
						L.Parent = Holder
						local Stroke = Instance.new("UIStroke")
						Stroke.Thickness = 0.75
						Stroke.Color = Color3.fromRGB(0, 0, 0)
						Stroke.Parent = L
					end
				end
			end
		end
	end
end
