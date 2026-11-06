-- Repro, third round of fixtures/stories-autosize-cull.luau: classes, layouts, exact
-- boundaries and scrolled canvases for AutomaticSize objects whose centre leaves their clip. Cards at
-- y = 110 in a 150 px clip (centre 160, outside) unless noted.
--   node cli/rur.mjs compare --fixture fixtures/stories-autosize-cull3.luau --viewport 1200x520 --time 0.3 --background 20,20,26
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
	local function Holder(Name, Scrolling)
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
		local Holder = Make(if Scrolling then "ScrollingFrame" else "Frame", {
			Name = Name,
			BackgroundColor3 = Color3.fromRGB(50, 50, 62),
			BorderSizePixel = 0,
			ClipsDescendants = true,
			Position = UDim2.fromOffset(40 + Column * 190, 60 + Row * 230),
			Size = UDim2.fromOffset(150, 150),
		}, Target)
		if Scrolling then
			Holder.ScrollBarThickness = 0
			Holder.CanvasSize = UDim2.fromOffset(0, 400)
		end
		return Holder
	end

	local function Card(Parent, ClassName, Y, Extra)
		local Card = Make(ClassName, {
			Name = "Card",
			AutomaticSize = Enum.AutomaticSize.Y,
			BackgroundColor3 = Color3.fromRGB(200, 70, 70),
			BorderSizePixel = 0,
			Position = UDim2.fromOffset(15, Y),
			Size = UDim2.fromOffset(120, 20),
		}, nil)
		for Key, Value in Extra or {} do
			Card[Key] = Value
		end
		Card.Parent = Parent
		return Card
	end

	local function Sizer(Parent, Height)
		return Make("Frame", { Name = "Content", BackgroundTransparency = 1, Size = UDim2.fromOffset(10, Height or 100) }, Parent)
	end

	local function Marker(Parent, Properties)
		local Object = Make("Frame", { Name = "Marker", BackgroundColor3 = Color3.fromRGB(80, 220, 110), BorderSizePixel = 0, Size = UDim2.fromOffset(12, 12) }, Parent)
		for Key, Value in Properties or {} do
			Object[Key] = Value
		end
		return Object
	end

	local TextProperties = { Text = "Text", TextColor3 = Color3.new(1, 1, 1), TextSize = 18, Font = Enum.Font.SourceSansBold, TextYAlignment = Enum.TextYAlignment.Top }

	-- 1: TextLabel sized by a child, plain marker child (probe 1 drew the label, not the marker)
	do
		local Card = Card(Holder("1 TextLabel"), "TextLabel", 110, TextProperties)
		Sizer(Card)
		Marker(Card, { Position = UDim2.fromOffset(90, 0) })
	end

	-- 2: TextLabel sized by its own wrapped text only
	do
		Card(Holder("2 TextLabel by text"), "TextLabel", 110, { Text = "one two three four five six seven eight nine ten eleven", TextWrapped = true, TextColor3 = Color3.new(1, 1, 1), TextSize = 20, Font = Enum.Font.SourceSansBold, TextYAlignment = Enum.TextYAlignment.Top })
	end

	-- 3: TextButton sized by a child
	do
		local Card = Card(Holder("3 TextButton"), "TextButton", 110, TextProperties)
		Card.AutoButtonColor = false
		Sizer(Card)
		Marker(Card, { Position = UDim2.fromOffset(90, 0) })
	end

	-- 4: ImageButton sized by a child
	do
		local Card = Card(Holder("4 ImageButton"), "ImageButton", 110, { Image = "rbxasset://textures/ui/GuiImagePlaceholder.png", AutoButtonColor = false })
		Sizer(Card)
		Marker(Card)
	end

	-- 5: CanvasGroup sized by a child
	do
		local Card = Card(Holder("5 CanvasGroup"), "CanvasGroup", 110)
		Sizer(Card)
		Marker(Card)
	end

	-- 6: ScrollingFrame with AutomaticSize, sized by a child
	do
		local Card = Card(Holder("6 ScrollingFrame"), "ScrollingFrame", 110, { ScrollBarThickness = 0, CanvasSize = UDim2.new() })
		Sizer(Card)
		Marker(Card)
	end

	-- 7: grid layout children
	do
		local Card = Card(Holder("7 grid children"), "Frame", 110)
		Make("UIGridLayout", { CellSize = UDim2.fromOffset(12, 50), CellPadding = UDim2.fromOffset(4, 0), SortOrder = Enum.SortOrder.LayoutOrder }, Card)
		Marker(Card, { LayoutOrder = 1 })
		Marker(Card, { LayoutOrder = 2, BackgroundColor3 = Color3.fromRGB(80, 160, 230) })
	end

	-- 8: list child holding a plain grandchild
	do
		local Card = Card(Holder("8 list grandchild"), "Frame", 110)
		Make("UIListLayout", { SortOrder = Enum.SortOrder.LayoutOrder }, Card)
		local Row = Make("Frame", { Name = "Row", BackgroundColor3 = Color3.fromRGB(80, 160, 230), BorderSizePixel = 0, LayoutOrder = 1, Size = UDim2.fromOffset(60, 30) }, Card)
		Marker(Row, { Position = UDim2.fromOffset(40, 10) })
		Sizer(Card, 70).LayoutOrder = 2
	end

	-- 9: centre exactly on the top edge (card -50..50)
	do
		local Card = Card(Holder("9 centre on top edge"), "Frame", -50)
		Sizer(Card)
		Marker(Card, { Position = UDim2.fromOffset(0, 60) })
	end

	-- 10: AutomaticSize X, centre exactly on the left edge (card -50..50)
	do
		local Card = Make("Frame", { Name = "Card", AutomaticSize = Enum.AutomaticSize.X, BackgroundColor3 = Color3.fromRGB(200, 70, 70), BorderSizePixel = 0, Position = UDim2.fromOffset(-50, 20), Size = UDim2.fromOffset(20, 100) }, Holder("10 centre on left edge"))
		Make("Frame", { Name = "Content", BackgroundTransparency = 1, Size = UDim2.fromOffset(100, 10) }, Card)
		Marker(Card, { Position = UDim2.fromOffset(60, 0) })
	end

	-- 11: scrolled canvas: canvas y 150..250 shows at 110..210 after scrolling by 40
	do
		local Holder = Holder("11 scrolled canvas", true)
		local Card = Card(Holder, "Frame", 150)
		Sizer(Card)
		Marker(Card)
		task.delay(0.1, function()
			Holder.CanvasPosition = Vector2.new(0, 40)
		end)
	end

	-- 12: transparent card with only a UIStroke
	do
		local Card = Card(Holder("12 stroke only"), "Frame", 110, { BackgroundTransparency = 1 })
		Make("UIStroke", { Color = Color3.fromRGB(240, 240, 240), Thickness = 3 }, Card)
		Sizer(Card)
	end
end
