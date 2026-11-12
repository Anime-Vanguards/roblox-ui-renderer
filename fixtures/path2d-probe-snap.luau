-- Where a Path2D vertex rounds to the 1/256 pixel grid (notes/path2d.md "Vertex snap"): level lines whose edge
-- vertices sweep across a rounding edge in steps of two thousandths of a grid step, at several heights and pixel
-- phases, and upright lines doing the same across, near the left edge. One grid step moves a ramp pixel of a
-- thickness 2 line by most of a level, so each line shows which way each of its two edges went.
-- rur compare --fixture fixtures/path2d-probe-snap.luau --viewport 800x480 --time 0.2 --background 20,20,26
return function(Target)
	local function Line(Name, From, To)
		local P = Instance.new("Path2D")
		P.Name = Name
		P.Color3 = Color3.new(1, 1, 1)
		P.Thickness = 2
		P.Parent = Target
		-- (offsets are whole pixels, so the points are scales of the 800 x 480 target)
		P:SetControlPoints({
			Path2DControlPoint.new(UDim2.fromScale(From.X / 800, From.Y / 480)),
			Path2DControlPoint.new(UDim2.fromScale(To.X / 800, To.Y / 480)),
		})
	end
	-- (a vertex is at 1.5 x the point, 2.25 px to each side: 384 grid steps to the GUI pixel)
	local function Swept(Base, Index, Count)
		local Edge = math.floor(Base * 384) + 0.5
		return (Edge + (Index / (Count - 1) - 0.4) * 0.08) / 384
	end
	for Band, Y in { 8, 20, 20.3, 34.6, 50, 80.45, 100, 130.7, 170, 200.2, 250, 290.55, 330, 380.3, 420, 466 } do
		for Index = 0, 39 do
			local X = 60 + Index * 18
			local At = Swept(Y, Index, 40)
			Line(`Level{Band}_{Index}`, Vector2.new(X, At), Vector2.new(X + 14, At))
		end
	end
	for Column, X in { 6, 15.3, 24, 34.6, 46 } do
		for Index = 0, 41 do
			local Y = 10 + Index * 11
			local At = Swept(X, Index, 42)
			Line(`Upright{Column}_{Index}`, Vector2.new(At, Y), Vector2.new(At, Y + 8))
		end
	end
end
