-- Rotated ancestors: AbsolutePosition/AbsoluteSize/AbsoluteRotation of descendants (nested
-- rotations, anchor points, UIScale, list layouts inside rotated frames, text bounds).
return function(Target)
	local Nodes = {}

	local function Frame(Parent, Name, Props)
		local F = Instance.new("Frame")
		F.Name = Name
		F.BorderSizePixel = 0
		for Key, Value in Props do
			(F :: any)[Key] = Value
		end
		F.Parent = Parent
		table.insert(Nodes, F)
		return F
	end

	local Angles = { 20, -35, 90, 180, 7.5 }
	for Index, Angle in Angles do
		local Base = Frame(Target, `R{Index}`, {
			AnchorPoint = Vector2.new(0.5, 0.5),
			Position = UDim2.fromOffset(120 + (Index - 1) * 210, 150),
			Size = UDim2.fromOffset(160, 90),
			Rotation = Angle,
		})
		Frame(Base, "Child", { Position = UDim2.fromOffset(20, 20), Size = UDim2.fromOffset(60, 40), Rotation = 15 })
		Frame(Base, "Anchored", { AnchorPoint = Vector2.new(1, 1), Position = UDim2.fromScale(1, 1), Size = UDim2.fromOffset(30, 20) })
		local Mid = Frame(Base, "Mid", { AnchorPoint = Vector2.new(0.5, 0.5), Position = UDim2.fromScale(0.5, 0.5), Size = UDim2.fromOffset(80, 50), Rotation = -Angle / 2 + 10 })
		Frame(Mid, "Deep", { Position = UDim2.fromOffset(5, 7), Size = UDim2.fromOffset(20, 10), Rotation = 33 })
	end

	-- Rotated + UIScale
	local Scaled = Frame(Target, "Scaled", { Position = UDim2.fromOffset(100, 350), Size = UDim2.fromOffset(200, 100), Rotation = 30 })
	local UIScale = Instance.new("UIScale")
	UIScale.Scale = 0.8
	UIScale.Parent = Scaled
	Frame(Scaled, "Child", { Position = UDim2.fromOffset(10, 10), Size = UDim2.fromOffset(50, 30) })

	-- List layout inside a rotated frame
	local Listed = Frame(Target, "Listed", { Position = UDim2.fromOffset(450, 350), Size = UDim2.fromOffset(200, 150), Rotation = -12 })
	local List = Instance.new("UIListLayout")
	List.Padding = UDim.new(0, 5)
	List.HorizontalAlignment = Enum.HorizontalAlignment.Center
	List.Parent = Listed
	Frame(Listed, "A", { Size = UDim2.fromOffset(60, 20) })
	Frame(Listed, "B", { Size = UDim2.fromOffset(90, 30), Rotation = 45 })

	-- Text inside a rotated frame
	local Label = Instance.new("TextLabel")
	Label.Name = "Label"
	Label.Position = UDim2.fromOffset(10, 10)
	Label.Size = UDim2.fromOffset(150, 40)
	Label.Text = "Rotated parent"
	Label.TextSize = 18
	Label.Parent = Listed
	table.insert(Nodes, Label)

	local function PathOf(Node)
		local Parts = {}
		while Node and Node ~= Target do
			table.insert(Parts, 1, Node.Name)
			Node = Node.Parent
		end
		return table.concat(Parts, "/")
	end

	task.wait()
	local Results = {}
	for _, Node in Nodes do
		Results[PathOf(Node)] = {
			Node.AbsolutePosition.X,
			Node.AbsolutePosition.Y,
			Node.AbsoluteSize.X,
			Node.AbsoluteSize.Y,
			Node.AbsoluteRotation,
		}
	end
	Results.LabelBounds = { Label.TextBounds.X, Label.TextBounds.Y }
	return Results
end
