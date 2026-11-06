-- ScrollingFrame bar thresholds: canvas exactly frame - thickness, per inset.
return function(Target)
	local Results = {}
	local Built = {}
	local Count = 0
	local function Scroller(Name, Props, Canvas)
		local Column = Count % 8
		local Row = Count // 8
		Count += 1
		local S = Instance.new("ScrollingFrame")
		S.Name = Name
		S.Position = UDim2.fromOffset(10 + Column * 140, 10 + Row * 100)
		S.Size = UDim2.fromOffset(120, 90)
		S.BorderSizePixel = 0
		S.CanvasSize = Canvas
		for Key, Value in Props do
			(S :: any)[Key] = Value
		end
		S.Parent = Target
		S.CanvasPosition = Vector2.new(100000, 100000)
		table.insert(Built, S)
	end
	for _, Inset in { "None", "ScrollBar" } do
		for _, T in { 12, 5 } do
			for _, W in { -1, 0, 1 } do
				Scroller(`H_{Inset}_T{T}_{W}`, { ScrollBarThickness = T, VerticalScrollBarInset = Enum.ScrollBarInset[Inset], HorizontalScrollBarInset = Enum.ScrollBarInset[Inset] }, UDim2.fromOffset(120 - T + W, 300))
				Scroller(`V_{Inset}_T{T}_{W}`, { ScrollBarThickness = T, VerticalScrollBarInset = Enum.ScrollBarInset[Inset], HorizontalScrollBarInset = Enum.ScrollBarInset[Inset] }, UDim2.fromOffset(300, 90 - T + W))
			end
		end
	end
	for _, Inset in { "None", "ScrollBar" } do
		Scroller(`Exact_{Inset}`, { VerticalScrollBarInset = Enum.ScrollBarInset[Inset], HorizontalScrollBarInset = Enum.ScrollBarInset[Inset] }, UDim2.fromOffset(120, 90))
		Scroller(`ExactW_{Inset}`, { VerticalScrollBarInset = Enum.ScrollBarInset[Inset], HorizontalScrollBarInset = Enum.ScrollBarInset[Inset] }, UDim2.fromOffset(121, 90))
		Scroller(`ExactH_{Inset}`, { VerticalScrollBarInset = Enum.ScrollBarInset[Inset], HorizontalScrollBarInset = Enum.ScrollBarInset[Inset] }, UDim2.fromOffset(120, 91))
		Scroller(`Frac_{Inset}`, { VerticalScrollBarInset = Enum.ScrollBarInset[Inset], HorizontalScrollBarInset = Enum.ScrollBarInset[Inset] }, UDim2.new(0, 108, 0.1, 210))
	end
	task.wait()
	for _, S in Built do
		Results[S.Name] = { S.AbsoluteWindowSize.X, S.AbsoluteWindowSize.Y, S.AbsoluteCanvasSize.X, S.AbsoluteCanvasSize.Y, S.CanvasPosition.X, S.CanvasPosition.Y }
	end
	return Results
end
