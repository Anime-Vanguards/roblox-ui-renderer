-- Up to which size Studio places glyphs at quarter pixels: one letter repeated, every TextSize from 28 to 45
-- in two faces, white on black.
-- compare: --viewport 1500x1000 --time 0.2 --background 0,0,0
return function(Target)
	local Y = 4
	for Size = 28, 45 do
		for Index, Name in { "FredokaOne", "SourceSans" } do
			local Label = Instance.new("TextLabel")
			Label.Name = `{Name}{Size}`
			Label.BackgroundTransparency = 1
			Label.TextColor3 = Color3.new(1, 1, 1)
			Label.Font = (Enum.Font :: any)[Name]
			Label.TextSize = Size
			Label.Text = string.rep("n", 14) .. "HHHH"
			Label.TextXAlignment = Enum.TextXAlignment.Left
			Label.Size = UDim2.fromOffset(740, Size + 6)
			Label.Position = UDim2.fromOffset(8 + (Index - 1) * 748, Y)
			Label.Parent = Target
		end
		Y += Size + 8
	end
end
