-- Incremental layout workout (not a Studio parity fixture): every frame mutates something that
-- affects layout in a different way (sizes inside nested AutomaticSize lists, text, sort order,
-- visibility, reparenting, creation/destruction, grid/page/table layouts, scrolling, UIScale,
-- rotation, constraints, padding, flex). Run it frame by frame through a live session:
--   (a live-session workout: the engine golden corpus replays it)
return function(Target)
	local RunService = game:GetService("RunService")

	local Count = 0
	local function Cell(Name, W, H)
		local Column = Count % 5
		local Row = Count // 5
		Count += 1
		local F = Instance.new("Frame")
		F.Name = Name
		F.Position = UDim2.fromOffset(10 + Column * 236, 10 + Row * 226)
		F.Size = UDim2.fromOffset(W or 226, H or 216)
		F.BackgroundColor3 = Color3.fromRGB(40, 40, 52)
		F.BorderSizePixel = 0
		F.Parent = Target
		return F
	end

	local function New(ClassName, Parent, Props)
		local Object = Instance.new(ClassName)
		for Key, Value in Props or {} do
			(Object :: any)[Key] = Value
		end
		Object.Parent = Parent
		return Object
	end

	local function Box(Parent, Name, W, H, Props)
		local F = New("Frame", nil, Props)
		F.Name = Name
		F.Size = if typeof(W) == "UDim2" then W else UDim2.fromOffset(W, H)
		F.BackgroundColor3 = Color3.fromHSV((#Parent:GetChildren() * 0.13) % 1, 0.6, 0.9)
		F.BorderSizePixel = 0
		F.Parent = Parent
		return F
	end

	local function Label(Parent, Name, Text, Props)
		local L = New("TextLabel", nil, {
			Name = Name,
			Text = Text,
			FontFace = Font.new("rbxasset://fonts/families/SourceSansPro.json"),
			TextSize = 16,
			BackgroundColor3 = Color3.fromRGB(70, 70, 90),
			TextColor3 = Color3.new(1, 1, 1),
			BorderSizePixel = 0,
		})
		for Key, Value in Props or {} do
			(L :: any)[Key] = Value
		end
		L.Parent = Parent
		return L
	end

	local Steps: { (number) -> () } = {}

	-- 1. Nested AutomaticSize Y stacks: a deep item's height animates, ancestors grow, siblings move
	do
		local Root = Cell("NestedAuto")
		local Outer = Box(Root, "Outer", UDim2.new(1, -10, 0, 0), nil, { AutomaticSize = Enum.AutomaticSize.Y, Position = UDim2.fromOffset(5, 5) })
		New("UIListLayout", Outer, { Padding = UDim.new(0, 4), SortOrder = Enum.SortOrder.LayoutOrder })
		New("UIPadding", Outer, { PaddingTop = UDim.new(0, 3), PaddingBottom = UDim.new(0, 5), PaddingLeft = UDim.new(0, 2) })
		local Inner = Box(Outer, "Inner", UDim2.new(1, 0, 0, 0), nil, { AutomaticSize = Enum.AutomaticSize.Y, LayoutOrder = 1 })
		New("UIListLayout", Inner, { Padding = UDim.new(0, 2), SortOrder = Enum.SortOrder.LayoutOrder })
		local Grow = Box(Inner, "Grow", UDim2.new(0.5, 0, 0, 10), nil, { LayoutOrder = 1 })
		Box(Inner, "After", UDim2.new(1, 0, 0, 12), nil, { LayoutOrder = 2 })
		local Text = Label(Outer, "Text", "wrapped text that reflows as the width changes", { Size = UDim2.new(1, 0, 0, 0), AutomaticSize = Enum.AutomaticSize.Y, TextWrapped = true, LayoutOrder = 2 })
		Box(Outer, "Last", UDim2.new(1, 0, 0, 14), nil, { LayoutOrder = 3 })
		table.insert(Steps, function(Frame)
			Grow.Size = UDim2.new(0.5, 0, 0, 10 + (Frame % 17) * 2)
			if Frame % 23 == 0 then
				Outer.Size = UDim2.new(1, -10 - (Frame % 3) * 30, 0, 0)
			end
			if Frame % 29 == 0 then
				Text.Text = string.rep("word ", 3 + Frame % 11)
			end
		end)
	end

	-- 2. AutomaticSize X text whose Text changes; a horizontal list re-flows after it
	do
		local Root = Cell("AutoText")
		local Row = Box(Root, "Row", UDim2.fromOffset(0, 30), nil, { AutomaticSize = Enum.AutomaticSize.X, Position = UDim2.fromOffset(5, 5) })
		New("UIListLayout", Row, { FillDirection = Enum.FillDirection.Horizontal, Padding = UDim.new(0, 3), VerticalAlignment = Enum.VerticalAlignment.Center })
		local Counter = Label(Row, "Counter", "0", { AutomaticSize = Enum.AutomaticSize.X, Size = UDim2.fromOffset(0, 24) })
		Box(Row, "Tail", 20, 20)
		local Scaled = Label(Root, "Scaled", "Scaled text", { TextScaled = true, Position = UDim2.fromOffset(5, 60), Size = UDim2.fromOffset(100, 30) })
		local Rich = Label(Root, "Rich", "<b>rich</b> text", { RichText = true, AutomaticSize = Enum.AutomaticSize.XY, Position = UDim2.fromOffset(5, 110), Size = UDim2.fromOffset(0, 0) })
		table.insert(Steps, function(Frame)
			Counter.Text = tostring(Frame * 37 % 100000)
			Scaled.Size = UDim2.fromOffset(60 + (Frame % 40) * 3, 20 + (Frame % 13) * 2)
			if Frame % 7 == 0 then
				Rich.Text = `<b>rich</b> <font size="{12 + Frame % 10}">text</font> {Frame}`
			end
		end)
	end

	-- 3. Sort order / LayoutOrder permutations and visibility toggles; Name sorting
	do
		local Root = Cell("Sorting")
		local List = New("UIListLayout", Root, { SortOrder = Enum.SortOrder.LayoutOrder, Padding = UDim.new(0, 2) })
		local Items = {}
		for Index = 1, 6 do
			table.insert(Items, Box(Root, `Item{Index}`, UDim2.new(0.2 * (Index % 3 + 1), 0, 0, 12 + Index * 2), nil, { LayoutOrder = Index }))
		end
		table.insert(Steps, function(Frame)
			if Frame % 5 == 0 then
				for Index, Item in Items do
					Item.LayoutOrder = (Index * 7 + Frame) % 6
				end
			end
			if Frame % 11 == 0 then
				Items[(Frame // 11) % 6 + 1].Visible = not Items[(Frame // 11) % 6 + 1].Visible
			end
			if Frame % 37 == 0 then
				List.SortOrder = if List.SortOrder == Enum.SortOrder.Name then Enum.SortOrder.LayoutOrder else Enum.SortOrder.Name
			end
			if Frame % 41 == 0 then
				Items[3].Name = `Item{Frame % 9}`
			end
		end)
	end

	-- 4. Grid: cell size changes, items created and destroyed
	do
		local Root = Cell("Grid")
		local Grid = New("UIGridLayout", Root, { CellSize = UDim2.fromOffset(30, 30), CellPadding = UDim2.fromOffset(4, 4) })
		local Items = {}
		for Index = 1, 5 do
			table.insert(Items, Box(Root, `G{Index}`, 10, 10))
		end
		table.insert(Steps, function(Frame)
			Grid.CellSize = UDim2.new(0, 24 + Frame % 12, 0.1, Frame % 5)
			if Frame % 9 == 0 then
				table.insert(Items, Box(Root, `G{Frame}`, 10, 10))
			end
			if Frame % 13 == 0 and #Items > 2 then
				table.remove(Items, 1):Destroy()
			end
			if Frame % 31 == 0 then
				Grid.FillDirection = if Grid.FillDirection == Enum.FillDirection.Horizontal then Enum.FillDirection.Vertical else Enum.FillDirection.Horizontal
			end
		end)
	end

	-- 5. ScrollingFrame: automatic canvas grows past the frame (bars appear), CanvasPosition moves
	do
		local Root = Cell("Scrolling")
		local Scroller = New("ScrollingFrame", Root, {
			Size = UDim2.new(1, -10, 1, -10),
			Position = UDim2.fromOffset(5, 5),
			CanvasSize = UDim2.new(),
			AutomaticCanvasSize = Enum.AutomaticSize.XY,
			ScrollBarThickness = 8,
			BackgroundColor3 = Color3.fromRGB(30, 30, 30),
		})
		New("UIListLayout", Scroller, { Padding = UDim.new(0, 3) })
		local Rows = {}
		for Index = 1, 3 do
			table.insert(Rows, Box(Scroller, `Row{Index}`, 150, 30))
		end
		table.insert(Steps, function(Frame)
			if Frame % 8 == 0 and #Rows < 12 then
				table.insert(Rows, Box(Scroller, `Row{Frame}`, 150 + (Frame % 5) * 20, 30))
			end
			Scroller.CanvasPosition = Vector2.new((Frame * 3) % 40, (Frame * 7) % 200)
			if Frame % 43 == 0 then
				Scroller.ScrollBarThickness = 4 + Frame % 10
			end
		end)
	end

	-- 6. UIScale animation on a container with a list inside; nested UIScale
	do
		local Root = Cell("Scaling")
		local Holder = Box(Root, "Holder", 150, 150, { Position = UDim2.fromScale(0.5, 0.5), AnchorPoint = Vector2.new(0.5, 0.5) })
		local Scale = New("UIScale", Holder, { Scale = 1 })
		New("UIListLayout", Holder, { Padding = UDim.new(0.02, 2), HorizontalAlignment = Enum.HorizontalAlignment.Center })
		local Child = Box(Holder, "Child", UDim2.new(0.8, 0, 0, 30), nil)
		local ChildScale = New("UIScale", Child, { Scale = 1 })
		Label(Holder, "Caption", "scaled caption", { Size = UDim2.new(1, 0, 0, 20) })
		table.insert(Steps, function(Frame)
			Scale.Scale = 0.6 + (Frame % 30) / 50
			ChildScale.Scale = 0.8 + (Frame % 7) / 20
		end)
	end

	-- 7. Rotation of a frame with rotated children; AnchorPoint / scale position animation
	do
		local Root = Cell("Rotation")
		local Spinner = Box(Root, "Spinner", 100, 60, { Position = UDim2.fromScale(0.5, 0.5), AnchorPoint = Vector2.new(0.5, 0.5) })
		local Inner = Box(Spinner, "Inner", 30, 20, { Position = UDim2.fromScale(1, 0), AnchorPoint = Vector2.new(1, 0), Rotation = 20 })
		Box(Inner, "Leaf", 10, 10, { Position = UDim2.fromScale(0.5, 0.5) })
		local Mover = Box(Root, "Mover", 20, 20)
		table.insert(Steps, function(Frame)
			Spinner.Rotation = (Frame * 6) % 360
			Inner.Rotation = -(Frame * 3) % 360
			Mover.Position = UDim2.new((Frame % 50) / 60, 0, 0.8, 0)
			Mover.AnchorPoint = Vector2.new((Frame % 3) / 2, 0.5)
		end)
	end

	-- 8. Constraints: aspect ratio, size constraint, text size constraint
	do
		local Root = Cell("Constraints")
		local Aspect = Box(Root, "Aspect", UDim2.fromScale(0.8, 0.5), nil)
		local Ratio = New("UIAspectRatioConstraint", Aspect, { AspectRatio = 1 })
		local Clamped = Box(Root, "Clamped", 10, 10, { Position = UDim2.fromOffset(0, 120) })
		local Constraint = New("UISizeConstraint", Clamped, { MinSize = Vector2.new(20, 20) })
		local Text = Label(Root, "Constrained", "Constrained", { TextScaled = true, Position = UDim2.fromOffset(100, 150), Size = UDim2.fromOffset(120, 50) })
		local TextConstraint = New("UITextSizeConstraint", Text, { MaxTextSize = 30 })
		table.insert(Steps, function(Frame)
			Ratio.AspectRatio = 0.5 + (Frame % 20) / 10
			Constraint.MinSize = Vector2.new(20 + Frame % 30, 20 + Frame % 17)
			TextConstraint.MaxTextSize = 10 + Frame % 25
			if Frame % 47 == 0 then
				Ratio.DominantAxis = if Ratio.DominantAxis == Enum.DominantAxis.Width then Enum.DominantAxis.Height else Enum.DominantAxis.Width
			end
		end)
	end

	-- 9. Flex: UIFlexItem modes and the list's flex settings change
	do
		local Root = Cell("Flex")
		local Row = Box(Root, "Row", UDim2.new(1, 0, 0, 60), nil)
		local List = New("UIListLayout", Row, { FillDirection = Enum.FillDirection.Horizontal, HorizontalFlex = Enum.UIFlexAlignment.Fill })
		local A = Box(Row, "A", 30, 40)
		local B = Box(Row, "B", 50, 20)
		local Flex = New("UIFlexItem", B, { FlexMode = Enum.UIFlexMode.Grow })
		Box(Row, "C", 20, 50)
		local Modes = { Enum.UIFlexMode.Grow, Enum.UIFlexMode.Shrink, Enum.UIFlexMode.Fill, Enum.UIFlexMode.None }
		local Aligns = { Enum.UIFlexAlignment.Fill, Enum.UIFlexAlignment.SpaceBetween, Enum.UIFlexAlignment.SpaceAround, Enum.UIFlexAlignment.None }
		table.insert(Steps, function(Frame)
			if Frame % 6 == 0 then
				Flex.FlexMode = Modes[(Frame // 6) % 4 + 1]
			end
			if Frame % 10 == 0 then
				List.HorizontalFlex = Aligns[(Frame // 10) % 4 + 1]
			end
			A.Size = UDim2.fromOffset(30 + Frame % 60, 40)
			Row.Size = UDim2.new(1, -(Frame % 50), 0, 60)
		end)
	end

	-- 10. Page layout (JumpToIndex) and table layout (cell sizes)
	do
		local Root = Cell("PagesTable")
		local Pages = Box(Root, "Pages", 200, 80, { ClipsDescendants = true })
		local PageLayout = New("UIPageLayout", Pages, { Animated = false, Padding = UDim.new(0, 5) })
		for Index = 1, 3 do
			Box(Pages, `Page{Index}`, UDim2.fromScale(1, 1), nil)
		end
		local Table = Box(Root, "Table", 200, 100, { Position = UDim2.fromOffset(0, 100) })
		New("UITableLayout", Table, { Padding = UDim2.fromOffset(2, 2) })
		local Cells = {}
		for RowIndex = 1, 2 do
			local Row = Box(Table, `Row{RowIndex}`, 10, 10)
			for Column = 1, 3 do
				table.insert(Cells, Box(Row, `C{RowIndex}{Column}`, 20 + Column * 5, 15 + RowIndex * 5))
			end
		end
		table.insert(Steps, function(Frame)
			if Frame % 12 == 0 then
				PageLayout:JumpToIndex((Frame // 12) % 3)
			end
			local Target = Cells[(Frame % #Cells) + 1]
			Target.Size = UDim2.fromOffset(15 + Frame % 25, 12 + Frame % 15)
		end)
	end

	-- 11. Reparenting between containers, Folders inside lists, layout objects added / removed
	do
		local Root = Cell("Reparent")
		local Left = Box(Root, "Left", 100, 200, { AutomaticSize = Enum.AutomaticSize.Y })
		New("UIListLayout", Left, { Padding = UDim.new(0, 2) })
		local Right = Box(Root, "Right", 100, 200, { Position = UDim2.fromOffset(110, 0) })
		local RightList = New("UIListLayout", Right, { Padding = UDim.new(0, 2) })
		local Folder = New("Folder", Left, { Name = "Folder" })
		Box(Folder, "InFolder", 80, 15)
		local Traveller = Box(Left, "Traveller", 60, 25)
		New("UIPadding", Traveller, { PaddingLeft = UDim.new(0, 4) })
		Box(Traveller, "Passenger", UDim2.fromScale(1, 0.5), nil)
		table.insert(Steps, function(Frame)
			if Frame % 10 == 0 then
				Traveller.Parent = if Traveller.Parent == Left then Right else Left
			end
			if Frame % 25 == 0 then
				if RightList.Parent then
					RightList.Parent = nil
				else
					RightList.Parent = Right
				end
			end
			if Frame % 15 == 0 then
				Box(Folder, `F{Frame}`, 70, 8)
			end
		end)
	end

	-- 12. Padding and list properties on an automatic container
	do
		local Root = Cell("Padding")
		local Container = Box(Root, "Container", 0, 0, { AutomaticSize = Enum.AutomaticSize.XY })
		local Padding = New("UIPadding", Container, {})
		local List = New("UIListLayout", Container, {})
		Box(Container, "P1", 40, 20)
		Box(Container, "P2", 20, 40)
		Label(Container, "P3", "pad", { AutomaticSize = Enum.AutomaticSize.XY, Size = UDim2.new() })
		table.insert(Steps, function(Frame)
			Padding.PaddingLeft = UDim.new(0, Frame % 20)
			Padding.PaddingTop = UDim.new(0.1, Frame % 7)
			List.Padding = UDim.new(0, Frame % 9)
			if Frame % 19 == 0 then
				List.FillDirection = if List.FillDirection == Enum.FillDirection.Vertical then Enum.FillDirection.Horizontal else Enum.FillDirection.Vertical
			end
			if Frame % 33 == 0 then
				Container.AutomaticSize = if Container.AutomaticSize == Enum.AutomaticSize.XY then Enum.AutomaticSize.Y else Enum.AutomaticSize.XY
			end
		end)
	end

	-- 13. Subtrees destroyed and rebuilt; TextBox text changes; wrapped truncated text
	do
		local Root = Cell("Rebuild")
		New("UIListLayout", Root, { Padding = UDim.new(0, 4) })
		local Built = nil
		local Input = New("TextBox", Root, { Size = UDim2.new(1, 0, 0, 24), Text = "", PlaceholderText = "type here", TextSize = 14, FontFace = Font.new("rbxasset://fonts/families/GothamSSm.json") })
		local Truncated = Label(Root, "Truncated", "a long line that gets truncated at the end", { Size = UDim2.fromOffset(120, 20), TextTruncate = Enum.TextTruncate.AtEnd })
		table.insert(Steps, function(Frame)
			if Frame % 14 == 0 then
				if Built then
					Built:Destroy()
				end
				Built = Box(Root, "Built", UDim2.new(1, 0, 0, 0), nil, { AutomaticSize = Enum.AutomaticSize.Y })
				New("UIListLayout", Built, {})
				for Index = 1, 1 + Frame % 4 do
					Label(Built, `L{Index}`, string.rep("x", Index * 3), { AutomaticSize = Enum.AutomaticSize.XY, Size = UDim2.new() })
				end
			end
			if Frame % 4 == 0 then
				Input.Text = string.sub("the quick brown fox jumps", 1, (Frame // 4) % 26)
			end
			Truncated.Size = UDim2.fromOffset(60 + Frame % 100, 20)
		end)
	end

	-- 14. Moving subtrees (the move fast path): a panel with text, a list, a scrolling frame, rotated
	-- and UIScale'd descendants slides around, toggles visibility and turns; a scrolled list's rows
	-- move with CanvasPosition
	do
		local Root = Cell("Moving")
		local Panel = Box(Root, "Panel", 150, 150, { AnchorPoint = Vector2.new(0.5, 0.5) })
		New("UIPadding", Panel, { PaddingLeft = UDim.new(0.05, 3), PaddingTop = UDim.new(0, 5) })
		local Header = Label(Panel, "Header", "Moving panel", { Size = UDim2.new(1, 0, 0, 20), TextXAlignment = Enum.TextXAlignment.Left })
		local Body = Box(Panel, "Body", UDim2.new(1, 0, 0, 0), nil, { Position = UDim2.fromOffset(0, 24), AutomaticSize = Enum.AutomaticSize.Y })
		New("UIListLayout", Body, { Padding = UDim.new(0, 2) })
		for Index = 1, 3 do
			Label(Body, `Line{Index}`, `line {Index} of the body`, { Size = UDim2.new(1, 0, 0, 0), AutomaticSize = Enum.AutomaticSize.Y, TextWrapped = true })
		end
		local Tilted = Box(Panel, "Tilted", 40, 20, { Position = UDim2.new(1, -10, 1, -10), AnchorPoint = Vector2.new(1, 1), Rotation = 15 })
		local TiltedChild = Box(Tilted, "TiltedChild", 12, 12, { Position = UDim2.fromScale(0.5, 0.5), Rotation = -30 })
		New("UIScale", TiltedChild, { Scale = 1.5 })
		local Scroller = New("ScrollingFrame", Panel, {
			Size = UDim2.fromOffset(60, 40),
			Position = UDim2.new(0, 0, 1, -45),
			CanvasSize = UDim2.fromOffset(0, 120),
			ScrollBarThickness = 5,
		})
		New("UIListLayout", Scroller, {})
		for Index = 1, 4 do
			Label(Scroller, `Row{Index}`, `row {Index}`, { Size = UDim2.new(1, 0, 0, 25) })
		end
		table.insert(Steps, function(Frame)
			Panel.Position = UDim2.new(0.5, (Frame * 3) % 40 - 20, 0.5, math.floor(math.sin(Frame / 5) * 20))
			if Frame % 17 == 0 then
				Panel.AnchorPoint = Vector2.new((Frame % 3) / 2, 0.5)
			end
			if Frame % 21 == 0 then
				Body.Visible = not Body.Visible
			end
			if Frame % 9 == 0 then
				Panel.Rotation = (Frame * 4) % 30
			end
			if Frame % 5 == 0 then
				Tilted.Position = UDim2.new(1, -10 - Frame % 20, 1, -10)
			end
			Scroller.CanvasPosition = Vector2.new(0, (Frame * 2) % 90)
			if Frame % 27 == 0 then
				Header.Text = `Moving panel {Frame}`
			end
		end)
	end

	local Frame = 0
	RunService.Heartbeat:Connect(function()
		Frame += 1
		for _, Step in Steps do
			Step(Frame)
		end
	end)
end
