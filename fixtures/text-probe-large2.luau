-- Large text (fixtures/text-probe-large.luau: from TextSize 68, 102 px at 1.5x, the client's glyphs differ from rur's
-- by about a column): "Hg" and "Il" in SourceSans, FredokaOne and Arimo at TextSize 64 .. 100 in plain Frames, white on
-- a dark background, left aligned at whole device pixels, to see where the change starts and what it is (a glyph size
-- cap with scaled quads, other hinting, or placement).
-- rur compare --fixture fixtures/text-probe-large2.luau --viewport 1500x1000 --time 0.2 --background 0,0,0
return function(Target)
	local Faces = {
		{ "Source", Font.new("rbxasset://fonts/families/SourceSansPro.json") },
		{ "Fredoka", Font.new("rbxasset://fonts/families/FredokaOne.json") },
		{ "Arimo", Font.new("rbxasset://fonts/families/Arimo.json") },
	}
	local Sizes = { 64, 66, 67, 68, 69, 70, 72, 76, 80, 90, 100 }
	for Column, Face in Faces do
		local Holder = Instance.new("Frame")
		Holder.Name = Face[1]
		Holder.Size = UDim2.fromOffset(480, 1000)
		Holder.Position = UDim2.fromOffset((Column - 1) * 500, 0)
		Holder.BackgroundColor3 = Color3.fromRGB(20, 30, 50)
		Holder.BorderSizePixel = 0
		Holder.Parent = Target
		local Y = 2
		for Index, Size in Sizes do
			local X = if Index % 2 == 1 then 4 else 244
			local L = Instance.new("TextLabel")
			L.Name = string.format("S%d", Size)
			L.BackgroundTransparency = 1
			L.TextColor3 = Color3.fromRGB(255, 255, 255)
			L.FontFace = Face[2]
			L.Text = "HgIl"
			L.TextSize = Size
			L.TextXAlignment = Enum.TextXAlignment.Left
			L.TextYAlignment = Enum.TextYAlignment.Top
			L.Position = UDim2.fromOffset(X, Y)
			L.Size = UDim2.fromOffset(234, Size + 2)
			L.Parent = Holder
			if Index % 2 == 0 then
				Y += math.max(Size, Sizes[Index - 1]) + 4
			end
		end
	end
end
