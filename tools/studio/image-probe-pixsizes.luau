-- Rounded Pixelated images of every whole physical size (at 1.5x) from 18 to 150 px
-- compare: --viewport 1200x900 --time 0.5 --background 20,20,26
return function(Target)
	local X, Y, RowH = 6, 6, 0
	for Px = 18, 150 do
		if Px % 16 ~= 0 then
			if X + Px > 1790 then
				X = 6
				Y += RowH + 6
				RowH = 0
			end
			local Label = Instance.new("ImageLabel")
			Label.Name = `T_{Px}`
			Label.Position = UDim2.fromOffset(X / 1.5, Y / 1.5)
			Label.Size = UDim2.fromOffset(Px / 1.5, Px / 1.5)
			Label.BackgroundTransparency = 1
			Label.BorderSizePixel = 0
			Label.Image = "rbxasset://textures/ui/Motor.png"
			Label.ResampleMode = Enum.ResamplerMode.Pixelated
			Label.Parent = Target
			local Corner = Instance.new("UICorner")
			Corner.CornerRadius = UDim.new(0, 4)
			Corner.Parent = Label
			X += Px + 6
			RowH = math.max(RowH, Px)
		end
	end
end
