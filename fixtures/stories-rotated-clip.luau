-- Repro: rotated descendants of a clipping (ClipsDescendants = true), unrotated frame,
-- the structure of a level-up screen story: its
-- DesignCanvas clips and holds stripes rotated -12.8 degrees that Studio draws past the canvas.
--   node cli/rur.mjs compare --fixture fixtures/stories-rotated-clip.luau --viewport 1100x640 --time 0.3 --background 20,20,26
return function(Target)
	local function Box(Name, Parent, X, Y, W, H, Color, Rotation)
		local Box = Instance.new("Frame")
		Box.Name = Name
		Box.Position = UDim2.fromOffset(X, Y)
		Box.Size = UDim2.fromOffset(W, H)
		Box.BackgroundColor3 = Color
		Box.BorderSizePixel = 0
		Box.Rotation = Rotation or 0
		Box.Parent = Parent
		return Box
	end

	local function Clipper(Name, X, Y)
		local Frame = Box(Name, Target, X, Y, 200, 150, Color3.fromRGB(50, 50, 60))
		Frame.ClipsDescendants = true
		return Frame
	end

	-- A: plain child and rotated child overflowing a clipping frame
	local A = Clipper("A", 40, 60)
	Box("Plain", A, 120, 100, 120, 40, Color3.fromRGB(90, 160, 230))
	Box("Rotated", A, -40, 20, 200, 30, Color3.fromRGB(230, 120, 90), -15)

	-- B: rotated child holding an unrotated grandchild that overflows the rotated child and the clipper
	local B = Clipper("B", 300, 60)
	local Arm = Box("Arm", B, 20, 40, 160, 60, Color3.fromRGB(120, 220, 120), 20)
	Box("Hand", Arm, 120, 30, 90, 50, Color3.fromRGB(220, 200, 90))

	-- C: the rotated child sits in an unrotated, non-clipping frame inside the clipper
	local C = Clipper("C", 560, 60)
	local Middle = Box("Middle", C, 10, 10, 180, 130, Color3.fromRGB(70, 70, 85))
	Box("Rotated", Middle, -30, 90, 250, 30, Color3.fromRGB(200, 110, 220), 10)

	-- D: two clipping levels, rotated grandchild of the inner one
	local D = Clipper("D", 820, 60)
	local Inner = Box("Inner", D, 30, 30, 140, 90, Color3.fromRGB(70, 70, 85))
	Inner.ClipsDescendants = true
	Box("Rotated", Inner, -20, 40, 200, 24, Color3.fromRGB(90, 200, 200), -8)

	-- E: a rotated clipping frame (clipping is off under rotation) with an overflowing child
	local E = Box("E", Target, 60, 330, 200, 150, Color3.fromRGB(50, 50, 60), 5)
	E.ClipsDescendants = true
	Box("Plain", E, 120, 100, 120, 40, Color3.fromRGB(90, 160, 230))

	-- F: rotated child of a clipping ScrollingFrame
	local F = Instance.new("ScrollingFrame")
	F.Name = "F"
	F.Position = UDim2.fromOffset(330, 330)
	F.Size = UDim2.fromOffset(200, 150)
	F.BackgroundColor3 = Color3.fromRGB(50, 50, 60)
	F.BorderSizePixel = 0
	F.CanvasSize = UDim2.fromOffset(0, 300)
	F.ScrollBarThickness = 6
	F.Parent = Target
	Box("Rotated", F, -40, 110, 260, 30, Color3.fromRGB(230, 120, 90), 12)

	-- G: rotated child of a clipping CanvasGroup
	local G = Instance.new("CanvasGroup")
	G.Name = "G"
	G.Position = UDim2.fromOffset(600, 330)
	G.Size = UDim2.fromOffset(200, 150)
	G.BackgroundColor3 = Color3.fromRGB(50, 50, 60)
	G.BorderSizePixel = 0
	G.Parent = Target
	Box("Rotated", G, -40, 110, 260, 30, Color3.fromRGB(120, 220, 120), 12)

	-- H: rotated child with UICorner in a clipping frame
	local H = Clipper("H", 860, 330)
	local Rounded = Box("Rounded", H, 60, -30, 110, 220, Color3.fromRGB(220, 200, 90), 30)
	local Corner = Instance.new("UICorner")
	Corner.CornerRadius = UDim.new(0, 16)
	Corner.Parent = Rounded
end
