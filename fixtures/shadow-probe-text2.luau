-- UIShadow probe: the blur of Mode = Text on large solid glyphs (a full block, a thin bar), clear text with
-- a red shadow on black, so the red channel is the alpha byte of the shadow: blur radii, text sizes, offsets.
-- compare --viewport 1200x900 --time 0.3 --background 0,0,0
return function(Target)
	local function Label(X, Y, W, H, Text, Size, ShadowProps, Props)
		local L = Instance.new("TextLabel")
		L.Position = UDim2.fromOffset(X, Y)
		L.Size = UDim2.fromOffset(W, H)
		L.BackgroundTransparency = 1
		L.BorderSizePixel = 0
		L.Text = Text
		L.TextColor3 = Color3.new(0, 0, 1)
		L.TextTransparency = 1
		L.FontFace = Font.fromEnum(Enum.Font.SourceSans)
		L.TextSize = Size
		for Key, Value in Props or {} do
			(L :: any)[Key] = Value
		end
		local S = Instance.new("UIShadow")
		S.Mode = Enum.ApplyShadowMode.Text
		S.Color = Color3.new(1, 0, 0)
		for Key, Value in ShadowProps do
			(S :: any)[Key] = Value
		end
		S.Parent = L
		L.Parent = Target
		return L
	end

	-- A: a full block at TextSize 100, blur radii
	for Index, Blur in { 0, 2, 4, 8, 16, 24 } do
		Label(10 + (Index - 1) * 198, 10, 190, 190, "\u{2588}", 100, { BlurRadius = UDim.new(0, Blur) })
	end
	-- B: larger blurs, and the block at smaller sizes with one blur
	Label(10, 210, 290, 220, "\u{2588}", 100, { BlurRadius = UDim.new(0, 32) })
	Label(310, 210, 290, 220, "\u{2588}", 100, { BlurRadius = UDim.new(0, 48) })
	Label(610, 210, 190, 220, "\u{2588}", 60, { BlurRadius = UDim.new(0, 8) })
	Label(810, 210, 190, 220, "\u{2588}", 30, { BlurRadius = UDim.new(0, 8) })
	Label(1010, 210, 180, 220, "\u{2588}", 14, { BlurRadius = UDim.new(0, 8) })
	-- C: thin bars, a row of blocks, offset, spread, transparency, a scale blur radius
	Label(10, 440, 150, 190, "|", 100, { BlurRadius = UDim.new(0, 8) })
	Label(170, 440, 150, 190, "_", 100, { BlurRadius = UDim.new(0, 8) })
	Label(330, 440, 230, 190, "\u{2588}\u{2588}", 60, { BlurRadius = UDim.new(0, 8) })
	Label(570, 440, 190, 190, "\u{2588}", 60, { BlurRadius = UDim.new(0, 8), Offset = UDim2.fromOffset(20.5, -10.25) })
	Label(770, 440, 190, 190, "\u{2588}", 60, { BlurRadius = UDim.new(0, 8), Spread = UDim2.fromOffset(20, 20) })
	Label(970, 440, 220, 190, "\u{2588}", 60, { BlurRadius = UDim.new(0, 8), Transparency = 0.5 })
	-- D: a scale blur radius, a scale offset, the block under UIScale 2, a fractional position, hard at a fraction
	Label(10, 650, 190, 190, "\u{2588}", 60, { BlurRadius = UDim.new(0.1, 0) })
	Label(210, 650, 190, 190, "\u{2588}", 60, { BlurRadius = UDim.new(0, 8), Offset = UDim2.fromScale(0.2, 0.1) })
	local Scaled = Label(450, 700, 95, 95, "\u{2588}", 30, { BlurRadius = UDim.new(0, 4) })
	local Scale = Instance.new("UIScale")
	Scale.Scale = 2
	Scale.Parent = Scaled
	Label(610.3, 650.7, 190, 190, "\u{2588}", 60, { BlurRadius = UDim.new(0, 8) }, { Position = UDim2.new(0, 610, 0, 650), AnchorPoint = Vector2.new(0.002, 0.004) })
	Label(810, 650, 190, 190, "\u{2588}", 60, { BlurRadius = UDim.new(0, 0), Offset = UDim2.fromOffset(10.3, 5.7) })
	Label(1010, 650, 180, 190, "\u{2588}", 60, { BlurRadius = UDim.new(0, 1) })
end
