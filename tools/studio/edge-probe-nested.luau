-- Hard edges of Frames turned inside turned parents: for each chain of parent and own rotations, 30 x 30 Frames
-- under a UIGradient (no anti-aliasing) placed so that the left edge, at the summed angle on the screen, passes a
-- pixel centre at j / 2048 px (j = -20 .. 20) at device scale 1.5. The parents are 1200 x 900 transparent Frames
-- turned about the screen's centre. Which side Studio puts each centre on shows the arithmetic of its matrices.
-- compare: --viewport 1200x900 --time 0.5 --background 0,0,0
return function(Target)
	local Scale = 1.5
	local W, H = 1200, 900
	local Chains = {
		{ 0, 30 }, { 10, 20 }, { 20, 10 }, { 15, 15 }, { 12.5, 17.5 }, { 7, 13 }, { 25, 20 }, { 17, 13 },
		{ 10, 10, 10 }, { 5, 10, 15 },
	}
	local function Turned(Parent, Rotation)
		local F = Instance.new("Frame")
		F.AnchorPoint = Vector2.new(0.5, 0.5)
		F.Position = UDim2.fromOffset(W / 2, H / 2)
		F.Size = UDim2.fromOffset(W, H)
		F.Rotation = Rotation
		F.BackgroundTransparency = 1
		F.BorderSizePixel = 0
		F.Parent = Parent
		return F
	end
	local Root = Instance.new("Frame")
	Root.Name = "Root"
	Root.Size = UDim2.fromOffset(W, H)
	Root.BackgroundTransparency = 1
	Root.BorderSizePixel = 0
	Root.Parent = Target
	local Index = 0
	for ChainIndex, Chain in Chains do
		local Parent = Root
		local Total = 0
		for Level = 1, #Chain - 1 do
			Parent = Turned(Parent, Chain[Level])
			Parent.Name = `C{ChainIndex}_L{Level}`
		end
		for _, Angle in Chain do
			Total += Angle
		end
		local Theta = math.rad(Total)
		local Nx, Ny = math.cos(Theta), math.sin(Theta)
		for J = -20, 20 do
			local Column, Row = Index % 24, Index // 24
			Index += 1
			local Px, Py = math.floor((25 + Column * 49) * Scale) + 0.5, math.floor((25 + Row * 50) * Scale) + 0.5
			local Half = 15 * Scale
			-- the element's centre on the screen, in logical px
			local Sx = (Px + (Half + J / 2048) * Nx + 5.3 * -Ny) / Scale
			local Sy = (Py + (Half + J / 2048) * Ny + 5.3 * Nx) / Scale
			-- into the innermost parent's own axes: undo each parent's turn about the screen's centre, outermost first
			local Lx, Ly = Sx, Sy
			for Level = 1, #Chain - 1 do
				local A = -math.rad(Chain[Level])
				local Dx, Dy = Lx - W / 2, Ly - H / 2
				Lx, Ly = W / 2 + Dx * math.cos(A) - Dy * math.sin(A), H / 2 + Dx * math.sin(A) + Dy * math.cos(A)
			end
			local E = Instance.new("Frame")
			E.Name = `N{ChainIndex}_{J + 20}`
			E.AnchorPoint = Vector2.new(0.5, 0.5)
			E.Position = UDim2.fromOffset(Lx, Ly)
			E.Size = UDim2.fromOffset(30, 30)
			E.Rotation = Chain[#Chain]
			E.BorderSizePixel = 0
			E.BackgroundColor3 = Color3.new(1, 1, 1)
			Instance.new("UIGradient").Parent = E
			E.Parent = Parent
		end
	end
end
