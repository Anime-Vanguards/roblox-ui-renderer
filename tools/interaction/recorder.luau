-- Studio-side input recorder (installed by tools/interaction/studio.mjs). Owns a temporary ScreenGui
-- under CoreGui, logs every input event of the watched objects and of UserInputService with frame
-- numbers, and polls watched properties every frame (logging changes only). _G.__RUR_INPUT holds it.
local UIS = game:GetService("UserInputService")
local CoreGui = game:GetService("CoreGui")
local RunService = game:GetService("RunService")
-- Studio-only services (the offline replay runs this recorder in rur, where they may not exist)
local _, Selection = pcall(function()
	return game:GetService("Selection")
end)
local _, History = pcall(function()
	return game:GetService("ChangeHistoryService")
end)
local HttpService = game:GetService("HttpService")

local Previous = _G.__RUR_INPUT
if Previous and Previous.Destroy then
	pcall(Previous.Destroy)
end
local Old = CoreGui:FindFirstChild("__RUR_Input")
if Old then
	Old:Destroy()
end

local Recorder = { Log = {}, Watched = {}, Frame = 0, Start = os.clock(), Connections = {}, Polls = {}, NextInputId = 0 }
Recorder.InputIds = setmetatable({}, { __mode = "k" })
_G.__RUR_INPUT = Recorder

local function InputId(Input: Instance): number
	local Existing = Recorder.InputIds[Input]
	if Existing then
		return Existing
	end

	Recorder.NextInputId += 1
	Recorder.InputIds[Input] = Recorder.NextInputId
	return Recorder.NextInputId
end

local function Describe(Value: any): any
	local Kind = typeof(Value)
	if Kind == "Instance" then
		if Value:IsA("InputObject") then
			return {
				input = Value.UserInputType.Name,
				state = Value.UserInputState.Name,
				key = Value.KeyCode.Name,
				pos = { Value.Position.X, Value.Position.Y, Value.Position.Z },
				delta = { Value.Delta.X, Value.Delta.Y, Value.Delta.Z },
				id = InputId(Value),
			}
		end
		return Value:GetFullName()
	elseif Kind == "Vector2" then
		return { Value.X, Value.Y }
	elseif Kind == "Vector3" then
		return { Value.X, Value.Y, Value.Z }
	elseif Kind == "EnumItem" then
		return Value.Name
	elseif Kind == "Color3" then
		return { Value.R, Value.G, Value.B }
	elseif Kind == "UDim2" then
		return { Value.X.Scale, Value.X.Offset, Value.Y.Scale, Value.Y.Offset }
	elseif Kind == "number" or Kind == "string" or Kind == "boolean" or Kind == "nil" then
		return Value
	end
	return tostring(Value)
end
Recorder.Describe = Describe

function Recorder.Add(Entry: { [string]: any })
	Entry.f = Recorder.Frame
	Entry.t = math.floor((os.clock() - Recorder.Start) * 100000 + 0.5) / 100000
	table.insert(Recorder.Log, Entry)
end

function Recorder.Note(Text: string)
	Recorder.Add({ note = Text })
end

function Recorder.Connect(Signal: RBXScriptSignal, Name: string, Owner: string?)
	table.insert(Recorder.Connections, Signal:Connect(function(...)
		local Args = {}
		for Index = 1, select("#", ...) do
			Args[Index] = Describe((select(Index, ...)))
		end
		Recorder.Add({ e = Name, o = Owner, a = Args })
	end))
end

local ObjectEvents = { "MouseEnter", "MouseLeave", "MouseMoved", "MouseWheelForward", "MouseWheelBackward", "InputBegan", "InputChanged", "InputEnded", "SelectionGained", "SelectionLost" }
local ButtonEvents = { "MouseButton1Down", "MouseButton1Up", "MouseButton1Click", "MouseButton2Down", "MouseButton2Up", "MouseButton2Click", "Activated", "SecondaryActivated" }
local BoxEvents = { "Focused", "FocusLost", "ReturnPressedFromOnScreenKeyboard" }

-- Watches `Object`: its input events plus per-frame polling of `Properties` (default GuiState)
function Recorder.Watch(Object: Instance, Name: string, Properties: { string }?, Events: { string }?)
	Recorder.Watched[Name] = Object
	for _, Event in Events or ObjectEvents do
		local Ok, Signal = pcall(function()
			return (Object :: any)[Event]
		end)
		if Ok and typeof(Signal) == "RBXScriptSignal" then
			Recorder.Connect(Signal, Event, Name)
		end
	end

	if not Events then
		if Object:IsA("GuiButton") then
			for _, Event in ButtonEvents do
				Recorder.Connect((Object :: any)[Event], Event, Name)
			end
		elseif Object:IsA("TextBox") then
			for _, Event in BoxEvents do
				Recorder.Connect((Object :: any)[Event], Event, Name)
			end
		end
	end

	table.insert(Recorder.Polls, { Object = Object, Name = Name, Properties = Properties or { "GuiState" }, Last = {} })
end

