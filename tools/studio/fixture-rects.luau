-- Rect rasterization probe: white frames at fractional absolute positions/sizes on black.
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

local Holder = Instance.new("Frame")
Holder.Position = UDim2.fromOffset(20, 20)
Holder.Size = UDim2.fromOffset(1000, 600)
Holder.BackgroundTransparency = 1
Holder.Parent = Background

local Results = {}
for Index = 0, 23 do
	local Frame = Instance.new("Frame")
	Frame.Name = `R{Index}`
	local Column = Index % 6
	local Row = Index // 6
	-- positions/sizes with scale components produce arbitrary fractional absolute values
	Frame.Position = UDim2.new(Column * 0.15 + Index * 0.00137, 0, Row * 0.2 + Index * 0.00291, 0)
	Frame.Size = UDim2.new(0.05 + Index * 0.00173, 0, 0.1 + Index * 0.00211, 0)
	Frame.BackgroundColor3 = Color3.new(1, 1, 1)
	Frame.BorderSizePixel = 0
	Frame.Parent = Holder
	table.insert(Results, Frame)
end

Gui.Parent = CoreGui
task.wait(0.3)
local Out = {}
for _, Frame in Results do
	table.insert(Out, { name = Frame.Name, x = Frame.AbsolutePosition.X, y = Frame.AbsolutePosition.Y, w = Frame.AbsoluteSize.X, h = Frame.AbsoluteSize.Y })
end
return HttpService:JSONEncode({ viewport = { workspace.CurrentCamera.ViewportSize.X, workspace.CurrentCamera.ViewportSize.Y }, frames = Out })
