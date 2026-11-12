-- Path2D strokes: straight segments by thickness and direction, corners between straight segments,
-- end caps, a zero-length segment and a single point.
--   node cli/rur.mjs compare --fixture fixtures/path2d-strokes.luau --viewport 800x480 --time 0.2 --background 20,20,26
return function(Target)
	local Point = Path2DControlPoint.new
	local function Path(Points, Thickness, Props)
		local P = Instance.new("Path2D")
		P.Color3 = Color3.new(1, 1, 1)
		P.Thickness = Thickness
		for Key, Value in Props or {} do
			(P :: any)[Key] = Value
		end
		local List = {}
		for Index, Position in Points do
			List[Index] = Point(UDim2.fromOffset(Position[1], Position[2]))
		end
		P:SetControlPoints(List)
		P.Parent = Target
		return P
	end

	local Thicknesses = { 1, 2, 3, 4, 5, 6, 8, 10, 14, 20 }
	for Index, Thickness in Thicknesses do
		local X = 20 + (Index - 1) * 78
		-- horizontal, vertical, 45 degrees, a shallow slope
		Path({ { X, 24 }, { X + 56, 24 } }, Thickness)
		Path({ { X + 28, 44 }, { X + 28, 100 } }, Thickness)
		Path({ { X, 116 }, { X + 56, 172 } }, Thickness)
		Path({ { X, 190 }, { X + 56, 203 } }, Thickness)
	end

	-- corners: a right angle, an obtuse and two sharp ones, thin to thick
	for Index, Thickness in { 1, 3, 6, 12, 20 } do
		local X = 20 + (Index - 1) * 156
		Path({ { X, 236 }, { X + 40, 236 }, { X + 40, 276 } }, Thickness)
		Path({ { X + 60, 276 }, { X + 90, 236 }, { X + 130, 250 } }, Thickness)
		Path({ { X, 300 }, { X + 60, 316 }, { X, 332 } }, Thickness)
		Path({ { X + 80, 300 }, { X + 130, 306 }, { X + 80, 312 } }, Thickness)
	end

	-- closed shapes
	for Index, Thickness in { 1, 4, 10, 20 } do
		local X = 20 + (Index - 1) * 120
		-- (one vertex of the thin triangle's last corner is within a hundredth of a 1/256 pixel step of a rounding
		-- edge, and Studio's GPU puts it on the other side: one pixel there is 3 levels off. notes/path2d.md
		-- "Vertex snap")
		Path({ { X, 360 }, { X + 70, 360 }, { X + 35, 420 } }, Thickness, { Closed = true })
	end

	-- a zero-length segment, a single point (no segment), two points closed
	for Index, Thickness in { 2, 10, 20 } do
		local X = 520 + (Index - 1) * 50
		Path({ { X, 370 }, { X, 370 } }, Thickness)
		Path({ { X, 410 } }, Thickness)
		Path({ { X, 440 }, { X + 24, 440 } }, Thickness, { Closed = true })
	end
	Path({ { 690, 370 }, { 690, 370 } }, 10, { Closed = true })
end
