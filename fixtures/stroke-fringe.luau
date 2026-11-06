-- Border UIStroke fringe alpha (strokes round 6): the anti-aliased outer fringe of rounded (ring-mode) Outer strokes
-- on black, read on the left / right edges (vertical edges: immune to Studio's vertical capture stretch). Columns:
-- Thickness 1.00 .. 2.00 in 0.05 steps (D = 1.5 .. 3 px at 1.5x, so the fringe pixel's coverage sweeps 0 .. 1).
-- Rows: plain white; white with a white -> white UIGradient; plain white at Transparency 0.5; white with a UIGradient
-- whose transparency is 0.5 throughout; plain white at Transparency 0.2; gradient transparency 0.2.
-- compare: --viewport 1200x900 --time 0.3 --background 0,0,0
return function(Target)
	local White = ColorSequence.new(Color3.new(1, 1, 1))
	local Rows = {
		{},
		{ Gradient = {} },
		{ Transparency = 0.5 },
		{ Gradient = { Transparency = NumberSequence.new(0.5) } },
		{ Transparency = 0.2 },
		{ Gradient = { Transparency = NumberSequence.new(0.2) } },
	}
	for Row, Spec in Rows do
		for Col = 0, 20 do
			local F = Instance.new("Frame")
			F.Name = `R{Row}C{Col}`
			F.Position = UDim2.fromOffset(20 + Col * 55, 20 + (Row - 1) * 140)
			F.Size = UDim2.fromOffset(40, 100)
			F.BackgroundTransparency = 1
			F.BorderSizePixel = 0
			local Corner = Instance.new("UICorner")
			Corner.CornerRadius = UDim.new(0, 8)
			Corner.Parent = F
			local S = Instance.new("UIStroke")
			S.Color = Color3.new(1, 1, 1)
			S.Thickness = 1 + Col * 0.05
			S.BorderStrokePosition = Enum.BorderStrokePosition.Outer
			S.Transparency = Spec.Transparency or 0
			if Spec.Gradient then
				local G = Instance.new("UIGradient")
				G.Color = White
				if Spec.Gradient.Transparency then
					G.Transparency = Spec.Gradient.Transparency
				end
				G.Parent = S
			end
			S.Parent = F
			F.Parent = Target
		end
	end
end
