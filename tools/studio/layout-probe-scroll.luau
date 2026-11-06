-- ScrollingFrame numbers: AbsoluteWindowSize / AbsoluteCanvasSize / clamped CanvasPosition per
-- canvas size, ScrollingDirection, ScrollBarThickness and insets; child offsets for fractional and
-- quantized CanvasPosition values.
return function(Target)
	local Results = {}
	local Built = {}
	local Count = 0

	local function Scroller(Name, Props, Canvas, Position)
		local Column = Count % 9
		local Row = Count // 9
		Count += 1
		local S = Instance.new("ScrollingFrame")
		S.Name = Name
		S.Position = UDim2.fromOffset(10 + Column * 130, 10 + Row * 100)
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
		S.Parent = Target
		if Position then
			S.CanvasPosition = Position
		end
		table.insert(Built, { Name = Name, Frame = S, Child = Child })
		return S
	end

	local Canvases = {
		{ "V", UDim2.fromOffset(120, 300) },
		{ "H", UDim2.fromOffset(300, 90) },
		{ "B", UDim2.fromOffset(300, 300) },
		{ "h5", UDim2.fromOffset(125, 90) },
		{ "v5", UDim2.fromOffset(120, 95) },
		{ "V100", UDim2.fromOffset(100, 300) },
		{ "V115", UDim2.fromOffset(115, 300) },
		{ "N", UDim2.fromOffset(120, 90) },
		{ "S", UDim2.fromOffset(60, 40) },
		{ "Scale", UDim2.new(1, 0, 2, 0) },
		{ "Scale2", UDim2.new(2, 10, 0.5, 5) },
	}

	for _, Direction in { "XY", "X", "Y" } do
		for _, Canvas in Canvases do
			Scroller(`{Direction}_{Canvas[1]}`, { ScrollingDirection = Enum.ScrollingDirection[Direction] }, Canvas[2], Vector2.new(100000, 100000))
		end
	end
	for _, Inset in { "ScrollBar", "Always" } do
		for _, Canvas in { Canvases[1], Canvases[2], Canvases[3], Canvases[8], Canvases[7] } do
			Scroller(`VI{Inset}_{Canvas[1]}`, { VerticalScrollBarInset = Enum.ScrollBarInset[Inset] }, Canvas[2], Vector2.new(100000, 100000))
			Scroller(`HI{Inset}_{Canvas[1]}`, { HorizontalScrollBarInset = Enum.ScrollBarInset[Inset] }, Canvas[2], Vector2.new(100000, 100000))
			Scroller(`BI{Inset}_{Canvas[1]}`, { HorizontalScrollBarInset = Enum.ScrollBarInset[Inset], VerticalScrollBarInset = Enum.ScrollBarInset[Inset] }, Canvas[2], Vector2.new(100000, 100000))
		end
	end
	for _, Thickness in { 0, 5 } do
		for _, Canvas in { Canvases[3], Canvases[7] } do
			Scroller(`T{Thickness}_{Canvas[1]}`, { ScrollBarThickness = Thickness, VerticalScrollBarInset = Enum.ScrollBarInset.ScrollBar, HorizontalScrollBarInset = Enum.ScrollBarInset.ScrollBar }, Canvas[2], Vector2.new(100000, 100000))
		end
	end
	Scroller("Disabled_B", { ScrollingEnabled = false, VerticalScrollBarInset = Enum.ScrollBarInset.ScrollBar }, Canvases[3][2], Vector2.new(100000, 100000))
	Scroller("DirY_ScrollBarInset_B", { ScrollingDirection = Enum.ScrollingDirection.Y, VerticalScrollBarInset = Enum.ScrollBarInset.ScrollBar, HorizontalScrollBarInset = Enum.ScrollBarInset.ScrollBar }, Canvases[3][2], Vector2.new(100000, 100000))
	Scroller("DirX_ScrollBarInset_B", { ScrollingDirection = Enum.ScrollingDirection.X, VerticalScrollBarInset = Enum.ScrollBarInset.ScrollBar, HorizontalScrollBarInset = Enum.ScrollBarInset.ScrollBar }, Canvases[3][2], Vector2.new(100000, 100000))

	-- Canvas positions (quantization)
	for _, Value in { 1, 2, 3, 10, 33.3, 100.5, 101.25, 150.7, 0.2, 0.4, 0.5, 0.6, 0.8, 1.333, 1.667, 7.1, 7.9 } do
		Scroller(`P{Value}`, {}, UDim2.fromOffset(300, 300), Vector2.new(Value, Value))
	end

	task.wait()
	for _, Entry in Built do
		local S = Entry.Frame
		Results[Entry.Name] = {
			window = { S.AbsoluteWindowSize.X, S.AbsoluteWindowSize.Y },
			canvas = { S.AbsoluteCanvasSize.X, S.AbsoluteCanvasSize.Y },
			position = { S.CanvasPosition.X, S.CanvasPosition.Y },
			child = { Entry.Child.AbsolutePosition.X - S.AbsolutePosition.X, Entry.Child.AbsolutePosition.Y - S.AbsolutePosition.Y },
		}
	end
	return Results
end
