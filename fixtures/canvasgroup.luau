-- CanvasGroup: rounded clipping of children, GroupTransparency, GroupColor3, nesting.
return function(Target)
	local function Group(X, Y, Radius, Props)
		local G = Instance.new("CanvasGroup")
		G.Position = UDim2.fromOffset(X, Y)
		G.Size = UDim2.fromOffset(140, 90)
		G.BackgroundColor3 = Color3.fromRGB(40, 90, 200)
		G.BorderSizePixel = 0
		for Key, Value in Props or {} do
			(G :: any)[Key] = Value
		end
		if Radius then
			local Corner = Instance.new("UICorner")
			Corner.CornerRadius = UDim.new(0, Radius)
			Corner.Parent = G
		end

		local Child = Instance.new("Frame")
		Child.Position = UDim2.fromOffset(-20, -10)
		Child.Size = UDim2.new(0.7, 0, 0.7, 0)
		Child.BackgroundColor3 = Color3.fromRGB(255, 200, 60)
		Child.BorderSizePixel = 0
		Child.Parent = G

		local Label = Instance.new("TextLabel")
		Label.Position = UDim2.fromOffset(10, 50)
		Label.Size = UDim2.fromOffset(120, 30)
		Label.BackgroundTransparency = 1
		Label.Text = "Group"
		Label.TextColor3 = Color3.new(1, 1, 1)
		Label.FontFace = Font.new("rbxasset://fonts/families/FredokaOne.json")
		Label.TextSize = 24
		Label.Parent = G

		G.Parent = Target
		return G
	end

	Group(30, 30, nil)
	Group(200, 30, 20)
	Group(370, 30, 20, { GroupTransparency = 0.5 })
	Group(540, 30, 20, { GroupColor3 = Color3.fromRGB(255, 120, 120) })
	Group(30, 160, 45, { BackgroundTransparency = 1 })

	local Nested = Group(200, 160, 12)
	local Inner = Group(0, 0, 30, { GroupTransparency = 0.3 })
	Inner.Size = UDim2.fromOffset(80, 60)
	Inner.Position = UDim2.fromOffset(70, 40)
	Inner.Parent = Nested
end
