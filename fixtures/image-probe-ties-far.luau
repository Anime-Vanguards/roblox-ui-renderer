-- Snapping probe, continued from fixtures/image-probe-ties.luau (edges under 256 px): where a frame's
-- right edge just below a .5 tie lands at larger x, where float32 device coordinates are coarser.
-- A currency card (x 601.00116, w 319.99765 at 1.5x) ends at 1381.4982 and Studio snaps it
-- down to 1381, while the 1/256 grid rule rounds it up. White frames on black, right edge at
-- m + 0.5 - e, e = 0.0010 .. 0.0024 device px in 0.0001 steps (rows of 12 px); columns end near
-- device x 150, 700, 1150 and 1600. Left edges on whole pixels. Geometry as Scale on a 1200-logical
-- holder (UDim offsets are whole numbers); the exact edges come from Studio's AbsolutePosition / Size.
-- compare: --viewport 1200x900 --time 0.5 --background 0,0,0
return function(Target)
	local Ends = { 150, 700, 1150, 1600 }
	local Scale = 1.5
	local Holder = Instance.new("Frame")
	Holder.Name = "Holder"
	Holder.Size = UDim2.fromOffset(1200, 1200)
	Holder.BackgroundTransparency = 1
	Holder.Parent = Target
	-- Device px -> UDim on the 1200-logical holder
	local function U(Px: number): UDim
		local Logical = Px / Scale
		local Offset = math.floor(Logical)
		return UDim.new((Logical - Offset) / 1200, Offset)
	end
	for Column, End in Ends do
		for Row = 0, 14 do
			local E = 0.001 + Row * 0.0001
			local LeftDevice = End - 120
			local RightDevice = End + 0.5 - E
			local Frame = Instance.new("Frame")
			Frame.Name = `C{Column}R{Row}`
			Frame.BorderSizePixel = 0
			Frame.BackgroundColor3 = Color3.new(1, 1, 1)
			local Top = 10 + Row * 12
			Frame.Position = UDim2.new(U(LeftDevice), U(Top))
			Frame.Size = UDim2.new(U(RightDevice - LeftDevice), U(8))
			Frame.Parent = Holder
		end
	end
end
