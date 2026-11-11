-- Path2D against its parent: scale and offset positions, handles as sizes, a turned parent and a parent turned
-- inside it, a UIScale, a parent of negative size, a parent with no height, and a path of the story's own frame.
--   node cli/rur.mjs compare --fixture fixtures/path2d-layout.luau --viewport 800x480 --time 0.2 --background 0,0,0
return function(Target)
	local Point = Path2DControlPoint.new
	local Offset = UDim2.fromOffset
	local GRAY = Color3.fromRGB(60, 60, 60)
	local function Box(Parent, X, Y, W, H, Props)
		local Frame = Instance.new("Frame")
		Frame.BorderSizePixel = 0
		Frame.BackgroundColor3 = GRAY
		Frame.Position = Offset(X, Y)
		Frame.Size = Offset(W, H)
		for Key, Value in Props or {} do
			(Frame :: any)[Key] = Value
		end
		Frame.Parent = Parent
		return Frame
	end
	local function Path(Parent, Points, Thickness, Color, Closed)
		local P = Instance.new("Path2D")
		P.Color3 = Color or Color3.new(1, 1, 1)
		P.Thickness = Thickness
		P.Closed = Closed == true
		P:SetControlPoints(Points)
		P.Parent = Parent
		return P
	end
	local GREEN, ORANGE = Color3.fromRGB(120, 255, 120), Color3.fromRGB(255, 180, 0)

	-- scale and offset against the parent; the same points closed
	local Scaled = { Point(UDim2.new(0, 0, 0, 0)), Point(UDim2.new(0.5, 10, 1, -20)), Point(UDim2.new(1, 0, 0, 0)) }
	Path(Box(Target, 20, 20, 200, 100), Scaled, 3)
	Path(Box(Target, 240, 20, 200, 100), Scaled, 3, GREEN, true)
	-- handles resolve as sizes: scale against the parent's size, plus the offset
	do
		local Holder = Box(Target, 460, 20, 200, 100)
		Path(Holder, { Point(Offset(0, 0), Offset(-30, 0), Offset(0, 60)), Point(Offset(100, 50), UDim2.new(-0.25, 0, 0, 0), UDim2.new(0.1, 5, 0.5, 0)), Point(Offset(180, 10), Offset(0, 40), Offset(20, -20)) }, 3)
		-- a handle whose scale and offset cancel is none: this one is a straight line
		Path(Holder, { Point(Offset(0, 90), Offset(0, 0), UDim2.new(0.1, -20, 0, 0)), Point(Offset(100, 90)) }, 3, ORANGE)
	end
	-- the story's own frame holds a path
	Path(Target, { Point(Offset(680, 20), Offset(0, 0), Offset(60, 0)), Point(UDim2.new(1, -20, 0, 120), Offset(0, -60), Offset(0, 0)) }, 5, ORANGE)

	-- a turned parent, and a parent turned inside it (they draw nothing: the renderer's edge of a turned fill
	-- differs from Studio's by a few levels in a few pixels, and this scene is about the paths)
	do
		local Turned = Box(Target, 60, 190, 160, 90, { Rotation = 30, BackgroundTransparency = 1 })
		Path(Turned, { Point(Offset(0, 0), Offset(0, 0), Offset(60, 0)), Point(UDim2.new(1, 0, 1, 0)) }, 4)
		local Inner = Box(Turned, 90, 10, 60, 40, { Rotation = 45, BackgroundTransparency = 1 })
		Path(Inner, { Point(Offset(0, 0)), Point(UDim2.new(1, 0, 1, 0)), Point(UDim2.new(0, 0, 1, 0)) }, 4, GREEN, true)
	end
	-- a UIScale scales the parent, not the path's offsets or its thickness
	do
		local Holder = Box(Target, 280, 170, 90, 60)
		local Scale = Instance.new("UIScale")
		Scale.Scale = 2
		Scale.Parent = Holder
		Path(Holder, { Point(Offset(0, 0), Offset(0, 0), Offset(40, 0)), Point(UDim2.new(1, 0, 1, 0)) }, 4)
		local Inner = Box(Holder, 20, 30, 20, 15, { BackgroundColor3 = Color3.fromRGB(100, 60, 60) })
		Path(Inner, { Point(Offset(0, 0)), Point(UDim2.new(1, 0, 1, 0)) }, 4, GREEN)
	end
	-- a parent of negative size is the same rectangle from its least corner; a parent with no height
	do
		local Negative = Box(Target, 600, 180, -100, 100)
		Path(Negative, { Point(UDim2.new(0, 0, 0, 0)), Point(UDim2.new(0.5, 10, 0.5, 20)), Point(UDim2.new(1, 0, 1, 0)) }, 3)
		local Flat = Box(Target, 620, 180, 160, 0)
		Path(Flat, { Point(UDim2.new(0, 0, 0.5, 20)), Point(UDim2.new(1, 0, 1, 80)) }, 3, GREEN)
	end
	-- a path follows its parent in a list, and a parent sized by its content
	do
		local List = Box(Target, 20, 330, 420, 130, { BackgroundTransparency = 1 })
		local Layout = Instance.new("UIListLayout")
		Layout.FillDirection = Enum.FillDirection.Horizontal
		Layout.Padding = UDim.new(0, 10)
		Layout.Parent = List
		for Index = 1, 3 do
			local Item = Box(List, 0, 0, 80 + Index * 20, 60 + Index * 20)
			Path(Item, { Point(UDim2.new(0, 4, 1, -4)), Point(UDim2.new(0.5, 0, 0, 4), UDim2.new(-0.3, 0, 0, 0), UDim2.new(0.3, 0, 0, 0)), Point(UDim2.new(1, -4, 1, -4)) }, Index * 2, ORANGE)
		end
	end
end
