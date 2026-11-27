-- A touch device's own scroll bars (compare with --device touch): ScrollingFrames whose CanvasPosition a script wrote
-- show their bars for a second (then they fade), frames nothing scrolled show none, as a 0.747 touch client draws them
-- (notes/interaction.md "Touch scroll bars"). Top row: written frames over canvases of 400 to 10000 px; middle row: the
-- same frames never written (no bars on a touch device; the mouse's bars elsewhere); bottom row: written frames with
-- ScrollBarImageColor3, ScrollBarImageTransparency, thicknesses 4 and 30, the bar on the left, a border, a horizontal
-- frame and an idle bordered one. On other devices every frame shows the mouse's bars.
-- compare: --viewport 800x600 --time 0.3 --background 20,20,26 --device touch
return function(Target)
	local function Frame(Name, X, Y, W, H, CanvasX, CanvasY, Props)
		local S = Instance.new("ScrollingFrame")
		S.Name = Name
		S.Position = UDim2.fromOffset(X, Y)
		S.Size = UDim2.fromOffset(W, H)
		S.CanvasSize = UDim2.fromOffset(CanvasX, CanvasY)
		S.BackgroundColor3 = Color3.fromRGB(60, 60, 72)
		S.BorderSizePixel = 0
		for Key, Value in Props or {} do
			S[Key] = Value
		end
		S.Parent = Target
		return S
	end
	local Written = {}
	for Index, Canvas in { 400, 600, 800, 1000, 1500, 3000, 6000, 10000 } do
		local S = Frame("Top" .. Index, 10 + (Index - 1) * 95, 10, 85, 180, 0, Canvas * 0.6)
		table.insert(Written, { S, Vector2.new(0, math.floor((Canvas * 0.6 - 180) * 0.3)) })
		Frame("Idle" .. Index, 10 + (Index - 1) * 95, 205, 85, 180, 0, Canvas * 0.6)
	end
	local Bottom = {
		{ ScrollBarImageColor3 = Color3.fromRGB(255, 60, 60) },
		{ ScrollBarImageTransparency = 0.5 },
		{ ScrollBarThickness = 4 },
		{ ScrollBarThickness = 30 },
		{ VerticalScrollBarPosition = Enum.VerticalScrollBarPosition.Left },
		{ BorderSizePixel = 3, BorderColor3 = Color3.fromRGB(200, 120, 0) },
	}
	for Index, Props in Bottom do
		local S = Frame("Bottom" .. Index, 10 + (Index - 1) * 95, 400, 85, 180, 0, 800, Props)
		table.insert(Written, { S, Vector2.new(0, 200) })
	end
	local Wide = Frame("Wide", 580, 400, 210, 85, 900, 0)
	table.insert(Written, { Wide, Vector2.new(250, 0) })
	Frame("IdleBorder", 580, 495, 210, 85, 900, 0, { BorderSizePixel = 3, BorderColor3 = Color3.fromRGB(200, 120, 0) })
	task.defer(function()
		for _, Pair in Written do
			Pair[1].CanvasPosition = Pair[2]
		end
	end)
end
