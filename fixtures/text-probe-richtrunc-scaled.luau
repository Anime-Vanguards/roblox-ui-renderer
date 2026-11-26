-- Rich TextTruncate under UIScale in boxes too narrow for the first grapheme and the ellipsis (layout-probe-autoedge:
-- TextBounds.Y there is E x UIScale, 13.275 for TextSize 17 at UIScale 0.885, where a line is 14.667): "End " and
-- "Hello" in boxes just below and above that width, AtEnd and SplitWord, TextYAlignment Top / Center / Bottom, and
-- AutomaticSize Y labels whose height follows TextBounds.Y; UIScale 0.885 (TextSize 17), 1 (TextSize 17) and 1.3
-- (TextSize 26). Each label has a background so its rect and the drawn ellipsis show.
-- rur compare --fixture fixtures/text-probe-richtrunc-scaled.luau --viewport 1300x800 --time 0.2 --background 0,0,0
return function(Target)
	local Face = Font.new("rbxasset://fonts/families/SourceSansPro.json")
	local Cases = { { 17, 0.885, { 12, 16, 19, 20, 24 } }, { 17, 1, { 10, 14, 17, 18, 22 } }, { 26, 1.3, { 14, 22, 29, 30, 36 } } }
	local Aligns = { "Top", "Center", "Bottom", "Auto" }
	for CaseIndex, Case in Cases do
		local Size, Scale, Widths = Case[1], Case[2], Case[3]
		local Holder = Instance.new("Frame")
		Holder.Name = string.format("Scale%d", CaseIndex)
		Holder.BackgroundTransparency = 1
		Holder.Position = UDim2.fromOffset(4, 4 + (CaseIndex - 1) * 262)
		Holder.Size = UDim2.fromOffset(1290 / Scale, 255 / Scale)
		Holder.Parent = Target
		local UIScale = Instance.new("UIScale")
		UIScale.Scale = Scale
		UIScale.Parent = Holder
		for TextIndex, Text in { "End ", "Hello" } do
			for ModeIndex, Mode in { "AtEnd", "SplitWord" } do
				for AlignIndex, Align in Aligns do
					for WidthIndex, Width in Widths do
						local L = Instance.new("TextLabel")
						L.Name = string.format("t%d_%s_%s_w%d", TextIndex, Mode, Align, Width)
						L.BackgroundColor3 = Color3.fromRGB(50 + 30 * AlignIndex, 50, 90)
						L.BorderSizePixel = 0
						L.TextColor3 = Color3.fromRGB(255, 255, 255)
						L.FontFace = Face
						L.TextSize = Size
						L.RichText = true
						L.Text = Text
						L.TextTruncate = Enum.TextTruncate[Mode]
						local Column = (TextIndex - 1) * 2 + ModeIndex
						L.Position = UDim2.fromOffset((Column - 1) * 230 + (WidthIndex - 1) * 44, (AlignIndex - 1) * 48)
						if Align == "Auto" then
							L.AutomaticSize = Enum.AutomaticSize.Y
							L.Size = UDim2.fromOffset(Width, 0)
						else
							L.TextYAlignment = Enum.TextYAlignment[Align]
							L.Size = UDim2.fromOffset(Width, 40)
						end
						L.Parent = Holder
					end
				end
			end
		end
	end
end
