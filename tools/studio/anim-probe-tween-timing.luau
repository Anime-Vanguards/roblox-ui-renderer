-- Where and how TweenService advances: a Linear 0 -> 1 tween (1 s) on a NumberValue started from
-- a PreRender handler, one started from a Heartbeat handler and one started from plain script code,
-- sampled at every point of the frame (task.wait resumption, first/last Heartbeat handler, first/last
-- render-step binding, PreRender, Changed handlers), plus PlaybackState and Completed timing.
return function(Target)
	local RunService = game:GetService("RunService")
	local TweenService = game:GetService("TweenService")
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

	local Values = {}
	local Tweens = {}
	for _, Name in { "FromPreRender", "FromHeartbeat", "FromScript" } do
		local Value = Instance.new("NumberValue")
		Value.Name = Name
		Values[Name] = Value
		Tweens[Name] = TweenService:Create(Value, TweenInfo.new(1, Enum.EasingStyle.Linear), { Value = 1 })
	end

	local function Snapshot(Where)
		Add(Where, Values.FromPreRender.Value, Values.FromHeartbeat.Value, Values.FromScript.Value)
	end

	local Connections = {}
	for Name, Value in Values do
		table.insert(Connections, Value.Changed:Connect(function(New)
			Add(`Changed{Name}`, New)
		end))
		table.insert(Connections, Tweens[Name].Completed:Connect(function(State)
			Add(`Completed{Name}`, State, Tweens[Name].PlaybackState, Value.Value)
		end))
		table.insert(Connections, Tweens[Name]:GetPropertyChangedSignal("PlaybackState"):Connect(function()
			Add(`State{Name}`, Tweens[Name].PlaybackState, Value.Value)
		end))
	end

	-- Heartbeat handlers fire newest-connected first: "HbLast" is connected first
	table.insert(Connections, RunService.Heartbeat:Connect(function(Dt)
		Add("HbLast", Dt, Values.FromPreRender.Value, Values.FromHeartbeat.Value, Values.FromScript.Value)
	end))
	RunService:BindToRenderStep("__rur_anim_first", -100000, function(Dt)
		Add("BindFirst", Dt, Values.FromPreRender.Value, Values.FromHeartbeat.Value, Values.FromScript.Value)
	end)
	RunService:BindToRenderStep("__rur_anim_last", 100000, function()
		Snapshot("BindLast")
	end)
	table.insert(Connections, RunService.PreRender:Connect(function(Dt)
		Add("PreRender", Dt, Values.FromPreRender.Value, Values.FromHeartbeat.Value, Values.FromScript.Value)
	end))

	local RenderFrame = 0
	table.insert(Connections, RunService.PreRender:Connect(function()
		RenderFrame += 1
		if RenderFrame == 6 then
			Add("PlayFromPreRender")
			Tweens.FromPreRender:Play()
			Add("PlayedFromPreRender", Tweens.FromPreRender.PlaybackState, Values.FromPreRender.Value)
		end
	end))
	local HeartbeatFrame = 0
	table.insert(Connections, RunService.Heartbeat:Connect(function()
		HeartbeatFrame += 1
		if HeartbeatFrame == 42 then
			Add("PlayFromHeartbeat")
			Tweens.FromHeartbeat:Play()
			Add("PlayedFromHeartbeat", Tweens.FromHeartbeat.PlaybackState, Values.FromHeartbeat.Value)
		end
	end))
	table.insert(Connections, RunService.Heartbeat:Connect(function(Dt)
		Add("HbFirst", Dt, Values.FromPreRender.Value, Values.FromHeartbeat.Value, Values.FromScript.Value)
	end))

	task.spawn(function()
		while Recording do
			task.wait()
			Snapshot("Wait")
		end
	end)

	task.wait(0.3)
	Add("PlayFromScript")
	Tweens.FromScript:Play()
	Add("PlayedFromScript", Tweens.FromScript.PlaybackState, Values.FromScript.Value)

	task.wait(1.5)
	Recording = false
	for _, Connection in Connections do
		Connection:Disconnect()
	end
	RunService:UnbindFromRenderStep("__rur_anim_first")
	RunService:UnbindFromRenderStep("__rur_anim_last")
	for _, Value in Values do
		Value:Destroy()
	end

	return { Log = Log }
end
