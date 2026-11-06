-- Rotated text: where do the glyph quads of a rotated TextLabel land and how is the atlas sampled? Labels
-- rotated by small and large angles (a level-up screen tilts its labels by -13 degrees) at whole and fractional
-- positions, plain white text and text with a UIStroke / a UIGradient, on the parent's flat background
-- (the labels are transparent).
-- rur compare --fixture fixtures/text-probe-rotate.luau --viewport 2060x1150 --time 0.2 --background 0,0,0
return function(Target)
	local Holder = Instance.new("Frame")
	Holder.Name = "Holder"
	Holder.BorderSizePixel = 0
	Holder.BackgroundColor3 = Color3.fromRGB(20, 24, 40)
	Holder.Size = UDim2.fromOffset(1360, 760)
	Holder.Parent = Target

	local Column, Y = 0, 20
	local function Label(Name, Rotation, Props, Height, Dx)
		Height = Height or 60
		if Y + Height > 740 then
			Y = 20
			Column += 1
		end
		local L = Instance.new("TextLabel")
		L.Name = Name
		L.BackgroundTransparency = 1
		L.TextColor3 = Color3.fromRGB(255, 255, 255)
		L.FontFace = Font.new("rbxasset://fonts/families/FredokaOne.json")
		L.TextSize = 30
		L.Text = "Mythic Rarity 42"
		L.Size = UDim2.fromOffset(300, Height)
		L.Position = UDim2.fromOffset(40 + Column * 440 + (Dx or 0), Y)
		L.Rotation = Rotation
		for Key, Value in Props do
			(L :: any)[Key] = Value
		end
		L.Parent = Holder
		Y += Height + 30
		return L
	end

	Label("R-13", -13, {})
	Label("R-13f", -13, {}, 60, 0.37)
	Label("R13", 13, {})
	Label("R5", 5, {})
	Label("R30", 30, {}, 60, 0.5)
	Label("R-13Src", -13, { FontFace = Font.new("rbxasset://fonts/families/SourceSansPro.json"), TextSize = 24 })
	Label("R90", 90, { TextSize = 24 }, 40)
	Label("R180", 180, {})
	Label("R45", 45, { TextSize = 24 }, 40)
	Label("R-13Left", -13, { TextXAlignment = Enum.TextXAlignment.Left, TextYAlignment = Enum.TextYAlignment.Top })
	local S = Label("R-13Stroke", -13, {})
	local Stroke = Instance.new("UIStroke")
	Stroke.Thickness = 2
	Stroke.Color = Color3.fromRGB(10, 10, 10)
	Stroke.Parent = S
	local G = Label("R-13Grad", -13, {})
	local Gradient = Instance.new("UIGradient")
	Gradient.Color = ColorSequence.new(Color3.fromRGB(255, 60, 40), Color3.fromRGB(60, 220, 255))
	Gradient.Parent = G
	Label("R-13Small", -13, { TextSize = 14 }, 30)
	Label("R-2", -2, {})
end
