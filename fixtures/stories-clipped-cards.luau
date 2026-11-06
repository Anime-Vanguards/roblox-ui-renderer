-- Repro: list cards partially cut by their clipping container, the structure of the
-- questionnaire of a mail screen (each question's shell, built in code:
-- AutomaticSize Y frame, UICorner 6, UIStroke 1 @ 0.52, BackgroundTransparency
-- 0.34, UIPadding, UIListLayout). Studio drew the partly visible third card's children but not the
-- card's own background and stroke. Columns vary one ingredient each.
--   node cli/rur.mjs compare --fixture fixtures/stories-clipped-cards.luau --viewport 1260x560 --time 0.3 --background 20,20,26
return function(Target)
	local function Card(Parent, Order, Options)
		local Card = Instance.new("Frame")
		Card.Name = "Card"
		Card.LayoutOrder = Order
		Card.BackgroundColor3 = Color3.fromRGB(8, 8, 10)
		Card.BackgroundTransparency = 0.34
		Card.BorderSizePixel = 0
		Card.Size = UDim2.new(1, 0, 0, Options.Height or 90)
		if Options.Automatic ~= false then
			Card.AutomaticSize = Enum.AutomaticSize.Y
		end
		if Options.Corner ~= false then
			local Corner = Instance.new("UICorner")
			Corner.CornerRadius = UDim.new(0, 6)
			Corner.Parent = Card
		end
		if Options.Stroke ~= false then
			local Stroke = Instance.new("UIStroke")
			Stroke.Color = Color3.fromRGB(65, 65, 76)
			Stroke.Thickness = 1
			Stroke.Transparency = 0.52
			Stroke.Parent = Card
		end
		local Padding = Instance.new("UIPadding")
		Padding.PaddingTop = UDim.new(0, 10)
		Padding.PaddingBottom = UDim.new(0, 10)
		Padding.PaddingLeft = UDim.new(0, 12)
		Padding.PaddingRight = UDim.new(0, 12)
		Padding.Parent = Card
		local List = Instance.new("UIListLayout")
		List.Padding = UDim.new(0, 8)
		List.SortOrder = Enum.SortOrder.LayoutOrder
		List.Parent = Card
		for Index = 1, Options.Rows or 3 do
			local Row = Instance.new("Frame")
			Row.Name = `Row{Index}`
			Row.LayoutOrder = Index
			Row.Size = UDim2.new(1, 0, 0, 24)
			Row.BackgroundColor3 = Color3.fromRGB(18, 18, 22)
			Row.BorderSizePixel = 0
			Row.Parent = Card
		end
		Card.Parent = Parent
		return Card
	end

	local function Column(Name, X, Scrolling, Options)
		local Holder
		if Scrolling then
			Holder = Instance.new("ScrollingFrame")
			Holder.CanvasSize = UDim2.new()
			Holder.AutomaticCanvasSize = Enum.AutomaticSize.Y
			Holder.ScrollBarThickness = 4
		else
			Holder = Instance.new("Frame")
			Holder.ClipsDescendants = true
		end
		Holder.Name = Name
		Holder.Position = UDim2.fromOffset(X, 40)
		Holder.Size = UDim2.fromOffset(190, 330)
		Holder.BackgroundColor3 = Color3.fromRGB(60, 90, 120)
		Holder.BorderSizePixel = 0
		Holder.Parent = Target
		local List = Instance.new("UIListLayout")
		List.Padding = UDim.new(0, 10)
		List.SortOrder = Enum.SortOrder.LayoutOrder
		List.Parent = Holder
		for Order = 1, 4 do
			Card(Holder, Order, Options)
		end
	end

	Column("Replica", 20, true, {})
	Column("NoStroke", 225, true, { Stroke = false })
	Column("NoCorner", 430, true, { Corner = false })
	Column("NoAutoSize", 635, true, { Automatic = false, Height = 128 })
	Column("PlainClip", 840, false, {})
	Column("NoStrokeNoCorner", 1045, true, { Stroke = false, Corner = false })
end
