-- Engine probe: the task library, legacy wait / spawn / delay, frame event order and the virtual
-- clocks (runtime/engine/scheduler.luau), printed with their virtual times for the differential
-- harness, now the golden fixtures (tools/engine/golden.mjs).
return function(Target)
	local Run = game:GetService("RunService")
	local Base = os.clock()
	local function T()
		return string.format("%.6f", os.clock() - Base)
	end
	local Log = {}
	local function Note(Text)
		table.insert(Log, `{T()} {Text}`)
	end

	-- task library edge cases
	local Thread = task.delay(0.05, function(...)
		Note("delay args " .. table.concat({ ... }, ","))
	end, "a", "b")
	Note("delay returns " .. type(Thread))
	task.delay(-1, function() Note("negative delay") end)
	task.delay(0 / 0, function() Note("nan delay") end)
	task.delay(nil, function() Note("nil delay") end)
	local Cancelled = task.delay(0.01, function() Note("cancelled ran") end)
	task.cancel(Cancelled)
	local Waiting = task.spawn(function()
		task.wait(1)
		Note("cancelled wait ran")
	end)
	task.cancel(Waiting)
	Note("cancel status " .. coroutine.status(Waiting))
	task.spawn(function(A, B)
		Note(`spawn args {A} {B}`)
	end, 1, 2)
	local Co = coroutine.create(function(X)
		Note("coroutine spawn " .. tostring(X))
		local Y = coroutine.yield()
		Note("coroutine resumed " .. tostring(Y))
	end)
	task.spawn(Co, "x")
	task.defer(Co, "deferred")
	task.defer(function() Note("defer 1") end)
	task.defer(function()
		Note("defer 2")
		task.defer(function() Note("defer nested") end)
	end)
	task.spawn(function()
		local Waited = task.wait()
		Note(string.format("wait() %.6f", Waited))
		local Waited2 = task.wait(0.1)
		Note(string.format("wait(0.1) %.6f", Waited2))
	end)
	-- legacy globals: at least 0.03 s, first Heartbeat after
	spawn(function() Note("spawn") end)
	delay(0.01, function() Note("legacy delay") end)
	task.spawn(function()
		local Elapsed, Clock = wait()
		Note(string.format("legacy wait %.6f %.6f", Elapsed, Clock - Base))
		local Elapsed2 = wait(0.1)
		Note(string.format("legacy wait(0.1) %.6f", Elapsed2))
	end)
	Note(`errors {tostring(pcall(task.spawn, 5))}`)
	Note(`error message {select(2, pcall(task.delay, 1, 5))}`)
	task.spawn(function()
		error("boom in spawn")
	end)
	task.delay(0.02, function()
		error("boom in delay")
	end)

	-- frame events: order within the first frames, dts
	local Frames = 0
	local Events = {}
	local Connections = {}
	local function Track(Name)
		table.insert(Connections, Run[Name]:Connect(function(A, B)
			if Frames < 3 then
				table.insert(Events, `{Name}:{string.format("%.6f", if Name == "Stepped" then B else A)}`)
			end
		end))
	end
	for _, Name in { "Heartbeat", "RenderStepped", "PreRender", "Stepped", "PreSimulation", "PostSimulation", "PreAnimation" } do
		Track(Name)
	end
	Run:BindToRenderStep("Late", 300, function(Dt)
		if Frames < 3 then
			table.insert(Events, `bind300:{string.format("%.6f", Dt)}`)
		end
	end)
	Run:BindToRenderStep("Early", 100.7, function()
		Frames += 1
		if Frames < 3 then
			table.insert(Events, "bind100")
		end
	end)
	Run:BindToRenderStep("Early2", 100, function()
		if Frames < 3 then
			table.insert(Events, "bind100b")
		end
	end)
	Run:BindToRenderStep("Gone", 50, function() table.insert(Events, "unbound ran") end)
	Run:UnbindFromRenderStep("Gone")
	Note(`bind error {select(2, pcall(function() Run:BindToRenderStep("x", 1) end))}`)

	-- clocks
	Note(string.format("clocks os.clock %.3f tick-rel %s time %s elapsed %.3f", os.clock(), tostring(tick() > 1e9), tostring(time()), elapsedTime()))
	Note(`DateTime {typeof(DateTime.now())} {DateTime.now().UnixTimestampMillis % 1000 >= 0}`)
	local Busy = os.clock()
	for _ = 1, 300 do
		os.clock()
	end
	Note(string.format("busy loop drift %.6f", os.clock() - Busy))
	Note(string.format("server time step %s", tostring(workspace:GetServerTimeNow() > 0)))
	Note(`random {math.random(1, 1000)} {math.random(1, 1000)} {Random.new(5):NextInteger(1, 100)}`)

	task.delay(0.5, function()
		for _, Connection in Connections do
			Connection:Disconnect()
		end
		Run:UnbindFromRenderStep("Late")
		Run:UnbindFromRenderStep("Early")
		Run:UnbindFromRenderStep("Early2")
		for _, Line in Log do
			print(Line)
		end
		print("events", table.concat(Events, " "))
	end)
end
