-- AutomaticSize X/Y/XY: plain children, UIPadding, text labels, list/grid layouts, nested automatic
-- sizes, scale-sized/positioned/anchored children, invisible children, UIScale, size constraints.
return function(Target)
	local Colors = {
		Color3.fromRGB(80, 160, 255),
		Color3.fromRGB(255, 120, 80),
		Color3.fromRGB(120, 230, 120),
		Color3.fromRGB(230, 200, 90),
		Color3.fromRGB(200, 100, 255),
	}
	local Face = Font.new("rbxasset://fonts/families/SourceSansPro.json")

	local Count = 0
	local function Cell(Name)
		local Column = Count % 6
		local Row = Count // 6
		Count += 1
		local F = Instance.new("Frame")
		F.Name = Name
		F.Position = UDim2.fromOffset(10 + Column * 197, 10 + Row * 150)
		F.Size = UDim2.fromOffset(185, 140)
		F.BackgroundColor3 = Color3.fromRGB(34, 34, 44)
		F.BorderSizePixel = 0
		F.Parent = Target
		return F
	end

	local function Box(Parent, Name, Props)
		local F = Instance.new("Frame")
		F.Name = Name
		F.BackgroundColor3 = Colors[(#Parent:GetChildren() % #Colors) + 1]
		F.BorderSizePixel = 0
		for Key, Value in Props or {} do
			(F :: any)[Key] = Value
		end
		F.Parent = Parent
		return F
	end

	local function Auto(Parent, Name, Axis, Props)
		local Defaults = { AutomaticSize = Enum.AutomaticSize[Axis], BackgroundColor3 = Color3.fromRGB(70, 70, 90) }
		for Key, Value in Props or {} do
			Defaults[Key] = Value
		end
		return Box(Parent, Name, Defaults)
	end

	local function Label(Parent, Name, Text, Props)
		local L = Instance.new("TextLabel")
		L.Name = Name
		L.BackgroundColor3 = Color3.fromRGB(90, 60, 60)
		L.BorderSizePixel = 0
		L.TextColor3 = Color3.new(1, 1, 1)
		L.FontFace = Face
		L.TextSize = 16
		L.Text = Text
		for Key, Value in Props or {} do
			(L :: any)[Key] = Value
		end
		L.Parent = Parent
		return L
	end

	local function Padding(Parent, L, R, T, B)
		local P = Instance.new("UIPadding")
		P.PaddingLeft = L
		P.PaddingRight = R
		P.PaddingTop = T
		P.PaddingBottom = B
		P.Parent = Parent
		return P
	end

	-- Plain children
	for _, Axis in { "X", "Y", "XY" } do
		local C = Cell(`Plain{Axis}`)
		local A = Auto(C, "A", Axis, { Position = UDim2.fromOffset(5, 5), Size = UDim2.fromOffset(20, 20) })
		Box(A, "P", { Position = UDim2.fromOffset(10, 8), Size = UDim2.fromOffset(60, 30) })
		Box(A, "Q", { Position = UDim2.fromOffset(40, 50), Size = UDim2.fromOffset(35, 25) })
	end

	-- UIPadding (offset and scale)
	do
		local C = Cell("PaddedXY")
		local A = Auto(C, "A", "XY", { Position = UDim2.fromOffset(5, 5), Size = UDim2.fromOffset(10, 10) })
		Padding(A, UDim.new(0, 6), UDim.new(0, 9), UDim.new(0, 4), UDim.new(0, 11))
		Box(A, "P", { Position = UDim2.fromOffset(3, 2), Size = UDim2.fromOffset(50, 30) })
	end
	do
		local C = Cell("PaddedScale")
		local A = Auto(C, "A", "Y", { Position = UDim2.fromOffset(5, 5), Size = UDim2.fromOffset(120, 10) })
		Padding(A, UDim.new(0.1, 0), UDim.new(0.05, 2), UDim.new(0.2, 0), UDim.new(0, 5))
		Box(A, "P", { Size = UDim2.fromOffset(50, 40) })
	end

	-- Base size larger than the content
	do
		local C = Cell("BaseLarger")
		local A = Auto(C, "A", "XY", { Position = UDim2.fromOffset(5, 5), Size = UDim2.fromOffset(150, 100) })
		Box(A, "P", { Position = UDim2.fromOffset(10, 10), Size = UDim2.fromOffset(40, 30) })
	end

	-- Scale-sized / scale-positioned / anchored / negative children
	do
		local C = Cell("ScaleChildren")
		local A = Auto(C, "A", "XY", { Position = UDim2.fromOffset(5, 5), Size = UDim2.fromOffset(60, 40) })
		Box(A, "Half", { Size = UDim2.new(0.5, 10, 0.5, 5) })
		Box(A, "Far", { Position = UDim2.new(1, 5, 0, 10), Size = UDim2.fromOffset(20, 20) })
	end
	do
		local C = Cell("AnchoredChildren")
		local A = Auto(C, "A", "XY", { Position = UDim2.fromOffset(20, 20), Size = UDim2.fromOffset(20, 20) })
		Box(A, "Centered", { AnchorPoint = Vector2.new(0.5, 0.5), Position = UDim2.fromOffset(40, 30), Size = UDim2.fromOffset(40, 30) })
		Box(A, "Negative", { Position = UDim2.fromOffset(-10, -8), Size = UDim2.fromOffset(30, 20) })
	end
	do
		local C = Cell("Invisible")
		local A = Auto(C, "A", "XY", { Position = UDim2.fromOffset(5, 5), Size = UDim2.fromOffset(10, 10) })
		Box(A, "Shown", { Size = UDim2.fromOffset(40, 30) })
		Box(A, "Hidden", { Position = UDim2.fromOffset(50, 50), Size = UDim2.fromOffset(60, 60), Visible = false })
	end
	do
		local C = Cell("ScaledChild")
		local A = Auto(C, "A", "XY", { Position = UDim2.fromOffset(5, 5), Size = UDim2.fromOffset(10, 10) })
		local P = Box(A, "P", { Position = UDim2.fromOffset(10, 10), Size = UDim2.fromOffset(80, 60) })
		local S = Instance.new("UIScale")
		S.Scale = 0.5
		S.Parent = P
	end
	do
		local C = Cell("ScaledSelf")
		local A = Auto(C, "A", "XY", { Position = UDim2.fromOffset(5, 5), Size = UDim2.fromOffset(10, 10) })
		local S = Instance.new("UIScale")
		S.Scale = 1.4
		S.Parent = A
		Box(A, "P", { Position = UDim2.fromOffset(4, 4), Size = UDim2.fromOffset(50, 30) })
	end

	-- Text
	for _, Axis in { "X", "Y", "XY" } do
		local C = Cell(`Text{Axis}`)
		Label(C, "L", "Automatic text size", { AutomaticSize = Enum.AutomaticSize[Axis], Position = UDim2.fromOffset(5, 5), Size = UDim2.fromOffset(40, 10) })
		Label(C, "Wrapped", "Wrapped automatic label with several words", { AutomaticSize = Enum.AutomaticSize[Axis], TextWrapped = true, Position = UDim2.fromOffset(5, 50), Size = UDim2.fromOffset(120, 10) })
	end
	do
		local C = Cell("TextPadded")
		local L = Label(C, "L", "Padded", { AutomaticSize = Enum.AutomaticSize.XY, Position = UDim2.fromOffset(5, 5), Size = UDim2.fromOffset(0, 0) })
		Padding(L, UDim.new(0, 8), UDim.new(0, 12), UDim.new(0, 3), UDim.new(0, 6))
		local W = Label(C, "W", "Wrapped padded text in a column", { AutomaticSize = Enum.AutomaticSize.Y, TextWrapped = true, Position = UDim2.fromOffset(5, 50), Size = UDim2.fromOffset(110, 0) })
		Padding(W, UDim.new(0, 5), UDim.new(0, 5), UDim.new(0, 5), UDim.new(0, 5))
	end
	do
		local C = Cell("TextNewlines")
		Label(C, "L", "First line\nSecond, longer line\nthird", { AutomaticSize = Enum.AutomaticSize.XY, Position = UDim2.fromOffset(5, 5), Size = UDim2.fromOffset(0, 0) })
		Label(C, "Empty", "", { AutomaticSize = Enum.AutomaticSize.XY, Position = UDim2.fromOffset(5, 90), Size = UDim2.fromOffset(0, 0) })
	end
	do
		local C = Cell("TextConstraint")
		local L = Label(C, "L", "Constrained size", { AutomaticSize = Enum.AutomaticSize.XY, TextSize = 30, Position = UDim2.fromOffset(5, 5), Size = UDim2.fromOffset(0, 0) })
		local T = Instance.new("UITextSizeConstraint")
		T.MaxTextSize = 18
		T.Parent = L
		local L2 = Label(C, "L2", "Min size", { AutomaticSize = Enum.AutomaticSize.XY, TextSize = 8, Position = UDim2.fromOffset(5, 60), Size = UDim2.fromOffset(0, 0) })
		local T2 = Instance.new("UITextSizeConstraint")
		T2.MinTextSize = 22
		T2.Parent = L2
	end
	do
		local C = Cell("TextButtonAuto")
		local B = Instance.new("TextButton")
		B.Name = "B"
		B.AutomaticSize = Enum.AutomaticSize.XY
		B.Position = UDim2.fromOffset(5, 5)
		B.Size = UDim2.fromOffset(0, 30)
		B.FontFace = Face
		B.TextSize = 18
		B.Text = "Button text"
		B.Parent = C
	end

	-- Layouts inside automatic frames
	do
		local C = Cell("ListY")
		local A = Auto(C, "A", "Y", { Position = UDim2.fromOffset(5, 5), Size = UDim2.fromOffset(120, 0) })
		local L = Instance.new("UIListLayout")
		L.Padding = UDim.new(0, 4)
		L.SortOrder = Enum.SortOrder.LayoutOrder
		L.Parent = A
		Box(A, "P", { Size = UDim2.fromOffset(60, 20), LayoutOrder = 1 })
		Box(A, "Q", { Size = UDim2.new(1, 0, 0, 25), LayoutOrder = 2 })
		Label(A, "T", "List text", { AutomaticSize = Enum.AutomaticSize.XY, Size = UDim2.fromOffset(0, 0), LayoutOrder = 3 })
	end
	do
		local C = Cell("ListX")
		local A = Auto(C, "A", "X", { Position = UDim2.fromOffset(5, 5), Size = UDim2.fromOffset(0, 40) })
		local L = Instance.new("UIListLayout")
		L.FillDirection = Enum.FillDirection.Horizontal
		L.Padding = UDim.new(0, 3)
		L.VerticalAlignment = Enum.VerticalAlignment.Center
		L.Parent = A
		Box(A, "P", { Size = UDim2.fromOffset(30, 20) })
		Box(A, "Q", { Size = UDim2.fromOffset(45, 35) })
		Box(A, "R", { Size = UDim2.new(0, 20, 1, 0) })
	end
	do
		local C = Cell("ListPadded")
		local A = Auto(C, "A", "XY", { Position = UDim2.fromOffset(5, 5), Size = UDim2.fromOffset(0, 0) })
		Padding(A, UDim.new(0, 5), UDim.new(0, 7), UDim.new(0, 3), UDim.new(0, 9))
		local L = Instance.new("UIListLayout")
		L.Padding = UDim.new(0, 2)
		L.HorizontalAlignment = Enum.HorizontalAlignment.Center
		L.Parent = A
		Box(A, "P", { Size = UDim2.fromOffset(70, 20) })
		Box(A, "Q", { Size = UDim2.fromOffset(40, 30) })
	end
	do
		local C = Cell("GridXY")
		local A = Auto(C, "A", "Y", { Position = UDim2.fromOffset(5, 5), Size = UDim2.fromOffset(130, 0) })
		local G = Instance.new("UIGridLayout")
		G.CellSize = UDim2.fromOffset(38, 25)
		G.CellPadding = UDim2.fromOffset(4, 6)
		G.Parent = A
		for Index = 1, 5 do
			Box(A, `G{Index}`)
		end
	end
	do
		local C = Cell("Nested")
		local A = Auto(C, "A", "XY", { Position = UDim2.fromOffset(5, 5), Size = UDim2.fromOffset(0, 0) })
		local L = Instance.new("UIListLayout")
		L.Padding = UDim.new(0, 3)
		L.Parent = A
		local B = Auto(A, "B", "XY", { Size = UDim2.fromOffset(0, 0) })
		Padding(B, UDim.new(0, 4), UDim.new(0, 4), UDim.new(0, 4), UDim.new(0, 4))
		Label(B, "T", "Nested label", { AutomaticSize = Enum.AutomaticSize.XY, Size = UDim2.fromOffset(0, 0) })
		local D = Auto(A, "D", "Y", { Size = UDim2.fromOffset(90, 0) })
		Box(D, "E", { Size = UDim2.fromOffset(50, 35) })
	end
	do
		local C = Cell("MinSize")
		local A = Auto(C, "A", "XY", { Position = UDim2.fromOffset(5, 5), Size = UDim2.fromOffset(10, 10) })
		local S = Instance.new("UISizeConstraint")
		S.MinSize = Vector2.new(90, 70)
		S.Parent = A
		Box(A, "P", { Size = UDim2.fromOffset(40, 30) })
		local B = Auto(C, "B", "XY", { Position = UDim2.fromOffset(5, 85), Size = UDim2.fromOffset(10, 10) })
		local S2 = Instance.new("UISizeConstraint")
		S2.MinSize = Vector2.new(30, 20)
		S2.Parent = B
		Box(B, "P", { Size = UDim2.fromOffset(60, 40) })
	end
	do
		local C = Cell("ScrollAuto")
		local S = Instance.new("ScrollingFrame")
		S.Name = "S"
		S.Position = UDim2.fromOffset(5, 5)
		S.Size = UDim2.fromOffset(150, 120)
		S.CanvasSize = UDim2.fromOffset(0, 0)
		S.AutomaticCanvasSize = Enum.AutomaticSize.Y
		S.BorderSizePixel = 0
		S.Parent = C
		local L = Instance.new("UIListLayout")
		L.Padding = UDim.new(0, 5)
		L.Parent = S
		for Index = 1, 5 do
			Box(S, `I{Index}`, { Size = UDim2.new(1, -10, 0, 30) })
		end
	end
	do
		local C = Cell("AutoInList")
		local L = Instance.new("UIListLayout")
		L.FillDirection = Enum.FillDirection.Horizontal
		L.Padding = UDim.new(0, 4)
		L.Parent = C
		Label(C, "A", "One", { AutomaticSize = Enum.AutomaticSize.X, Size = UDim2.fromOffset(0, 30) })
		Label(C, "B", "Two words", { AutomaticSize = Enum.AutomaticSize.X, Size = UDim2.fromOffset(0, 30) })
		Box(C, "Fill", { Size = UDim2.fromOffset(20, 30) })
	end
end
