-- Script writes of CanvasPosition beyond the range: where does Studio clamp? (temporary CoreGui GUI)
local CoreGui = game:GetService("CoreGui")
local HttpService = game:GetService("HttpService")
local Gui = Instance.new("ScreenGui")
Gui.Name = "__RUR_Input"
Gui.IgnoreGuiInset = true
local Out = {}
local function Scroller(Name, W, H, CW, CH, Props)
	local S = Instance.new("ScrollingFrame")
	S.Name = Name
	S.Size = UDim2.fromOffset(W, H)
	S.CanvasSize = UDim2.fromOffset(CW, CH)
	for Key, Value in Props or {} do
		S[Key] = Value
	end
	S.Parent = Gui
	return S
end
local A = Scroller("A", 200, 200, 200, 400)
local B = Scroller("B", 300, 300, 3000, 300)
local C = Scroller("C", 300, 300, 300, 3000)
Gui.Parent = CoreGui
task.wait()
for _, S in { A, B, C } do
	S.CanvasPosition = Vector2.new(5000, 5000)
end
local Immediate = {}
for _, S in { A, B, C } do
	Immediate[S.Name] = { S.CanvasPosition.X, S.CanvasPosition.Y }
end
task.wait(0.1)
for _, S in { A, B, C } do
	Out[S.Name] = {
		immediate = Immediate[S.Name],
		after = { S.CanvasPosition.X, S.CanvasPosition.Y },
		canvas = { S.AbsoluteCanvasSize.X, S.AbsoluteCanvasSize.Y },
		window = { S.AbsoluteWindowSize.X, S.AbsoluteWindowSize.Y },
	}
end
Gui:Destroy()
return HttpService:JSONEncode(Out)
