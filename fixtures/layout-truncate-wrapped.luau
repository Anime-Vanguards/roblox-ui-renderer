-- Wrapped TextTruncate in boxes one and two lines tall (SourceSans 16), AtEnd and SplitWord, three texts, box widths
-- 10 .. 100 local px. A client cuts an AtEnd line with the rest of its paragraph as an unwrapped line ("A so…" where
-- the next word wrapped away), and shows nothing of the first line in a box narrower than the ellipsis.
-- compare: --viewport 1040x960 --time 0.2 --background 0,0,0
return function(Target)
	local Face = Font.new("rbxasset://fonts/families/SourceSansPro.json")
	local Texts = { "A somewhat longer label text", "Guild Halls of the Northern Reach", "Hi there friend" }
	local Count = 0
	for T, Text in Texts do
		for _, Mode in { "AtEnd", "SplitWord" } do
			for _, H in { 16, 36 } do
				for Width = 10, 100, 6 do
					Count += 1
					local L = Instance.new("TextLabel")
					L.Name = `W{T}_{Mode}_{H}_{Width}`
					L.Position = UDim2.fromOffset(10 + ((Count - 1) % 8) * 128, 10 + ((Count - 1) // 8) * 39)
					L.Size = UDim2.fromOffset(Width, H)
					L.BackgroundColor3 = Color3.fromRGB(30, 32, 44)
					L.BorderSizePixel = 0
					L.TextColor3 = Color3.fromRGB(240, 240, 240)
					L.FontFace = Face
					L.TextSize = 16
					L.Text = Text
					L.TextWrapped = true
					L.TextTruncate = Enum.TextTruncate[Mode]
					L.Parent = Target
				end
			end
		end
	end
end
