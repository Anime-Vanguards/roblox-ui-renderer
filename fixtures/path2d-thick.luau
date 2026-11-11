-- Path2D at the ends of Thickness: the widest stroke (100, and what a larger value clamps to), its sharp corner
-- and tight arc, a stroke that crosses itself, and strokes thinner than a pixel.
--   node cli/rur.mjs compare --fixture fixtures/path2d-thick.luau --viewport 800x480 --time 0.2 --background 0,0,0
return function(Target)
	local Point = Path2DControlPoint.new
	local Offset = UDim2.fromOffset
	local function Path(Thickness, Points, Color, Closed)
		local P = Instance.new("Path2D")
		P.Color3 = Color or Color3.new(1, 1, 1)
		P.Thickness = Thickness
		P.Closed = Closed == true
		local List = {}
		for Index, Entry in Points do
			List[Index] = Point(UDim2.new(0, Entry[1], Entry[7] or 0, Entry[2]), Offset(Entry[3] or 0, Entry[4] or 0), Offset(Entry[5] or 0, Entry[6] or 0))
		end
		P:SetControlPoints(List)
		P.Parent = Target
	end
	-- (position x, y, left tangent x, y, right tangent x, y, position y scale)
	-- the widest line, and a wider value (it draws 100 wide)
	Path(100, { { 20, 70 }, { 200, 80 } })
	Path(500, { { 230, 70 }, { 300, 70 } })
	-- a sharp corner and a right angle at 100
	Path(100, { { 340, 60 }, { 520, 110 }, { 340, 160 } }, Color3.fromRGB(255, 200, 0))
	Path(100, { { 580, 70 }, { 720, 70 }, { 720, 200 } }, Color3.fromRGB(0, 200, 255))
	-- a tight arc at 100: the radius is less than half the width
	Path(100, { { 80, 330, 0, 0, 0, -30 }, { 130, 280, -30, 0, 0, 0 } }, Color3.fromRGB(255, 80, 80))
	-- a closed triangle at 60, and a 30 wide stroke that crosses itself
	Path(60, { { 250, 250 }, { 400, 250 }, { 325, 380 } }, Color3.fromRGB(120, 255, 120), true)
	Path(30, { { 460, 260 }, { 640, 260 }, { 640, 400 }, { 540, 400 }, { 540, 220 } })
	-- thinner than a pixel: 0.75, 0.5, 0.25, 0.1, level and sloped
	for Index, Thickness in { 0.75, 0.5, 0.25, 0.1 } do
		Path(Thickness, { { 680, 300 + Index * 12 }, { 780, 300 + Index * 12 } })
		Path(Thickness, { { 680, 370 + Index * 12 }, { 780, 380 + Index * 12 } })
	end
	-- 1 wide on and between pixel rows
	Path(1, { { 20, 440 }, { 200, 440 } })
	Path(1, { { 20, 0, 0, 0, 0, 0, 450.5 / 480 }, { 200, 0, 0, 0, 0, 0, 450.5 / 480 } })
	Path(1, { { 220, 440 }, { 400, 460 } })
end
