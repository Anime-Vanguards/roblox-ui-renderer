-- Hover-only: which positions does VirtualInput accept? No GUI of ours; then a non-Active Frame,
-- an Active Frame and a TextButton each under a separate point.
local UIS = game:GetService("UserInputService")
local CoreGui = game:GetService("CoreGui")
local HttpService = game:GetService("HttpService")
local Result = { before = tostring(UIS:GetMouseLocation()), tries = {} }

local Virtual = UIS:CreateVirtualInput()
local function Try(Label, X, Y)
	local Ok, Error = pcall(function()
		Virtual:SendMousePosition(Vector2.new(X, Y))
	end)
	task.wait(0.1)
	table.insert(Result.tries, { label = Label, ok = Ok, error = if Ok then nil else tostring(Error), mouse = tostring(UIS:GetMouseLocation()) })
end

Try("empty", 600, 600)

local Old = CoreGui:FindFirstChild("__RUR_Input")
if Old then
	Old:Destroy()
end
local Gui = Instance.new("ScreenGui")
Gui.Name = "__RUR_Input"
Gui.IgnoreGuiInset = true
Gui.DisplayOrder = 2000000000
local function Make(Class, X, Active)
	local Object = Instance.new(Class)
	Object.Position = UDim2.fromOffset(X, 100)
	Object.Size = UDim2.fromOffset(100, 100)
	Object.Active = Active
	Object.Parent = Gui
	return Object
end
Make("Frame", 100, false)
Make("Frame", 300, true)
Make("TextButton", 500, true)
Make("TextLabel", 700, true)
Gui.Parent = CoreGui
task.wait(0.05)

Try("inactive frame", 150, 150)
Try("active frame", 350, 150)
Try("text button", 550, 150)
Try("active label", 750, 150)
Gui.Enabled = false
task.wait(0.05)
Try("disabled gui over button", 550, 150)
Gui:Destroy()
Result.after = tostring(UIS:GetMouseLocation())
return HttpService:JSONEncode(Result)
