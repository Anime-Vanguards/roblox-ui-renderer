-- What a rounded image's arc pixels sample: in each column the same minified texture at the same sub-pixel place
-- rounded (row 1), unrounded (row 2), and a white rounded image (row 3, its coverage). Over black, an arc pixel
-- of row 1 is row 2's colour times row 3's coverage when the arc samples as the interior does.
-- compare: --viewport 1200x900 --time 0.5 --background 0,0,0
return function(Target)
	local Textures = { "rbxasset://textures/explosion.png", "rbxasset://textures/ui/Settings/Help/XboxController.png" }
	local Cases = { { 40.3, 6 }, { 40.3, 12 }, { 64.6, 16 }, { 25.7, 8 }, { 90.2, 24 } }
	local Column = 0
	for T, Texture in Textures do
		for _, Case in Cases do
			local X = 10.37 + Column * 100
			Column += 1
			for Row = 1, 3 do
				local E = Instance.new("ImageLabel")
				E.Name = `T{T}_{Case[1]}_{Case[2]}_R{Row}`
				E.Position = UDim2.fromOffset(X, 10.71 + (Row - 1) * 120)
				E.Size = UDim2.fromOffset(Case[1], Case[1])
				E.BackgroundTransparency = 1
				E.BorderSizePixel = 0
				E.Image = if Row == 3 then "rbxasset://textures/ui/InGameMenu/WhiteSquare.png" else Texture
				if Row ~= 2 then
					local C = Instance.new("UICorner")
					C.CornerRadius = UDim.new(0, Case[2])
					C.Parent = E
				end
				E.Parent = Target
			end
		end
	end
end
