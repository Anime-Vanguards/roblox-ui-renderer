-- An AutomaticSize XY frame with a vertical UIListLayout whose items include a scale-width item
-- (a mode window's `Holder/Right`: AnchorPoint (1, 0.5) at (1, -20, 0.5, 0), Size 100 x 100, padding 15,
-- UIScale 0.9, a 450 x 155 item and a (1, 0, 0, 31) label bar: Studio 432 wide, offline the bar
-- grew it to the whole holder, 1180). Variants without the UIScale, the padding, the fixed item,
-- with other anchors and positions.
return function(Target)
	local Results = {}
	local Nodes = {}
	local Count = 0
	local function Case(Name, Options)
		Count += 1
		local Holder = Instance.new("Frame")
		Holder.BackgroundTransparency = 1
		Holder.Position = UDim2.fromOffset(((Count - 1) % 2) * 600, ((Count - 1) // 2) * 110)
		Holder.Size = UDim2.fromOffset(590, 100)
		Holder.Parent = Target

		local Right = Instance.new("Frame")
		Right.AutomaticSize = Enum.AutomaticSize.XY
		Right.AnchorPoint = Options.Anchor or Vector2.new(1, 0.5)
		Right.Position = Options.Position or UDim2.new(1, -20, 0.5, 0)
		Right.Size = Options.Size or UDim2.fromOffset(100, 100)
		Right.BackgroundTransparency = 1
		Right.Parent = Holder

		if Options.Fixed ~= false then
			local Canvas = Instance.new(if Options.Group then "CanvasGroup" else "Frame")
			Canvas.Size = UDim2.fromOffset(Options.FixedWidth or 250, 40)
			Canvas.BackgroundTransparency = 1
			Canvas.Parent = Right
			Nodes[`{Name}_fixed`] = Canvas
		end

		local List = Instance.new("UIListLayout")
		List.Padding = UDim.new(0, 10)
		List.VerticalAlignment = Enum.VerticalAlignment.Center
		List.HorizontalAlignment = Options.Align or Enum.HorizontalAlignment.Left
		List.Parent = Right

		if Options.Padding ~= false then
			local Padding = Instance.new("UIPadding")
			Padding.PaddingLeft = UDim.new(0, 15)
			Padding.PaddingRight = UDim.new(0, 15)
			Padding.PaddingTop = UDim.new(0, 15)
			Padding.PaddingBottom = UDim.new(0, 15)
			Padding.Parent = Right
		end

		local Bar = Instance.new("Frame")
		Bar.LayoutOrder = -1
		Bar.Size = Options.BarSize or UDim2.new(1, 0, 0, 31)
		Bar.BackgroundTransparency = 1
		Bar.Parent = Right
		if Options.BarLabel ~= false then
			local BarList = Instance.new("UIListLayout")
			BarList.FillDirection = Enum.FillDirection.Horizontal
			BarList.Parent = Bar
			local Label = Instance.new("TextLabel")
			Label.AutomaticSize = Enum.AutomaticSize.XY
			Label.Size = UDim2.new()
			Label.FontFace = Font.new("rbxasset://fonts/families/SourceSansPro.json")
			Label.TextSize = 20
			Label.Text = "Selected Option"
			Label.Parent = Bar
		end

		if Options.UIScale ~= false then
			local Scale = Instance.new("UIScale")
			Scale.Scale = Options.UIScale or 0.9
			Scale.Parent = Right
		end

		Nodes[`{Name}_right`] = Right
		Nodes[`{Name}_bar`] = Bar
	end

	Case("story", {})
	Case("noScale", { UIScale = false })
	Case("noPad", { Padding = false })
	Case("noFixed", { Fixed = false })
	Case("noLabel", { BarLabel = false })
	Case("group", { Group = true })
	Case("anchor0", { Anchor = Vector2.new(0, 0.5), Position = UDim2.new(0, 20, 0.5, 0) })
	Case("anchorHalf", { Anchor = Vector2.new(0.5, 0.5), Position = UDim2.new(0.5, 0, 0.5, 0) })
	Case("anchor1Mid", { Position = UDim2.new(0.5, 0, 0.5, 0) })
	Case("zeroSize", { Size = UDim2.new() })
	Case("scale1", { UIScale = 1 })
	Case("halfBar", { BarSize = UDim2.new(0.5, 0, 0, 31) })
	Case("offsetBar", { BarSize = UDim2.new(1, -40, 0, 31) })
	Case("center", { Align = Enum.HorizontalAlignment.Center })
	Case("wideFixed", { FixedWidth = 500 })
	Case("anchor0NoScale", { Anchor = Vector2.new(0, 0.5), Position = UDim2.new(0, 20, 0.5, 0), UIScale = false })

	task.wait()
	for Key, Node in Nodes do
		Results[Key] = { Node.AbsolutePosition.X, Node.AbsolutePosition.Y, Node.AbsoluteSize.X, Node.AbsoluteSize.Y }
	end
	return Results
end
