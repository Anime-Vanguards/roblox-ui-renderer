-- Float32 layout arithmetic, part 2: raw AbsolutePosition / AbsoluteSize under nested UIScales (own
-- UIScale + AnchorPoint), a UIGridLayout, a ScrollingFrame canvas, AutomaticSize, aspect ratio and
-- SizeConstraint, plus raw TextBounds.
return function(Target)
	local Results = {}
	local Frames = {}
	local function Add(Key, Instance)
		table.insert(Frames, { Key = Key, Instance = Instance })
	end
	local function Frame(Parent, Props)
		local Object = Instance.new(Props.Class or "Frame")
		Props.Class = nil
		Object.BackgroundTransparency = 1
		for Name, Value in Props do
			Object[Name] = Value
		end
		Object.Parent = Parent
		return Object
	end
	local function Scale(Parent, Value)
		local UIScale = Instance.new("UIScale")
		UIScale.Scale = Value
		UIScale.Parent = Parent
	end
	for Index, S1 in { 1, 0.885, 1.3 } do
		local Holder = Frame(Target, { Position = UDim2.new(0.011 * Index, 1, 0.013 * Index, 2), Size = UDim2.new(0.8, 0, 0.9, 0) })
		Add(`H{Index}`, Holder)
		if S1 ~= 1 then
			Scale(Holder, S1)
		end
		-- (a) own UIScale + anchor, nested
		for J, S2 in { 1.13, 0.77, 0.6 } do
			local Inner = Frame(Holder, { Position = UDim2.new(0.1 * J, 3, 0.07 * J, 5), Size = UDim2.new(0.21, 7, 0.17, 3), AnchorPoint = Vector2.new(0.37, 0.61) })
			Scale(Inner, S2)
			Add(`H{Index}_I{J}`, Inner)
			for K = 1, 3 do
				local Child = Frame(Inner, { Position = UDim2.new(0.13 * K, K, 0.29 * K, -K), Size = UDim2.new(0.31, 1, 0.23, 2), AnchorPoint = Vector2.new(0.19 * K, 0.41) })
				Add(`H{Index}_I{J}_{K}`, Child)
				if K == 2 then
					Scale(Child, 1.07)
					local Deep = Frame(Child, { Position = UDim2.new(0.33, 1, 0.21, 0), Size = UDim2.new(0.4, 0, 0.3, 1), AnchorPoint = Vector2.new(0.5, 0.5) })
					Add(`H{Index}_I{J}_{K}_D`, Deep)
				end
			end
		end
		-- (b) grid
		local Grid = Frame(Holder, { Position = UDim2.new(0.05, 0, 0.4, 3), Size = UDim2.new(0.43, 1, 0.27, 0) })
		Add(`H{Index}_G`, Grid)
		local GridLayout = Instance.new("UIGridLayout")
		GridLayout.CellSize = UDim2.new(0.137, 3, 0.211, 1)
		GridLayout.CellPadding = UDim2.new(0.019, 1, 0.023, 2)
		GridLayout.HorizontalAlignment = Enum.HorizontalAlignment.Center
		GridLayout.VerticalAlignment = Enum.VerticalAlignment.Center
		GridLayout.Parent = Grid
		for K = 1, 7 do
			local Cell = Frame(Grid, { LayoutOrder = K })
			Add(`H{Index}_G{K}`, Cell)
		end
		-- (c) scrolling frame
		local Scroll = Frame(Holder, { Class = "ScrollingFrame", Position = UDim2.new(0.55, 1, 0.41, 0), Size = UDim2.new(0.37, 3, 0.3, 1), CanvasSize = UDim2.new(1.7, 11, 2.3, 7), CanvasPosition = Vector2.new(13.37, 21.71), ScrollBarThickness = 7 })
		Add(`H{Index}_S`, Scroll)
		for K = 1, 3 do
			local Child = Frame(Scroll, { Position = UDim2.new(0.21 * K, K, 0.17 * K, 3), Size = UDim2.new(0.19, 1, 0.13, 0), AnchorPoint = Vector2.new(0.3, 0.2) })
			Add(`H{Index}_S{K}`, Child)
		end
		-- (d) automatic size
		local Auto = Frame(Holder, { Position = UDim2.new(0.03, 0, 0.75, 1), Size = UDim2.new(0.1, 3, 0.02, 1), AutomaticSize = Enum.AutomaticSize.XY })
		Add(`H{Index}_A`, Auto)
		for K = 1, 2 do
			local Child = Frame(Auto, { Position = UDim2.new(0, 7 * K, 0, 3 * K), Size = UDim2.new(0, 37 * K, 0, 11 * K) })
			Add(`H{Index}_A{K}`, Child)
		end
		-- (e) aspect ratio, (f) size constraint
		local Aspect = Frame(Holder, { Position = UDim2.new(0.31, 2, 0.77, 1), Size = UDim2.new(0.17, 3, 0.19, 0) })
		local Ratio = Instance.new("UIAspectRatioConstraint")
		Ratio.AspectRatio = 1.37
		Ratio.Parent = Aspect
		Add(`H{Index}_R`, Aspect)
		local Relative = Frame(Holder, { Position = UDim2.new(0.52, 0, 0.8, 0), Size = UDim2.new(0.13, 1, 0.11, 0), SizeConstraint = Enum.SizeConstraint.RelativeYY })
		Add(`H{Index}_Y`, Relative)
		-- (g) text
		local Text = Frame(Holder, { Class = "TextLabel", Position = UDim2.new(0.7, 1, 0.8, 0), Size = UDim2.new(0, 0, 0, 0), AutomaticSize = Enum.AutomaticSize.XY, Text = "Samples", TextSize = 19, FontFace = Font.new("rbxasset://fonts/families/SourceSansPro.json") })
		Add(`H{Index}_T`, Text)
	end
	task.wait()
	for _, Entry in Frames do
		local Position, Size = Entry.Instance.AbsolutePosition, Entry.Instance.AbsoluteSize
		local Row = { Position.X, Position.Y, Size.X, Size.Y }
		if Entry.Instance:IsA("TextLabel") then
			table.insert(Row, Entry.Instance.TextBounds.X)
			table.insert(Row, Entry.Instance.TextBounds.Y)
		end
		if Entry.Instance:IsA("ScrollingFrame") then
			table.insert(Row, Entry.Instance.AbsoluteCanvasSize.X)
			table.insert(Row, Entry.Instance.AbsoluteCanvasSize.Y)
			table.insert(Row, Entry.Instance.CanvasPosition.X)
			table.insert(Row, Entry.Instance.CanvasPosition.Y)
		end
		Results[Entry.Key] = Row
	end
	return Results
end
