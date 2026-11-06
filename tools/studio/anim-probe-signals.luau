-- Signal semantics that decide animation ordering: deferred vs immediate property Changed events,
-- handler order (connection order vs newest first) for instance signals, BindableEvents and
-- RunService events, RenderStepped vs PreRender, task.defer vs deferred handlers, Once, Wait.
return function(Target)
	local RunService = game:GetService("RunService")
	local Log = {}
	local Start = os.clock()

	local function Add(Name, ...)
		local Entry = { Name, string.format("%.17g", os.clock() - Start) }
		for Index = 1, select("#", ...) do
			table.insert(Entry, tostring((select(Index, ...))))
		end
		table.insert(Log, Entry)
	end

	-- Deferred or immediate Changed events, and handler order
	local Value = Instance.new("NumberValue")
	local Connections = {}
	for _, Label in { "A", "B", "C" } do
		table.insert(Connections, Value.Changed:Connect(function(New)
			Add(`Changed{Label}`, New)
		end))
	end
	table.insert(Connections, Value:GetPropertyChangedSignal("Value"):Connect(function()
		Add("PropertyChangedD", Value.Value)
	end))
	Add("BeforeSet")
	Value.Value = 1
	Add("AfterSet")
	Value.Value = 2
	Add("AfterSet2")
	task.defer(function()
		Add("DeferAfterSets")
	end)
	task.wait()
	Add("Resumed1")

	-- BindableEvent: order and deferral
	local Bindable = Instance.new("BindableEvent")
	for _, Label in { "A", "B", "C" } do
		table.insert(Connections, Bindable.Event:Connect(function(Argument)
			Add(`Bindable{Label}`, Argument)
		end))
	end
	table.insert(Connections, Bindable.Event:Once(function(Argument)
		Add("BindableOnce", Argument)
	end))
	task.spawn(function()
		local Argument = Bindable.Event:Wait()
		Add("BindableWait", Argument)
	end)
	Add("BeforeFire")
	Bindable:Fire("x")
	Add("AfterFire")
	Bindable:Fire("y")
	Add("AfterFire2")
	task.wait()
	Add("Resumed2")

	-- RenderStepped connected before PreRender; Heartbeat connected in two different frames
	local Frames = 0
	local Recording = true
	table.insert(Connections, RunService.RenderStepped:Connect(function()
		if Recording then
			Add("RenderStepped")
		end
	end))
	table.insert(Connections, RunService.PreRender:Connect(function()
		if Recording then
			Add("PreRender")
		end
	end))
	table.insert(Connections, RunService.Heartbeat:Connect(function()
		if Recording then
			Frames += 1
			Add("HbEarly", Frames)
		end
	end))
	task.wait()
	table.insert(Connections, RunService.Heartbeat:Connect(function()
		if Recording then
			Add("HbLate")
		end
	end))

	-- A handler that connects another handler to the same signal while it runs
	local Nested = Instance.new("BindableEvent")
	table.insert(Connections, Nested.Event:Connect(function()
		Add("NestedOuter")
		table.insert(Connections, Nested.Event:Connect(function()
			Add("NestedInner")
		end))
	end))
	Nested:Fire()
	task.wait()
	Nested:Fire()
	task.wait(0.1)
	Recording = false

	for _, Connection in Connections do
		Connection:Disconnect()
	end
	Value:Destroy()
	Bindable:Destroy()
	Nested:Destroy()

	return { Log = Log }
end
