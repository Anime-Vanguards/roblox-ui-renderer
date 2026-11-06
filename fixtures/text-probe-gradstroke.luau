-- A label's UIGradient over coloured text strokes (does it multiply the stroke colour or replace it?),
-- over a legacy TextStroke, and fallback glyphs on a line (which baseline do glyphs from another font take?).
-- rur compare --fixture fixtures/text-probe-gradstroke.luau --viewport 2060x1150 --time 0.2 --background 0,0,0
return function(Target)
	local Column, Y = 0, 4
	local function Label(Name, Props, Height)
		Height = Height or 70
		if Y + Height > 1140 then
			Y = 4
			Column += 1
		end
		local L = Instance.new("TextLabel")
		L.Name = Name
		L.BorderSizePixel = 0
		L.BackgroundColor3 = Color3.fromRGB(24, 28, 36)
		L.TextColor3 = Color3.fromRGB(255, 255, 255)
		L.FontFace = Font.new("rbxasset://fonts/families/FredokaOne.json")
		L.TextSize = 44
		L.TextXAlignment = Enum.TextXAlignment.Left
		L.TextYAlignment = Enum.TextYAlignment.Top
		L.Size = UDim2.fromOffset(660, Height)
		L.Position = UDim2.fromOffset(4 + Column * 680, Y)
		for Key, Value in Props do
			(L :: any)[Key] = Value
		end
		L.Parent = Target
		Y += Height + 4
		return L
	end
	local function Gradient(Parent, Rotation)
		local G = Instance.new("UIGradient")
		G.Color = ColorSequence.new(Color3.fromRGB(255, 60, 40), Color3.fromRGB(250, 220, 60))
		G.Rotation = Rotation or 0
		G.Parent = Parent
		return G
	end
	local function Stroke(Parent, Props)
		local S = Instance.new("UIStroke")
		S.Thickness = 3
		for Key, Value in Props do
			(S :: any)[Key] = Value
		end
		S.Parent = Parent
		return S
	end

	local A = Label("GradBlueStroke", { Text = "WAVE Oxo 42" })
	Gradient(A)
	Stroke(A, { Color = Color3.fromRGB(30, 40, 200) })

	local B = Label("GradGreenStrokeT", { Text = "WAVE Oxo 42" })
	Gradient(B, 90)
	Stroke(B, { Color = Color3.fromRGB(40, 200, 90), Transparency = 0.4 })

	local C = Label("GradStrokeOwn", { Text = "WAVE Oxo 42" })
	Gradient(C)
	Gradient(Stroke(C, { Color = Color3.fromRGB(120, 120, 255) }), 90)

	local D = Label("GradLegacy", { Text = "WAVE Oxo 42", TextStrokeTransparency = 0, TextStrokeColor3 = Color3.fromRGB(30, 40, 200) })
	Gradient(D)

	local E = Label("GradTinted", { Text = "WAVE Oxo 42", TextColor3 = Color3.fromRGB(120, 200, 255) })
	Gradient(E)
	Stroke(E, { Color = Color3.fromRGB(200, 60, 160) })

	Label("FallbackFO", { Text = "Atk \u{2265} 5 \u{2192} \u{2605} \u{4E2D}\u{6587} \u{00E9}" })
	Label("FallbackSS", { Text = "Atk \u{2265} 5 \u{2192} \u{2605} \u{4E2D}\u{6587} \u{00E9}", FontFace = Font.new("rbxasset://fonts/families/SourceSansPro.json") })
	Label("FallbackGotham", { Text = "Atk \u{2265} 5 \u{2192} \u{2605} \u{4E2D}\u{6587}", FontFace = Font.new("rbxasset://fonts/families/GothamSSm.json", Enum.FontWeight.Bold) })
	Label("FallbackSmall", { Text = "Atk \u{2265} 5 \u{2192} \u{2605} \u{4E2D}\u{6587}", TextSize = 16 }, 30)
end
