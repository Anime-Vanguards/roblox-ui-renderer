-- Path2D among its parent's children and under clips: draw order by child order and ZIndex, ClipsDescendants (plain,
-- rotated, an ancestor's), a CanvasGroup, a scrolled ScrollingFrame, hidden parents, a UIScale, padding and text.
-- (The scene keeps to what the renderer draws exactly as Studio 0.747 does, so that every pixel here is a path's
-- or its order's: a square UIStroke, a rounded fill without a stroke, and text of full blocks. notes/path2d.md.)
--   node cli/rur.mjs compare --fixture fixtures/path2d-order.luau --viewport 800x480 --time 0.2 --background 0,0,0
return function(Target)
	local Point = Path2DControlPoint.new
	local RED, BLUE, GREEN, WHITE = Color3.new(1, 0, 0), Color3.new(0, 0, 1), Color3.new(0, 1, 0), Color3.new(1, 1, 1)
	local function Box(Parent, X, Y, W, H, Color, Props)
		local Frame = Instance.new("Frame")
		Frame.BorderSizePixel = 0
		Frame.BackgroundColor3 = Color
		Frame.Position = UDim2.fromOffset(X, Y)
		Frame.Size = UDim2.fromOffset(W, H)
		for Key, Value in Props or {} do
			(Frame :: any)[Key] = Value
		end
		Frame.Parent = Parent
		return Frame
	end
	local function Path(Parent, Points, Thickness, Props)
		local P = Instance.new("Path2D")
		P.Color3 = WHITE
		P.Thickness = Thickness
		for Key, Value in Props or {} do
			(P :: any)[Key] = Value
		end
		local List = {}
		for Index, Position in Points do
			List[Index] = if typeof(Position) == "UDim2" then Point(Position) else Point(UDim2.fromOffset(Position[1], Position[2]))
		end
		P:SetControlPoints(List)
		P.Parent = Parent
		return P
	end
	local GRAY = Color3.fromRGB(60, 60, 60)
	local function Cell(Column, Row, Props)
		return Box(Target, 10 + Column * 198, 10 + Row * 118, 180, 100, GRAY, Props)
	end
	local ACROSS = { { 10, 50 }, { 170, 50 } }

	-- row 0: order among siblings
	do
		local Holder = Cell(0, 0)
		Box(Holder, 20, 20, 80, 60, RED)
		Path(Holder, ACROSS, 10)
		Box(Holder, 90, 30, 80, 60, BLUE)
	end
	do
		local Holder = Cell(1, 0)
		Path(Holder, ACROSS, 10)
		Box(Holder, 20, 20, 80, 60, RED)
		Box(Holder, 90, 30, 80, 60, BLUE)
	end
	do
		local Holder = Cell(2, 0)
		Box(Holder, 20, 20, 80, 60, RED, { ZIndex = 2 })
		Path(Holder, ACROSS, 10)
		Box(Holder, 90, 30, 80, 60, BLUE)
		Path(Holder, { { 10, 70 }, { 170, 70 } }, 10, { ZIndex = 3, Color3 = GREEN })
		Path(Holder, { { 10, 30 }, { 170, 30 } }, 10, { ZIndex = 0, Color3 = Color3.new(1, 1, 0) })
	end
	do
		local Holder = Cell(3, 0)
		local Red = Box(Holder, 20, 20, 140, 60, RED)
		Box(Red, 20, 10, 40, 40, GREEN, { ZIndex = 5 })
		Path(Holder, ACROSS, 10)
		Path(Red, { { 70, 10 }, { 70, 50 } }, 10, { Color3 = BLUE, ZIndex = 9 })
	end

	-- row 1: clips
	local OUT = { { -30, 50 }, { 210, 50 } }
	local DOWN = { { 90, 60 }, { 90, 112 } }
	do
		local Holder = Cell(0, 1, { ClipsDescendants = true })
		Path(Holder, OUT, 10)
		Path(Holder, DOWN, 10)
	end
	do
		local Holder = Cell(1, 1)
		Path(Holder, { { -8, 50 }, { 188, 50 } }, 10)
		Path(Holder, DOWN, 10)
	end
	do
		local Holder = Cell(2, 1, { ClipsDescendants = true, Rotation = 20, BackgroundTransparency = 1, Size = UDim2.fromOffset(140, 70), Position = UDim2.fromOffset(10 + 2 * 198 + 20, 10 + 118 + 15) })
		Path(Holder, { { -30, 35 }, { 170, 35 } }, 10)
		Path(Holder, { { 70, -30 }, { 70, 100 } }, 10, { Color3 = GREEN })
	end
	do
		local Outer = Cell(3, 1, { ClipsDescendants = true })
		local Inner = Box(Outer, 40, 20, 300, 60, RED)
		Path(Inner, { { -60, 30 }, { 280, 30 } }, 10)
		Path(Inner, { { 20, -40 }, { 20, 100 } }, 10, { Color3 = BLUE })
	end

	-- row 2: a CanvasGroup, its colour, a scrolled frame, hidden parents
	do
		local Group = Instance.new("CanvasGroup")
		Group.BorderSizePixel = 0
		Group.BackgroundColor3 = GRAY
		Group.Position = UDim2.fromOffset(10, 10 + 2 * 118)
		Group.Size = UDim2.fromOffset(180, 100)
		Group.GroupTransparency = 0.5
		Group.Parent = Target
		Box(Group, 20, 20, 80, 60, RED)
		Path(Group, OUT, 10)
		Path(Group, DOWN, 10)
	end
	do
		local Group = Instance.new("CanvasGroup")
		Group.BorderSizePixel = 0
		Group.BackgroundColor3 = GRAY
		Group.Position = UDim2.fromOffset(10 + 198, 10 + 2 * 118)
		Group.Size = UDim2.fromOffset(180, 100)
		Group.GroupColor3 = Color3.new(0.5, 1, 0.5)
		Group.Parent = Target
		Path(Group, ACROSS, 10)
		Box(Group, 90, 30, 80, 60, BLUE)
	end
	do
		local Scroll = Instance.new("ScrollingFrame")
		Scroll.BorderSizePixel = 0
		Scroll.BackgroundColor3 = GRAY
		Scroll.Position = UDim2.fromOffset(10 + 2 * 198, 10 + 2 * 118)
		Scroll.Size = UDim2.fromOffset(180, 100)
		Scroll.CanvasSize = UDim2.fromOffset(0, 300)
		Scroll.ScrollBarThickness = 0
		Scroll.Parent = Target
		Box(Scroll, 10, 80, 160, 10, RED)
		Path(Scroll, { { 10, 60 }, { 170, 60 } }, 6)
		Path(Scroll, { UDim2.new(0, 10, 0.5, 0), UDim2.new(1, -10, 0.5, 0) }, 6, { Color3 = GREEN })
		Path(Scroll, { { 20, 70 }, { 20, 140 } }, 6, { Color3 = BLUE })
		Scroll.CanvasPosition = Vector2.new(0, 40)
	end
	do
		local Holder = Cell(3, 2)
		local Hidden = Box(Holder, 0, 0, 180, 30, RED, { Visible = false })
		Path(Hidden, { { 10, 15 }, { 170, 15 } }, 10)
		Path(Holder, { { 10, 45 }, { 170, 45 } }, 10, { Visible = false })
		Path(Holder, { { 10, 60 }, { 170, 60 } }, 0)
		local Clear = Box(Holder, 0, 70, 180, 30, RED, { BackgroundTransparency = 1 })
		Path(Clear, { { 10, 15 }, { 170, 15 } }, 10)
	end

	-- row 3: a UIScale, padding and a stroke, a later sibling of the parent, text
	do
		local Holder = Cell(0, 3, { Size = UDim2.fromOffset(90, 50) })
		local Scale = Instance.new("UIScale")
		Scale.Scale = 2
		Scale.Parent = Holder
		-- (rounded corners do not cut the path that ends in them)
		local Corner = Instance.new("UICorner")
		Corner.CornerRadius = UDim.new(0, 15)
		Corner.Parent = Holder
		Path(Holder, { UDim2.new(0, 0, 0, 0), UDim2.new(1, 0, 1, 0) }, 4)
		Path(Holder, { { 10, 10 }, { 80, 10 } }, 4, { Color3 = GREEN })
	end
	do
		local Holder = Cell(1, 3)
		local Padding = Instance.new("UIPadding")
		Padding.PaddingLeft = UDim.new(0, 30)
		Padding.PaddingTop = UDim.new(0, 30)
		Padding.Parent = Holder
		local Stroke = Instance.new("UIStroke")
		Stroke.Thickness = 6
		Stroke.Color = RED
		Stroke.Parent = Holder
		Path(Holder, { UDim2.new(0, 0, 0, 0), UDim2.new(1, 0, 1, 0) }, 6)
		Path(Holder, { { -3, 50 }, { 60, 50 } }, 6, { Color3 = GREEN })
	end
	do
		local Holder = Cell(2, 3)
		Path(Holder, ACROSS, 10, { ZIndex = 10 })
		Box(Target, 10 + 2 * 198 + 60, 10 + 3 * 118 + 20, 60, 60, BLUE)
	end
	do
		local Label = Instance.new("TextLabel")
		Label.BorderSizePixel = 0
		Label.BackgroundColor3 = GRAY
		Label.Position = UDim2.fromOffset(10 + 3 * 198, 10 + 3 * 118)
		Label.Size = UDim2.fromOffset(180, 100)
		Label.Text = "\u{2588}\u{2588}\u{2588}\u{2588}"
		Label.TextSize = 40
		Label.TextColor3 = RED
		Label.Font = Enum.Font.SourceSans
		Label.Parent = Target
		Path(Label, ACROSS, 10)
	end
end
