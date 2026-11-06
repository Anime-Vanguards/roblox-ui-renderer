-- Engine API micro-benchmark (a fixture; run with RUR_REAL_CLOCK=1 so os.realclock exists):
--   node tools/engine/bench/compare.mjs [--runs 5] [--before <another rur-native build>]
--   RUR_REAL_CLOCK=1 node cli/rur.mjs render --fixture tools/engine/bench/api.luau --no-png --out out/bench
-- Prints nanoseconds per operation for the hot script-facing paths (property reads / writes of the
-- common value types, method calls, Instance.new, parenting, signals, userdata identity).
-- Each row subtracts the cost of an empty loop of the same count.
return function(Target)
	local Clock = os.realclock
	if not Clock then
		print("set RUR_REAL_CLOCK=1")
		return
	end
	local Empty = 0
	local function Time(Count, Callback)
		local Best = math.huge
		-- best of 3 (the machine is often busy)
		for _ = 1, 3 do
			local Start = Clock()
			Callback(Count)
			local Elapsed = Clock() - Start
			if Elapsed < Best then
				Best = Elapsed
			end
		end
		return Best / Count * 1e9
	end
	local function Measure(Label, Count, Callback)
		local PerOp = Time(Count, Callback) - Empty
		print(string.format("%-34s %8.1f ns/op", Label, PerOp))
	end
	Empty = Time(200000, function(Count)
		local Value
		for Index = 1, Count do
			Value = Index
		end
		return Value
	end)

	local Frame = Instance.new("Frame")
	Frame.Parent = Target
	local Label = Instance.new("TextLabel")
	Label.Parent = Frame
	local Other = Instance.new("Frame")
	Other.Parent = Target
	local Holder = Instance.new("Folder")
	Holder.Parent = Target
	for Index = 1, 10 do
		local Child = Instance.new("Frame")
		Child.Name = "Child" .. Index
		Child.Parent = Holder
	end
	Label:SetAttribute("Score", 5)

	local N = 200000

	-- Reads
	Measure("read Size (UDim2)", N, function(Count)
		local Value
		for _ = 1, Count do
			Value = Frame.Size
		end
		return Value
	end)
	Measure("read BackgroundTransparency", N, function(Count)
		local Value
		for _ = 1, Count do
			Value = Frame.BackgroundTransparency
		end
		return Value
	end)
	Measure("read Visible (bool)", N, function(Count)
		local Value
		for _ = 1, Count do
			Value = Frame.Visible
		end
		return Value
	end)
	Measure("read BackgroundColor3", N, function(Count)
		local Value
		for _ = 1, Count do
			Value = Frame.BackgroundColor3
		end
		return Value
	end)
	Measure("read AutomaticSize (enum)", N, function(Count)
		local Value
		for _ = 1, Count do
			Value = Frame.AutomaticSize
		end
		return Value
	end)
	Measure("read Text (string)", N, function(Count)
		local Value
		for _ = 1, Count do
			Value = Label.Text
		end
		return Value
	end)
	Measure("read Name", N, function(Count)
		local Value
		for _ = 1, Count do
			Value = Frame.Name
		end
		return Value
	end)
	Measure("read Parent", N, function(Count)
		local Value
		for _ = 1, Count do
			Value = Label.Parent
		end
		return Value
	end)
	Measure("read ClassName", N, function(Count)
		local Value
		for _ = 1, Count do
			Value = Label.ClassName
		end
		return Value
	end)
	Measure("read child by name", N, function(Count)
		local Value
		for _ = 1, Count do
			Value = Frame.TextLabel
		end
		return Value
	end)
	Measure("read event (Changed)", N, function(Count)
		local Value
		for _ = 1, Count do
			Value = Frame.Changed
		end
		return Value
	end)

	-- Writes
	local Sizes = { UDim2.fromOffset(10, 10), UDim2.fromOffset(20, 20) }
	Measure("write Size (UDim2)", N, function(Count)
		for Index = 1, Count do
			Frame.Size = Sizes[Index % 2 + 1]
		end
	end)
	Measure("write BackgroundTransparency", N, function(Count)
		for Index = 1, Count do
			Frame.BackgroundTransparency = Index % 2
		end
	end)
	Measure("write Visible (bool)", N, function(Count)
		for Index = 1, Count do
			Frame.Visible = Index % 2 == 0
		end
	end)
	local Colors = { Color3.new(1, 0, 0), Color3.new(0, 1, 0) }
	Measure("write BackgroundColor3", N, function(Count)
		for Index = 1, Count do
			Frame.BackgroundColor3 = Colors[Index % 2 + 1]
		end
	end)
	local Automatic = { Enum.AutomaticSize.X, Enum.AutomaticSize.None }
	Measure("write AutomaticSize (enum)", N, function(Count)
		for Index = 1, Count do
			Frame.AutomaticSize = Automatic[Index % 2 + 1]
		end
	end)
	local Texts = { "Hello", "World" }
	Measure("write Text (string)", N, function(Count)
		for Index = 1, Count do
			Label.Text = Texts[Index % 2 + 1]
		end
	end)
	Measure("write Name", N, function(Count)
		for Index = 1, Count do
			Other.Name = Texts[Index % 2 + 1]
		end
	end)
	local Parents = { Frame, Other }
	Measure("write Parent (reparent)", 20000, function(Count)
		for Index = 1, Count do
			Label.Parent = Parents[Index % 2 + 1]
		end
	end)
	Label.Parent = Frame

	-- Methods
	Measure("FindFirstChild (hit)", N, function(Count)
		local Value
		for _ = 1, Count do
			Value = Frame:FindFirstChild("TextLabel")
		end
		return Value
	end)
	Measure("FindFirstChild (miss)", N, function(Count)
		local Value
		for _ = 1, Count do
			Value = Frame:FindFirstChild("Missing")
		end
		return Value
	end)
	Measure("IsA", N, function(Count)
		local Value
		for _ = 1, Count do
			Value = Label:IsA("GuiObject")
		end
		return Value
	end)
	Measure("GetChildren (10)", 50000, function(Count)
		local Value
		for _ = 1, Count do
			Value = Holder:GetChildren()
		end
		return Value
	end)
	Measure("FindFirstChildOfClass", N, function(Count)
		local Value
		for _ = 1, Count do
			Value = Frame:FindFirstChildOfClass("TextLabel")
		end
		return Value
	end)
	Measure("FindFirstChildWhichIsA", N, function(Count)
		local Value
		for _ = 1, Count do
			Value = Frame:FindFirstChildWhichIsA("GuiObject")
		end
		return Value
	end)
	Measure("IsDescendantOf", N, function(Count)
		local Value
		for _ = 1, Count do
			Value = Label:IsDescendantOf(Target)
		end
		return Value
	end)
	Measure("GetAttribute", N, function(Count)
		local Value
		for _ = 1, Count do
			Value = Label:GetAttribute("Score")
		end
		return Value
	end)
	Measure("SetAttribute", N, function(Count)
		for Index = 1, Count do
			Label:SetAttribute("Score", Index % 2)
		end
	end)
	Measure("GetPropertyChangedSignal", N, function(Count)
		local Value
		for _ = 1, Count do
			Value = Label:GetPropertyChangedSignal("Text")
		end
		return Value
	end)
	Measure("GetFullName", N, function(Count)
		local Value
		for _ = 1, Count do
			Value = Label:GetFullName()
		end
		return Value
	end)

	-- Creation
	Measure("Instance.new", 20000, function(Count)
		local Value
		for _ = 1, Count do
			Value = Instance.new("Frame")
		end
		return Value
	end)
	Measure("Instance.new + Parent", 20000, function(Count)
		for _ = 1, Count do
			local Child = Instance.new("Frame")
			Child.Parent = Other
		end
	end)
	Other:ClearAllChildren()
	Measure("Instance.new + Parent + Destroy", 20000, function(Count)
		for _ = 1, Count do
			local Child = Instance.new("Frame")
			Child.Parent = Other
			Child:Destroy()
		end
	end)

	-- Signals
	Measure("Changed:Connect", 20000, function(Count)
		for _ = 1, Count do
			Label.Changed:Connect(function() end)
		end
	end)
	local Signal = Frame:GetPropertyChangedSignal("Position")
	Measure("Connect + Disconnect", 20000, function(Count)
		for _ = 1, Count do
			Signal:Connect(function() end):Disconnect()
		end
	end)
	local Bindable = Instance.new("BindableEvent")
	Measure("BindableEvent:Fire (no handler)", N, function(Count)
		for _ = 1, Count do
			Bindable:Fire(1)
		end
	end)
	Bindable.Event:Connect(function() end)
	Measure("BindableEvent:Fire (1 handler)", 20000, function(Count)
		for _ = 1, Count do
			Bindable:Fire(1)
		end
	end)

	-- Identity
	local Same = Frame.TextLabel
	Measure("userdata == (same instance)", N, function(Count)
		local Value
		for _ = 1, Count do
			Value = Same == Label
		end
		return Value
	end)
	local Keys = { [Label] = true }
	Measure("table key lookup (instance)", N, function(Count)
		local Value
		for _ = 1, Count do
			Value = Keys[Label]
		end
		return Value
	end)
	Measure("typeof(instance)", N, function(Count)
		local Value
		for _ = 1, Count do
			Value = typeof(Label)
		end
		return Value
	end)

	-- Datatypes
	Measure("UDim2.fromOffset", N, function(Count)
		local Value
		for Index = 1, Count do
			Value = UDim2.fromOffset(Index, Index)
		end
		return Value
	end)
end
