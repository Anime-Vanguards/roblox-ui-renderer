-- Remaining TweenService details: when a non-delayed tween captures its start value (a write right
-- after Play()), the order tweens are stepped in (Changed handler order), the order of Completed
-- events for tweens finishing in the same step, error messages of TweenService:Create, a tween
-- restarted from its own Completed handler, and TweenService:SmoothDamp samples.
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

	local Result = { Errors = {}, SmoothDamp = {} }
	local Connections = {}
	local Linear = Enum.EasingStyle.Linear

	-- Error messages
	local Frame = Instance.new("Frame")
	Frame.Parent = Target
	for Label, Call in {
		BadProperty = function()
			TweenService:Create(Frame, TweenInfo.new(1), { NotAProperty = 1 })
		end,
		TypeMismatch = function()
			TweenService:Create(Frame, TweenInfo.new(1), { BackgroundTransparency = UDim2.new() })
		end,
		NumberForInt = function()
			TweenService:Create(Frame, TweenInfo.new(1), { ZIndex = 2.5 })
		end,
		ReadOnly = function()
			TweenService:Create(Frame, TweenInfo.new(1), { AbsoluteSize = Vector2.new(1, 1) })
		end,
		NilInstance = function()
			TweenService:Create(nil, TweenInfo.new(1), { Value = 1 })
		end,
		NilInfo = function()
			TweenService:Create(Frame, nil, { BackgroundTransparency = 1 })
		end,
		NotTable = function()
			TweenService:Create(Frame, TweenInfo.new(1), 5)
		end,
		EmptyTable = function()
			local Tween = TweenService:Create(Frame, TweenInfo.new(0.1), {})
			Tween:Play()
			error(`ok {Tween.PlaybackState}`)
		end,
		StringNumber = function()
			TweenService:Create(Frame, TweenInfo.new(1), { BackgroundTransparency = "0.5" })
		end,
		GetValueString = function()
			error(`ok {TweenService:GetValue(0.5, "Quad", "Out")}`)
		end,
		GetValueNumbers = function()
			error(`ok {TweenService:GetValue(0.5, 3, 1)}`)
		end,
	} do
		local _, Error = pcall(Call)
		Result.Errors[Label] = tostring(Error)
	end

	-- (a) Start value of a non-delayed tween written right after Play()
	local Capture = Instance.new("NumberValue")
	local CaptureTween = TweenService:Create(Capture, TweenInfo.new(0.2, Linear), { Value = 1 })

	-- (b/c) Step order: three tweens played A, B, C; Changed and Completed handler order
	local Order = {}
	local OrderTweens = {}
	for _, Label in { "A", "B", "C" } do
		local Value = Instance.new("NumberValue")
		table.insert(Connections, Value.Changed:Connect(function()
			Add(`Changed{Label}`)
		end))
		local Tween = TweenService:Create(Value, TweenInfo.new(0.1, Linear), { Value = 1 })
		table.insert(Connections, Tween.Completed:Connect(function()
			Add(`Completed{Label}`)
		end))
		Order[Label] = Value
		table.insert(OrderTweens, Tween)
	end

	-- (d) Restart from the Completed handler (a common looping pattern)
	local Loop = Instance.new("NumberValue")
	local LoopTween = TweenService:Create(Loop, TweenInfo.new(0.1, Linear), { Value = 1 })
	local Loops = 0
	table.insert(Connections, LoopTween.Completed:Connect(function()
		Loops += 1
		Add("LoopCompleted", Loop.Value)
		if Loops < 3 then
			Loop.Value = 0
			LoopTween:Play()
		end
	end))

	local RenderFrame = 0
	table.insert(Connections, RunService.PreRender:Connect(function()
		RenderFrame += 1
		if RenderFrame == 2 then
			CaptureTween:Play()
			Capture.Value = 0.5
			for _, Tween in OrderTweens do
				Tween:Play()
			end
			LoopTween:Play()
		end
	end))
	table.insert(Connections, RunService.PreRender:Connect(function(Dt)
		Add("Frame", RenderFrame + 1, Dt, Capture.Value, Loop.Value)
	end))

	-- (e) SmoothDamp samples: numbers, Vector2, Vector3
	local Ok, Error = pcall(function()
		for _, Case in {
			{ 0, 10, 0, 0.3, math.huge, 1 / 60 },
			{ 0, 10, 0, 0.3, math.huge, 0.1 },
			{ 5, -3, 2, 0.1, math.huge, 1 / 60 },
			{ 0, 10, 0, 0.3, 5, 1 / 60 },
			{ 0, 10, 0, 0, math.huge, 1 / 60 },
			{ 9.99, 10, 0, 0.3, math.huge, 1 / 60 },
		} do
			local Value, Velocity = TweenService:SmoothDamp(table.unpack(Case))
			table.insert(Result.SmoothDamp, { Case = Case, Value = Fmt(Value), Velocity = Fmt(Velocity) })
		end
		local Value, Velocity = TweenService:SmoothDamp(Vector2.new(0, 0), Vector2.new(10, 5), Vector2.new(1, 0), 0.25, math.huge, 1 / 60)
		table.insert(Result.SmoothDamp, { Case = "Vector2", Value = tostring(Value), Velocity = tostring(Velocity) })
		local Value3, Velocity3 = TweenService:SmoothDamp(Vector3.new(0, 0, 0), Vector3.new(10, 5, -3), Vector3.zero, 0.25, math.huge, 1 / 60)
		table.insert(Result.SmoothDamp, { Case = "Vector3", Value = tostring(Value3), Velocity = tostring(Velocity3) })
		-- A 60-step trajectory
		local Trajectory = {}
		local Current, Speed = 0, 0
		for _ = 1, 60 do
			Current, Speed = TweenService:SmoothDamp(Current, 1, Speed, 0.2, math.huge, 1 / 60)
			table.insert(Trajectory, Fmt(Current) .. " " .. Fmt(Speed))
		end
		Result.SmoothDampTrajectory = Trajectory
	end)
	if not Ok then
		Result.SmoothDampError = tostring(Error)
	end

	task.wait(0.8)
	Recording = false
	for _, Connection in Connections do
		Connection:Disconnect()
	end
	Frame:Destroy()

	Result.Log = Log
	return Result
end
