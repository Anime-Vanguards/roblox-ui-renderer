-- Repro, sixth round of fixtures/stories-autosize-cull.luau: what a client draws below an AutomaticSize object it
-- culls (its centre outside its clip). Its own children are culled with it; this asks about grandchildren and
-- deeper, turned children, turned cards and the classes that keep themselves (a client's hit test skips only the
-- culled object's own children: notes/interaction.md). Cards (red) sit at y = 110 in a 150 px clip (centre outside)
-- unless noted; middle frames are yellow or clear, markers green.
--   node cli/rur.mjs compare --fixture fixtures/stories-autosize-cull6.luau --viewport 1200x700 --time 0.3 --background 20,20,26
return function(Target)
	local function Make(ClassName, Properties, Parent)
		local Object = Instance.new(ClassName)
		for Key, Value in Properties do
			Object[Key] = Value
		end
		Object.Parent = Parent
		return Object
	end

	local Index = 0
	local function Holder(Name, Class)
		Index += 1
		local Column, Row = (Index - 1) % 6, (Index - 1) // 6
		Make("TextLabel", {
			Name = "Caption",
			BackgroundTransparency = 1,
			Font = Enum.Font.SourceSans,
			Position = UDim2.fromOffset(40 + Column * 190, 36 + Row * 230),
			Size = UDim2.fromOffset(170, 18),
			Text = Name,
			TextColor3 = Color3.fromRGB(200, 200, 200),
			TextSize = 15,
			TextXAlignment = Enum.TextXAlignment.Left,
		}, Target)
		local Props = {
			Name = Name,
			BackgroundColor3 = Color3.fromRGB(50, 50, 62),
			BorderSizePixel = 0,
			ClipsDescendants = true,
			Position = UDim2.fromOffset(40 + Column * 190, 60 + Row * 230),
			Size = UDim2.fromOffset(150, 150),
		}
		if Class == "ScrollingFrame" then
			Props.CanvasSize = UDim2.new()
			Props.ScrollBarThickness = 0
		end
		return Make(Class or "Frame", Props, Target)
	end
	local function Card(Parent, Props, Class)
		local Object = Make(Class or "Frame", {
			Name = "Card",
			AutomaticSize = Enum.AutomaticSize.Y,
			BackgroundColor3 = Color3.fromRGB(200, 70, 70),
			BorderSizePixel = 0,
			Position = UDim2.fromOffset(15, 110),
			Size = UDim2.fromOffset(120, 20),
		}, Parent)
		for Key, Value in Props or {} do
			(Object :: any)[Key] = Value
		end
		-- the sizing child: 100 px tall, clear
		Make("Frame", { Name = "Content", BackgroundTransparency = 1, Size = UDim2.fromOffset(10, 100) }, Object)
		return Object
	end
	local function Mid(Parent, Props)
		local Object = Make("Frame", {
			Name = "Mid",
			BackgroundColor3 = Color3.fromRGB(230, 200, 80),
			BorderSizePixel = 0,
			Position = UDim2.fromOffset(10, 10),
			Size = UDim2.fromOffset(60, 30),
		}, Parent)
		for Key, Value in Props or {} do
			(Object :: any)[Key] = Value
		end
		return Object
	end
	local function Marker(Parent, Props)
		local Object = Make("Frame", { Name = "Marker", BackgroundColor3 = Color3.fromRGB(80, 220, 110), BorderSizePixel = 0, Position = UDim2.fromOffset(4, 4), Size = UDim2.fromOffset(16, 16) }, Parent)
		for Key, Value in Props or {} do
			(Object :: any)[Key] = Value
		end
		return Object
	end

	-- a: a clear child holding a marker (the marker is a grandchild)
	Marker(Mid(Card(Holder("a grandchild")), { BackgroundTransparency = 1 }))
	-- b: a yellow child holding a marker
	Marker(Mid(Card(Holder("b child + grandchild"))))
	-- c: two clear levels, then a marker (great-grandchild)
	Marker(Mid(Mid(Card(Holder("c great-grandchild")), { BackgroundTransparency = 1 }), { BackgroundTransparency = 1, Position = UDim2.fromOffset(0, 0) }))
	-- d: a turned marker as the card's own child
	Marker(Card(Holder("d turned child")), { Rotation = 5, Position = UDim2.fromOffset(30, 10) })
	-- e: a turned yellow child with a marker
	Marker(Mid(Card(Holder("e turned child's child")), { Rotation = 5 }))
	-- f: a culled TextLabel card (keeps itself) with a yellow child and a marker
	Marker(Mid(Card(Holder("f text card"), { Text = "", BackgroundTransparency = 0 }, "TextLabel")))
	-- g: a culled ImageButton card with a yellow child and a marker
	Marker(Mid(Card(Holder("g image button card"), nil, "ImageButton")))
	-- h: a culled CanvasGroup card with a yellow child and a marker
	Marker(Mid(Card(Holder("h canvas group card"), nil, "CanvasGroup")))
	-- i: a turned card (3 degrees) with a yellow child and a marker
	Marker(Mid(Card(Holder("i turned card"), { Rotation = 3 })))
	-- j: a list-placed yellow child with a marker
	do
		local Card = Card(Holder("j list child's child"))
		Make("UIListLayout", { SortOrder = Enum.SortOrder.LayoutOrder }, Card)
		Card.Content.LayoutOrder = 2
		Marker(Mid(Card, { LayoutOrder = 1 }))
	end
	-- k: a clear child holding a TextLabel with a background
	Make("TextLabel", { Name = "Label", BackgroundColor3 = Color3.fromRGB(80, 220, 110), BorderSizePixel = 0, Position = UDim2.fromOffset(4, 4), Size = UDim2.fromOffset(40, 18), Text = "Ab", TextSize = 16 }, Mid(Card(Holder("k grandchild label")), { BackgroundTransparency = 1 }))
	-- l: an AutomaticSize X card off the clip's left (in1b's AGrand): yellow child placed back inside, marker in it
	Marker(Mid(Card(Holder("l auto X, centre left"), { AutomaticSize = Enum.AutomaticSize.X, Position = UDim2.fromOffset(-100, 30), Size = UDim2.fromOffset(150, 40) }), { Position = UDim2.fromOffset(90, 5), Size = UDim2.fromOffset(60, 30) }), { Position = UDim2.fromOffset(10, 4) })
	-- m: in an unscrolled ScrollingFrame window
	Marker(Mid(Card(Holder("m scrolling window", "ScrollingFrame"))))
	-- n: a clear child holding a ScrollingFrame with a background
	Make("ScrollingFrame", { Name = "Scroll", BackgroundColor3 = Color3.fromRGB(80, 220, 110), BorderSizePixel = 0, CanvasSize = UDim2.new(), ScrollBarThickness = 0, Position = UDim2.fromOffset(4, 4), Size = UDim2.fromOffset(30, 20) }, Mid(Card(Holder("n grandchild scroller")), { BackgroundTransparency = 1 }))
	-- o: the card's centre inside (control: everything draws)
	Marker(Mid(Card(Holder("o centre inside (control)"), { Position = UDim2.fromOffset(15, 20) })))
end
