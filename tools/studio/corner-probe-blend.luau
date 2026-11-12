-- Rounded images over their own background: a flat-coloured image (WhiteSquare tinted) over BackgroundColor3,
-- several tints, transparencies and radii, so an arc pixel's value is tint x c + background x (1 - c)
-- compare: --viewport 1200x900 --time 0.5 --background 0,0,0
return function(Target)
	local Tints = { Color3.fromRGB(20, 20, 20), Color3.fromRGB(200, 40, 60), Color3.fromRGB(255, 255, 255), Color3.fromRGB(10, 120, 240) }
	local Backs = { Color3.fromRGB(70, 70, 90), Color3.fromRGB(255, 255, 255), Color3.fromRGB(0, 0, 0) }
	local Y = 10
	for B, Back in Backs do
		local X = 10
		for T, Tint in Tints do
			for _, Radius in { 8, 24 } do
				for _, Transparency in { 0, 0.4 } do
					local E = Instance.new("ImageLabel")
					E.Name = `B{B}_T{T}_R{Radius}_{Transparency * 10}`
					E.Position = UDim2.fromOffset(X + 0.3, Y + 0.3)
					E.Size = UDim2.fromOffset(Radius * 2 + 10, Radius * 2 + 10)
					E.BorderSizePixel = 0
					E.BackgroundColor3 = Back
					E.Image = "rbxasset://textures/ui/InGameMenu/WhiteSquare.png"
					E.ImageColor3 = Tint
					E.ImageTransparency = Transparency
					local C = Instance.new("UICorner")
					C.CornerRadius = UDim.new(0, Radius)
					C.Parent = E
					E.Parent = Target
					X += Radius * 2 + 16
				end
			end
		end
		Y += 70
	end
end
