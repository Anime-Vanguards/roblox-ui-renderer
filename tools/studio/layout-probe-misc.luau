-- SizeConstraint RelativeXX/RelativeYY, AnchorPoint variations, nested UIScale, UISizeConstraint
-- MinSize, UITextSizeConstraint, LineHeight, TextWrapped + alignment, UIPadding scale.
return function(Target)
	local Results = {}
	local Nodes = {}
	local Count = 0
	local function Holder(W, H)
		Count += 1
		local F = Instance.new("Frame")
		F.Position = UDim2.fromOffset(((Count - 1) % 6) * 195, ((Count - 1) // 6) * 150)
		F.Size = UDim2.fromOffset(W or 180, H or 140)
		F.BackgroundTransparency = 1
		F.Parent = Target
		return F
	end
	local function Frame(Parent, Key, Props)
		local F = Instance.new("Frame")
		for K, V in Props do
			(F :: any)[K] = V
		end
		F.Parent = Parent
		Nodes[Key] = F
		return F
	end
	for _, Constraint in { "RelativeXY", "RelativeXX", "RelativeYY" } do
		local H = Holder(180, 120)
		Frame(H, `SC_{Constraint}`, { Size = UDim2.new(0.5, 5, 0.25, 3), Position = UDim2.new(0.1, 0, 0.2, 0), SizeConstraint = Enum.SizeConstraint[Constraint] })
		Frame(H, `SC_{Constraint}_pos`, { Size = UDim2.fromOffset(10, 10), Position = UDim2.new(0.5, 0, 0.5, 0), SizeConstraint = Enum.SizeConstraint[Constraint] })
	end
	for Index, Anchor in { Vector2.new(0, 0), Vector2.new(0.5, 0.5), Vector2.new(1, 1), Vector2.new(0.25, 0.75), Vector2.new(-0.5, 1.5) } do
		local H = Holder()
		Frame(H, `Anchor{Index}`, { Size = UDim2.new(0.3, 7, 0, 33), Position = UDim2.new(0.5, 3, 0.5, -4), AnchorPoint = Anchor })
	end
	do
		local H = Holder()
		local S1 = Instance.new("UIScale")
		S1.Scale = 0.8
		S1.Parent = H
		local A = Frame(H, "Nested_A", { Size = UDim2.fromOffset(120, 80), Position = UDim2.fromOffset(10, 10) })
		local S2 = Instance.new("UIScale")
		S2.Scale = 1.25
		S2.Parent = A
		local B = Frame(A, "Nested_B", { Size = UDim2.new(0.5, 10, 0.5, 0), Position = UDim2.new(0.5, 5, 0, 7), AnchorPoint = Vector2.new(0.5, 0) })
		local S3 = Instance.new("UIScale")
		S3.Scale = 0.6
		S3.Parent = B
		Frame(B, "Nested_C", { Size = UDim2.fromOffset(20, 20), Position = UDim2.fromScale(1, 1), AnchorPoint = Vector2.new(1, 1) })
	end
	do
		local H = Holder()
		local A = Frame(H, "MinSize", { Size = UDim2.fromOffset(20, 10) })
		local C = Instance.new("UISizeConstraint")
		C.MinSize = Vector2.new(50, 40)
		C.Parent = A
	end
	do
		local H = Holder()
		local Pad = Instance.new("UIPadding")
		Pad.PaddingLeft = UDim.new(0.1, 3)
		Pad.PaddingTop = UDim.new(0.2, -2)
		Pad.PaddingRight = UDim.new(0, 12)
		Pad.PaddingBottom = UDim.new(0.05, 0)
		Pad.Parent = H
		Frame(H, "PadScale_A", { Size = UDim2.fromScale(1, 1) })
		Frame(H, "PadScale_B", { Size = UDim2.fromOffset(10, 10), Position = UDim2.fromScale(1, 1), AnchorPoint = Vector2.new(1, 1) })
	end
	local Face = Font.new("rbxasset://fonts/families/SourceSansPro.json")
	for Index, Spec in {
		{ 1.5, false, "Left", "Top" },
		{ 0.8, false, "Center", "Center" },
		{ 2, true, "Right", "Bottom" },
		{ 1.25, true, "Center", "Center" },
	} do
		local H = Holder()
		local L = Instance.new("TextLabel")
		L.Size = UDim2.fromOffset(120, 120)
		L.FontFace = Face
		L.TextSize = 18
		L.LineHeight = Spec[1]
		L.TextWrapped = Spec[2]
		L.TextXAlignment = Enum.TextXAlignment[Spec[3]]
		L.TextYAlignment = Enum.TextYAlignment[Spec[4]]
		L.Text = "Line one\nsecond line of text here\nthree"
		L.Parent = H
		Nodes[`LH{Index}`] = L
	end
	do
		local H = Holder()
		local L = Instance.new("TextLabel")
		L.Size = UDim2.fromOffset(150, 40)
		L.FontFace = Face
		L.TextSize = 40
		L.Text = "Constrained"
		L.Parent = H
		local C = Instance.new("UITextSizeConstraint")
		C.MaxTextSize = 22
		C.MinTextSize = 10
		C.Parent = L
		Nodes["TSC"] = L
	end
	task.wait()
	for Key, Node in Nodes do
		local Root = Node
		while Root.Parent ~= Target do
			Root = Root.Parent
		end
		local Origin = Root.AbsolutePosition
		local Entry = { Node.AbsolutePosition.X - Origin.X, Node.AbsolutePosition.Y - Origin.Y, Node.AbsoluteSize.X, Node.AbsoluteSize.Y }
		if Node:IsA("TextLabel") then
			table.insert(Entry, Node.TextBounds.X)
			table.insert(Entry, Node.TextBounds.Y)
		end
		Results[Key] = Entry
	end
	return Results
end
