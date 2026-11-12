-- Continuous spring and clock-driven animation with no module but this file, so it mounts in any place:
-- damped springs in closed form (critically damped, under-damped, over-damped; a Color3 and a UDim2) stepped by
-- Heartbeat's dt and retargeted every 0.8 s, a looping eased value, a spring retargeted every second, a
-- RenderStepped rotation from the sum of its dt, a Heartbeat-dt progress bar and a task.wait() counter.
-- fixtures/anim-springs.luau runs the same lanes through a project's own spring libraries and needs that
-- project mounted; this one is the scene compared with Studio. Every loop and handler stops itself within a
-- frame once Target leaves the DataModel, so a Studio compare leaves nothing running.
-- compare: --viewport 640x360 --time 1.5 --background 22,24,30
return function(Target)
	local RunService = game:GetService("RunService")

	local function New(ClassName: string, Properties: { [string]: any }, Parent: Instance?): any
		local Object = Instance.new(ClassName)
		for Key, Value in Properties do
			Object[Key] = Value
		end
		Object.Parent = Parent
		return Object
	end

	-- One spring coordinate: x'' = -w^2 (x - goal) - 2 z w x', advanced by its exact solution over dt
	local function Step(X: number, V: number, Goal: number, Period: number, Damping: number, Dt: number): (number, number)
		local W = 2 * math.pi / Period
		local D = X - Goal
		if Damping == 1 then
			local C = V + W * D
			local E = math.exp(-W * Dt)
			return Goal + (D + C * Dt) * E, (C - W * (D + C * Dt)) * E
		elseif Damping < 1 then
			local Wd = W * math.sqrt(1 - Damping * Damping)
			local C = (V + Damping * W * D) / Wd
			local E = math.exp(-Damping * W * Dt)
			local Cos, Sin = math.cos(Wd * Dt), math.sin(Wd * Dt)
			local Offset = D * Cos + C * Sin
			return Goal + Offset * E, (-Damping * W * Offset + Wd * (C * Cos - D * Sin)) * E
		end
		local Root = W * math.sqrt(Damping * Damping - 1)
		local R1, R2 = -Damping * W + Root, -Damping * W - Root
		local C2 = (V - R1 * D) / (R2 - R1)
		local C1 = D - C2
		local E1, E2 = math.exp(R1 * Dt), math.exp(R2 * Dt)
		return Goal + C1 * E1 + C2 * E2, C1 * R1 * E1 + C2 * R2 * E2
	end

	-- A spring over several coordinates
	local function Spring(Start: { number }, Period: number, Damping: number)
		local Self = { X = table.clone(Start), V = table.create(#Start, 0), Goal = table.clone(Start) }
		function Self.Advance(Dt: number)
			for Index = 1, #Self.X do
				Self.X[Index], Self.V[Index] = Step(Self.X[Index], Self.V[Index], Self.Goal[Index], Period, Damping, Dt)
			end
		end
		return Self
	end

	local Root = New("Frame", {
		Name = "AnimSprings",
		Size = UDim2.fromOffset(640, 360),
		BackgroundColor3 = Color3.fromRGB(22, 24, 30),
		BorderSizePixel = 0,
	}, Target)

	local Alive = true
	local Connections = {}
	local Mounted: () -> boolean

	-- Three lanes, plus colour and size
	local Lanes = {
		{ Name = "Critical", Spring = Spring({ 0 }, 0.35, 1) },
		{ Name = "Under", Spring = Spring({ 0 }, 0.5, 0.35) },
		{ Name = "Over", Spring = Spring({ 0 }, 0.3, 1.8) },
	}
	for Index, Lane in Lanes do
		Lane.Frame = New("Frame", {
			Name = `Lane{Lane.Name}`,
			Size = UDim2.fromOffset(24, 24),
			Position = UDim2.fromOffset(20, 16 + (Index - 1) * 34),
			BackgroundColor3 = Color3.fromRGB(120, 180, 255),
			BorderSizePixel = 0,
		}, Root)
	end
	local Colour = Spring({ 255 / 255, 90 / 255, 120 / 255 }, 0.4, 1)
	local Size = Spring({ 30, 30 }, 0.3, 0.6)
	local ColourSize = New("Frame", {
		Name = "ColourSize",
		Position = UDim2.fromOffset(330, 20),
		Size = UDim2.fromOffset(30, 30),
		BackgroundColor3 = Color3.fromRGB(255, 90, 120),
		BorderSizePixel = 0,
	}, Root)

	-- A looping wobbly spring (restarted when it settles) and an eased loop of 0.9 s
	local Wobbly = Spring({ 0 }, 0.45, 0.25)
	Wobbly.Goal[1] = 1
	local WobblyBox = New("Frame", {
		Name = "LoopWobbly",
		Size = UDim2.fromOffset(20, 20),
		Position = UDim2.fromOffset(20, 130),
		BackgroundColor3 = Color3.fromRGB(255, 200, 80),
		BorderSizePixel = 0,
	}, Root)
	local TimedBox = New("Frame", {
		Name = "LoopTimed",
		Size = UDim2.fromOffset(20, 20),
		Position = UDim2.fromOffset(20, 160),
		BackgroundColor3 = Color3.fromRGB(255, 200, 80),
		BorderSizePixel = 0,
	}, Root)
	local TimedAt = 0

	-- Retargeted every second: position and rotation
	local Box = Spring({ 20, 0 }, 0.5, 0.5)
	local SprBox = New("Frame", {
		Name = "Retargeted",
		Size = UDim2.fromOffset(28, 28),
		Position = UDim2.fromOffset(20, 200),
		BackgroundColor3 = Color3.fromRGB(150, 255, 150),
		BorderSizePixel = 0,
	}, Root)

	-- Heartbeat: progress bar filled by dt (1.2 s per pass)
	local Track = New("Frame", {
		Name = "HeartbeatProgress",
		Size = UDim2.fromOffset(280, 10),
		Position = UDim2.fromOffset(20, 250),
		BackgroundColor3 = Color3.fromRGB(50, 54, 64),
		BorderSizePixel = 0,
	}, Root)
	local Fill = New("Frame", { Size = UDim2.fromScale(0, 1), BackgroundColor3 = Color3.fromRGB(255, 120, 200), BorderSizePixel = 0 }, Track)
	local Progress = 0

	table.insert(Connections, RunService.Heartbeat:Connect(function(Dt)
		if not Mounted() then
			return
		end
		for _, Lane in Lanes do
			Lane.Spring.Advance(Dt)
			Lane.Frame.Position = UDim2.fromOffset(20 + Lane.Spring.X[1] * 260, Lane.Frame.Position.Y.Offset)
		end
		Colour.Advance(Dt)
		Size.Advance(Dt)
		ColourSize.BackgroundColor3 = Color3.new(math.clamp(Colour.X[1], 0, 1), math.clamp(Colour.X[2], 0, 1), math.clamp(Colour.X[3], 0, 1))
		ColourSize.Size = UDim2.fromOffset(Size.X[1], Size.X[2])
		Wobbly.Advance(Dt)
		if math.abs(Wobbly.X[1] - 1) < 0.001 and math.abs(Wobbly.V[1]) < 0.001 then
			Wobbly.X[1], Wobbly.V[1] = 0, 0
		end
		WobblyBox.Position = UDim2.fromOffset(20 + Wobbly.X[1] * 260, 130)
		TimedAt = (TimedAt + Dt / 0.9) % 1
		local Eased = if TimedAt < 0.5 then 4 * TimedAt ^ 3 else 1 - (-2 * TimedAt + 2) ^ 3 / 2
		TimedBox.Position = UDim2.fromOffset(20 + Eased * 260, 160)
		Box.Advance(Dt)
		SprBox.Position = UDim2.fromOffset(Box.X[1], 200)
		SprBox.Rotation = Box.X[2]
		Progress = (Progress + Dt / 1.2) % 1
		Fill.Size = UDim2.fromScale(Progress, 1)
	end))

	-- RenderStepped: rotation from the frames' own clock (the sum of RenderStepped's dt, which a compare replays
	-- from Studio's recorded frames; os.clock() is read at a different instant on each side)
	local Spinner = New("Frame", {
		Name = "ClockSpinner",
		Size = UDim2.fromOffset(60, 12),
		Position = UDim2.fromOffset(470, 60),
		AnchorPoint = Vector2.new(0.5, 0.5),
		BackgroundColor3 = Color3.fromRGB(255, 255, 255),
		BorderSizePixel = 0,
	}, Root)
	local Elapsed = 0
	table.insert(Connections, RunService.RenderStepped:Connect(function(Dt)
		if Mounted() then
			Elapsed += Dt
			Spinner.Rotation = Elapsed * 180
		end
	end))

	-- task.wait() counter
	local Counter = New("TextLabel", {
		Name = "WaitCounter",
		Size = UDim2.fromOffset(280, 30),
		Position = UDim2.fromOffset(20, 280),
		BackgroundTransparency = 1,
		TextXAlignment = Enum.TextXAlignment.Left,
		TextColor3 = Color3.fromRGB(230, 230, 240),
		TextSize = 18,
		FontFace = Font.fromEnum(Enum.Font.GothamMedium),
		Text = "task.wait() x 0",
	}, Root)

	-- Destroying the mount disconnects Root's own events before a deferred AncestryChanged handler
	-- could run, so every loop and RunService handler checks for itself.
	function Mounted(): boolean
		if not Alive then
			return false
		end
		if Root:IsDescendantOf(game) then
			return true
		end
		Alive = false
		for _, Connection in Connections do
			Connection:Disconnect()
		end
		return false
	end

	task.spawn(function()
		local Waits = 0
		while Mounted() do
			task.wait()
			Waits += 1
			if Mounted() then
				Counter.Text = `task.wait() x {Waits}`
			end
		end
	end)
	task.spawn(function()
		local On = false
		while Mounted() do
			On = not On
			for _, Lane in Lanes do
				Lane.Spring.Goal[1] = if On then 1 else 0
			end
			Colour.Goal = if On then { 80 / 255, 200 / 255, 255 / 255 } else { 255 / 255, 90 / 255, 120 / 255 }
			Size.Goal = if On then { 70, 40 } else { 30, 30 }
			task.wait(0.8)
		end
	end)
	task.spawn(function()
		local On = false
		while Mounted() do
			On = not On
			Box.Goal = if On then { 270, 90 } else { 20, 0 }
			task.wait(1)
		end
	end)
end
