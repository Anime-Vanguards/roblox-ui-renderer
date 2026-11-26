-- AutomaticSize objects with no clipping ancestor whose centre is past a screen edge (the window is the screen): frames
-- (AutomaticSize X / Y / XY, holding a block), TextLabels with AutomaticSize, and controls whose centre is just inside
-- or that are not automatic, along each of the four edges. "hold" frames are transparent and hold a child lying wholly
-- on the screen. Which of them does a client draw?
--   node cli/rur.mjs compare --fixture fixtures/layout-autosize-offscreen.luau --viewport 1200x700 --time 0.2 --background 20,20,26
return function(Target)
	local W, H = 1200, 700
	local Face = Font.new("rbxasset://fonts/families/SourceSansPro.json", Enum.FontWeight.Bold)
	local function Block(Parent, Size, Color)
		local B = Instance.new("Frame")
		B.Name = "B"
		B.Size = Size
		B.BackgroundColor3 = Color
		B.BorderSizePixel = 0
		B.Parent = Parent
		return B
	end
	local function Auto(Name, Axis, Position, Hold)
		local F = Instance.new("Frame")
		F.Name = Name
		F.AutomaticSize = Enum.AutomaticSize[Axis]
		F.Size = if Axis == "X" then UDim2.fromOffset(0, 40) elseif Axis == "Y" then UDim2.fromOffset(40, 0) else UDim2.fromOffset(0, 0)
		F.Position = Position
		F.BackgroundColor3 = Color3.fromRGB(200, 60, 60)
		F.BackgroundTransparency = if Hold then 1 else 0
		F.BorderSizePixel = 0
		F.Parent = Target
		local Inner = Block(F, UDim2.fromOffset(100, 100), Color3.fromRGB(90, 200, 120))
		Inner.BackgroundTransparency = if Hold then 1 else 0.3
		if Hold then
			-- a child wholly on the screen, positioned inside the frame's on-screen part
			local C = Instance.new("Frame")
			C.Name = "C"
			C.Size = UDim2.fromOffset(20, 20)
			C.BackgroundColor3 = Color3.fromRGB(240, 220, 80)
			C.BorderSizePixel = 0
			C.Position = Hold
			C.Parent = F
		end
		return F
	end
	local function Plain(Name, Position)
		local F = Instance.new("Frame")
		F.Name = Name
		F.Size = UDim2.fromOffset(100, 100)
		F.Position = Position
		F.BackgroundColor3 = Color3.fromRGB(80, 120, 220)
		F.BorderSizePixel = 0
		F.Parent = Target
	end
	local function Label(Name, Axis, Position)
		local L = Instance.new("TextLabel")
		L.Name = Name
		L.AutomaticSize = Enum.AutomaticSize[Axis]
		L.Size = if Axis == "X" then UDim2.fromOffset(0, 40) elseif Axis == "Y" then UDim2.fromOffset(100, 0) else UDim2.fromOffset(0, 0)
		L.Position = Position
		L.FontFace = Face
		L.TextSize = 32
		L.Text = "Edgewise"
		L.TextColor3 = Color3.new(1, 1, 1)
		L.BackgroundColor3 = Color3.fromRGB(120, 60, 160)
		L.BorderSizePixel = 0
		L.Parent = Target
	end
	-- left edge (frames 100 wide: x = -55 puts the centre at -5, x = -45 at +5)
	Auto("left_x_out", "X", UDim2.fromOffset(-55, 20))
	Auto("left_x_in", "X", UDim2.fromOffset(-45, 130))
	Auto("left_xy_out", "XY", UDim2.fromOffset(-55, 240))
	Auto("left_y_out", "Y", UDim2.fromOffset(-25, 350)) -- 40 wide, centre -5
	Auto("left_hold_out", "XY", UDim2.fromOffset(-60, 460), UDim2.fromOffset(70, 40))
	Plain("left_plain_out", UDim2.fromOffset(-55, 570))
	Label("left_label_out", "XY", UDim2.fromOffset(-70, 640))
	-- right edge
	Auto("right_x_out", "X", UDim2.fromOffset(W - 45, 20))
	Auto("right_x_in", "X", UDim2.fromOffset(W - 55, 130))
	Auto("right_xy_out", "XY", UDim2.fromOffset(W - 45, 240))
	Auto("right_hold_out", "XY", UDim2.fromOffset(W - 40, 460), UDim2.fromOffset(5, 40))
	Plain("right_plain_out", UDim2.fromOffset(W - 45, 570))
	Label("right_label_out", "XY", UDim2.fromOffset(W - 50, 640))
	-- top edge (y = -55 / -45)
	Auto("top_y_out", "Y", UDim2.fromOffset(200, -55))
	Auto("top_xy_out", "XY", UDim2.fromOffset(320, -55))
	Auto("top_xy_in", "XY", UDim2.fromOffset(440, -45))
	Auto("top_hold_out", "XY", UDim2.fromOffset(560, -60), UDim2.fromOffset(40, 70))
	Plain("top_plain_out", UDim2.fromOffset(680, -55))
	Label("top_label_out", "XY", UDim2.fromOffset(800, -22))
	-- bottom edge
	Auto("bottom_y_out", "Y", UDim2.fromOffset(200, H - 45))
	Auto("bottom_xy_out", "XY", UDim2.fromOffset(320, H - 45))
	Auto("bottom_xy_in", "XY", UDim2.fromOffset(440, H - 55))
	Auto("bottom_hold_out", "XY", UDim2.fromOffset(560, H - 40), UDim2.fromOffset(40, 5))
	Plain("bottom_plain_out", UDim2.fromOffset(680, H - 45))
	Label("bottom_label_out", "XY", UDim2.fromOffset(800, H - 10))
	-- a corner
	Auto("corner_xy_out", "XY", UDim2.fromOffset(W - 45, H - 45))
end
