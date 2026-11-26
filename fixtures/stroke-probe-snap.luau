-- Sharp UIStroke edges a hair beside a pixel centre: each row puts the stroke's outer edge (Outer), both its edges
-- (Center) or its inner edge (Inner) E = e + d device px from a frame edge at whole device px, for e = 1.5 at 1x and
-- 2.5 at 1.5x, 2x and 3x (rows 1-3, 4-6, 7-9, 10-12: the scale each group is for), d = -0.004 .. 0.004 px in steps
-- of 0.0005 (columns). A GPU snaps a quad's corners to 1/256 px, so an edge within 1/512 px of a pixel centre is on
-- it, and that centre falls by the top-left rule.
-- compare: --viewport 1080x660 --time 0.2 --background 0,0,0
return function(Target)
	local Groups = { { Scale = 1, E = 1.5 }, { Scale = 1.5, E = 2.5 }, { Scale = 2, E = 2.5 }, { Scale = 3, E = 2.5 } }
	local Positions = { Enum.BorderStrokePosition.Outer, Enum.BorderStrokePosition.Center, Enum.BorderStrokePosition.Inner }
	for GroupIndex, Group in Groups do
		for PositionIndex, Position in Positions do
			local Row = (GroupIndex - 1) * #Positions + PositionIndex - 1
			for Column = 0, 16 do
				local Device = Group.E + (Column - 8) * 0.0005
				local Frame = Instance.new("Frame")
				Frame.Name = `S{Group.Scale}_{PositionIndex}_{Column}`
				Frame.Position = UDim2.fromOffset(30 + Column * 60, 30 + Row * 50)
				Frame.Size = UDim2.fromOffset(40, 30)
				Frame.BackgroundTransparency = 1
				Frame.BorderSizePixel = 0

				local Stroke = Instance.new("UIStroke")
				Stroke.ApplyStrokeMode = Enum.ApplyStrokeMode.Border
				Stroke.Color = Color3.new(1, 1, 1)
				Stroke.Thickness = (if Position == Enum.BorderStrokePosition.Center then 2 * Device else Device) / Group.Scale
				Stroke.BorderStrokePosition = Position
				Stroke.Parent = Frame

				Frame.Parent = Target
			end
		end
	end
end
