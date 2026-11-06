-- ScrollingFrame, second pass: canvas minimum per inset, scroll bar on the left, fractional frame
-- positions + CanvasPosition rounding, UIScale, CanvasSize scale vs parent padding, AutomaticCanvasSize.
return function(Target)
	local Results = {}
	local Built = {}
	local Count = 0

	local function Holder(W, H)
		local Column = Count % 8
		local Row = Count // 8
		Count += 1
		local F = Instance.new("Frame")
		F.Position = UDim2.fromOffset(10 + Column * 145, 10 + Row * 110)
		F.Size = UDim2.fromOffset(W or 140, H or 105)
		F.BackgroundTransparency = 1
		F.Parent = Target
		return F
	end

	local function Scroller(Name, Props, Canvas, Position, Parent, FramePosition)
		local S = Instance.new("ScrollingFrame")
		S.Name = Name
		S.Position = FramePosition or UDim2.fromOffset(0, 0)
		S.Size = UDim2.fromOffset(120, 90)
		S.BorderSizePixel = 0
		S.CanvasSize = Canvas
		for Key, Value in Props do
			(S :: any)[Key] = Value
		end
		local Child = Instance.new("Frame")
		Child.Name = "Child"
		Child.Size = UDim2.fromOffset(10, 10)
		Child.Parent = S
		S.Parent = Parent or Holder()
		if Position then
			S.CanvasPosition = Position
		end
		table.insert(Built, { Name = Name, Frame = S, Child = Child })
		return S
	end

	-- Canvas minimum per inset
	for _, Canvas in { { "W300H80", UDim2.fromOffset(300, 80) }, { "W80H300", UDim2.fromOffset(80, 300) }, { "W300H60", UDim2.fromOffset(300, 60) }, { "W100H300", UDim2.fromOffset(100, 300) } } do
		for _, Insets in { { "NN", "None", "None" }, { "SN", "ScrollBar", "None" }, { "NS", "None", "ScrollBar" }, { "SS", "ScrollBar", "ScrollBar" } } do
			Scroller(`Min_{Canvas[1]}_{Insets[1]}`, { VerticalScrollBarInset = Enum.ScrollBarInset[Insets[2]], HorizontalScrollBarInset = Enum.ScrollBarInset[Insets[3]] }, Canvas[2], Vector2.new(100000, 100000))
		end
	end

	-- Vertical scroll bar on the left
	for _, Inset in { "None", "ScrollBar" } do
		Scroller(`Left_{Inset}`, { VerticalScrollBarPosition = Enum.VerticalScrollBarPosition.Left, VerticalScrollBarInset = Enum.ScrollBarInset[Inset] }, UDim2.fromOffset(300, 300), nil)
		Scroller(`LeftScrolled_{Inset}`, { VerticalScrollBarPosition = Enum.VerticalScrollBarPosition.Left, VerticalScrollBarInset = Enum.ScrollBarInset[Inset] }, UDim2.fromOffset(300, 300), Vector2.new(20, 30))
	end

	-- Fractional frame positions (physical half pixels) and CanvasPosition rounding
	for _, FrameX in { 11, 12 } do
		local Parent = Holder(140, 105)
		local Row = Instance.new("Frame")
		Row.BackgroundTransparency = 1
		Row.Size = UDim2.fromScale(1, 1)
		Row.Parent = Parent
		for Index, Value in { 0.2, 0.4, 0.5, 1, 1.5, 2, 3, 0.9 } do
			local Holder2 = Holder(140, 105)
			Scroller(`Frac{FrameX}_{Value}`, {}, UDim2.fromOffset(300, 300), Vector2.new(Value, Value), Holder2, UDim2.fromOffset(FrameX - 10 + Index % 2, FrameX - 10))
		end
	end
	do
		local Parent = Holder(140, 105)
		Scroller("ScalePos_1", {}, UDim2.fromOffset(300, 300), Vector2.new(1, 1), Parent, UDim2.new(0.0021, 0, 0.0033, 0))
		local Parent2 = Holder(140, 105)
		Scroller("ScalePos_07", {}, UDim2.fromOffset(300, 300), Vector2.new(0.7, 2.2), Parent2, UDim2.new(0.0047, 0, 0.0071, 0))
	end

	-- UIScale on the parent
	do
		local Parent = Holder(140, 105)
		local UIScale = Instance.new("UIScale")
		UIScale.Scale = 0.8
		UIScale.Parent = Parent
		Scroller("UIScale_B", {}, UDim2.fromOffset(300, 300), Vector2.new(100000, 100000), Parent)
		local Parent2 = Holder(140, 105)
		local UIScale2 = Instance.new("UIScale")
		UIScale2.Scale = 1.25
		UIScale2.Parent = Parent2
		Scroller("UIScale125_P", {}, UDim2.fromOffset(300, 300), Vector2.new(3, 7), Parent2)
	end

	-- CanvasSize scale vs parent padding / parent size
	do
		local Parent = Holder(140, 105)
		local Pad = Instance.new("UIPadding")
		Pad.PaddingLeft = UDim.new(0, 10)
		Pad.PaddingTop = UDim.new(0, 5)
		Pad.PaddingRight = UDim.new(0, 6)
		Pad.Parent = Parent
		Scroller("ScalePad", {}, UDim2.new(1, 0, 1.5, 0), Vector2.new(100000, 100000), Parent)
		local Parent2 = Holder(100, 70)
		Scroller("ScaleSmallParent", {}, UDim2.new(2, 0, 3, 0), Vector2.new(100000, 100000), Parent2)
	end

	-- AutomaticCanvasSize
	local function AutoCase(Name, Axis, Build, Props)
		local S = Scroller(Name, Props or {}, UDim2.fromOffset(0, 0), nil)
		S.AutomaticCanvasSize = Enum.AutomaticSize[Axis]
		Build(S)
		return S
	end
	local function Box(Parent, Name, X, Y, W, H)
		local F = Instance.new("Frame")
		F.Name = Name
		F.Position = UDim2.fromOffset(X, Y)
		F.Size = UDim2.fromOffset(W, H)
		F.Parent = Parent
		return F
	end
	for _, Axis in { "X", "Y", "XY" } do
		AutoCase(`Auto{Axis}_Plain`, Axis, function(S)
			Box(S, "A", 10, 20, 200, 30)
			Box(S, "B", 5, 150, 40, 60)
		end)
		AutoCase(`Auto{Axis}_List`, Axis, function(S)
			local L = Instance.new("UIListLayout")
			L.Padding = UDim.new(0, 5)
			L.Parent = S
			Box(S, "A", 0, 0, 150, 40)
			Box(S, "B", 0, 0, 60, 50)
			Box(S, "C", 0, 0, 80, 30)
		end)
		AutoCase(`Auto{Axis}_Padded`, Axis, function(S)
			local Pad = Instance.new("UIPadding")
			Pad.PaddingLeft = UDim.new(0, 7)
			Pad.PaddingRight = UDim.new(0, 9)
			Pad.PaddingTop = UDim.new(0, 11)
			Pad.PaddingBottom = UDim.new(0, 13)
			Pad.Parent = S
			Box(S, "A", 10, 20, 200, 30)
			Box(S, "B", 5, 100, 40, 60)
		end)
		AutoCase(`Auto{Axis}_Small`, Axis, function(S)
			Box(S, "A", 10, 10, 30, 20)
		end)
		AutoCase(`Auto{Axis}_ScaleChild`, Axis, function(S)
			local F = Box(S, "A", 0, 0, 0, 0)
			F.Size = UDim2.new(0.5, 10, 0.5, 20)
			Box(S, "B", 0, 120, 150, 10)
		end)
	end
	AutoCase("AutoY_WithCanvas", "Y", function(S)
		S.CanvasSize = UDim2.fromOffset(200, 250)
		Box(S, "A", 0, 0, 50, 100)
	end)
	AutoCase("AutoY_WithCanvasSmall", "Y", function(S)
		S.CanvasSize = UDim2.fromOffset(200, 50)
		Box(S, "A", 0, 0, 50, 140)
	end)
	AutoCase("AutoY_Inset", "Y", function(S)
		S.VerticalScrollBarInset = Enum.ScrollBarInset.ScrollBar
		local L = Instance.new("UIListLayout")
		L.Parent = S
		local F = Box(S, "A", 0, 0, 0, 0)
		F.Size = UDim2.new(1, 0, 0, 200)
	end)

	task.wait()
	for _, Entry in Built do
		local S = Entry.Frame
		local Record = {
			window = { S.AbsoluteWindowSize.X, S.AbsoluteWindowSize.Y },
			canvas = { S.AbsoluteCanvasSize.X, S.AbsoluteCanvasSize.Y },
			position = { S.CanvasPosition.X, S.CanvasPosition.Y },
			frame = { S.AbsolutePosition.X, S.AbsolutePosition.Y, S.AbsoluteSize.X, S.AbsoluteSize.Y },
			child = { Entry.Child.AbsolutePosition.X - S.AbsolutePosition.X, Entry.Child.AbsolutePosition.Y - S.AbsolutePosition.Y, Entry.Child.AbsoluteSize.X, Entry.Child.AbsoluteSize.Y },
		}
		for _, Other in S:GetChildren() do
			if Other:IsA("GuiObject") and Other ~= Entry.Child then
				Record[Other.Name] = { Other.AbsolutePosition.X - S.AbsolutePosition.X, Other.AbsolutePosition.Y - S.AbsolutePosition.Y, Other.AbsoluteSize.X, Other.AbsoluteSize.Y }
			end
		end
		Results[Entry.Name] = Record
	end
	return Results
end
