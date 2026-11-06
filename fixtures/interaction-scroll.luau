-- Wheel scrolling. Studio reference: tools/interaction/experiments/wheel2.mjs (S1:
-- 300x300 frame, CanvasSize 300x3000; Outer/Inner nesting). The label logs CanvasPosition.Y changes.
--   node cli/rur.mjs render --fixture fixtures/interaction-scroll.luau --viewport 900x600 --time 1
--     --actions '[{"t":0.2,"type":"wheel","notches":-1,"x":200,"y":200}]'
return function(Target)
	local function Scroller(Name, X, Y, W, H, CW, CH, Parent)
		local S = Instance.new("ScrollingFrame")
		S.Name = Name
		S.Position = UDim2.fromOffset(X, Y)
		S.Size = UDim2.fromOffset(W, H)
		S.CanvasSize = UDim2.fromOffset(CW, CH)
		S.Parent = Parent or Target
		return S
	end
	local S1 = Scroller("S1", 50, 50, 300, 300, 300, 3000)
	for Index = 0, 29 do
		local Row = Instance.new("TextLabel")
		Row.Name = "Row" .. Index
		Row.Position = UDim2.fromOffset(0, Index * 100)
		Row.Size = UDim2.new(1, -20, 0, 90)
		Row.Text = "Row " .. Index
		Row.TextSize = 24
		Row.Parent = S1
	end
	local Outer = Scroller("Outer", 450, 50, 400, 400, 400, 4000)
	Scroller("Inner", 20, 20, 200, 200, 200, 400, Outer)

	local Log = Instance.new("TextLabel")
	Log.Name = "Log"
	Log.Position = UDim2.fromOffset(50, 500)
	Log.Size = UDim2.fromOffset(800, 60)
	Log.BackgroundTransparency = 1
	Log.TextColor3 = Color3.new(1, 1, 1)
	Log.TextSize = 14
	Log.TextWrapped = true
	Log.Text = ""
	Log.Parent = Target
	local Values = {}
	S1:GetPropertyChangedSignal("CanvasPosition"):Connect(function()
		table.insert(Values, string.format("%.1f", S1.CanvasPosition.Y))
		Log.Text = table.concat(Values, " ")
	end)
end
