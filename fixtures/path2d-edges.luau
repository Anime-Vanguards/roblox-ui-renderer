-- Path2D edge profile: nearly horizontal and nearly vertical strokes whose edges sweep through every
-- sub-pixel phase, by thickness (notes/path2d.md: the alpha ramp across a stroke).
--   node cli/rur.mjs compare --fixture fixtures/path2d-edges.luau --viewport 800x480 --time 0.2 --background 0,0,0
return function(Target)
	local Point = Path2DControlPoint.new
	local function Line(X0, Y0, X1, Y1, Thickness)
		local P = Instance.new("Path2D")
		P.Color3 = Color3.new(1, 1, 1)
		P.Thickness = Thickness
		P:SetControlPoints({ Point(UDim2.fromOffset(X0, Y0)), Point(UDim2.fromOffset(X1, Y1)) })
		P.Parent = Target
	end
	local Thicknesses = { 1, 2, 3, 4, 5, 6, 8, 10, 14, 20, 0.5, 1.5, 2.5 }
	for Index, Thickness in Thicknesses do
		-- 600 px long with a rise of 3: the edge moves 0.005 px per px
		Line(20, 14 + (Index - 1) * 34, 620, 17 + (Index - 1) * 34, Thickness)
	end
	for Index, Thickness in Thicknesses do
		Line(640 + (Index - 1) * 12, 20, 642 + (Index - 1) * 12, 460, math.min(Thickness, 6))
	end
end
