-- Wrapped AutomaticSize Y labels under a fine UIScale sweep (0.700 to 1.300 in 0.005 steps): their heights come back
-- from the text's bounds through float32 local units and the UIScale and can land a few float32 ulps under the last
-- line's bottom. Does the client keep that line (TextBounds.Y, drawn lines)? Each holder (one per UIScale) holds
-- labels of three sizes and two texts; holders overlap, only the layout and the text are compared.
--   node cli/rur.mjs compare --fixture fixtures/layout-autotext-linefit.luau --viewport 1200x900 --time 0.2 --background 0,0,0
return function(Target)
	local Face = Font.new("rbxasset://fonts/families/SourceSansPro.json")
	local Texts = { "Claim your daily reward before it expires", "Which units would you like to unequip now" }
	local Sizes = { 14, 17, 22 }
	local Count = 0
	for Step = 0, 120 do
		local Scale = 0.7 + Step * 0.005
		local Holder = Instance.new("Frame")
		Holder.Name = string.format("s%03d", Step)
		Holder.Position = UDim2.fromOffset((Count % 11) * 105, (Count // 11) * 78)
		Holder.Size = UDim2.fromOffset(100, 600)
		Holder.BackgroundTransparency = 1
		Holder.Parent = Target
		Count += 1
		local UIScale = Instance.new("UIScale")
		UIScale.Scale = Scale
		UIScale.Parent = Holder
		local List = Instance.new("UIListLayout")
		List.SortOrder = Enum.SortOrder.LayoutOrder
		List.Parent = Holder
		local Order = 0
		for SizeIndex, TextSize in Sizes do
			for TextIndex, Text in Texts do
				Order += 1
				local L = Instance.new("TextLabel")
				L.Name = `L{TextSize}_{TextIndex}`
				L.LayoutOrder = Order
				L.FontFace = Face
				L.TextSize = TextSize
				L.TextWrapped = true
				L.TextColor3 = Color3.new(1, 1, 1)
				L.BackgroundTransparency = 1
				L.AutomaticSize = Enum.AutomaticSize.Y
				L.Size = UDim2.fromOffset(70 + SizeIndex * 9 + TextIndex * 4, 0)
				L.Text = Text
				L.Parent = Holder
			end
		end
	end
end
