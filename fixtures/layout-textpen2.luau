-- Centred text pen placement for rich text, UIStroke'd text and FredokaOne (a UI kit's button label
-- path), as fixtures/layout-textpen.luau does for plain SourceSans: content boxes at device-pixel
-- fractions 0, .25, .5, .75 (DPI 1.5), odd / even box widths and a range of string widths, every
-- string starting with an isolated "I" stem (tools/analysis/layout_textpen.py).
return function(Target)
	local Texts = { "I", "I  x", "I  xx", "I  xxx", "I  xxxx", "I  xm", "I  mm", "I  xmx" }
	local Widths = { 60, 61 }
	local Variants = {
		{ Name = "rich", Rich = true, Stroke = false, Face = Font.new("rbxasset://fonts/families/SourceSansPro.json") },
		{ Name = "stroke", Rich = false, Stroke = true, Face = Font.new("rbxasset://fonts/families/SourceSansPro.json") },
		{ Name = "fredoka", Rich = false, Stroke = false, Face = Font.new("rbxasset://fonts/families/FredokaOne.json") },
		{ Name = "fredrich", Rich = true, Stroke = true, Face = Font.new("rbxasset://fonts/families/FredokaOne.json") },
	}
	local Fractions = {
		{ Name = "f00", Scale = 0 },
		{ Name = "f25", Scale = 1 / 6 },
		{ Name = "f50", Scale = 1 },
		{ Name = "f75", Scale = 1 / 2 },
	}
	local RowWidth = 1200

	local Row = 0
	for _, Variant in Variants do
		for _, Fraction in Fractions do
			local Holder = Instance.new("Frame")
			Holder.Name = `{Variant.Name}_{Fraction.Name}`
			Holder.BackgroundTransparency = 1
			Holder.Position = UDim2.fromOffset(0, 4 + Row * 34)
			Holder.Size = UDim2.fromOffset(RowWidth, 32)
			Holder.Parent = Target
			Row += 1

			local Column = 0
			for _, Width in Widths do
				for TextIndex, Text in Texts do
					local Label = Instance.new("TextLabel")
					Label.Name = `{Variant.Name}_{Fraction.Name}_w{Width}_t{TextIndex}`
					Label.BackgroundTransparency = 1
					Label.TextColor3 = Color3.new(1, 1, 1)
					Label.FontFace = Variant.Face
					Label.TextSize = 20
					Label.RichText = Variant.Rich
					Label.Text = Text
					Label.TextXAlignment = Enum.TextXAlignment.Center
					Label.TextYAlignment = Enum.TextYAlignment.Center
					Label.Size = UDim2.fromOffset(Width, 28)
					Label.Position = UDim2.new(Fraction.Scale / RowWidth, 4 + Column * 72, 0, 2)
					if Variant.Stroke then
						local Stroke = Instance.new("UIStroke")
						Stroke.Thickness = 2
						Stroke.Parent = Label
					end
					Label.Parent = Holder
					Column += 1
				end
			end
		end
	end
end
