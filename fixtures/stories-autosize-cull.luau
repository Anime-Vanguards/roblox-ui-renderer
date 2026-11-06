-- Repro: Studio skips the own background / stroke of an AutomaticSize frame that its
-- clip cuts, depending on where the frame's centre lies (a mail questionnaire's cards, see
-- fixtures/stories-clipped-cards2.luau). Each case sits in its own 150x150 clipping container; the card
-- is a red frame sized by AutomaticSize around a 100 px tall child, with a small green marker child at
-- its top-left corner (children always draw).
--   node cli/rur.mjs compare --fixture fixtures/stories-autosize-cull.luau --viewport 1200x520 --time 0.3 --background 20,20,26
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
	local function Case(Name, Options)
		Index += 1
		local Column, Row = (Index - 1) % 6, (Index - 1) // 6
		local Holder
		if Options.Scrolling then
			Holder = Make("ScrollingFrame", { Name = Name, CanvasSize = UDim2.new(), ScrollBarThickness = 0 }, nil)
		else
			Holder = Make("Frame", { Name = Name, ClipsDescendants = true }, nil)
		end
		Holder.Position = UDim2.fromOffset(40 + Column * 190, 60 + Row * 230)
		Holder.Size = UDim2.fromOffset(150, 150)
		Holder.BackgroundColor3 = Color3.fromRGB(50, 50, 62)
		Holder.BorderSizePixel = 0
		Holder.Parent = Target

		local Axis = Options.Axis or "Y"
		local Card = Make(Options.Class or "Frame", {
			Name = "Card",
			BackgroundColor3 = Color3.fromRGB(200, 70, 70),
			BorderSizePixel = 0,
			Position = UDim2.fromOffset(Options.X or 15, Options.Y or 15),
			Size = if Axis == "X" then UDim2.fromOffset(20, 100) else UDim2.fromOffset(120, 20),
		}, nil)
		if Options.Automatic ~= false then
			Card.AutomaticSize = if Axis == "X" then Enum.AutomaticSize.X else Enum.AutomaticSize.Y
		elseif Axis == "X" then
			Card.Size = UDim2.fromOffset(100, 100)
		else
			Card.Size = UDim2.fromOffset(120, 100)
		end
		if Options.Class == "TextLabel" then
			Card.Text = "Label"
			Card.TextColor3 = Color3.new(1, 1, 1)
			Card.TextSize = 18
			Card.Font = Enum.Font.SourceSansBold
			Card.TextYAlignment = Enum.TextYAlignment.Top
		elseif Options.Class == "ImageLabel" then
			Card.Image = "rbxasset://textures/ui/GuiImagePlaceholder.png"
		end
		if Options.Decorated then
			Make("UICorner", { CornerRadius = UDim.new(0, 8) }, Card)
			Make("UIStroke", { Color = Color3.fromRGB(240, 240, 240), Thickness = 2 }, Card)
		end
		-- Sizes the card: 100 px along the automatic axis
		Make("Frame", { Name = "Content", BackgroundTransparency = 1, Size = if Axis == "X" then UDim2.fromOffset(100, 10) else UDim2.fromOffset(10, 100) }, Card)
		Make("Frame", { Name = "Marker", BackgroundColor3 = Color3.fromRGB(80, 220, 110), BorderSizePixel = 0, Size = UDim2.fromOffset(12, 12) }, Card)
		Card.Parent = Holder

		local Caption = Make("TextLabel", {
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
		return Caption
	end

	-- Bottom edge: centre inside (card 60..160), centre outside (110..210), centre exactly on the edge (100..200)
	Case("B centre in", { Y = 60 })
	Case("B centre out", { Y = 110 })
	Case("B centre on edge", { Y = 100 })
	-- Top edge: centre inside (-40..60), outside (-60..40)
	Case("T centre in", { Y = -40 })
	Case("T centre out", { Y = -60 })
	-- Right edge, AutomaticSize X: centre outside (110..210)
	Case("R X centre out", { Axis = "X", X = 110 })
	-- Controls and variants of "B centre out"
	Case("B no autosize", { Y = 110, Automatic = false })
	Case("B decorated", { Y = 110, Decorated = true })
	Case("B scrolling", { Y = 110, Scrolling = true })
	Case("B TextLabel", { Y = 110, Class = "TextLabel" })
	Case("B ImageLabel", { Y = 110, Class = "ImageLabel" })
	Case("R X centre in", { Axis = "X", X = 60 })
end
