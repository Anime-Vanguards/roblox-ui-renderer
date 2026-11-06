-- TweenService semantics: DelayTime, RepeatCount, Reverses, Pause/Resume, Cancel/replay, Play while
-- playing, Play after completion, the same property tweened twice, a script write during a tween,
-- Play from a render-step binding, zero/negative Time, destroyed targets, Tween:Destroy().
-- Every scenario owns one NumberValue (0 -> 1, Linear unless noted); actions run at render frame
-- indices (counted in a PreRender handler that runs after the tween step). Values are logged at
-- every PreRender; PlaybackState changes and Completed are logged as they fire.
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
	local Scenarios = {}
	local Order = {}

	local function Scenario(Name, Info, Goal)
		local Value = Instance.new("NumberValue")
		Value.Name = Name
		local Tween = TweenService:Create(Value, Info, { Value = Goal or 1 })
		table.insert(Connections, Tween.Completed:Connect(function(State)
			Add("Completed", Name, State, Tween.PlaybackState, Value.Value)
		end))
		table.insert(Connections, Tween:GetPropertyChangedSignal("PlaybackState"):Connect(function()
			Add("State", Name, Tween.PlaybackState, Value.Value)
		end))
		Scenarios[Name] = { Value = Value, Tween = Tween }
		table.insert(Order, Name)
		return Value, Tween
	end

	local Linear = Enum.EasingStyle.Linear
	local Out = Enum.EasingDirection.Out

	local DelayValue, DelayTween = Scenario("Delay", TweenInfo.new(0.2, Linear, Out, 0, false, 0.1))
	local _, RepeatTween = Scenario("Repeat2", TweenInfo.new(0.15, Linear, Out, 2))
	local _, ReverseTween = Scenario("Reverses", TweenInfo.new(0.15, Linear, Out, 0, true))
	local _, ReverseRepeatTween = Scenario("ReverseRepeatDelay", TweenInfo.new(0.1, Linear, Out, 1, true, 0.05))
	local _, DelayRepeatTween = Scenario("DelayRepeat", TweenInfo.new(0.1, Linear, Out, 2, false, 0.05))
	local _, PauseTween = Scenario("PauseResume", TweenInfo.new(0.3, Linear))
	local CancelValue, CancelTween = Scenario("CancelReplay", TweenInfo.new(0.3, Linear))
	local _, PlayTwiceTween = Scenario("PlayTwice", TweenInfo.new(0.3, Linear))
	local _, AfterTween = Scenario("PlayAfterComplete", TweenInfo.new(0.1, Linear))
	local SetValue, SetTween = Scenario("SetDuring", TweenInfo.new(0.3, Linear))
	local _, BindTween = Scenario("FromBind", TweenInfo.new(0.3, Linear))
	local _, ZeroTween = Scenario("ZeroTime", TweenInfo.new(0, Linear))
	local _, NegativeTween = Scenario("NegativeTime", TweenInfo.new(-1, Linear))
	local DestroyValue, DestroyTween = Scenario("DestroyTarget", TweenInfo.new(0.3, Linear))
	local _, DestroyTweenTween = Scenario("DestroyTween", TweenInfo.new(0.3, Linear))
	local _, ForeverTween = Scenario("RepeatForever", TweenInfo.new(0.1, Linear, Out, -1))
	local _, PausedDelayTween = Scenario("PauseDuringDelay", TweenInfo.new(0.1, Linear, Out, 0, false, 0.1))
	local _, CancelDelayTween = Scenario("CancelThenPlayDelay", TweenInfo.new(0.1, Linear, Out, 0, false, 0.05))

	-- The same property tweened twice: Override1 then Override2 (a second Tween on the same value)
	local OverrideValue, OverrideTween1 = Scenario("Override", TweenInfo.new(0.3, Linear))
	local OverrideTween2 = TweenService:Create(OverrideValue, TweenInfo.new(0.3, Linear), { Value = -1 })
	table.insert(Connections, OverrideTween2.Completed:Connect(function(State)
		Add("Completed", "Override2", State, OverrideTween2.PlaybackState, OverrideValue.Value)
	end))
	table.insert(Connections, OverrideTween2:GetPropertyChangedSignal("PlaybackState"):Connect(function()
		Add("State", "Override2", OverrideTween2.PlaybackState, OverrideValue.Value)
	end))

	-- Two tweens on different properties of one Frame: they must not cancel each other
	local Frame = Instance.new("Frame")
	Frame.Parent = Target
	local FrameTweenA = TweenService:Create(Frame, TweenInfo.new(0.2, Linear), { BackgroundTransparency = 1 })
	local FrameTweenB = TweenService:Create(Frame, TweenInfo.new(0.2, Linear), { Rotation = 90 })
	for Label, Tween in { FrameA = FrameTweenA, FrameB = FrameTweenB } do
		table.insert(Connections, Tween.Completed:Connect(function(State)
			Add("Completed", Label, State)
		end))
	end

	local Actions = {
		[2] = function()
			DelayTween:Play()
			Add("Played", "Delay", DelayTween.PlaybackState, DelayValue.Value)
			RepeatTween:Play()
			ReverseTween:Play()
			ReverseRepeatTween:Play()
			DelayRepeatTween:Play()
			PauseTween:Play()
			CancelTween:Play()
			PlayTwiceTween:Play()
			AfterTween:Play()
			SetTween:Play()
			ZeroTween:Play()
			Add("Played", "ZeroTime", ZeroTween.PlaybackState, Scenarios.ZeroTime.Value.Value)
			NegativeTween:Play()
			Add("Played", "NegativeTime", NegativeTween.PlaybackState, Scenarios.NegativeTime.Value.Value)
			DestroyTween:Play()
			DestroyTweenTween:Play()
			ForeverTween:Play()
			PausedDelayTween:Play()
			CancelDelayTween:Play()
			OverrideTween1:Play()
			FrameTweenA:Play()
			FrameTweenB:Play()
		end,
		[4] = function()
			SetValue.Value = 5
			Add("Set", "SetDuring", SetValue.Value)
			PausedDelayTween:Pause()
			Add("Paused", "PauseDuringDelay", PausedDelayTween.PlaybackState)
			CancelDelayTween:Cancel()
			Add("Cancelled", "CancelThenPlayDelay", CancelDelayTween.PlaybackState)
		end,
		[5] = function()
			PauseTween:Pause()
			Add("Paused", "PauseResume", PauseTween.PlaybackState, Scenarios.PauseResume.Value.Value)
			CancelTween:Cancel()
			Add("Cancelled", "CancelReplay", CancelTween.PlaybackState, CancelValue.Value)
			PlayTwiceTween:Play()
			Add("PlayedAgain", "PlayTwice", PlayTwiceTween.PlaybackState, Scenarios.PlayTwice.Value.Value)
			OverrideTween2:Play()
			Add("PlayedOverride2", OverrideTween1.PlaybackState, OverrideTween2.PlaybackState, OverrideValue.Value)
			DestroyValue:Destroy()
			Add("DestroyedTarget", DestroyTween.PlaybackState)
			DestroyTweenTween:Destroy()
			Add("DestroyedTween", DestroyTweenTween.PlaybackState)
		end,
		[7] = function()
			PausedDelayTween:Play()
			Add("Resumed", "PauseDuringDelay", PausedDelayTween.PlaybackState)
			CancelDelayTween:Play()
			Add("Replayed", "CancelThenPlayDelay", CancelDelayTween.PlaybackState)
		end,
		[10] = function()
			PauseTween:Play()
			Add("Resumed", "PauseResume", PauseTween.PlaybackState, Scenarios.PauseResume.Value.Value)
			CancelTween:Play()
			Add("Replayed", "CancelReplay", CancelTween.PlaybackState, CancelValue.Value)
		end,
		[20] = function()
			AfterTween:Play()
			Add("PlayedAgain", "PlayAfterComplete", AfterTween.PlaybackState, Scenarios.PlayAfterComplete.Value.Value)
		end,
		[40] = function()
			ForeverTween:Cancel()
			Add("Cancelled", "RepeatForever", ForeverTween.PlaybackState, Scenarios.RepeatForever.Value.Value)
		end,
	}

	local RenderFrame = 0
	-- Connected first so it runs after the logger (handlers run newest first)
	table.insert(Connections, RunService.PreRender:Connect(function()
		RenderFrame += 1
		local Action = Actions[RenderFrame]
		if Action then
			Action()
		end
	end))
	table.insert(Connections, RunService.PreRender:Connect(function(Dt)
		local Row = { "Frame", Fmt(os.clock() - Start), tostring(RenderFrame + 1), Fmt(Dt) }
		for _, Name in Order do
			table.insert(Row, Fmt(Scenarios[Name].Value.Value))
		end
		table.insert(Row, Fmt(OverrideValue.Value))
		table.insert(Row, Fmt(Frame.BackgroundTransparency))
		table.insert(Row, Fmt(Frame.Rotation))
		if Recording then
			table.insert(Log, Row)
		end
	end))
	-- Play from a render-step binding, before the tween step of the same frame (frame 3)
	local BindFrame = 0
	RunService:BindToRenderStep("__rur_anim_bind", 0, function()
		BindFrame += 1
		if BindFrame == 3 then
			BindTween:Play()
			Add("PlayedFromBind", BindTween.PlaybackState, Scenarios.FromBind.Value.Value)
		end
	end)

	task.wait(1.2)
	Recording = false
	for _, Connection in Connections do
		Connection:Disconnect()
	end
	RunService:UnbindFromRenderStep("__rur_anim_bind")
	for _, Scenario in Scenarios do
		pcall(function()
			Scenario.Tween:Cancel()
		end)
		Scenario.Value:Destroy()
	end
	OverrideTween2:Cancel()
	Frame:Destroy()

	local Columns = table.clone(Order)
	table.insert(Columns, "OverrideValue")
	table.insert(Columns, "FrameTransparency")
	table.insert(Columns, "FrameRotation")
	return { Columns = Columns, Log = Log }
end
