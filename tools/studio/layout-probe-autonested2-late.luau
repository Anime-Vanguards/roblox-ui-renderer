-- (read after 30 frames) Which descendants count for AutomaticSize (layout-probe-autonested: a scale-sized child's
-- scale-sized child makes the frame fill the available size; a childless scale child does not).
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
	local function Case(Name, Build)
		Count += 1
		local Holder = New("Frame", Target, {
			Position = UDim2.fromOffset(((Count - 1) % 5) * 220, ((Count - 1) // 5) * 180),
			Size = UDim2.fromOffset(200, 160),
			BackgroundTransparency = 1,
		})
		local F = New("Frame", Holder, { AutomaticSize = Enum.AutomaticSize.XY, Size = UDim2.fromOffset(0, 0), Position = UDim2.fromOffset(10, 5) })
		Nodes[Name] = F
		Build(F, Name)
		New("Frame", F, { Size = UDim2.fromOffset(60, 20) })
	end
	Case("oversize", function(F, Name)
		local C = New("Frame", F, { Size = UDim2.fromOffset(40, 20) })
		Nodes[Name .. "_g"] = New("Frame", C, { Size = UDim2.fromOffset(100, 90) })
	end)
	Case("oversizeScale", function(F, Name)
		local C = New("Frame", F, { Size = UDim2.fromScale(0.2, 0.2) })
		Nodes[Name .. "_g"] = New("Frame", C, { Size = UDim2.fromOffset(100, 90) })
	end)
	Case("scalepos", function(F, Name)
		local C = New("Frame", F, { Size = UDim2.fromScale(0.5, 0.5), Position = UDim2.fromScale(0.5, 0) })
		Nodes[Name .. "_c"] = C
		Nodes[Name .. "_g"] = New("Frame", C, { Size = UDim2.fromScale(1, 1) })
	end)
	Case("offsetgrand", function(F, Name)
		local C = New("Frame", F, { Size = UDim2.fromScale(1, 1) })
		Nodes[Name .. "_g"] = New("Frame", C, { Size = UDim2.fromOffset(10, 10), Position = UDim2.fromOffset(120, 100) })
	end)
	Case("scalegrandpos", function(F, Name)
		local C = New("Frame", F, { Size = UDim2.fromScale(1, 1) })
		Nodes[Name .. "_g"] = New("Frame", C, { Size = UDim2.fromOffset(10, 10), Position = UDim2.fromScale(0.5, 0.5) })
	end)
	Case("hiddengrand", function(F, Name)
		local C = New("Frame", F, { Size = UDim2.fromScale(1, 1) })
		Nodes[Name .. "_g"] = New("Frame", C, { Size = UDim2.fromScale(1, 1), Visible = false })
	end)
	Case("hiddenchild", function(F, Name)
		local C = New("Frame", F, { Size = UDim2.fromScale(1, 1), Visible = false })
		Nodes[Name .. "_g"] = New("Frame", C, { Size = UDim2.fromScale(1, 1) })
	end)
	Case("greatgrand", function(F, Name)
		local C = New("Frame", F, { Size = UDim2.fromScale(1, 1) })
		local G = New("Frame", C, { Size = UDim2.fromOffset(50, 50) })
		Nodes[Name .. "_gg"] = New("Frame", G, { Size = UDim2.fromScale(2, 2) })
	end)
	Case("autogrand", function(F, Name)
		local C = New("Frame", F, { Size = UDim2.fromScale(1, 1) })
		local G = New("Frame", C, { Size = UDim2.new(), AutomaticSize = Enum.AutomaticSize.XY })
		Nodes[Name .. "_g"] = G
		New("Frame", G, { Size = UDim2.fromOffset(30, 70) })
	end)
	Case("listchild", function(F, Name)
		local C = New("Frame", F, { Size = UDim2.fromScale(1, 1) })
		New("UIListLayout", C, {})
		Nodes[Name .. "_g"] = New("Frame", C, { Size = UDim2.new(0.5, 0, 0, 30) })
	end)
	Case("text", function(F, Name)
		local C = New("Frame", F, { Size = UDim2.fromScale(1, 1) })
		Nodes[Name .. "_g"] = New("TextLabel", C, { Size = UDim2.fromScale(1, 0.5), Text = "text", TextScaled = true })
	end)
	Case("uiscale", function(F, Name)
		local C = New("Frame", F, { Size = UDim2.fromScale(1, 1) })
		New("UIScale", C, { Scale = 0.5 })
		Nodes[Name .. "_g"] = New("Frame", C, { Size = UDim2.fromScale(1, 1) })
	end)
	for _ = 1, 30 do
		task.wait()
	end
	for Key, Node in Nodes do
		local Root = Node
		while Root.Parent ~= Target do
			Root = Root.Parent
		end
		Results[Key] = { Node.AbsolutePosition.X - Root.AbsolutePosition.X, Node.AbsolutePosition.Y - Root.AbsolutePosition.Y, Node.AbsoluteSize.X, Node.AbsoluteSize.Y }
	end
	return Results
end
