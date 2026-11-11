-- Path2D curves: quadratic and cubic segments whose stroke edges show where Studio's straight pieces meet
-- (notes/path2d.md: a curve is cut at thirtieths), closed curves and curves that cross themselves.
--   node cli/rur.mjs compare --fixture fixtures/path2d-curves.luau --viewport 1200x700 --time 0.2 --background 0,0,0
return function(Target)
	local Point = Path2DControlPoint.new
	local Offset = UDim2.fromOffset
	local function Path(Thickness, Points, Closed)
		local P = Instance.new("Path2D")
		P.Color3 = Color3.new(1, 1, 1)
		P.Thickness = Thickness
		P.Closed = Closed == true
		local List = {}
		for Index, Entry in Points do
			List[Index] = Point(Offset(Entry[1], Entry[2]), Offset(Entry[3] or 0, Entry[4] or 0), Offset(Entry[5] or 0, Entry[6] or 0))
		end
		P:SetControlPoints(List)
		P.Parent = Target
	end
	-- (position x, y, left tangent x, y, right tangent x, y)
	-- a quarter arc of radius 300
	Path(4, { { 40, 340, 0, 0, 0, -166 }, { 340, 40, -166, 0, 0, 0 } })
	-- a quarter arc of radius 40
	Path(4, { { 380, 80, 0, 0, 0, -22 }, { 420, 40, -22, 0, 0, 0 } })
	-- an S with long handles
	Path(4, { { 460, 60, 0, 0, 300, 0 }, { 760, 300, -300, 0, 0, 0 } })
	-- a quadratic piece (one handle) and a long shallow cubic
	Path(4, { { 800, 320, 0, 0, 180, -280 }, { 1160, 320, 0, 0, 0, 0 } })
	Path(4, { { 40, 420, 0, 0, 300, 40 }, { 1160, 420, -300, -40, 0, 0 } })
	-- three points: two curved pieces of different length, then a tight loop
	Path(4, { { 40, 640, 0, 0, 60, -160 }, { 300, 560, -80, 60, 30, -20 }, { 380, 640, 0, -60, 0, 0 } })
	Path(4, { { 480, 660, 0, 0, 400, -260 }, { 620, 660, -400, -260, 0, 0 } })
	-- a thick tight arc: the outer bevels
	Path(60, { { 900, 660, 0, 0, 0, -44 }, { 980, 580, -44, 0, 0, 0 } })
	-- a closed blob (the last point curves back to the first), and a closed figure of eight that crosses itself
	Path(6, { { 1040, 520, -40, 30, 40, -30 }, { 1140, 560, 0, -50, 0, 50 }, { 1040, 620, 50, 20, -50, -20 } }, true)
	Path(14, { { 820, 60, -60, -60, 60, 60 }, { 1000, 60, -60, 60, 60, -60 }, { 1100, 120, 0, -40, 0, 40 } }, true)
	-- a hairpin: handles far longer than the segment
	Path(3, { { 700, 520, 0, 0, 380, -260 }, { 760, 520, -380, -260, 0, 0 } })
end
