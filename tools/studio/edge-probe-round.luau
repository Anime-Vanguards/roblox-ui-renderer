-- Coverage against signed distance on a turned edge: 30 x 30 elements turned 7, 20, 30 and 45 degrees, each
-- placed so that its left edge passes a pixel centre at j / 2048 px (j = -20 .. 20) at device scale 1.5
-- compare: --viewport 1200x900 --time 0.5 --background 0,0,0
return function(Target)
	local Kind = "round"
	local Scale = 1.5
	local Root = Instance.new("Frame")
	Root.Name = "Root"
	Root.Size = UDim2.fromOffset(1024, 1024)
	Root.BackgroundTransparency = 1
	Root.BorderSizePixel = 0
	Root.Parent = Target
	local Index = 0
	for _, Angle in { 7, 20, 30, 45 } do
		local Radians = math.rad(Angle)
		local Nx, Ny = math.cos(Radians), math.sin(Radians)
		for J = -20, 20 do
			local Column, Row = Index % 24, Index // 24
			Index += 1
			-- the pixel centre the left edge passes, in physical px
			local Px, Py = math.floor((25 + Column * 49) * Scale) + 0.5, math.floor((25 + Row * 60) * Scale) + 0.5
			-- the element's centre: half a width along the normal from the edge, 5.3 px along the edge, J / 2048 px off
			local Half = 15 * Scale
			local Cx = Px + (Half + J / 2048) * Nx + 5.3 * -Ny
			local Cy = Py + (Half + J / 2048) * Ny + 5.3 * Nx
			local Element = Instance.new(if Kind == "image" then "ImageLabel" else "Frame")
			Element.Name = `E_{Angle}_{J + 20}`
			Element.AnchorPoint = Vector2.new(0.5, 0.5)
			Element.Position = UDim2.fromScale(Cx / Scale / 1024, Cy / Scale / 1024)
			Element.Size = UDim2.fromOffset(30, 30)
			Element.Rotation = Angle
			Element.BorderSizePixel = 0
			Element.BackgroundColor3 = Color3.new(1, 1, 1)
			if Kind == "image" then
				Element.BackgroundTransparency = 1
				Element.Image = "rbxasset://textures/ui/InGameMenu/WhiteSquare.png"
			elseif Kind == "round" then
				local Corner = Instance.new("UICorner")
				Corner.CornerRadius = UDim.new(0, 6)
				Corner.Parent = Element
			elseif Kind == "hard" then
				Instance.new("UIGradient").Parent = Element
			end
			Element.Parent = Root
		end
	end
end
