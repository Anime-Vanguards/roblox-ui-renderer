-- Engine probe (input port, phase 5): logs every input-visible event with warn() so the differential
-- harness compares their order and arguments between the Luau runtime and the Rust engine. Without
-- input (the one-shot --probes run) it only logs the static answers. Live script:
--   node tools/engine/golden.mjs --corpus live --filter input (with tools/engine/probes/input.events.json)
-- Layout (logical px, a 1200x900 viewport): buttons A (50,50) B (300,50), inactive frame F with
-- button C (550,50), scrolling frames S1 (50,200) with S2 inside, TextBoxes T (450,200) and M
-- (450,300, MultiLine), Draggable D (900,50), selected button E (450,500), Interactable = false N
-- with button K (900,200).
local UserInputService = game:GetService("UserInputService")
local GuiService = game:GetService("GuiService")
local CoreGui = game:GetService("CoreGui")

local function Num(Value)
	return string.format("%.4f", Value)
end

local function V(Value)
	if typeof(Value) == "Vector3" then
		return `({Num(Value.X)},{Num(Value.Y)},{Num(Value.Z)})`
	elseif typeof(Value) == "Vector2" then
		return `({Num(Value.X)},{Num(Value.Y)})`
	elseif typeof(Value) == "UDim2" then
		return `({Num(Value.X.Scale)},{Value.X.Offset},{Num(Value.Y.Scale)},{Value.Y.Offset})`
	end
	return tostring(Value)
end

local function InputText(Input)
	if typeof(Input) ~= "Instance" then
		return tostring(Input)
	end
	return `{Input.UserInputType.Name}/{Input.UserInputState.Name}/{Input.KeyCode.Name}@{V(Input.Position)}d{V(Input.Delta)}`
end

local function Log(...)
	local Parts = {}
	for Index = 1, select("#", ...) do
		table.insert(Parts, tostring((select(Index, ...))))
	end
	warn(table.concat(Parts, " "))
end

