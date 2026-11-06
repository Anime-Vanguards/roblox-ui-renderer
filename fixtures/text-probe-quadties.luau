-- Scaled glyph quads whose corners land just below a half pixel (Round 8, notes/text.md). Rich TextScaled
-- labels under assorted UIScales draw floor(u) px glyphs scaled by u / floor(u) about the pen point with the
-- corners snapped to whole px; over ~200 labels of letters of every width some corners fall within 1/640 px
-- below k + 0.5, where the snap rule shows (a button "Review & craft": "v" right edge 796.4994). Half the
-- labels carry a UIStroke (its bands scale the same way). White on flat dark holders.
-- rur compare --fixture fixtures/text-probe-quadties.luau --viewport 1200x900 --time 0.2 --background 0,0,0
return function(Target)
	local Texts = {
		"abcdefgh", "ijklmnop", "qrstuvwx", "yzABCDEF", "GHIJKLMN", "OPQRSTUV", "WXYZ0123", "456789&?",
		"Review & craft", "Dark neutral", "vvcc wwmm", "Equip %",
	}
	local Scales = { 0.885, 0.8, 0.9, 0.95, 1.062, 0.7, 0.83, 0.97 }
	local Sizes = { 18, 24, 30 }
	local Count = 0
	for Row = 0, 24 do
		for Column = 0, 7 do
			Count += 1
			local I = Count
			local W = 90 + (I * 29) % 50
			local H = 20 + (I * 7) % 12
			local Holder = Instance.new("Frame")
			Holder.Name = string.format("Q%03d", I)
			Holder.BorderSizePixel = 0
			Holder.BackgroundColor3 = Color3.fromRGB(20, 24, 40)
			Holder.Size = UDim2.fromOffset(W, H)
			Holder.Position = UDim2.fromOffset(4 + Column * 149, 4 + Row * 35.5)
			local L = Instance.new("TextLabel")
			L.Name = "Label"
			L.BackgroundTransparency = 1
			L.TextColor3 = Color3.fromRGB(255, 255, 255)
			L.FontFace = Font.new("rbxasset://fonts/families/FredokaOne.json")
			L.RichText = true
			L.TextScaled = true
			L.TextWrapped = true
			L.TextSize = Sizes[I % 3 + 1]
			L.Text = Texts[I % #Texts + 1]
			L.Size = UDim2.fromScale(1, 1)
			L.Parent = Holder
			if I % 2 == 0 then
				local Stroke = Instance.new("UIStroke")
				Stroke.Thickness = 2.5
				Stroke.Color = Color3.fromRGB(0, 0, 0)
				Stroke.Parent = L
			end
			local U = Instance.new("UIScale")
			U.Scale = Scales[(I // 3) % #Scales + 1]
			U.Parent = Holder
			Holder.Parent = Target
		end
	end
end
