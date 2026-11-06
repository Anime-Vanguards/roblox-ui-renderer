-- A UI kit's HoverSurface (a hover preview: Studio sizes its two sections to their list
-- container, 320 x 320) rebuilt piece by piece to find what makes the AutomaticSize frame fill.
return function(Target)
	local Results = {}
	local Nodes = {}
	local Count = 0
	local function New(ClassName, Parent, Props)
		local Object = Instance.new(ClassName)
		for K, V in Props or {} do
			(Object :: any)[K] = V
		end
		Object.Parent = Parent
		return Object
	end
	local function Surface(Name, Parts)
		Count += 1
		local Holder = New("Frame", Target, {
			Position = UDim2.fromOffset(((Count - 1) % 3) * 340, ((Count - 1) // 3) * 340),
			Size = UDim2.fromOffset(320, 320),
			BackgroundTransparency = 1,
		})
		New("UIListLayout", Holder, { HorizontalAlignment = Enum.HorizontalAlignment.Center, VerticalAlignment = Enum.VerticalAlignment.Center, Padding = UDim.new(0, 24) })
		local F = New("Frame", Holder, { Name = Name, AutomaticSize = Enum.AutomaticSize.XY, BackgroundTransparency = 1, ClipsDescendants = false, Size = UDim2.fromOffset(0, 0) })
		Nodes[Name] = F
		if Parts.scale then
			New("UIScale", F, { Scale = 1 })
		end
		if Parts.arrows then
			New("ImageLabel", F, { Name = "ArrowLeft", AnchorPoint = Vector2.new(1, 0), BackgroundTransparency = 1, Position = UDim2.fromOffset(8, 14), Size = UDim2.fromOffset(32, 32), Visible = false, ZIndex = 3 })
			New("ImageLabel", F, { Name = "ArrowRight", AnchorPoint = Vector2.zero, BackgroundTransparency = 1, Position = UDim2.new(1, -8, 0, 14), Rotation = 180, Size = UDim2.fromOffset(32, 32), Visible = false, ZIndex = 3 })
		end
		local Chrome = New("Frame", F, { Name = "Chrome", BackgroundTransparency = 1, Size = UDim2.fromScale(1, 1), ZIndex = 1 })
		Nodes[Name .. "_chrome"] = Chrome
		if Parts.glow then
			New("ImageLabel", Chrome, { Name = "Glow", AnchorPoint = Vector2.new(0.5, 0.5), BackgroundTransparency = 1, Position = UDim2.fromScale(0.5, 0.5), Size = UDim2.new(1, 48, 1, 48), Visible = false, ZIndex = 0 })
		end
		local Background = New("Frame", Chrome, { Name = "SurfaceBackground", Size = UDim2.fromScale(1, 1), ZIndex = 1 })
		if Parts.stroke then
			New("UICorner", Background, { CornerRadius = UDim.new(0, 6) })
			New("UIStroke", Background, { Thickness = 1 })
		end
		local Main = New("Frame", F, { Name = "Main", AutomaticSize = Enum.AutomaticSize.XY, BackgroundTransparency = 1, Size = UDim2.fromScale(0, 0), ZIndex = 2 })
		Nodes[Name .. "_main"] = Main
		New("UIListLayout", Main, { HorizontalAlignment = Enum.HorizontalAlignment.Left, Padding = UDim.new(0, 4), SortOrder = Enum.SortOrder.LayoutOrder })
		New("UIPadding", Main, { PaddingBottom = UDim.new(0, 8), PaddingLeft = UDim.new(0, 8), PaddingRight = UDim.new(0, 8), PaddingTop = UDim.new(0, 8) })
		New("TextLabel", Main, { Name = "Label", AutomaticSize = Enum.AutomaticSize.XY, BackgroundTransparency = 1, LayoutOrder = 1, Size = UDim2.new(), Text = "Click to view...", TextSize = 14, FontFace = Font.new("rbxasset://fonts/families/SourceSansPro.json") })
	end
	Surface("all", { scale = true, arrows = true, glow = true, stroke = true })
	Surface("noscale", { arrows = true, glow = true, stroke = true })
	Surface("noarrows", { scale = true, glow = true, stroke = true })
	Surface("noglow", { scale = true, arrows = true, stroke = true })
	Surface("nostroke", { scale = true, arrows = true, glow = true })
	Surface("bare", {})
	task.wait()
	task.wait()
	for Key, Node in Nodes do
		local Root = Node
		while Root.Parent ~= Target do
			Root = Root.Parent
		end
		Results[Key] = { Node.AbsolutePosition.X - Root.AbsolutePosition.X, Node.AbsolutePosition.Y - Root.AbsolutePosition.Y, Node.AbsoluteSize.X, Node.AbsoluteSize.Y }
	end
	return Results
end
