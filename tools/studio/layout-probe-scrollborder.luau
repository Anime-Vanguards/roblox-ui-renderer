-- ScrollingFrame window / canvas / max CanvasPosition vs BorderSizePixel and BorderMode (the
-- interaction probes measured window = frame - (thickness + 1) per visible bar with the default
-- border), plus the bar-need boundary with a border.
return function(Target)
	local Results = {}
	local Built = {}
	local Count = 0

	local function Scroller(Name, Border, Mode, Thickness, Canvas)
		local Column = Count % 10
		local Row = Count // 10
		Count += 1
		local S = Instance.new("ScrollingFrame")
		S.Name = Name
		S.Position = UDim2.fromOffset(10 + Column * 118, 10 + Row * 98)
		S.Size = UDim2.fromOffset(100, 80)
		S.BorderSizePixel = Border
		S.BorderMode = Enum.BorderMode[Mode]
		S.ScrollBarThickness = Thickness
		S.CanvasSize = Canvas
		local Child = Instance.new("Frame")
		Child.Name = "Child"
		Child.Size = UDim2.fromScale(0.5, 0.5)
		Child.Parent = S
		S.Parent = Target
		table.insert(Built, { Name = Name, Frame = S, Child = Child })
		return S
	end

	local Canvases = {
		{ "B", UDim2.fromOffset(300, 300) },
		{ "V", UDim2.fromOffset(60, 300) },
		{ "H", UDim2.fromOffset(300, 50) },
	}
	for _, Border in { 0, 1, 3 } do
		for _, Mode in { "Outline", "Middle", "Inset" } do
			for _, Thickness in { 12, 0 } do
				for _, Canvas in Canvases do
					Scroller(`b{Border}_{Mode}_t{Thickness}_{Canvas[1]}`, Border, Mode, Thickness, Canvas[2])
				end
			end
		end
	end
	-- bar-need boundary: vertical bar needed; horizontal needs canvas.X >= frame - thickness (- border?)
	for _, Border in { 0, 1, 3 } do
		for _, Width in { 86, 87, 88, 89, 90 } do
			Scroller(`edge_b{Border}_w{Width}`, Border, "Outline", 12, UDim2.fromOffset(Width, 300))
		end
	end

	task.wait()
	for _, Entry in Built do
		Entry.Frame.CanvasPosition = Vector2.new(5000, 5000)
	end
	task.wait()
	task.wait()
	for _, Entry in Built do
		local S = Entry.Frame
		Results[Entry.Name] = {
			window = { S.AbsoluteWindowSize.X, S.AbsoluteWindowSize.Y },
			canvas = { S.AbsoluteCanvasSize.X, S.AbsoluteCanvasSize.Y },
			position = { S.CanvasPosition.X, S.CanvasPosition.Y },
			child = { Entry.Child.AbsolutePosition.X - S.AbsolutePosition.X, Entry.Child.AbsolutePosition.Y - S.AbsolutePosition.Y, Entry.Child.AbsoluteSize.X, Entry.Child.AbsoluteSize.Y },
		}
	end
	return Results
end
