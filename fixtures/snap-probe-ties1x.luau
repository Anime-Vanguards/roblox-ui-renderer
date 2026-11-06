-- Edge snapping near half pixels at 1x DPI: frames whose top edge (block A) or left edge (block B) sits at
-- k + 0.5 - e device px, e = 0.0002 .. 0.0024 (12 steps), k spread over the whole viewport (A: 41 rows,
-- y 20 .. 740; B: 40 columns, x 620 .. 1166). White on black; the readout (tools/analysis/snap_ties.py)
-- checks whether pixel row / column k is lit. a window's count pill (top 423.498779 at 1x) lands on
-- 423 in Studio where the 1/256-grid rule (snapDevice) gives 424.
-- compare: --viewport 1200x780 --time 0.3 --background 0,0,0   (Studio at 100 % display scale)
return function(Target)
	local Scale = 1 -- device px per logical px (Studio at 100 %)
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
	local function Frame(Name: string, X: number, Y: number, W: number, H: number)
		local F = Instance.new("Frame")
		F.Name = Name
		F.BorderSizePixel = 0
		F.BackgroundColor3 = Color3.new(1, 1, 1)
		F.Position = UDim2.new(U(X), U(Y))
		F.Size = UDim2.new(U(W), U(H))
		F.Parent = Holder
	end
	for Row = 0, 40 do
		local K = 20 + Row * 18
		for Column = 0, 11 do
			local E = 0.0002 * (Column + 1)
			Frame(`A{Row}_{Column}`, 10 + Column * 48, K + 0.5 - E, 40, 8)
		end
	end
	for Column = 0, 39 do
		local M = 620 + Column * 14
		for Row = 0, 11 do
			local E = 0.0002 * (Row + 1)
			Frame(`B{Column}_{Row}`, M + 0.5 - E, 20 + Row * 60, 8, 50)
		end
	end
end
