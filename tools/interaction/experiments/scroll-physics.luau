local RunService = game:GetService("RunService")
local Scroll = R.Scroll
local Virtual = UIS:CreateVirtualInput()
local Point = Vector2.new(250, 200)
Virtual:SendMousePosition(Point)
local Scale = Instance.new("UIScale", Scroll)
local Results = {}
local Cases = {
	{ name = "one-down", actions = { { frame = 5, wheel = -1 } } },
	{ name = "one-up", actions = { { frame = 5, wheel = 1 } } },
	{ name = "half-down", actions = { { frame = 5, wheel = -0.5 } } },
	{ name = "two-down", actions = { { frame = 5, wheel = -2 } } },
	{ name = "burst", actions = { { frame = 5, wheel = -1 }, { frame = 8, wheel = -1 }, { frame = 11, wheel = -1 } } },
	{ name = "reverse", actions = { { frame = 5, wheel = -1 }, { frame = 9, wheel = 1 } } },
	{ name = "reset-velocity", actions = { { frame = 5, wheel = -1 }, { frame = 9, reset = true } } },
	{ name = "script-write", actions = { { frame = 5, wheel = -1 }, { frame = 9, position = 800 } } },
	{ name = "tiny-half-pixel", actions = { { frame = 5, wheel = -0.005 } } },
	{ name = "tiny-one-pixel", actions = { { frame = 5, wheel = -0.01 } } },
	{ name = "tiny-two-pixel", actions = { { frame = 5, wheel = -0.02 } } },
	{ name = "scale-half", scale = 0.5, actions = { { frame = 5, wheel = -1 } } },
	{ name = "scale-double", scale = 2, actions = { { frame = 5, wheel = -1 } } },
	{ name = "disable-enable", actions = { { frame = 5, wheel = -1 }, { frame = 9, enabled = false }, { frame = 20, enabled = true } } },
	{ name = "same-value-write", actions = { { frame = 5, wheel = -1 }, { frame = 9, samePosition = true } } },
	{ name = "near-bottom", position = 5680, actions = { { frame = 5, wheel = -1 } } },
	{ name = "at-top", position = 0, actions = { { frame = 5, wheel = 1 } } },
	{ name = "elastic-bottom", position = 5680, elastic = "Always", actions = { { frame = 5, wheel = -1 } } },
	{ name = "large-delta", actions = { { frame = 5, wheel = -10 } } },
	{ name = "same-frame-burst", actions = { { frame = 5, wheel = -1 }, { frame = 5, wheel = -1 } } },
	{ name = "slow-35ms", block = 0.035, actions = { { frame = 5, wheel = -1 } } },
	{ name = "slow-90ms", block = 0.09, actions = { { frame = 5, wheel = -1 } } },
	{ name = "canvas-shrink-listener", actions = { { frame = 5, wheel = -1 }, { frame = 9, canvas = 800 }, { frame = 14, canvas = 6000 } } },
	{ name = "canvas-shrink-no-listener", listener = false, actions = { { frame = 5, wheel = -1 }, { frame = 9, canvas = 800 }, { frame = 14, canvas = 6000 } } },
}
for _, Spec in Cases do
	Scroll:ResetScrollVelocity()
	Scroll.ScrollingEnabled = true
	Scroll.ElasticBehavior = Enum.ElasticBehavior[Spec.elastic or "WhenScrollable"]
	Scale.Scale = Spec.scale or 1
	Scroll.CanvasSize = UDim2.fromOffset(400, 6000)
	Scroll.CanvasPosition = Vector2.new(0, Spec.position or 1000)
	task.wait(0.4)
	Point = Scroll.AbsolutePosition + Scroll.AbsoluteSize / 2
	Virtual:SendMousePosition(Point)
	local Result = { name = Spec.name, actions = Spec.actions, scale = Scale.Scale, elastic = Scroll.ElasticBehavior.Name,
		listener = Spec.listener ~= false,
		window = R.Describe(Scroll.AbsoluteWindowSize), canvas = R.Describe(Scroll.AbsoluteCanvasSize),
		samples = {}, heartbeats = {}, inputs = {}, pointers = {}, changes = {} }
	local Start = os.clock()
	local Frame = 0
	local function Sample(Dt)
		local P = Scroll.CanvasPosition
		local V = Scroll:GetScrollVelocity()
		return { f = Frame, t = os.clock() - Start, dt = Dt, x = P.X, y = P.Y, vx = V.X, vy = V.Y }
	end
	local Connections = {}
	table.insert(Connections, RunService.Heartbeat:Connect(function(Dt)
		table.insert(Result.heartbeats, Sample(Dt))
	end))
	if Spec.listener ~= false then
		table.insert(Connections, Scroll:GetPropertyChangedSignal("CanvasPosition"):Connect(function()
			table.insert(Result.changes, Sample(0))
		end))
	end
	table.insert(Connections, UIS.InputChanged:Connect(function(Input)
		if Input.UserInputType == Enum.UserInputType.MouseWheel then
			table.insert(Result.inputs, { f = Frame, t = os.clock() - Start, z = Input.Position.Z, delta = Input.Delta.Z })
		end
	end))
	table.insert(Connections, UIS.PointerAction:Connect(function(Wheel, Pan, Pinch, Processed)
		table.insert(Result.pointers, { f = Frame, t = os.clock() - Start, wheel = Wheel,
			pan = R.Describe(Pan), pinch = Pinch, processed = Processed })
	end))
	local Done = Instance.new("BindableEvent")
	local Failure = nil
	table.insert(Connections, RunService.PreRender:Connect(function(Dt)
		Frame += 1
		table.insert(Result.samples, Sample(Dt))
		local Ok, Error = pcall(function()
			for _, Action in Spec.actions do
				if Action.frame == Frame then
					Action.sentAt = os.clock() - Start
					if Action.wheel then Virtual:SendPointerAction(Point, { Wheel = Action.wheel }) end
					if Action.reset then Scroll:ResetScrollVelocity() end
					if Action.position then Scroll.CanvasPosition = Vector2.new(0, Action.position) end
					if Action.samePosition then Scroll.CanvasPosition = Scroll.CanvasPosition end
					if Action.enabled ~= nil then Scroll.ScrollingEnabled = Action.enabled end
					if Action.canvas then Scroll.CanvasSize = UDim2.fromOffset(400, Action.canvas) end
				end
			end
		end)
		if not Ok then
			Failure = Error
			Done:Fire()
			return
		end
		if Spec.block and Frame >= 5 and Frame <= 12 then
			local Until = os.clock() + Spec.block
			while os.clock() < Until do end
		end
		if Frame >= 60 then Done:Fire() end
	end))
	for _, Connection in Connections do table.insert(R.Connections, Connection) end
	Done.Event:Wait()
	for _, Connection in Connections do Connection:Disconnect() end
	Done:Destroy()
	if Failure then error(Failure) end
	table.insert(Results, Result)
end
Virtual:SendMousePosition(Vector2.new(10, 10))
return HttpService:JSONEncode({ version = version(), placeId = game.PlaceId, source = "Studio MCP VirtualInput.SendPointerAction", cases = Results })
