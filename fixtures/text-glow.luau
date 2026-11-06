return function(Target)
	local function Label(X, Y, WithShadow, Bg)
		local L = Instance.new("TextLabel")
		L.Position = UDim2.fromOffset(X, Y)
		L.Size = UDim2.fromOffset(200, 60)
		L.BackgroundColor3 = Color3.fromRGB(255, 60, 60)
		L.BackgroundTransparency = if Bg then 0 else 1
		L.BorderSizePixel = 0
		L.Text = "Glow"
		L.TextColor3 = Color3.new(1, 1, 1)
		L.FontFace = Font.new("rbxasset://fonts/families/FredokaOne.json")
		L.TextSize = 40
		L.Parent = Target
		if WithShadow then
			local S = Instance.new("UIShadow")
			S.Color = Color3.fromRGB(255, 60, 60)
			S.BlurRadius = UDim.new(0, 8)
			S.Parent = L
		end
	end
	Label(20, 20, false, false)
	Label(260, 20, false, true)
	Label(500, 20, true, false)
	Label(740, 20, true, true)
end
