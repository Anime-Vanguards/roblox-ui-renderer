-- Fractional pixel sizes: TextSize x UIScale x DPI that is not a whole number of device px (odd TextSizes at
-- 1.5x, UIScale 0.625 like a level-up screen). Plain vs rich text, a rich <font size> run, TextScaled, the Legacy
-- font's 1.5x size, a UIStroke, and the same labels rotated, in large glyphs so a quad scale shows.
-- rur compare --fixture fixtures/text-probe-fracsize.luau --viewport 1200x900 --time 0.2 --background 20,20,26
return function(Target)
	local Y = 6
	local function Label(Name, Props, Parent)
		local L = Instance.new("TextLabel")
		L.Name = Name
		L.BackgroundTransparency = 1
		L.TextColor3 = Color3.new(1, 1, 1)
		L.TextXAlignment = Enum.TextXAlignment.Left
		L.Size = UDim2.fromOffset(560, 56)
		for Key, Value in Props do
			L[Key] = Value
		end
		L.Parent = Parent or Target
		return L
	end
	local Gotham = Font.fromEnum(Enum.Font.GothamBlack)
	local Fredoka = Font.fromEnum(Enum.Font.FredokaOne)

	-- Column 1: plain / rich at odd TextSizes (x.5 px at 1.5x)
	for Index, Size in { 51, 37, 25, 13 } do
		Label(`Plain{Size}`, { FontFace = Gotham, TextSize = Size, Text = "WAVE gy", Position = UDim2.fromOffset(6, Y) })
		Label(`Rich{Size}`, { FontFace = Gotham, TextSize = Size, RichText = true, Text = "WAVE gy", Position = UDim2.fromOffset(300, Y) })
		Label(`FPlain{Size}`, { FontFace = Fredoka, TextSize = Size, Text = "Wave gy", Position = UDim2.fromOffset(600, Y) })
		Label(`FRich{Size}`, { FontFace = Fredoka, TextSize = Size, RichText = true, Text = "Wave gy", Position = UDim2.fromOffset(900, Y) })
		Y += 58
	end

	-- A rich <font size> run of an odd size inside even-sized text
	Label("RunSize", { FontFace = Fredoka, TextSize = 30, RichText = true, Text = `Ab <font size="51">Wg</font> Cd`, Position = UDim2.fromOffset(6, Y) })
	-- Legacy font (1.5x TextSize): 11 -> 24.75 px
	Label("Legacy11", { Font = Enum.Font.Arial, TextSize = 11, Text = "Legacy WAVE gy", Position = UDim2.fromOffset(300, Y) })
	Label("Legacy27", { Font = Enum.Font.Arial, TextSize = 27, Text = "WAVE gy", Position = UDim2.fromOffset(600, Y) })
	-- UIStroke on an odd size
	local Stroked = Label("Stroke51", { FontFace = Gotham, TextSize = 51, Text = "WAVE", TextColor3 = Color3.fromRGB(255, 220, 90), Position = UDim2.fromOffset(900, Y) })
	local Stroke = Instance.new("UIStroke")
	Stroke.Thickness = 2
	Stroke.Color = Color3.fromRGB(10, 10, 10)
	Stroke.Parent = Stroked
	Y += 64

	-- TextScaled (plain steps whole UIScale'd sizes: 33 -> 49.5 px; rich whole px)
	Label("Scaled", { FontFace = Gotham, TextScaled = true, Text = "WAVE", Size = UDim2.fromOffset(200, 33), Position = UDim2.fromOffset(6, Y) })
	Label("ScaledRich", { FontFace = Gotham, TextScaled = true, RichText = true, Text = "WAVE", Size = UDim2.fromOffset(200, 33), Position = UDim2.fromOffset(300, Y) })
	Y += 58

	-- UIScale 0.625 (a level-up screen): TextSize 40 -> 25 logical -> 37.5 px; 18 -> 11 -> 16.5 px
	local Scaled = Instance.new("Frame")
	Scaled.Name = "UIScale625"
	Scaled.BackgroundTransparency = 1
	Scaled.Size = UDim2.fromOffset(1900, 300)
	Scaled.Position = UDim2.fromOffset(6, Y)
	Scaled.Parent = Target
	local Scale = Instance.new("UIScale")
	Scale.Scale = 0.625
	Scale.Parent = Scaled
	Label("S625Plain", { FontFace = Gotham, TextSize = 40, Text = "WAVE gy", Size = UDim2.fromOffset(440, 60), Position = UDim2.fromOffset(0, 0) }, Scaled)
	Label("S625Rich", { FontFace = Gotham, TextSize = 40, RichText = true, Text = "WAVE gy", Size = UDim2.fromOffset(440, 60), Position = UDim2.fromOffset(480, 0) }, Scaled)
	Label("S625Arimo", { FontFace = Font.new("rbxasset://fonts/families/Arimo.json", Enum.FontWeight.Bold), TextSize = 40, Text = "Mythic", Size = UDim2.fromOffset(440, 60), Position = UDim2.fromOffset(960, 0) }, Scaled)
	Label("S625Small", { FontFace = Gotham, TextSize = 18, Text = "WAVE gy small", Size = UDim2.fromOffset(440, 60), Position = UDim2.fromOffset(1440, 0) }, Scaled)
	Y += 50

	-- Rotated (-13 degrees like a level-up screen): plain and rich odd sizes
	Label("RotPlain51", { FontFace = Gotham, TextSize = 51, Text = "WAVE", Rotation = -13, Size = UDim2.fromOffset(260, 70), Position = UDim2.fromOffset(20, Y + 20) })
	Label("RotRich51", { FontFace = Gotham, TextSize = 51, RichText = true, Text = "WAVE", Rotation = -13, Size = UDim2.fromOffset(260, 70), Position = UDim2.fromOffset(320, Y + 20) })
	Label("RotFPlain37", { FontFace = Fredoka, TextSize = 37, Text = "Wave gy", Rotation = 13, Size = UDim2.fromOffset(260, 70), Position = UDim2.fromOffset(620, Y + 20) })
	Label("RotFRich37", { FontFace = Fredoka, TextSize = 37, RichText = true, Text = "Wave gy", Rotation = 13, Size = UDim2.fromOffset(260, 70), Position = UDim2.fromOffset(920, Y + 20) })
end
