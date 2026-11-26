-- UIStroke UIGradients in CanvasGroups: gradient-stroked frames (rounded and sharp, red to blue with a hard step,
-- rotated 0 and 35 degrees) placed plainly, under a parent turned 30 degrees whose child turns -30 (the rotations
-- cancel), and under 45 / -45, in a resampled CanvasGroup (fractional size, its texture scaled on the screen), in
-- an unresampled one and on the screen. Columns: on screen, unresampled group, resampled group (two sizes).
-- compare: --viewport 1200x760 --time 0.2 --background 20,20,26
return function(Target)
	local Step = ColorSequence.new({
		ColorSequenceKeypoint.new(0, Color3.fromRGB(255, 40, 0)),
		ColorSequenceKeypoint.new(0.499, Color3.fromRGB(255, 220, 0)),
		ColorSequenceKeypoint.new(0.5, Color3.fromRGB(0, 120, 255)),
		ColorSequenceKeypoint.new(1, Color3.fromRGB(160, 0, 255)),
	})
	local function Stroked(Parent, X, Y, Radius, GradientRotation, Position)
		local F = Instance.new("Frame")
		F.AnchorPoint = Vector2.new(0.5, 0.5)
		F.Position = UDim2.fromOffset(X, Y)
		F.Size = UDim2.fromOffset(70, 44)
		F.BackgroundTransparency = 1
		F.BorderSizePixel = 0
		if Radius > 0 then
			local Corner = Instance.new("UICorner")
			Corner.CornerRadius = UDim.new(0, Radius)
			Corner.Parent = F
		end
		local S = Instance.new("UIStroke")
		S.ApplyStrokeMode = Enum.ApplyStrokeMode.Border
		S.Color = Color3.new(1, 1, 1)
		S.Thickness = 4
		S.BorderStrokePosition = Position
		S.Parent = F
		local G = Instance.new("UIGradient")
		G.Color = Step
		G.Rotation = GradientRotation
		G.Parent = S
		F.Parent = Parent
		return F
	end
	-- A frame turned Turn degrees holding the stroked frame turned back by -Turn
	local function Cancelled(Parent, X, Y, Turn, Radius, GradientRotation, Position)
		local Outer = Instance.new("Frame")
		Outer.AnchorPoint = Vector2.new(0.5, 0.5)
		Outer.Position = UDim2.fromOffset(X, Y)
		Outer.Size = UDim2.fromOffset(90, 64)
		Outer.Rotation = Turn
		Outer.BackgroundTransparency = 1
		Outer.BorderSizePixel = 0
		Outer.Parent = Parent
		local Inner = Stroked(Outer, 45, 32, Radius, GradientRotation, Position)
		Inner.Rotation = -Turn
	end
	local function Cell(Parent, OX, OY)
		local Row = 0
		for _, Radius in { 12, 0 } do
			for _, Rotation in { 0, 35 } do
				local Y = OY + 50 + Row * 82
				Stroked(Parent, OX + 60, Y, Radius, Rotation, Enum.BorderStrokePosition.Outer)
				Cancelled(Parent, OX + 160, Y, 30, Radius, Rotation, Enum.BorderStrokePosition.Center)
				Cancelled(Parent, OX + 255, Y, 45, Radius, Rotation, Enum.BorderStrokePosition.Outer)
				Row += 1
			end
		end
	end
	local function Group(X, Y, W, H)
		local G = Instance.new("CanvasGroup")
		G.Position = UDim2.fromOffset(X, Y)
		G.Size = UDim2.new(0, W, 0, H)
		G.BackgroundColor3 = Color3.fromRGB(30, 30, 40)
		G.BorderSizePixel = 0
		G.Parent = Target
		return G
	end
	-- On the screen
	Cell(Target, 0, 0)
	-- An unresampled group (whole logical px, a multiple of 2 at 1.5x)
	Cell(Group(300, 0, 300, 360), 0, 0)
	-- Resampled groups: sizes with a fraction of a logical px
	local Resampled = Group(600, 0, 300, 360)
	Resampled.Size = UDim2.new(0.0003, 300, 0.0007, 360)
	Cell(Resampled, 0, 0)
	local Resampled2 = Group(900, 0, 300, 360)
	Resampled2.Size = UDim2.new(0.00041, 300, 0.00023, 360)
	Cell(Resampled2, 0, 0)
	-- Second row: the same in groups at fractional positions
	local Moved = Group(300, 380, 300, 360)
	Moved.Position = UDim2.new(0.00037, 300, 0.00051, 380)
	Cell(Moved, 0, 0)
	local Both = Group(600, 380, 300, 360)
	Both.Position = UDim2.new(0.00029, 600, 0.00013, 380)
	Both.Size = UDim2.new(0.00057, 300, 0.00031, 360)
	Cell(Both, 0, 0)
end