return function(Target)
	local function Make(Class, Name, Parent, X, Y, W, H)
		local Object = Instance.new(Class)
		Object.Name = Name
		Object.Position = UDim2.fromOffset(X, Y)
		Object.Size = UDim2.fromOffset(W, H)
		Object.BorderSizePixel = 0
		Object.Parent = Parent
		return Object
	end

	local Watched, GuiStateChanges, Extra = {}, {}, {}
	local function States()
		local Parts = {}
		for _, Get in Extra do
			table.insert(Parts, Get())
		end
		for _, Object in Watched do
			table.insert(Parts, `{Object.Name}={Object.GuiState.Name}/{GuiStateChanges[Object]}`)
		end
		return table.concat(Parts, " ")
	end

	local function Watch(Object, Detail)
		local Name = Object.Name
		Object.MouseEnter:Connect(function(X, Y) Log(Name, "MouseEnter", X, Y) end)
		Object.MouseLeave:Connect(function(X, Y) Log(Name, "MouseLeave", X, Y) end)
		if Detail then
			Object.MouseMoved:Connect(function(X, Y) Log(Name, "MouseMoved", X, Y) end)
		end
		Object.InputBegan:Connect(function(Input) Log(Name, "InputBegan", InputText(Input)) end)
		Object.InputChanged:Connect(function(Input) Log(Name, "InputChanged", InputText(Input)) end)
		Object.InputEnded:Connect(function(Input) Log(Name, "InputEnded", InputText(Input)) end)
		-- (GuiState changes of one event fire in the Luau runtime's hash order of its Touched table,
		-- which differs from run to run: the states and change counts are logged per UIS event)
		table.insert(Watched, Object)
		GuiStateChanges[Object] = 0
		Object:GetPropertyChangedSignal("GuiState"):Connect(function() GuiStateChanges[Object] += 1 end)
		if Object:IsA("GuiButton") then
			for _, Event in { "MouseButton1Down", "MouseButton1Up", "MouseButton2Down", "MouseButton2Up" } do
				Object[Event]:Connect(function(X, Y) Log(Name, Event, X, Y) end)
			end
			Object.MouseButton1Click:Connect(function() Log(Name, "MouseButton1Click") end)
			Object.MouseButton2Click:Connect(function() Log(Name, "MouseButton2Click") end)
			Object.Activated:Connect(function(Input, Count) Log(Name, "Activated", InputText(Input), Count) end)
			Object.SecondaryActivated:Connect(function(Input) Log(Name, "SecondaryActivated", InputText(Input)) end)
		end
		Object.MouseWheelForward:Connect(function(X, Y) Log(Name, "MouseWheelForward", X, Y) end)
		Object.MouseWheelBackward:Connect(function(X, Y) Log(Name, "MouseWheelBackward", X, Y) end)
	end

	local A = Make("TextButton", "A", Target, 50, 50, 200, 80)
	A.Text = "A"
	A.BackgroundColor3 = Color3.fromRGB(200, 120, 40)
	Watch(A, true)
	local AChild = Make("Frame", "AChild", A, 150, 40, 80, 60)
	AChild.BackgroundColor3 = Color3.fromRGB(40, 120, 200)
	Watch(AChild)
	local B = Make("ImageButton", "B", Target, 300, 50, 200, 80)
	B.Image = "rbxasset://textures/ui/GuiImagePlaceholder.png"
	B.HoverImage = "rbxasset://textures/ui/LuaChat/icons/ic-back.png"
	B.PressedImage = "rbxasset://textures/ui/LuaChat/icons/ic-close.png"
	B.BackgroundColor3 = Color3.fromRGB(145, 85, 115)
	Watch(B)
	local F = Make("Frame", "F", Target, 550, 50, 200, 80)
	F.Active = false
	Watch(F)
	local C = Make("TextButton", "C", F, 20, 20, 100, 40)
	C.Text = "C"
	C.AutoButtonColor = false
	Watch(C)

	local S1 = Make("ScrollingFrame", "S1", Target, 50, 200, 300, 300)
	S1.CanvasSize = UDim2.fromOffset(300, 1500)
	S1.ScrollBarThickness = 12
	Watch(S1)
	S1:GetPropertyChangedSignal("CanvasPosition"):Connect(function() Log("S1", "CanvasPosition", V(S1.CanvasPosition)) end)
	for Index = 1, 12 do
		local Row = Make("TextButton", `Row{Index}`, S1, 0, 250 + Index * 90, 260, 80)
		Row.Text = `Row {Index}`
		Row.Activated:Connect(function() Log(Row.Name, "Activated") end)
	end
	local S2 = Make("ScrollingFrame", "S2", S1, 20, 60, 200, 150)
	S2.CanvasSize = UDim2.fromOffset(200, 600)
	Watch(S2)
	-- (S1 and S2 springs that run in the same frame write in the Luau runtime's hash order of its
	-- Motions table: S2's moves are counted and shown with the states)
	local S2Moves = 0
	S2:GetPropertyChangedSignal("CanvasPosition"):Connect(function() S2Moves += 1 end)
	table.insert(Extra, function() return `S2={V(S2.CanvasPosition)}/{S2Moves}` end)

	local T = Make("TextBox", "T", Target, 450, 200, 400, 60)
	T.Font = Enum.Font.SourceSans
	T.TextSize = 32
	T.TextXAlignment = Enum.TextXAlignment.Left
	T.ClearTextOnFocus = false
	T.Text = "hello world, again"
	Watch(T)
	local M = Make("TextBox", "M", Target, 450, 300, 400, 150)
	M.Font = Enum.Font.SourceSans
	M.TextSize = 24
	M.MultiLine = true
	M.TextWrapped = true
	M.TextXAlignment = Enum.TextXAlignment.Left
	M.TextYAlignment = Enum.TextYAlignment.Top
	M.Text = ""
	M.PlaceholderText = "type here"
	Watch(M)
	for _, Box in { T, M } do
		local Name = Box.Name
		Box.Focused:Connect(function() Log(Name, "Focused", Box.CursorPosition, Box.SelectionStart) end)
		Box.FocusLost:Connect(function(Enter, Input) Log(Name, "FocusLost", Enter, InputText(Input), Box:IsFocused()) end)
		Box:GetPropertyChangedSignal("Text"):Connect(function() Log(Name, "Text", Box.Text) end)
		Box:GetPropertyChangedSignal("CursorPosition"):Connect(function() Log(Name, "CursorPosition", Box.CursorPosition) end)
		Box:GetPropertyChangedSignal("SelectionStart"):Connect(function() Log(Name, "SelectionStart", Box.SelectionStart) end)
	end

	local D = Make("Frame", "D", Target, 900, 50, 100, 100)
	D.Active = true
	D.Draggable = true
	Watch(D)
	D.DragBegin:Connect(function(Start) Log("D", "DragBegin", V(Start)) end)
	D.DragStopped:Connect(function(X, Y) Log("D", "DragStopped", X, Y) end)
	D:GetPropertyChangedSignal("Position"):Connect(function() Log("D", "Position", V(D.Position)) end)

	local E = Make("TextButton", "E", Target, 450, 500, 200, 60)
	E.Text = "E"
	Watch(E)
	E.SelectionGained:Connect(function() Log("E", "SelectionGained") end)
	E.SelectionLost:Connect(function() Log("E", "SelectionLost") end)

	local N = Make("Frame", "N", Target, 900, 200, 200, 200)
	N.Interactable = false
	Watch(N)
	local K = Make("TextButton", "K", N, 20, 20, 100, 60)
	Watch(K)

	UserInputService.InputBegan:Connect(function(Input, Processed) Log("UIS", "InputBegan", InputText(Input), Processed, States()) end)
	UserInputService.InputChanged:Connect(function(Input, Processed) Log("UIS", "InputChanged", InputText(Input), Processed, States()) end)
	UserInputService.InputEnded:Connect(function(Input, Processed)
		local Focused = UserInputService:GetFocusedTextBox()
		Log("UIS", "InputEnded", InputText(Input), Processed, States(), V(UserInputService:GetMouseLocation()),
			UserInputService:IsKeyDown(Enum.KeyCode.LeftShift), UserInputService:IsMouseButtonPressed(Enum.UserInputType.MouseButton1),
			Focused and Focused.Name, V(UserInputService:GetMouseDelta()), #UserInputService:GetKeysPressed(),
			#UserInputService:GetMouseButtonsPressed(), Input:IsModifierKeyDown(Enum.ModifierKey.Shift))
		if Input.UserInputType == Enum.UserInputType.MouseButton1 then
			local Names = {}
			for _, Object in CoreGui:GetGuiObjectsAtPosition(Input.Position.X, Input.Position.Y) do
				table.insert(Names, Object.Name)
			end
			Log("UIS", "ObjectsAt", table.concat(Names, ","))
		end
	end)
	UserInputService.LastInputTypeChanged:Connect(function(Type) Log("UIS", "LastInputTypeChanged", Type.Name) end)
	UserInputService.TextBoxFocused:Connect(function(Box) Log("UIS", "TextBoxFocused", Box.Name) end)
	UserInputService.TextBoxFocusReleased:Connect(function(Box) Log("UIS", "TextBoxFocusReleased", Box.Name) end)

	local Player = game:GetService("Players").LocalPlayer
	if Player then
		local Mouse = Player:GetMouse()
		Mouse.Button1Down:Connect(function() Log("Mouse", "Button1Down", Mouse.X, Mouse.Y) end)
		Mouse.Button1Up:Connect(function() Log("Mouse", "Button1Up", Mouse.X, Mouse.Y) end)
		Mouse.WheelBackward:Connect(function() Log("Mouse", "WheelBackward", Mouse.X, Mouse.Y) end)
		Mouse.KeyDown:Connect(function(Key) Log("Mouse", "KeyDown", Key) end)
	end

	-- Scripted input: the E button is selected (Return activates it); VirtualInput at t = 1.2
	GuiService.SelectedObject = E
	Log("static", UserInputService:GetLastInputType().Name, V(UserInputService:GetMouseLocation()), A.GuiState.Name, K.GuiState.Name,
		S1:GetScrollVelocity(), UserInputService:GetFocusedTextBox())
	task.delay(1.2, function()
		local Virtual = UserInputService:CreateVirtualInput()
		Log("virtual", tostring(Virtual))
		Virtual:SendMousePosition(Vector2.new(100, 90))
		Virtual:SendMouseButton(Vector2.new(100, 90), Enum.UserInputType.MouseButton1, true, 1)
		Virtual:SendMouseButton(Vector2.new(100, 90), Enum.UserInputType.MouseButton1, false, 1)
		Virtual:SendPointerAction(Vector2.new(150, 300), { Wheel = -2 })
		Log("virtual done", V(UserInputService:GetMouseLocation()))
	end)
	task.delay(1.35, function()
		S1:ScrollToTop()
		N.Interactable = true
		Log("K after Interactable", K.GuiState.Name)
		T:CaptureFocus()
		T.CursorPosition = 100
		Log("T captured", T.CursorPosition, T:IsFocused())
		T.Text = "short"
		Log("T text", T.CursorPosition, T.SelectionStart)
		M:CaptureFocus()
		T:ReleaseFocus(true)
		M:ReleaseFocus()
		GuiService.SelectedObject = nil
	end)
end
