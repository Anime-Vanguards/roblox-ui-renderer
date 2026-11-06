-- UIShadow size dependence: blur 32 on a grid of frame sizes (cells far apart).
return function(Target)
	local Sizes = { 40, 70, 100, 160 }
	for Row, Height in Sizes do
		for Column, Width in Sizes do
			local F = Instance.new("Frame")
			F.Size = UDim2.fromOffset(Width, Height)
			F.Position = UDim2.fromOffset(60 + (Column - 1) * 270 + (160 - Width) / 2, 50 + (Row - 1) * 260 + (160 - Height) / 2)
			F.BackgroundColor3 = Color3.fromRGB(235, 235, 245)
			F.BorderSizePixel = 0
			local Shadow = Instance.new("UIShadow")
			Shadow.Color = Color3.fromRGB(255, 50, 50)
			Shadow.BlurRadius = UDim.new(0, 32)
			Shadow.Parent = F
			F.Parent = Target
		end
	end
end
