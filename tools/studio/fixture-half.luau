local HttpService = game:GetService("HttpService")
local CoreGui = game:GetService("CoreGui")
local Old = CoreGui:FindFirstChild("__RUR_Fixture")
if Old then
	Old:Destroy()
end
local Gui = Instance.new("ScreenGui")
Gui.Name = "__RUR_Fixture"
Gui.IgnoreGuiInset = true
Gui.DisplayOrder = 1000000
local Background = Instance.new("Frame")
Background.Size = UDim2.fromScale(1, 1)
Background.BackgroundColor3 = Color3.new(0, 0, 0)
Background.BorderSizePixel = 0
Background.Parent = Gui
local Out = {}
for Index = 0, 7 do
	local Frame = Instance.new("Frame")
	Frame.Position = UDim2.fromOffset(11 + Index * 40, 11 + Index * 2)
	Frame.Size = UDim2.fromOffset(21 + Index, 13 + Index)
	Frame.BackgroundColor3 = Color3.new(1, 1, 1)
	Frame.BorderSizePixel = 0
	Frame.Parent = Background
end
Gui.Parent = CoreGui
task.wait(0.3)
for _, Frame in Background:GetChildren() do
	table.insert(Out, { x = Frame.AbsolutePosition.X, y = Frame.AbsolutePosition.Y, w = Frame.AbsoluteSize.X, h = Frame.AbsoluteSize.Y })
end
return HttpService:JSONEncode({ frames = Out })
