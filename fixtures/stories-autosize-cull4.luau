-- Repro, fourth round of fixtures/stories-autosize-cull.luau: AutomaticSize objects in
-- ScrollingFrames. Every card's centre lies below the 150 px window; the canvas size, scrolling and
-- nesting vary.
--   node cli/rur.mjs compare --fixture fixtures/stories-autosize-cull4.luau --viewport 1200x520 --time 0.3 --background 20,20,26
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
	local function Scroller(Name, Canvas, Automatic)
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
		return Make("ScrollingFrame", {
			Name = Name,
			AutomaticCanvasSize = if Automatic then Enum.AutomaticSize.Y else Enum.AutomaticSize.None,
			BackgroundColor3 = Color3.fromRGB(50, 50, 62),
			BorderSizePixel = 0,
			CanvasSize = UDim2.fromOffset(0, Canvas),
			Position = UDim2.fromOffset(40 + Column * 190, 60 + Row * 230),
			ScrollBarThickness = 4,
			Size = UDim2.fromOffset(150, 150),
		}, Target)
	end

	local function Card(Parent, Y, Height)
		local Card = Make("Frame", {
			Name = "Card",
			AutomaticSize = Enum.AutomaticSize.Y,
			BackgroundColor3 = Color3.fromRGB(200, 70, 70),
			BorderSizePixel = 0,
			Position = UDim2.fromOffset(15, Y),
			Size = UDim2.fromOffset(110, 20),
		}, Parent)
		Make("Frame", { Name = "Content", BackgroundTransparency = 1, Size = UDim2.fromOffset(10, Height or 100) }, Card)
		Make("Frame", { Name = "Marker", BackgroundColor3 = Color3.fromRGB(80, 220, 110), BorderSizePixel = 0, Size = UDim2.fromOffset(12, 12) }, Card)
		return Card
	end

	-- s1: canvas 400, not scrolled: card 110..210 (centre 160: outside the window, inside the canvas)
	Card(Scroller("s1 canvas 400", 400), 110)

	-- s2: canvas 400, CanvasPosition 40 assigned after layout: card at canvas 150..250 shows at 110..210
	do
		local Scroll = Scroller("s2 scrolled 40", 400)
		Card(Scroll, 150)
		task.delay(0.1, function()
			Scroll.CanvasPosition = Vector2.new(0, 40)
		end)
	end

	-- s3: canvas 400, scrolled 40, card at canvas 70..170 shows at 30..130 (centre 80, inside)
	do
		local Scroll = Scroller("s3 scrolled, centre in", 400)
		Card(Scroll, 70)
		task.delay(0.1, function()
			Scroll.CanvasPosition = Vector2.new(0, 40)
		end)
	end

	-- s4: canvas 400, scrolled 150: card at canvas 260..360 shows at 110..210 (centre 160 on screen,
	-- 310 in the canvas)
	do
		local Scroll = Scroller("s4 scrolled 150", 400)
		Card(Scroll, 260)
		task.delay(0.1, function()
			Scroll.CanvasPosition = Vector2.new(0, 150)
		end)
	end

	-- s5: canvas 400, card at canvas 300..400 (centre 350: outside the window, inside the canvas), not scrolled
	Card(Scroller("s5 far below", 400), 300)

	-- s6: AutomaticCanvasSize Y: canvas grows to the card, 110..210 (centre 160)
	Card(Scroller("s6 automatic canvas", 0, true), 110)

	-- s7: canvas 160 (just past the card's centre 160? card 110..210)
	Card(Scroller("s7 canvas 160", 160), 110)

	-- s8: canvas 170 (centre 160 inside it)
	Card(Scroller("s8 canvas 170", 170), 110)

	-- s9: a mail list's nesting: AutomaticSize list container, third card's centre below the window
	do
		local Scroll = Scroller("s9 nested list", 0, true)
		local Container = Make("Frame", { Name = "Container", AutomaticSize = Enum.AutomaticSize.Y, BackgroundTransparency = 1, Size = UDim2.new(1, -10, 0, 0) }, Scroll)
		Make("UIListLayout", { Padding = UDim.new(0, 6), SortOrder = Enum.SortOrder.LayoutOrder }, Container)
		for Order = 1, 3 do
			local Row = Make("Frame", { Name = `Row{Order}`, AutomaticSize = Enum.AutomaticSize.Y, BackgroundColor3 = Color3.fromRGB(200, 70, 70), BorderSizePixel = 0, LayoutOrder = Order, Size = UDim2.new(1, 0, 0, 10) }, Container)
			Make("Frame", { Name = "Content", BackgroundTransparency = 1, Size = UDim2.fromOffset(10, 60) }, Row)
		end
	end

	-- s10: the same list directly in the ScrollingFrame
	do
		local Scroll = Scroller("s10 flat list", 0, true)
		Make("UIListLayout", { Padding = UDim.new(0, 6), SortOrder = Enum.SortOrder.LayoutOrder }, Scroll)
		for Order = 1, 3 do
			local Row = Make("Frame", { Name = `Row{Order}`, AutomaticSize = Enum.AutomaticSize.Y, BackgroundColor3 = Color3.fromRGB(200, 70, 70), BorderSizePixel = 0, LayoutOrder = Order, Size = UDim2.new(1, -10, 0, 10) }, Scroll)
			Make("Frame", { Name = "Content", BackgroundTransparency = 1, Size = UDim2.fromOffset(10, 60) }, Row)
		end
	end

	-- s11: a clipping Frame (not scrolling) inside the ScrollingFrame around the card
	do
		local Scroll = Scroller("s11 frame clip inside", 400)
		local Clip = Make("Frame", { Name = "Clip", BackgroundTransparency = 1, ClipsDescendants = true, Size = UDim2.fromOffset(150, 150) }, Scroll)
		Card(Clip, 110)
	end

	-- s12: the list of s10 with the third row's centre inside the window (rows 40 tall)
	do
		local Scroll = Scroller("s12 flat list, short rows", 0, true)
		Make("UIListLayout", { Padding = UDim.new(0, 6), SortOrder = Enum.SortOrder.LayoutOrder }, Scroll)
		for Order = 1, 4 do
			local Row = Make("Frame", { Name = `Row{Order}`, AutomaticSize = Enum.AutomaticSize.Y, BackgroundColor3 = Color3.fromRGB(200, 70, 70), BorderSizePixel = 0, LayoutOrder = Order, Size = UDim2.new(1, -10, 0, 10) }, Scroll)
			Make("Frame", { Name = "Content", BackgroundTransparency = 1, Size = UDim2.fromOffset(10, 40) }, Row)
		end
	end
end
