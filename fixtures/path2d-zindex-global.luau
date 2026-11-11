-- (ZIndexBehavior Global) Path2D draw order by ZIndex: paths of one ZIndex among themselves, against deeper descendants, and under a rotated
-- clipping ancestor. path2d-zindex.luau is the same scene in a ScreenGui of ZIndexBehavior Global.
--   node cli/rur.mjs compare --fixture fixtures/path2d-zindex-global.luau --viewport 800x250 --time 0.2 --background 0,0,0
return function(Target)
	(Target.Parent :: ScreenGui).ZIndexBehavior = Enum.ZIndexBehavior.Global
	local Point = Path2DControlPoint.new
	local RED, BLUE, GREEN, WHITE, YELLOW = Color3.new(1, 0, 0), Color3.new(0, 0, 1), Color3.new(0, 1, 0), Color3.new(1, 1, 1), Color3.new(1, 1, 0)
	local GRAY = Color3.fromRGB(60, 60, 60)
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
	local function Path(Parent, Points, Color, Props)
		local P = Instance.new("Path2D")
		P.Color3 = Color
		P.Thickness = 10
		for Key, Value in Props or {} do
			(P :: any)[Key] = Value
		end
		local List = {}
		for Index, Position in Points do
			List[Index] = Point(UDim2.fromOffset(Position[1], Position[2]))
		end
		P:SetControlPoints(List)
		P.Parent = Parent
		return P
	end
	local function Cell(Column, Row, Props)
		return Box(Target, 10 + Column * 198, 10 + Row * 118, 180, 100, GRAY, Props)
	end
	local ACROSS = { { 10, 50 }, { 170, 50 } }
	local DOWN = { { 60, 10 }, { 60, 90 } }
	local DOWN2 = { { 120, 10 }, { 120, 90 } }

	-- paths of one ZIndex: the later one is on top? and one of a lower ZIndex made last
	do
		local Holder = Cell(0, 0)
		Path(Holder, ACROSS, WHITE)
		Path(Holder, DOWN, GREEN)
		Path(Holder, DOWN2, YELLOW, { ZIndex = 0 })
	end
	-- frames and paths of mixed ZIndex
	do
		local Holder = Cell(1, 0)
		Box(Holder, 20, 20, 60, 60, RED, { ZIndex = 2 })
		Path(Holder, ACROSS, WHITE, { ZIndex = 2 })
		Box(Holder, 100, 20, 60, 60, BLUE)
		Path(Holder, { { 10, 30 }, { 170, 30 } }, GREEN, { ZIndex = 3 })
		Path(Holder, { { 10, 70 }, { 170, 70 } }, YELLOW)
	end
	-- against deeper descendants: a child's child of ZIndex 5, a path of the child of ZIndex 9, paths of the holder of 3 and 6
	do
		local Holder = Cell(2, 0)
		local Red = Box(Holder, 20, 10, 140, 80, RED)
		Box(Red, 20, 10, 100, 60, GREEN, { ZIndex = 5 })
		Path(Holder, { { 10, 35 }, { 170, 35 } }, WHITE, { ZIndex = 3 })
		Path(Holder, { { 10, 65 }, { 170, 65 } }, YELLOW, { ZIndex = 6 })
		Path(Red, { { 70, 5 }, { 70, 75 } }, BLUE, { ZIndex = 9 })
		Path(Red, { { 40, 5 }, { 40, 75 } }, Color3.new(0, 1, 1), { ZIndex = 4 })
	end
	-- the holder's own ZIndex against a sibling's path
	do
		local Holder = Cell(3, 0, { ZIndex = 4 })
		Path(Holder, ACROSS, WHITE, { ZIndex = -5 })
		local Sibling = Box(Target, 10 + 3 * 198 + 40, 30, 60, 60, BLUE, { ZIndex = 2, BackgroundTransparency = 1 })
		Path(Sibling, { { -30, 50 }, { 90, 50 } }, GREEN, { ZIndex = 50 })
		Box(Target, 10 + 3 * 198 + 110, 30, 40, 60, RED, { ZIndex = 3 })
	end

	-- row 1: a rotated clipping grandparent, a clipping parent under a rotated grandparent, a rotated path parent in a clip.
	-- (The turned frames draw nothing: the renderer's edge of a turned fill differs from Studio's by a few levels in
	-- a few pixels, and this scene is about the paths.)
	do
		local Outer = Cell(0, 1, { ClipsDescendants = true, Rotation = 15, BackgroundTransparency = 1, Size = UDim2.fromOffset(150, 80), Position = UDim2.fromOffset(25, 10 + 118 + 10) })
		local Inner = Box(Outer, 20, 10, 110, 60, RED, { BackgroundTransparency = 1 })
		Path(Inner, { { -50, 30 }, { 160, 30 } }, WHITE)
	end
	do
		local Outer = Cell(1, 1, { Rotation = 15, BackgroundTransparency = 1, Size = UDim2.fromOffset(150, 80), Position = UDim2.fromOffset(10 + 198 + 15, 10 + 118 + 10) })
		local Inner = Box(Outer, 20, 10, 110, 60, RED, { ClipsDescendants = true, BackgroundTransparency = 1 })
		Path(Inner, { { -50, 30 }, { 160, 30 } }, WHITE)
		Path(Inner, { { 50, -20 }, { 50, 80 } }, BLUE)
	end
	do
		local Outer = Cell(2, 1, { ClipsDescendants = true })
		local Inner = Box(Outer, 40, 20, 100, 60, RED, { Rotation = 30, BackgroundTransparency = 1 })
		Path(Inner, { { -80, 30 }, { 180, 30 } }, WHITE)
		Path(Inner, { { 50, -60 }, { 50, 120 } }, BLUE)
	end
	-- a path whose parent is the ScreenGui's frame itself, over and under frames
	do
		Path(Target, { { 10 + 3 * 198, 10 + 118 + 50 }, { 10 + 3 * 198 + 180, 10 + 118 + 50 } }, WHITE, { ZIndex = 2 })
		Box(Target, 10 + 3 * 198 + 20, 10 + 118 + 20, 50, 60, RED)
		Box(Target, 10 + 3 * 198 + 100, 10 + 118 + 20, 50, 60, BLUE, { ZIndex = 2 })
	end
end
