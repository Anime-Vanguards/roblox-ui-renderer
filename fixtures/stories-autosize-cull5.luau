-- Repro, fifth round of fixtures/stories-autosize-cull.luau: AutomaticSize objects in
-- scrolled ScrollingFrames (cull4's s2 / s4 scrolled after layout and drew their centre-outside cards).
-- Here CanvasPosition is set during the build, before layout, or later; cards' centres are outside the
-- 150 px window once scrolled.
--   node cli/rur.mjs compare --fixture fixtures/stories-autosize-cull5.luau --viewport 1200x520 --time 0.5 --background 20,20,26
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
	local function Scroller(Name)
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
			BackgroundColor3 = Color3.fromRGB(50, 50, 62),
			BorderSizePixel = 0,
			CanvasSize = UDim2.fromOffset(0, 400),
			Position = UDim2.fromOffset(40 + Column * 190, 60 + Row * 230),
			ScrollBarThickness = 4,
			Size = UDim2.fromOffset(150, 150),
		}, Target)
	end

	local function Card(Parent, Y)
		local Card = Make("Frame", {
			Name = "Card",
			AutomaticSize = Enum.AutomaticSize.Y,
			BackgroundColor3 = Color3.fromRGB(200, 70, 70),
			BorderSizePixel = 0,
			Position = UDim2.fromOffset(15, Y),
			Size = UDim2.fromOffset(110, 20),
		}, Parent)
		Make("Frame", { Name = "Content", BackgroundTransparency = 1, Size = UDim2.fromOffset(10, 100) }, Card)
		Make("Frame", { Name = "Marker", BackgroundColor3 = Color3.fromRGB(80, 220, 110), BorderSizePixel = 0, Size = UDim2.fromOffset(12, 12) }, Card)
		return Card
	end

	-- b1: CanvasPosition 40 set right after creating the frame, card at canvas 150..250 (110..210 shown)
	do
		local Scroll = Scroller("b1 scrolled at build")
		Scroll.CanvasPosition = Vector2.new(0, 40)
		Card(Scroll, 150)
	end

	-- b2: CanvasPosition 40 set after the card
	do
		local Scroll = Scroller("b2 scrolled after card")
		Card(Scroll, 150)
		Scroll.CanvasPosition = Vector2.new(0, 40)
	end

	-- b3: scrolled 40 after layout (as cull4 s2)
	do
		local Scroll = Scroller("b3 scrolled later")
		Card(Scroll, 150)
		task.delay(0.1, function()
			Scroll.CanvasPosition = Vector2.new(0, 40)
		end)
	end

	-- b4: scrolled after layout, then back to 0: card at 150..250, centre 200
	do
		local Scroll = Scroller("b4 scrolled back to 0")
		Card(Scroll, 110)
		task.delay(0.1, function()
			Scroll.CanvasPosition = Vector2.new(0, 40)
		end)
		task.delay(0.2, function()
			Scroll.CanvasPosition = Vector2.new(0, 0)
		end)
	end

	-- b5: card created after the scroll (scrolled 40 at build, card added 0.1 s later)
	do
		local Scroll = Scroller("b5 card after scroll")
		task.delay(0.05, function()
			Scroll.CanvasPosition = Vector2.new(0, 40)
		end)
		task.delay(0.15, function()
			Card(Scroll, 150)
		end)
	end

	-- b6: not scrolled, card moved after layout (Position changed later: 70 -> 110)
	do
		local Scroll = Scroller("b6 card moved later")
		local Moved = Card(Scroll, 70)
		task.delay(0.1, function()
			Moved.Position = UDim2.fromOffset(15, 110)
		end)
	end
end
