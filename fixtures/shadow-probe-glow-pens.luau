-- UIShadow probe: where Mode = Text places each glyph's glow. Rows of SourceSans "I" (a plain stem) at several text
-- sizes, alignments and positions, clear, with a red shadow at BlurRadius 1 on black (the red channel is the
-- shadow's alpha), so the glow's 50 % level marks each stem's edges.
-- compare --viewport 1200x900 --time 0.3 --background 0,0,0
return function(Target)
	local function Label(X, Y, W, H, Size, Props)
		local L = Instance.new("TextLabel")
		L.Position = UDim2.fromOffset(X, Y)
		L.Size = UDim2.fromOffset(W, H)
		L.BackgroundTransparency = 1
		L.BorderSizePixel = 0
		L.Text = "IIIIIIIIII"
		L.TextColor3 = Color3.new(0, 0, 1)
		L.TextTransparency = 1
		L.FontFace = Font.fromEnum(Enum.Font.SourceSans)
		L.TextSize = Size
		L.TextXAlignment = Enum.TextXAlignment.Left
		L.TextYAlignment = Enum.TextYAlignment.Top
		for Key, Value in Props or {} do
			(L :: any)[Key] = Value
		end
		local S = Instance.new("UIShadow")
		S.Mode = Enum.ApplyShadowMode.Text
		S.Color = Color3.new(1, 0, 0)
		S.BlurRadius = UDim.new(0, 1)
		S.Parent = L
		L.Parent = Target
		return L
	end

	-- A: left and top aligned, text sizes (left column) and the same sizes centred (right column)
	local Y = 10
	for _, Size in { 14, 20, 27, 33, 40, 56, 60, 80 } do
		Label(10, Y, 560, Size + 12, Size)
		Label(600, Y, 560, Size + 12, Size, { TextXAlignment = Enum.TextXAlignment.Center, TextYAlignment = Enum.TextYAlignment.Center })
		Y += Size + 20
	end
	-- B: TextSize 40 right aligned, bottom aligned, at a fractional position, and Fredoka
	Label(10, Y, 560, 60, 40, { TextXAlignment = Enum.TextXAlignment.Right, TextYAlignment = Enum.TextYAlignment.Bottom })
	Label(600.5, Y + 0.25, 560, 60, 40)
	Y += 70
	Label(10, Y, 560, 60, 40, { FontFace = Font.new("rbxasset://fonts/families/FredokaOne.json") })
	Label(600, Y, 560, 60, 40, { FontFace = Font.new("rbxasset://fonts/families/FredokaOne.json"), TextXAlignment = Enum.TextXAlignment.Center, TextYAlignment = Enum.TextYAlignment.Center })
end
