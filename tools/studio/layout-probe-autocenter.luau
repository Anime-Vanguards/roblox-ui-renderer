-- AutomaticSize Y frame with a centred child taller than its base size (a hover preview's
-- UnitInfoHolder: base 300, UIPadding 5 / 5, UIScale 1.1, a centred automatic child ~326 tall).
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
	local function Case(Name, Options)
		Count += 1
		local Holder = New("Frame", Target, {
			Position = UDim2.fromOffset(((Count - 1) % 6) * 200, ((Count - 1) // 6) * 420),
			Size = UDim2.fromOffset(190, 400),
			BackgroundTransparency = 1,
		})
		local F = New("Frame", Holder, { AutomaticSize = Enum.AutomaticSize.Y, Size = UDim2.fromOffset(150, 300) })
		Nodes[Name] = F
		if Options.pad then
			New("UIPadding", F, { PaddingTop = UDim.new(0, 5), PaddingBottom = UDim.new(0, 5) })
		end
		if Options.scale then
			New("UIScale", F, { Scale = 1.1 })
		end
		local C
		if Options.auto then
			C = New("Frame", F, { AutomaticSize = Enum.AutomaticSize.Y, Size = UDim2.fromOffset(100, 0), AnchorPoint = Vector2.new(0.5, Options.anchor or 0.5), Position = UDim2.fromScale(0.5, Options.pos or 0.5) })
			New("Frame", C, { Size = UDim2.fromOffset(100, Options.height or 326) })
		else
			C = New("Frame", F, { Size = UDim2.fromOffset(100, Options.height or 326), AnchorPoint = Vector2.new(0.5, Options.anchor or 0.5), Position = UDim2.fromScale(0.5, Options.pos or 0.5) })
		end
		Nodes[Name .. "_c"] = C
	end
	Case("plain", {})
	Case("pad", { pad = true })
	Case("scale", { scale = true })
	Case("padscale", { pad = true, scale = true })
	Case("auto", { auto = true })
	Case("autopadscale", { auto = true, pad = true, scale = true })
	Case("short", { height = 200 })
	Case("shortpad", { height = 200, pad = true })
	Case("top", { anchor = 0, pos = 0 })
	Case("toppad", { anchor = 0, pos = 0, pad = true })
	Case("bottom", { anchor = 1, pos = 1 })
	Case("bottompad", { anchor = 1, pos = 1, pad = true })
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
