-- Rich TextTruncate at the widths where a line starts to keep its first grapheme: "Hello", "Wow" and "End" (SourceSans
-- 14, AtEnd, left and top aligned) under UIScale 0.7 to 2, in boxes a tenth of a local px apart around the width of the
-- first grapheme and the ellipsis (widths through UDim scales). Shows where the client draws the ellipsis alone (at
-- floor(TextSize x UIScale) x UIScale), the first grapheme alone, or the grapheme and the ellipsis.
-- rur compare --fixture fixtures/text-probe-richtrunc-edge.luau --viewport 1500x590 --time 0.2 --background 0,0,0
return function(Target)
	local TextService = game:GetService("TextService")
	local Face = Font.new("rbxasset://fonts/families/SourceSansPro.json")
	local Size = 14
	local Columns = { { 0.7, 0.885, 1, 1.15 }, { 1.3, 1.6 }, { 2 } }
	local Y = 4
	for TextIndex, Text in { "Hello", "Wow", "End" } do
		local Tallest = 0
		for ColumnIndex, Scales in Columns do
			if ColumnIndex == 3 then
				Y += Tallest
				Tallest = 0
			end
			local RowY = Y
			for _, Scale in Scales do
				local E = math.floor(Size * Scale)
				local Huge = Vector2.new(10000, 10000)
				local Fit = TextService:GetTextSize(string.sub(Text, 1, 1), E, Enum.Font.SourceSans, Huge).X
					+ TextService:GetTextSize("\u{2026}", E, Enum.Font.SourceSans, Huge).X
				local Centre = math.round(Fit / Scale * 10) / 10
				local Holder = Instance.new("Frame")
				Holder.Name = string.format("s%03d_%s", math.round(Scale * 1000), Text)
				Holder.BackgroundTransparency = 1
				Holder.Position = UDim2.fromOffset(ColumnIndex == 2 and 560 or 4, RowY)
				Holder.Size = UDim2.fromOffset(1000, 30)
				Holder.Parent = Target
				local UIScale = Instance.new("UIScale")
				UIScale.Scale = Scale
				UIScale.Parent = Holder
				local X = 0
				for Step = -15, 5 do
					local Width = Centre + Step / 10
					local L = Instance.new("TextLabel")
					L.Name = string.format("w%04d", math.round(Width * 100))
					L.BackgroundColor3 = Color3.fromRGB(60 + 30 * (TextIndex - 1), 50, 90)
					L.BorderSizePixel = 0
					L.TextColor3 = Color3.fromRGB(255, 255, 255)
					L.FontFace = Face
					L.TextSize = Size
					L.RichText = true
					L.Text = Text
					L.TextTruncate = Enum.TextTruncate.AtEnd
					L.TextXAlignment = Enum.TextXAlignment.Left
					L.TextYAlignment = Enum.TextYAlignment.Top
					L.Position = UDim2.fromScale(X / 1000, 0)
					L.Size = UDim2.new(Width / 1000, 0, 0, 26)
					L.Parent = Holder
					X += math.ceil(Width) + 3
				end
				RowY += math.ceil(30 * Scale) + 4
			end
			Tallest = math.max(Tallest, RowY - Y)
		end
		Y += Tallest
	end
end
