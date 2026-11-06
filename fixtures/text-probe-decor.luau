-- Rich text decorations: where and how thick are <u> and <s> lines and the <mark> box, over sizes and fonts,
-- and how they combine with a run colour, TextTransparency, a UIStroke and a UIGradient. White text on the
-- label's dark background.
-- rur compare --fixture fixtures/text-probe-decor.luau --viewport 2060x1150 --time 0.2 --background 0,0,0
return function(Target)
	local Column, Y = 0, 4
	local function Label(Name, Props, Height)
		if Y + Height > 1140 then
			Y = 4
			Column += 1
		end
		local L = Instance.new("TextLabel")
		L.Name = Name
		L.BorderSizePixel = 0
		L.BackgroundColor3 = Color3.fromRGB(20, 24, 40)
		L.TextColor3 = Color3.fromRGB(255, 255, 255)
		L.FontFace = Font.new("rbxasset://fonts/families/FredokaOne.json")
		L.RichText = true
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

	local Text = 'Ab <u>under</u> <s>strike</s> <mark color="#E0C040">mk</mark> <u><s>both</s></u> x'
	for _, Size in { 10, 12, 14, 16, 20, 24, 30, 36, 48, 60 } do
		Label(`Fredoka{Size}`, { TextSize = Size, Text = Text }, math.ceil(Size * 1.3))
	end
	for _, Size in { 14, 18, 36 } do
		Label(`Source{Size}`, { TextSize = Size, Text = Text, FontFace = Font.new("rbxasset://fonts/families/SourceSansPro.json") }, math.ceil(Size * 1.3))
	end
	for _, Size in { 16, 24, 40 } do
		Label(`Arimo{Size}`, { TextSize = Size, Text = Text, FontFace = Font.new("rbxasset://fonts/families/Arimo.json") }, math.ceil(Size * 1.3))
	end
	Label("Gotham30", { TextSize = 30, Text = Text, FontFace = Font.new("rbxasset://fonts/families/GothamSSm.json", Enum.FontWeight.Bold) }, 40)
	Label("Bangers30", { TextSize = 30, Text = Text, FontFace = Font.new("rbxasset://fonts/families/Bangers.json") }, 40)
	Label("Color30", { TextSize = 30, Text = 'Ab <font color="#FF4040"><u>red</u> <s>red</s></font> <u><font color="#40FF40">gr</font>een</u> x' }, 40)
	Label("Trans30", { TextSize = 30, Text = Text, TextTransparency = 0.5 }, 40)
	Label("RunTrans30", { TextSize = 30, Text = 'Ab <font transparency="0.5"><u>under</u> <s>strike</s></font> x' }, 40)
	local Stroked = Label("Stroke30", { TextSize = 30, Text = Text }, 40)
	local Stroke = Instance.new("UIStroke")
	Stroke.Thickness = 2
	Stroke.Color = Color3.fromRGB(200, 40, 80)
	Stroke.Parent = Stroked
	local Graded = Label("Grad30", { TextSize = 30, Text = Text }, 40)
	local Gradient = Instance.new("UIGradient")
	Gradient.Color = ColorSequence.new(Color3.fromRGB(255, 60, 40), Color3.fromRGB(60, 220, 255))
	Gradient.Parent = Graded
	Label("TagStroke30", { TextSize = 30, Text = 'Ab <stroke color="#C02850" thickness="2"><u>under</u> <s>strike</s></stroke> x' }, 40)
	Label("Bold30", { TextSize = 30, Text = 'Ab <b><u>under</u> <s>strike</s></b> x' }, 40)
	Label("Size30", { TextSize = 30, Text = 'Ab <font size="18"><u>small</u> <s>small</s></font> <u>big</u> x' }, 40)
	Label("Mark30", { TextSize = 30, Text = 'A <mark color="#E0C040" transparency="0.5">half</mark> <mark color="#40C0E0"><u>mu</u></mark> x' }, 40)
	Label("Wrap30", { TextSize = 30, TextWrapped = true, Size = UDim2.fromOffset(160, 80), Text = '<u>under line wraps here</u>' }, 80)
end