function Recorder.Poll()
	for _, Poll in Recorder.Polls do
		for _, Property in Poll.Properties do
			local Ok, Value = pcall(function()
				return (Poll.Object :: any)[Property]
			end)
			local Described = if Ok then Describe(Value) else "ERR"
			local Key = HttpService:JSONEncode({ Described })
			if Poll.Last[Property] ~= Key then
				Poll.Last[Property] = Key
				Recorder.Add({ p = Property, o = Poll.Name, v = Described })
			end
		end
	end

	local Mouse = UIS:GetMouseLocation()
	local MouseKey = `{Mouse.X},{Mouse.Y}`
	if Recorder.LastMouse ~= MouseKey then
		Recorder.LastMouse = MouseKey
		Recorder.Add({ p = "MouseLocation", v = { Mouse.X, Mouse.Y } })
	end

	local Buttons = {}
	for _, Button in UIS:GetMouseButtonsPressed() do
		table.insert(Buttons, Button.UserInputType.Name)
	end
	local ButtonsKey = table.concat(Buttons, ",")
	if Recorder.LastButtons ~= ButtonsKey then
		Recorder.LastButtons = ButtonsKey
		Recorder.Add({ p = "ButtonsPressed", v = ButtonsKey })
	end

	local Keys = {}
	for _, Key in UIS:GetKeysPressed() do
		table.insert(Keys, Key.KeyCode.Name)
	end
	local KeysKey = table.concat(Keys, ",")
	if Recorder.LastKeys ~= KeysKey then
		Recorder.LastKeys = KeysKey
		Recorder.Add({ p = "KeysPressed", v = KeysKey })
	end

	local Focused = UIS:GetFocusedTextBox()
	local FocusedName = if Focused then Focused.Name else ""
	if Recorder.LastFocused ~= FocusedName then
		Recorder.LastFocused = FocusedName
		Recorder.Add({ p = "FocusedTextBox", v = FocusedName })
	end
end

table.insert(Recorder.Connections, RunService.Heartbeat:Connect(function()
	Recorder.Frame += 1
	Recorder.Poll()
end))
for _, Name in { "InputBegan", "InputChanged", "InputEnded" } do
	Recorder.Connect((UIS :: any)[Name], "UIS." .. Name, nil)
end
Recorder.Connect(UIS.TextBoxFocused, "UIS.TextBoxFocused", nil)
Recorder.Connect(UIS.TextBoxFocusReleased, "UIS.TextBoxFocusReleased", nil)

-- Studio state that must not change (anything outside the temporary GUI)
function Recorder.Safety(): { [string]: any }
	local CanUndo, UndoName = History:GetCanUndo()
	-- Every MCP execute_luau call records an "Assistant <n>" waypoint; only other waypoints matter
	UndoName = string.gsub(tostring(UndoName), "^Assistant %d+$", "Assistant")
	local CanRedo, RedoName = History:GetCanRedo()
	local Camera = workspace.CurrentCamera
	return {
		selection = #Selection:Get(),
		undo = `{CanUndo} {UndoName}`,
		redo = `{CanRedo} {RedoName}`,
		camera = tostring(Camera.CFrame),
	}
end

local Gui = Instance.new("ScreenGui")
Gui.Name = "__RUR_Input"
Gui.IgnoreGuiInset = true
Gui.DisplayOrder = 2000000000
Gui.ZIndexBehavior = Enum.ZIndexBehavior.Sibling
Gui.ResetOnSpawn = false
Recorder.Gui = Gui

-- Keys sent through UserInputService:CreateVirtualInput() (no position, so no CoreGui check); every
-- key still held is released by Destroy.
Recorder.Held = {}
function Recorder.SendKey(KeyCode: Enum.KeyCode, Down: boolean)
	if not Recorder.Virtual then
		Recorder.Virtual = UIS:CreateVirtualInput()
	end
	Recorder.Held[KeyCode] = if Down then true else nil
	Recorder.Virtual:SendKey(Down, KeyCode, false)
end

function Recorder.SendText(Text: string)
	if not Recorder.Virtual then
		Recorder.Virtual = UIS:CreateVirtualInput()
	end
	Recorder.Virtual:SendTextInput(Text)
end

function Recorder.Destroy()
	for KeyCode in Recorder.Held do
		pcall(function()
			Recorder.Virtual:SendKey(false, KeyCode, false)
		end)
	end
	table.clear(Recorder.Held)
	pcall(function()
		local GuiService = game:GetService("GuiService")
		local Selected = GuiService.SelectedObject
		if Selected and Recorder.Gui and Selected:IsDescendantOf(Recorder.Gui) then
			GuiService.SelectedObject = nil
		end
	end)
	pcall(function()
		local Focused = UIS:GetFocusedTextBox()
		if Focused and Recorder.Gui and Focused:IsDescendantOf(Recorder.Gui) then
			Focused:ReleaseFocus()
		end
	end)
	for _, Connection in Recorder.Connections do
		Connection:Disconnect()
	end
	table.clear(Recorder.Connections)
	if Recorder.Gui then
		Recorder.Gui:Destroy()
	end
	if _G.__RUR_INPUT == Recorder then
		_G.__RUR_INPUT = nil
	end
end

return Recorder
