-- Sharp Outer UIStroke reach at any device scale: white strokes of Thickness 1.55 + k / 768 logical px (k = 0 ..
-- 239) around transparent 40 x 30 frames at even logical positions, so whole device pixels at 1.5x, 2x, 2.5x and 3x.
-- A pixel centre just past the outer edge is drawn when the stroke has the 0.1 logical px margin, empty without it:
-- each scale's row of such centres shows where a sharp stroke starts to have one.
-- compare: --viewport 1200x600 --time 0.2 --background 0,0,0
return function(Target)
	for Index = 0, 239 do
		local Column, Row = Index % 20, Index // 20
		local Frame = Instance.new("Frame")
		Frame.Name = `K{Index}`
		Frame.Position = UDim2.fromOffset(30 + Column * 56, 30 + Row * 44)
		Frame.Size = UDim2.fromOffset(40, 30)
		Frame.BackgroundTransparency = 1
		Frame.BorderSizePixel = 0

		local Stroke = Instance.new("UIStroke")
		Stroke.ApplyStrokeMode = Enum.ApplyStrokeMode.Border
		Stroke.Color = Color3.new(1, 1, 1)
		Stroke.Thickness = 1.55 + Index / 768
		Stroke.BorderStrokePosition = Enum.BorderStrokePosition.Outer
		Stroke.Parent = Frame

		Frame.Parent = Target
	end
end
