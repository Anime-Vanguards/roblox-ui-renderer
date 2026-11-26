-- Path2Ds longer than a texture is wide at 3x: a straight path 3,000 logical px long (9,003 device px with its
-- strip at 3x), one 6,000 long, a tall one, a diagonal and a wide curve, each crossing the whole 1,200 x 700
-- viewport with its ends far off the screen. A client draws the part on the screen.
-- compare: --viewport 1200x700 --time 0.2 --background 20,20,26
return function(Target)
	local Point = Path2DControlPoint.new
	local Offset = UDim2.fromOffset
	local function Path(Thickness, Color, Points)
		local P = Instance.new("Path2D")
		P.Color3 = Color
		P.Thickness = Thickness
		local List = {}
		for Index, Entry in Points do
			List[Index] = Point(Offset(Entry[1], Entry[2]), Offset(Entry[3] or 0, Entry[4] or 0), Offset(Entry[5] or 0, Entry[6] or 0))
		end
		P:SetControlPoints(List)
		P.Parent = Target
	end
	-- 3,000 px straight, ends 900 px left and right of the screen
	Path(4, Color3.fromRGB(240, 240, 255), { { -900, 120 }, { 2100, 120 } })
	-- 6,000 px straight
	Path(10, Color3.fromRGB(255, 120, 60), { { -2400, 260 }, { 3600, 260 } })
	-- 3,000 px tall
	Path(6, Color3.fromRGB(90, 220, 120), { { 300, -1150 }, { 300, 1850 } })
	-- a diagonal from far above left to far below right
	Path(3, Color3.fromRGB(120, 160, 255), { { -1500, -900 }, { 2700, 1600 } })
	-- a wide curve dipping into the screen
	Path(5, Color3.fromRGB(255, 220, 80), { { -1600, 700 }, { 2800, 700, -800, -900 } })
	-- a closed loop around the screen, its corners off it
	Path(2, Color3.fromRGB(220, 120, 255), { { -200, 40 }, { 1400, 40 }, { 1400, 660 }, { -200, 660 }, { -200, 40 } })
end
