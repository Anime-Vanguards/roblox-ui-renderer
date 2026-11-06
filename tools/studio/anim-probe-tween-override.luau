-- Tween edge cases: partial property overlap between two tweens, overriding with a delayed tween,
-- re-playing the older tween, overriding a paused tween, when the start value is captured for a
-- delayed tween, the path of a reversing non-linear tween, Cancel/Pause on idle tweens, and a
-- zero-length tween with a delay. Actions run at render frame indices; values are logged at every
-- PreRender (after the tween step).
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

	local Connections = {}
	local Watched = {}
	local Columns = {}

	local function Watch(Name, Instance_, Property)
		table.insert(Watched, { Instance = Instance_, Property = Property })
		table.insert(Columns, Name)
	end

	local function Track(Name, Tween)
		table.insert(Connections, Tween.Completed:Connect(function(State)
			Add("Completed", Name, State)
		end))
		table.insert(Connections, Tween:GetPropertyChangedSignal("PlaybackState"):Connect(function()
			Add("State", Name, Tween.PlaybackState)
		end))
		return Tween
	end

	local Linear = Enum.EasingStyle.Linear
	local Out = Enum.EasingDirection.Out

	-- 1. Partial overlap: T1 tweens A and B, T2 (played later) tweens only A
	local Partial = Instance.new("Frame")
	Partial.Parent = Target
	Watch("PartialA", Partial, "BackgroundTransparency")
	Watch("PartialB", Partial, "Rotation")
	local PartialT1 = Track("PartialT1", TweenService:Create(Partial, TweenInfo.new(0.4, Linear), { BackgroundTransparency = 1, Rotation = 100 }))
	local PartialT2 = Track("PartialT2", TweenService:Create(Partial, TweenInfo.new(0.4, Linear), { BackgroundTransparency = 0.5 }))

	-- 2. Override by a delayed tween: when does the older one stop?
	local DelayedOverride = Instance.new("NumberValue")
	Watch("DelayedOverride", DelayedOverride, "Value")
	local DelayedT1 = Track("DelayedT1", TweenService:Create(DelayedOverride, TweenInfo.new(0.5, Linear), { Value = 1 }))
	local DelayedT2 = Track("DelayedT2", TweenService:Create(DelayedOverride, TweenInfo.new(0.2, Linear, Out, 0, false, 0.15), { Value = -1 }))

	-- 3. The older tween re-played after the newer one started
	local Replay = Instance.new("NumberValue")
	Watch("Replay", Replay, "Value")
	local ReplayT1 = Track("ReplayT1", TweenService:Create(Replay, TweenInfo.new(0.4, Linear), { Value = 1 }))
	local ReplayT2 = Track("ReplayT2", TweenService:Create(Replay, TweenInfo.new(0.4, Linear), { Value = -1 }))

	-- 4. A paused tween overridden
	local PausedOverride = Instance.new("NumberValue")
	Watch("PausedOverride", PausedOverride, "Value")
	local PausedT1 = Track("PausedT1", TweenService:Create(PausedOverride, TweenInfo.new(0.4, Linear), { Value = 1 }))
	local PausedT2 = Track("PausedT2", TweenService:Create(PausedOverride, TweenInfo.new(0.4, Linear), { Value = -1 }))

	-- 5. Start value of a delayed tween: written by a script during the delay
	local DelayedStart = Instance.new("NumberValue")
	Watch("DelayedStart", DelayedStart, "Value")
	local DelayedStartTween = Track("DelayedStart", TweenService:Create(DelayedStart, TweenInfo.new(0.2, Linear, Out, 0, false, 0.1), { Value = 1 }))

	-- 6. Reverse path of a non-linear tween (Quad Out, reverses) and of Back In
	local ReverseQuad = Instance.new("NumberValue")
	Watch("ReverseQuad", ReverseQuad, "Value")
	local ReverseQuadTween = Track("ReverseQuad", TweenService:Create(ReverseQuad, TweenInfo.new(0.25, Enum.EasingStyle.Quad, Out, 0, true), { Value = 1 }))
	local ReverseBack = Instance.new("NumberValue")
	Watch("ReverseBack", ReverseBack, "Value")
	local ReverseBackTween = Track("ReverseBack", TweenService:Create(ReverseBack, TweenInfo.new(0.25, Enum.EasingStyle.Back, Enum.EasingDirection.In, 1, true), { Value = 1 }))

	-- 7. Cancel / Pause on idle tweens, Play after Completed with a delay
	local Idle = Instance.new("NumberValue")
	Watch("Idle", Idle, "Value")
	local IdleCancel = Track("IdleCancel", TweenService:Create(Idle, TweenInfo.new(0.1, Linear), { Value = 1 }))
	local IdlePause = Track("IdlePause", TweenService:Create(Idle, TweenInfo.new(0.1, Linear), { Value = 1 }))
	local AfterDone = Instance.new("NumberValue")
	Watch("AfterDone", AfterDone, "Value")
	local AfterDoneTween = Track("AfterDone", TweenService:Create(AfterDone, TweenInfo.new(0.1, Linear, Out, 0, false, 0.1), { Value = 1 }))

	-- 8. Zero-length tween with a delay; zero-length reversing tween
	local ZeroDelay = Instance.new("NumberValue")
	Watch("ZeroDelay", ZeroDelay, "Value")
	local ZeroDelayTween = Track("ZeroDelay", TweenService:Create(ZeroDelay, TweenInfo.new(0, Linear, Out, 0, false, 0.1), { Value = 1 }))
	local ZeroReverse = Instance.new("NumberValue")
	Watch("ZeroReverse", ZeroReverse, "Value")
	local ZeroReverseTween = Track("ZeroReverse", TweenService:Create(ZeroReverse, TweenInfo.new(0, Linear, Out, 2, true), { Value = 1 }))

	-- 9. Two tweens started in the same frame on one property: which wins?
	local SameFrame = Instance.new("NumberValue")
	Watch("SameFrame", SameFrame, "Value")
	local SameFrameT1 = Track("SameFrameT1", TweenService:Create(SameFrame, TweenInfo.new(0.3, Linear), { Value = 1 }))
	local SameFrameT2 = Track("SameFrameT2", TweenService:Create(SameFrame, TweenInfo.new(0.3, Linear), { Value = -1 }))

	local Actions = {
		[2] = function()
			PartialT1:Play()
			DelayedT1:Play()
			ReplayT1:Play()
			PausedT1:Play()
			DelayedStartTween:Play()
			ReverseQuadTween:Play()
			ReverseBackTween:Play()
			IdleCancel:Cancel()
			Add("IdleCancel", IdleCancel.PlaybackState)
			IdlePause:Pause()
			Add("IdlePause", IdlePause.PlaybackState)
			AfterDoneTween:Play()
			ZeroDelayTween:Play()
			ZeroReverseTween:Play()
			SameFrameT1:Play()
			SameFrameT2:Play()
		end,
		[4] = function()
			DelayedStart.Value = 0.5
			PausedT1:Pause()
		end,
		[6] = function()
			PartialT2:Play()
			DelayedT2:Play()
			ReplayT2:Play()
			PausedT2:Play()
			Add("PlayedOverrides", PartialT1.PlaybackState, DelayedT1.PlaybackState, ReplayT1.PlaybackState, PausedT1.PlaybackState)
		end,
		[9] = function()
			ReplayT1:Play()
			Add("ReplayedT1", ReplayT1.PlaybackState, ReplayT2.PlaybackState)
		end,
		[30] = function()
			AfterDoneTween:Play()
			Add("AfterDoneReplay", AfterDoneTween.PlaybackState)
		end,
	}

	local RenderFrame = 0
	table.insert(Connections, RunService.PreRender:Connect(function()
		RenderFrame += 1
		local Action = Actions[RenderFrame]
		if Action then
			Action()
		end
	end))
	table.insert(Connections, RunService.PreRender:Connect(function(Dt)
		if not Recording then
			return
		end
		local Row = { "Frame", Fmt(os.clock() - Start), tostring(RenderFrame + 1), Fmt(Dt) }
		for _, Entry in Watched do
			table.insert(Row, Fmt(Entry.Instance[Entry.Property]))
		end
		table.insert(Log, Row)
	end))

	task.wait(1)
	Recording = false
	for _, Connection in Connections do
		Connection:Disconnect()
	end
	Partial:Destroy()

	return { Columns = Columns, Log = Log }
end
