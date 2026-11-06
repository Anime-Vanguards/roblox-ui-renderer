-- Hover-only: does VirtualInput:SendMousePosition reach GUI in edit mode? Which coordinate space?
-- Creates one temporary ScreenGui under CoreGui and destroys it before returning.
local UIS = game:GetService("UserInputService")
local CoreGui = game:GetService("CoreGui")
local HttpService = game:GetService("HttpService")

local Log = {}
local Start = os.clock()
local function Add(...)
	local Parts = { string.format("%.3f", os.clock() - Start) }
	for _, Value in { ... } do
		table.insert(Parts, tostring(Value))
	end
	table.insert(Log, table.concat(Parts, " "))
end

local Old = CoreGui:FindFirstChild("__RUR_Input")
if Old then
	Old:Destroy()
end

local Gui = Instance.new("ScreenGui")
Gui.Name = "__RUR_Input"
Gui.IgnoreGuiInset = true
Gui.DisplayOrder = 2000000000
Gui.ZIndexBehavior = Enum.ZIndexBehavior.Sibling

local Button = Instance.new("TextButton")
Button.Name = "Button"
Button.Position = UDim2.fromOffset(100, 100)
Button.Size = UDim2.fromOffset(200, 100)
Button.Parent = Gui
Gui.Parent = CoreGui

local Connections = {}
local function Watch(Signal, Name)
	table.insert(Connections, Signal:Connect(function(...)
		local Parts = { Name }
		for _, Value in { ... } do
			if typeof(Value) == "Instance" and Value:IsA("InputObject") then
				table.insert(Parts, `[{Value.UserInputType.Name} {Value.UserInputState.Name} pos={Value.Position} delta={Value.Delta} key={Value.KeyCode.Name}]`)
			else
				table.insert(Parts, tostring(Value))
			end
		end
		Add(table.unpack(Parts))
	end))
end
Watch(Button.MouseEnter, "MouseEnter")
Watch(Button.MouseLeave, "MouseLeave")
Watch(Button.MouseMoved, "MouseMoved")
Watch(Button.InputBegan, "InputBegan")
Watch(Button.InputChanged, "InputChanged")
Watch(Button.InputEnded, "InputEnded")
Watch(UIS.InputBegan, "UIS.InputBegan")
Watch(UIS.InputChanged, "UIS.InputChanged")
Watch(UIS.InputEnded, "UIS.InputEnded")
Watch(Button:GetPropertyChangedSignal("GuiState"), "GuiStateChanged")

local Before = {
	mouse = tostring(UIS:GetMouseLocation()),
	selection = #game:GetService("Selection"):Get(),
}

local Ok, Error = pcall(function()
	local Virtual = UIS:CreateVirtualInput()
	task.wait(0.05)
	Add("send 150,150")
	Virtual:SendMousePosition(Vector2.new(150, 150))
	task.wait(0.15)
	Add("state", Button.GuiState, "mouse", UIS:GetMouseLocation())
	Add("send 160,155")
	Virtual:SendMousePosition(Vector2.new(160, 155))
	task.wait(0.15)
	Add("state", Button.GuiState, "mouse", UIS:GetMouseLocation())
	Add("send 600,600")
	Virtual:SendMousePosition(Vector2.new(600, 600))
	task.wait(0.15)
	Add("state", Button.GuiState, "mouse", UIS:GetMouseLocation())
end)
if not Ok then
	Add("ERROR", Error)
end

for _, Connection in Connections do
	Connection:Disconnect()
end
Gui:Destroy()

return HttpService:JSONEncode({ before = Before, log = Log, after = { mouse = tostring(UIS:GetMouseLocation()), selection = #game:GetService("Selection"):Get() } })
