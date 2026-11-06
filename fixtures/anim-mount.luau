-- Build-time clocks: where in time the calls made while a story is being built land. 40 rows are
-- built in sequence with real work in between (each row creates 60 frames, so the build takes tens of
-- ms in Studio). While it is built, row i reads os.clock() and starts task.delay(0.2 + i * 0.002).
-- At the capture:
--   Clock bar: width = (os.clock() - build clock of row i) * 100 px, set every RenderStepped
--   Delay bar: grows by Heartbeat dt * 100 px from the moment row i's delay resumes
-- (widths are scale sizes of the 620 px row: offsets are whole pixels, scales keep float precision)
-- so with Studio's recorded clock replayed, each row's bar widths say how far into the build the
-- offline run placed row i's calls (tools/analysis/anim_mount.py compares them with Studio's).
--   node cli/rur.mjs compare --fixture fixtures/anim-mount.luau --viewport 640x520 --time 0.6
return function(Target)
	local RunService = game:GetService("RunService")
	local Rows = 40
	local ClockBars = {}
	local DelayBars = {}
	local BuildClocks = {}
	local Growing = {}
	local Alive = true

	local Holder = Instance.new("Frame")
	Holder.Name = "AnimMount"
	Holder.Size = UDim2.fromOffset(640, 520)
	Holder.BackgroundColor3 = Color3.fromRGB(24, 24, 30)
	Holder.BorderSizePixel = 0
	Holder.Parent = Target

	for Index = 1, Rows do
		local Row = Instance.new("Frame")
		Row.Name = `Row{Index}`
		Row.Position = UDim2.fromOffset(10, 8 + (Index - 1) * 12.5)
		Row.Size = UDim2.fromOffset(620, 10)
		Row.BackgroundTransparency = 1
		Row.Parent = Holder

		-- Work between the rows: invisible frames, so the build takes real time
		local Filler = Instance.new("Folder")
		Filler.Name = "Filler"
		Filler.Parent = Row
		for Extra = 1, 60 do
			local Frame = Instance.new("Frame")
			Frame.Name = `F{Extra}`
			Frame.Visible = false
			Frame.Size = UDim2.fromOffset(Extra, Extra)
			Frame.Parent = Filler
		end

		local ClockBar = Instance.new("Frame")
		ClockBar.Name = "ClockBar"
		ClockBar.Size = UDim2.fromOffset(0, 4)
		ClockBar.BackgroundColor3 = Color3.fromRGB(110, 200, 255)
		ClockBar.BorderSizePixel = 0
		ClockBar.Parent = Row
		ClockBars[Index] = ClockBar

		local DelayBar = Instance.new("Frame")
		DelayBar.Name = "DelayBar"
		DelayBar.Position = UDim2.fromOffset(0, 5)
		DelayBar.Size = UDim2.fromOffset(0, 4)
		DelayBar.BackgroundColor3 = Color3.fromRGB(255, 170, 90)
		DelayBar.BorderSizePixel = 0
		DelayBar.Parent = Row
		DelayBars[Index] = DelayBar

		BuildClocks[Index] = os.clock()
		task.delay(0.2 + Index * 0.002, function()
			Growing[Index] = 0
		end)
	end

	local Connections = {}
	table.insert(Connections, RunService.RenderStepped:Connect(function()
		local Now = os.clock()
		for Index, Bar in ClockBars do
			Bar.Size = UDim2.new((Now - BuildClocks[Index]) * 100 / 620, 0, 0, 4)
		end
	end))
	table.insert(Connections, RunService.Heartbeat:Connect(function(Dt)
		for Index, Width in Growing do
			Growing[Index] = Width + Dt * 100
			DelayBars[Index].Size = UDim2.new(Growing[Index] / 620, 0, 0, 4)
		end
	end))

	-- Stops within a frame once the target leaves the DataModel (Studio compare unmount)
	table.insert(Connections, Holder.AncestryChanged:Connect(function()
		if Alive and not Holder:IsDescendantOf(game) then
			Alive = false
			for _, Connection in Connections do
				Connection:Disconnect()
			end
		end
	end))
end
