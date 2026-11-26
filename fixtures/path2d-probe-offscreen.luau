-- Path2Ds whose end points lie outside the screen while their handles carry the stroke into it: a client draws
-- the part on screen although GetBoundingRect (the end points' box) is wholly outside. Ends below, right of, above
-- and left of a 640 x 360 viewport, a quadratic piece, a closed path and a path wholly off screen.
-- compare: --viewport 640x360 --time 0.2 --background 20,20,26
return function(Target)
	local Point = Path2DControlPoint.new
	local Offset = UDim2.fromOffset
	local function Path(Thickness, Points, Closed)
		local P = Instance.new("Path2D")
		P.Color3 = Color3.fromRGB(240, 240, 255)
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
	-- ends below the screen, a loop rising to about y 230
	Path(4, { { 60, 380, 0, 0, 300, -200 }, { 200, 380, -300, -200, 0, 0 } })
	-- ends right of the screen, a bulge reaching x 520
	Path(6, { { 680, 60, 0, 0, -200, 40 }, { 680, 180, -200, -40, 0, 0 } })
	-- ends above the screen, a dip to about y 120
	Path(3, { { 260, -20, 0, 0, 80, 180 }, { 420, -20, -80, 180, 0, 0 } })
	-- ends left of the screen, a bulge reaching x 110
	Path(10, { { -30, 220, 0, 0, 180, 30 }, { -30, 330, 180, -30, 0, 0 } })
	-- a quadratic piece (one handle) from below the screen to below it
	Path(5, { { 300, 400, 0, 0, 90, -260 }, { 480, 400, 0, 0, 0, 0 } })
	-- a closed path whose three points are below the screen
	Path(4, { { 500, 370, -40, -120, 40, -120 }, { 620, 370, 0, -100, 0, 100 }, { 560, 420, 60, 0, -60, 0 } }, true)
	-- wholly below the screen: draws nothing
	Path(4, { { 100, 420, 0, 0, 60, -20 }, { 300, 420, -60, -20, 0, 0 } })
end
