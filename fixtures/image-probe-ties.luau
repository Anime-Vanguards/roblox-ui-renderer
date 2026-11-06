-- Snapping probe: where a quad edge just below a .5 tie lands. Studio snaps a match list title's right edge
-- 922.49872 to 923 (fixtures/image-probe-span.luau), i.e. edges a little below k + 0.5 still round up. Here
-- white frames and white Stretch images on black have their left edge at k + 0.5 - e and their right edge at
-- m + 0.5 - e, e from 0.0002 to 0.05 px (device px, 1.5 DPI), rows of 12 px. Geometry in physical pixels as
-- Scale on a 1000 x 1000 holder, all under 256 px on x so float32 keeps 1e-5 px.
-- compare: --viewport 1200x900 --time 0.5 --background 0,0,0
-- analysis: python tools/analysis/image_ties.py image-probe-ties
return function(Target)
	local Holder = Instance.new("Frame")
	Holder.Name = "Holder"
	Holder.Size = UDim2.fromOffset(1000, 1000)
	Holder.BackgroundTransparency = 1
	Holder.Parent = Target

	-- Physical px -> UDim on the 1000-logical holder
	local function U(Px: number): UDim
		local Logical = Px / 1.5
		local Offset = math.floor(Logical)
		return UDim.new((Logical - Offset) / 1000, Offset)
	end

	local Epsilons = { 0.0002, 0.0005, 0.001, 0.0015, 0.002, 0.0025, 0.003, 0.004, 0.005, 0.006, 0.0075, 0.009, 0.011, 0.014, 0.017, 0.02, 0.025, 0.031, 0.04, 0.05 }
	for Index, E in Epsilons do
		local Y = 10 + (Index - 1) * 14
		-- Frames: left edge 20.5 - e, right edge 100.5 - e (width 80); images the same further right
		local Box = Instance.new("Frame")
		Box.Name = `F{Index}`
		Box.Position = UDim2.new(U(20.5 - E), U(Y))
		Box.Size = UDim2.new(U(80), U(10))
		Box.BackgroundColor3 = Color3.new(1, 1, 1)
		Box.BorderSizePixel = 0
		Box.Parent = Holder

		local Image = Instance.new("ImageLabel")
		Image.Name = `I{Index}`
		Image.Position = UDim2.new(U(140.5 - E), U(Y))
		Image.Size = UDim2.new(U(80), U(10))
		Image.BackgroundTransparency = 1
		Image.BorderSizePixel = 0
		Image.Image = "rbxasset://textures/ui/InGameMenu/WhiteSquare.png"
		Image.Parent = Holder

		-- Vertical edges: top at Y2 + 0.5 - e, height 10 (bottom at + 10.5 - e), 6 px wide
		local Column = Instance.new("Frame")
		Column.Name = `V{Index}`
		Column.Position = UDim2.new(U(260 + (Index - 1) * 10), U(10.5 - E))
		Column.Size = UDim2.new(U(6), U(10))
		Column.BackgroundColor3 = Color3.new(1, 1, 1)
		Column.BorderSizePixel = 0
		Column.Parent = Holder
	end
end
