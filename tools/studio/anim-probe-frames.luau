-- Frame structure in the current mode: RunService event order and arguments, BindToRenderStep
-- priorities, handler order on one signal, clocks, and where task.wait / task.delay / task.defer /
-- legacy wait / spawn / delay resume. Every entry is { name, os.clock() - start, args... } with
-- numbers as %.17g strings.
return function(Target)
	local RunService = game:GetService("RunService")
	local Log = {}
	local Start = os.clock()
	local Recording = true

	local function Fmt(Value)
		if type(Value) == "number" then
			return string.format("%.17g", Value)
		end
		return tostring(Value)
	end

	local function Add(Name, ...)
		if not Recording then
			return
		end
		local Entry = { Name, Fmt(os.clock() - Start) }
		for Index = 1, select("#", ...) do
			table.insert(Entry, Fmt((select(Index, ...))))
		end
		table.insert(Log, Entry)
	end

	local Connections = {}
	for _, Name in { "PreRender", "RenderStepped", "PreAnimation", "Stepped", "PreSimulation", "PostSimulation", "Heartbeat" } do
		table.insert(Connections, RunService[Name]:Connect(function(...)
			Add(Name, ...)
		end))
	end

	-- Handler order on one signal: connected A, B, C
	for _, Label in { "HbA", "HbB", "HbC" } do
		table.insert(Connections, RunService.Heartbeat:Connect(function()
			Add(Label)
		end))
	end

	local Priorities = { -100, 0, 1, 100, 200, 201, 300, 2000, 10000 }
	for Index = #Priorities, 1, -1 do
		local Priority = Priorities[Index]
		RunService:BindToRenderStep(`__rur_anim_{Priority}`, Priority, function(Dt)
			Add(`Bind{Priority}`, Dt)
		end)
	end
	-- Same priority bound twice: which runs first?
	RunService:BindToRenderStep("__rur_anim_sameA", 50, function()
		Add("Bind50A")
	end)
	RunService:BindToRenderStep("__rur_anim_sameB", 50, function()
		Add("Bind50B")
	end)

	-- Clocks, sampled at every Heartbeat
	table.insert(Connections, RunService.Heartbeat:Connect(function()
		Add("Clocks", os.clock(), time(), tick(), elapsedTime(), workspace.DistributedGameTime, workspace:GetServerTimeNow(), DateTime.now().UnixTimestampMillis, os.time())
	end))

	-- task.wait() / task.wait(0.1) / wait() / wait(0.1) loops
	task.spawn(function()
		for _ = 1, 40 do
			local Elapsed = task.wait()
			Add("TaskWait0", Elapsed)
		end
	end)
	task.spawn(function()
		for _ = 1, 6 do
			local Elapsed = task.wait(0.1)
			Add("TaskWait01", Elapsed)
		end
	end)
	task.spawn(function()
		for _ = 1, 20 do
			local Elapsed, Time = wait()
			Add("LegacyWait0", Elapsed, Time)
		end
	end)
	task.spawn(function()
		for _ = 1, 6 do
			local Elapsed, Time = wait(0.1)
			Add("LegacyWait01", Elapsed, Time)
		end
	end)

	-- Scheduled from inside phases of one frame (the 20th PreRender)
	local Frame = 0
	table.insert(Connections, RunService.PreRender:Connect(function()
		Frame += 1
		if Frame == 20 then
			Add("ScheduleFromPreRender")
			task.defer(function()
				Add("DeferFromPreRender")
			end)
			task.delay(0, function()
				Add("Delay0FromPreRender")
			end)
			task.delay(0.05, function()
				Add("Delay005FromPreRender")
			end)
			spawn(function()
				Add("SpawnFromPreRender")
			end)
			delay(0, function()
				Add("LegacyDelay0FromPreRender")
			end)
			task.spawn(function()
				Add("TaskSpawnFromPreRender")
				task.wait()
				Add("TaskWaitFromPreRender")
			end)
		end
	end))
	local HeartbeatFrame = 0
	table.insert(Connections, RunService.Heartbeat:Connect(function()
		HeartbeatFrame += 1
		if HeartbeatFrame == 30 then
			Add("ScheduleFromHeartbeat")
			task.defer(function()
				Add("DeferFromHeartbeat")
			end)
			task.delay(0, function()
				Add("Delay0FromHeartbeat")
			end)
			task.spawn(function()
				task.wait()
				Add("TaskWaitFromHeartbeat")
			end)
			spawn(function()
				Add("SpawnFromHeartbeat")
			end)
		end
	end))

	local Info = {
		IsRunning = RunService:IsRunning(),
		IsEdit = RunService:IsEdit(),
		IsClient = RunService:IsClient(),
		IsServer = RunService:IsServer(),
		IsStudio = RunService:IsStudio(),
		SignalBehavior = tostring(select(2, pcall(function()
			return workspace.SignalBehavior
		end))),
		Version = version(),
	}

	task.wait(1.6)
	Recording = false
	for _, Connection in Connections do
		Connection:Disconnect()
	end
	for _, Priority in Priorities do
		RunService:UnbindFromRenderStep(`__rur_anim_{Priority}`)
	end
	RunService:UnbindFromRenderStep("__rur_anim_sameA")
	RunService:UnbindFromRenderStep("__rur_anim_sameB")

	return { Info = Info, Log = Log }
end
