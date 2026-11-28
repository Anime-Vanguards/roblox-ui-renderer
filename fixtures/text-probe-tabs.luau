-- Tabs: what a tab is in a client's text (four spaces of its run's face and size, drawn as nothing). Staircases (a
-- growing prefix before a tab, and the same without it) in four faces, tabs alone, several tabs, a tab between words,
-- wrapped, rich (<b>, <font size>), other sizes and places, alignment, TextBox, and TextScaled labels, plain and rich.
-- The <font size="40"> tab cells (B*_richsize) take their line's baseline from the tab's run, the tallest, as a client
-- does (notes/text.md, "Alignment anchors and rich whitespace baselines").
-- compare: --viewport 1830x1000 --time 0.2 --background 0,0,0
return function(Target)
	local Faces = { "SourceSansPro", "FredokaOne", "Arimo", "RobotoMono" }
	local Col, Row = 0, 0
	local function Cell(Name, Text, Options)
		Options = Options or {}
		local L = Instance.new(Options.Class or "TextLabel")
		L.Name = Name
		local W, H = Options.W or 230, Options.H or 26
		L.Position = UDim2.fromOffset(Options.X or (4 + Col * 240), Options.Y or (4 + Row * 28))
		L.Size = UDim2.fromOffset(W, H)
		L.BackgroundTransparency = 1
		L.BorderSizePixel = 0
		L.TextColor3 = Color3.fromRGB(240, 240, 240)
		L.FontFace = Font.new(`rbxasset://fonts/families/{Options.Face or "SourceSansPro"}.json`)
		L.TextSize = Options.Size or 24
		L.TextXAlignment = Options.Align or Enum.TextXAlignment.Left
		L.TextScaled = Options.Scaled or false
		L.TextWrapped = Options.Wrapped or false
		L.RichText = Options.Rich or false
		if L:IsA("TextBox") then
			L.ClearTextOnFocus = false
		end
		L.Text = Text
		L.Parent = Target
		Row += 1
		if Row >= 34 then
			Row = 0
			Col += 1
		end
		return L
	end
	-- A: staircases per face (prefix of n "i", with and without a tab before the bar)
	for F, Face in Faces do
		for N = 0, 8 do
			Cell(`A{F}_t{N}`, string.rep("i", N) .. "\t|", { Face = Face })
			Cell(`A{F}_n{N}`, string.rep("i", N) .. "|", { Face = Face })
		end
	end
	-- B: tabs alone and in words (two faces)
	local Texts = {
		{ "tab", "\t" }, { "tabbar", "\t|" }, { "bartabbar", "|\t|" }, { "tab2bar", "\t\t|" }, { "hello", "Hello" },
		{ "hellotab", "Hello\t" }, { "hellotabbar", "Hello\t|" }, { "hellotab2bar", "Hello\t\t|" }, { "hello4sp", "Hello    |" },
		{ "hellosp", "Hello |" }, { "abc", "A\tB\tC" }, { "tabhello", "\tHello" }, { "space", " " }, { "spbar", " |" },
		{ "sp4bar", "    |" }, { "Vtab", "V\t|" }, { "tabV", "\tV" }, { "otabo", "o\to" },
	}
	for F, Face in { "SourceSansPro", "FredokaOne" } do
		for _, T in Texts do
			Cell(`B{F}_{T[1]}`, T[2], { Face = Face })
		end
		Cell(`B{F}_rich`, "Hello\t|", { Face = Face, Rich = true })
		Cell(`B{F}_richb`, "<b>Hello\t</b>|", { Face = Face, Rich = true })
		Cell(`B{F}_richsize`, "Hello<font size=\"40\">\t</font>|", { Face = Face, Rich = true, H = 44 })
		Cell(`B{F}_wrap`, "Hello\tWorld", { Face = Face, Wrapped = true, W = 70, H = 54 })
		Cell(`B{F}_wrapsp`, "Hello    World", { Face = Face, Wrapped = true, W = 70, H = 54 })
		Cell(`B{F}_wrap2`, "Hi\tthere\tyou", { Face = Face, Wrapped = true, W = 80, H = 54 })
		Cell(`B{F}_box`, "Hello\t|", { Face = Face, Class = "TextBox" })
		Cell(`B{F}_boxend`, "Hello\t", { Face = Face, Class = "TextBox" })
	end
	-- C: sizes
	for _, Size in { 12, 18, 37, 48 } do
		Cell(`C_s{Size}`, "Hello\t|", { Size = Size, H = Size + 4 })
		Cell(`C_n{Size}`, "Hello|", { Size = Size, H = Size + 4 })
	end
	-- D: places and alignment
	local Base = 4 + 4 * 240
	for I, X in { 0.5, 3, 7.25, 101 } do
		Cell(`D_x{I}`, "Hello\t|", { X = Base + X, Y = 4 + I * 28 })
	end
	Cell("D_center", "Hello\t|", { X = Base, Y = 4 + 6 * 28, W = 200, Align = Enum.TextXAlignment.Center })
	Cell("D_right", "Hello\t|", { X = Base, Y = 4 + 7 * 28, W = 200, Align = Enum.TextXAlignment.Right })
	Cell("D_centerend", "Hello\t", { X = Base, Y = 4 + 8 * 28, W = 200, Align = Enum.TextXAlignment.Center })
	Cell("D_rightend", "Hello\t", { X = Base, Y = 4 + 9 * 28, W = 200, Align = Enum.TextXAlignment.Right })
	-- E: TextScaled, unwrapped and wrapped
	for F, Face in { "SourceSansPro", "FredokaOne" } do
		for I, Box in { { 40, 60 }, { 61, 60 }, { 90, 60 }, { 200, 60 }, { 300, 20 } } do
			Cell(`E{F}_{I}`, "Hello\t", { Face = Face, Scaled = true, X = Base + (I - 1) * 0, Y = 300 + (F - 1) * 330 + (I - 1) * 64, W = Box[1], H = Box[2] })
			Cell(`E{F}_bar{I}`, "Hello\t|", { Face = Face, Scaled = true, X = Base + 280, Y = 300 + (F - 1) * 330 + (I - 1) * 64, W = Box[1], H = Box[2] })
			Cell(`E{F}_rich{I}`, "Hello\t", { Face = Face, Scaled = true, Rich = true, X = Base + 560, Y = 300 + (F - 1) * 330 + (I - 1) * 64, W = Box[1], H = Box[2] })
		end
	end
end
