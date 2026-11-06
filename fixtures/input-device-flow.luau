-- Controller and touch input through UserInputService:CreateVirtualInput(): PreferredInput, the glyph
-- lookup, ContextActionService priority / Pass / Sink, a sunk ButtonA not activating the selection,
-- GuiService:Select picking a selectable descendant, and a finger's one Touch InputObject.
return function(Target)
	local Input = game:GetService("UserInputService")
	local Actions = game:GetService("ContextActionService")
	local Selection = game:GetService("GuiService")
	local Virtual = Input:CreateVirtualInput()
	local Log = {}

	local Parent = Instance.new("Frame")
	Parent.Size = UDim2.fromOffset(200, 100)
	Parent.Parent = Target
	local First = Instance.new("TextButton")
	First.Name = "First"
	First.Size = UDim2.fromOffset(100, 50)
	First.Parent = Parent
	First.Activated:Connect(function()
		table.insert(Log, "activated")
	end)

	Actions:BindActionAtPriority("Low", function(_, State)
		table.insert(Log, `low:{State.Name}`)
		return Enum.ContextActionResult.Pass
	end, false, 100, Enum.KeyCode.ButtonX, Enum.KeyCode.ButtonA)
	Actions:BindActionAtPriority("High", function(_, State, Object)
		table.insert(Log, `high:{State.Name}:{Object.UserInputType.Name}`)
		return Enum.ContextActionResult.Pass
	end, false, 200, Enum.KeyCode.ButtonX)
	Actions:BindActionAtPriority("Sinks", function()
		table.insert(Log, "sinks")
		return Enum.ContextActionResult.Sink
	end, false, 300, Enum.KeyCode.ButtonY)
	Actions:BindActionAtPriority("Under", function()
		table.insert(Log, "under")
		return Enum.ContextActionResult.Pass
	end, false, 100, Enum.KeyCode.ButtonY)

	Selection:Select(Parent)
	table.insert(Log, `selected={Selection.SelectedObject and Selection.SelectedObject.Name}`)

	local Touches = {}
	Input.InputBegan:Connect(function(Object, Processed)
		if Object.KeyCode == Enum.KeyCode.ButtonY then
			table.insert(Log, `processed={Processed}`)
		elseif Object.KeyCode == Enum.KeyCode.ButtonX then
			table.insert(Log, `down={Input:IsGamepadButtonDown(Enum.UserInputType.Gamepad1, Enum.KeyCode.ButtonX)}`)
		elseif Object.UserInputType == Enum.UserInputType.Touch then
			Touches.Began = Object
		end
	end)
	Input.InputChanged:Connect(function(Object)
		if Object.UserInputType == Enum.UserInputType.Touch then
			Touches.Same = Object == Touches.Began
		end
	end)

	Virtual:SendKey(true, "ButtonX")
	Virtual:SendKey(false, "ButtonX")
	Virtual:SendKey(true, "ButtonY")
	Virtual:SendKey(false, "ButtonY")
	Virtual:SendKey(true, "ButtonA")
	Virtual:SendKey(false, "ButtonA")
	Actions:UnbindAction("Low")
	Virtual:SendKey(true, "ButtonA")
	Virtual:SendKey(false, "ButtonA")

	Virtual:SendMouseButton(Vector2.new(50, 50), "MouseButton1", true)
	Virtual:SendMousePosition(Vector2.new(70, 50))
	task.wait(0.05)
	Virtual:SendMouseButton(Vector2.new(70, 50), "MouseButton1", false)
	table.insert(Log, `touch={Touches.Began and Touches.Began.UserInputType.Name} same={Touches.Same}`)

	local Label = Instance.new("TextLabel")
	Label.Name = "Probe"
	Label.Size = UDim2.fromOffset(380, 40)
	Label.Text = `preferred={Input.PreferredInput.Name} glyph={Input:GetImageForKeyCode(Enum.KeyCode.ButtonR2)} {table.concat(Log, " ")}`
	Label.Parent = Target
end
