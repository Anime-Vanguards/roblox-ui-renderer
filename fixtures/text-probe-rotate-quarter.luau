-- How Studio draws rotated text: the same line at Rotation 0, 180, 90 and 270 (whole-pixel centres, so a
-- half or quarter turn maps pixels to pixels) and at small angles, in two faces and at two sizes.
-- compare: --viewport 1400x900 --time 0.2 --background 0,0,0
return function(Target)
	local Faces = { "FredokaOne", "SourceSans" }
	local Angles = { 0, 180, 90, 270, 1, -13, 30, 45 }
	for FaceIndex, Face in Faces do
		for SizeIndex, Size in { 30, 17 } do
			local Column = (FaceIndex - 1) * 2 + (SizeIndex - 1)
			for Index, Angle in Angles do
				local Label = Instance.new("TextLabel")
				Label.Name = `{Face}{Size}_{Angle}`
				Label.BackgroundTransparency = 1
				Label.TextColor3 = Color3.new(1, 1, 1)
				Label.Font = (Enum.Font :: any)[Face]
				Label.TextSize = Size
				Label.Text = "Mythic illness 42"
				Label.Size = UDim2.fromOffset(300, 300)
				Label.Position = UDim2.fromOffset(20 + Column * 340, -110 + (Index - 1) * 60)
				if Angle == 90 or Angle == 270 then
					Label.Position = UDim2.fromOffset(Column * 340 + (if Angle == 90 then -70 else -10), 500)
				end
				Label.Rotation = Angle
				Label.Parent = Target
			end
		end
	end
end
