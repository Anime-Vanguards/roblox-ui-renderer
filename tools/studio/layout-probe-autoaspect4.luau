-- AutomaticSize Y + UIAspectRatioConstraint 4 with Size (300, 0) (layout-probe-autoaspect3: a scale-width
-- child makes the width its natural width, 518 not 540, while autoaspect's fitW_box keeps the base):
-- which children decide the width.
return function(Target)
	local Results = {}
	local Nodes = {}
	local Count = 0
	local function Frame(Parent, Size, Props)
		local F = Instance.new("Frame")
		F.Size = Size
		F.BackgroundTransparency = 1
		for K, V in Props or {} do
			F[K] = V
		end
		F.Parent = Parent
		return F
	end
	local function Case(Name, Build, Ratio)
		Count += 1
		local Holder = Frame(Target, UDim2.fromOffset(390, 290), { Position = UDim2.fromOffset(((Count - 1) % 3) * 400, ((Count - 1) // 3) * 300) })
		local A = Frame(Holder, UDim2.fromOffset(300, 0), { AutomaticSize = Enum.AutomaticSize.Y })
		local C = Instance.new("UIAspectRatioConstraint")
		C.AspectRatio = Ratio or 4
		C.Parent = A
		Build(A)
		Nodes[Name] = A
	end
	Case("scale_inner100", function(A)
		local S = Frame(A, UDim2.new(1, 0, 0, 30))
		Frame(S, UDim2.fromOffset(100, 20))
	end)
	Case("scale_inner100_offset200", function(A)
		local S = Frame(A, UDim2.new(1, 0, 0, 30))
		Frame(S, UDim2.fromOffset(100, 20))
		Frame(A, UDim2.fromOffset(200, 20))
	end)
	Case("offset100", function(A)
		Frame(A, UDim2.fromOffset(100, 50))
	end)
	Case("scale_empty", function(A)
		Frame(A, UDim2.new(1, 0, 0, 30))
	end)
	Case("scale_list100", function(A)
		local S = Frame(A, UDim2.new(1, 0, 0, 0), { AutomaticSize = Enum.AutomaticSize.Y })
		Instance.new("UIListLayout").Parent = S
		Frame(S, UDim2.fromOffset(100, 20))
	end)
	Case("scale_list100_tall", function(A)
		local S = Frame(A, UDim2.new(1, 0, 0, 0), { AutomaticSize = Enum.AutomaticSize.Y })
		Instance.new("UIListLayout").Parent = S
		Frame(S, UDim2.fromOffset(100, 80))
	end)
	Case("scale_list400", function(A)
		local S = Frame(A, UDim2.new(1, 0, 0, 0), { AutomaticSize = Enum.AutomaticSize.Y })
		Instance.new("UIListLayout").Parent = S
		Frame(S, UDim2.fromOffset(400, 20))
	end)
	Case("half_list100", function(A)
		local S = Frame(A, UDim2.new(0.5, 0, 0, 0), { AutomaticSize = Enum.AutomaticSize.Y })
		Instance.new("UIListLayout").Parent = S
		Frame(S, UDim2.fromOffset(100, 20))
	end)
	Case("scale_list100_noauto", function(A)
		local S = Frame(A, UDim2.new(1, 0, 0, 40))
		Instance.new("UIListLayout").Parent = S
		Frame(S, UDim2.fromOffset(100, 20))
	end)
	for _ = 1, 3 do
		task.wait()
	end
	for Key, Node in Nodes do
		Results[Key] = { Node.AbsoluteSize.X, Node.AbsoluteSize.Y }
	end
	return Results
end
