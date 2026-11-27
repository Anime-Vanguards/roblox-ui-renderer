-- CanvasPosition written while a ScrollingFrame is outside the displayed tree (unparented, under an unparented frame, in
-- ReplicatedStorage), then parented into a cell, against writes on a displayed frame: each cell's label T reads the
-- frame's CanvasPosition at each step (float32, %.9g): right after the write, a frame later, right after parenting, and
-- a frame after that. Each frame is 100 x 100 (or 160 x 60 of scale size) with a 100 x 300 canvas. Below them, labels
-- U_* read what an unparented ScrollingFrame of each size, canvas and bar reports: the CanvasPosition a write of
-- (1e6, 1e6) clamps to, its AbsoluteSize, AbsoluteCanvasSize and AbsoluteWindowSize (alone, or under an unparented frame).
--   node cli/rur.mjs compare --fixture fixtures/layout-canvas-unparented.luau --viewport 1600x880 --time 0.2 --background 20,20,26
return function(Target)
	local Count = 0
	local Steps = {}
	local function Cell(Name)
		local Cell = Instance.new("Frame")
		Cell.Name = Name
		Cell.Position = UDim2.fromOffset((Count % 6) * 266, (Count // 6) * 220)
		Cell.Size = UDim2.fromOffset(262, 216)
		Cell.BackgroundColor3 = Color3.fromRGB(30, 30, 38)
		Cell.BorderSizePixel = 0
		Cell.Parent = Target
		Count += 1
		local T = Instance.new("TextLabel")
		T.Name = "T"
		T.Position = UDim2.fromOffset(2, 196)
		T.Size = UDim2.fromOffset(258, 14)
		T.BackgroundTransparency = 1
		T.TextColor3 = Color3.new(1, 1, 1)
		T.TextSize = 8
		T.TextXAlignment = Enum.TextXAlignment.Left
		T.Text = ""
		T.Parent = Cell
		return Cell, T
	end
	local function Scroller(Scaled)
		local S = Instance.new("ScrollingFrame")
		S.Name = "S"
		S.Position = UDim2.fromOffset(10, 10)
		if Scaled then
			S.Size = UDim2.new(0.37, 3, 0.3, 1)
			S.CanvasSize = UDim2.new(1.7, 11, 2.3, 7)
			S.ScrollBarThickness = 7
		else
			S.Size = UDim2.fromOffset(100, 100)
			S.CanvasSize = UDim2.fromOffset(100, 300)
		end
		S.BackgroundColor3 = Color3.fromRGB(60, 60, 80)
		S.BorderSizePixel = 0
		for K = 1, 3 do
			local Child = Instance.new("Frame")
			Child.Name = `C{K}`
			Child.Position = UDim2.fromOffset(10, (K - 1) * 90 + 5)
			Child.Size = UDim2.fromOffset(60, 80)
			Child.BackgroundColor3 = Color3.fromRGB(200, 40 + 50 * K, 50)
			Child.BorderSizePixel = 0
			Child.Parent = S
		end
		return S
	end
	local function Read(S)
		local P = S.CanvasPosition
		return string.format("%.9g,%.9g", P.X, P.Y)
	end
	-- Case(name, scaled, where, position): where the frame is when the position is written ("nil", "frame": an unparented
	-- frame, "storage": a Folder in ReplicatedStorage, "cell": already displayed, "hidden": under an invisible frame)
	local Cases = {}
	local function Case(Name, Scaled, Where, Position)
		local C, T = Cell(Name)
		local S = Scroller(Scaled)
		local Holder
		if Where == "frame" then
			Holder = Instance.new("Frame")
			Holder.Name = "U"
			Holder.Size = UDim2.fromOffset(240, 180)
			Holder.BackgroundTransparency = 1
			S.Parent = Holder
		elseif Where == "storage" then
			Holder = Instance.new("Folder")
			Holder.Name = "RURCanvasProbe" .. Name
			Holder.Parent = game:GetService("ReplicatedStorage")
			S.Parent = Holder
		elseif Where == "cell" then
			S.Parent = C
		elseif Where == "hidden" then
			Holder = Instance.new("Frame")
			Holder.Name = "U"
			Holder.Size = UDim2.fromOffset(240, 180)
			Holder.BackgroundTransparency = 1
			Holder.Visible = false
			Holder.Parent = C
			S.Parent = Holder
		end
		S.CanvasPosition = Position
		local Reads = { Read(S) }
		table.insert(Cases, function(Phase)
			if Phase == 1 then
				table.insert(Reads, Read(S))
				-- into the cell (the frame U with it, or the frame alone)
				if Where == "frame" or Where == "hidden" then
					Holder.Visible = true
					Holder.Parent = C
				elseif Where ~= "cell" then
					S.Parent = C
				end
				table.insert(Reads, Read(S))
			else
				table.insert(Reads, Read(S))
				T.Text = table.concat(Reads, " | ")
				if Where == "storage" then
					Holder:Destroy()
				end
			end
		end)
	end
	-- Unparented(name, size, canvas, thickness, holder): the unparented frame's reads, written into a label U_<name>
	local Row = 0
	local function Unparented(Name, Size, Canvas, Thickness, Holder)
		local U = Instance.new("TextLabel")
		U.Name = "U_" .. Name
		U.Position = UDim2.fromOffset((Row % 2) * 800, 660 + (Row // 2) * 20)
		U.Size = UDim2.fromOffset(796, 18)
		U.BackgroundTransparency = 1
		U.TextColor3 = Color3.new(1, 1, 1)
		U.TextSize = 10
		U.TextXAlignment = Enum.TextXAlignment.Left
		U.Parent = Target
		Row += 1
		local S = Instance.new("ScrollingFrame")
		S.Size = Size
		S.CanvasSize = Canvas
		S.ScrollBarThickness = Thickness
		if Holder == "storage" then
			local F = Instance.new("Folder")
			F.Name = "RURCanvasProbe_" .. Name
			F.Parent = game:GetService("ReplicatedStorage")
			S.Parent = F
		elseif Holder == "shown" then
			-- (a displayed holder: a transparent frame in the label)
			local F = Instance.new("Frame")
			F.Size = UDim2.fromOffset(40, 18)
			F.BackgroundTransparency = 1
			F.Parent = U
			S.Parent = F
		elseif Holder then
			local F = Instance.new("Frame")
			F.Size = Holder
			S.Parent = F
		end
		S.CanvasPosition = Vector2.new(1e6, 1e6)
		local P, A, C, W = S.CanvasPosition, S.AbsoluteSize, S.AbsoluteCanvasSize, S.AbsoluteWindowSize
		U.Text = string.format("%.9g,%.9g | %.9g,%.9g | %.9g,%.9g | %.9g,%.9g", P.X, P.Y, A.X, A.Y, C.X, C.Y, W.X, W.Y)
	end
	Unparented("p4c", UDim2.new(0.37, 3, 0.3, 1), UDim2.new(1.7, 11, 2.3, 7), 7)
	Unparented("p4c_t0", UDim2.new(0.37, 3, 0.3, 1), UDim2.new(1.7, 11, 2.3, 7), 0)
	Unparented("offsets", UDim2.fromOffset(100, 80), UDim2.fromOffset(50, 300), 12)
	Unparented("scale_size", UDim2.new(0.5, 100, 0.5, 80), UDim2.fromOffset(50, 300), 12)
	Unparented("scale_canvas", UDim2.fromOffset(100, 80), UDim2.fromScale(2, 3), 12)
	Unparented("scale_both", UDim2.new(0.5, 100, 0.5, 80), UDim2.new(2, 10, 3, 20), 12)
	Unparented("full_offsets", UDim2.fromScale(1, 1), UDim2.fromOffset(200, 400), 12)
	Unparented("full_scale", UDim2.fromScale(1, 1), UDim2.fromScale(1, 2), 12)
	Unparented("small_canvas", UDim2.fromOffset(100, 80), UDim2.fromScale(0.5, 0.25), 0)
	Unparented("offset_size_p4c_canvas", UDim2.fromOffset(3, 1), UDim2.new(1.7, 11, 2.3, 7), 7)
	Unparented("p4c_size_offset_canvas", UDim2.new(0.37, 3, 0.3, 1), UDim2.fromOffset(11, 7), 7)
	Unparented("frac_canvas", UDim2.fromOffset(40, 30), UDim2.new(0.37, 50, 0.61, 70), 5)
	Unparented("held_offset", UDim2.fromScale(0.5, 0.5), UDim2.fromScale(2, 3), 12, UDim2.fromOffset(300, 200))
	Unparented("held_scale", UDim2.new(0.5, 10, 0.5, 10), UDim2.fromScale(1, 2), 12, UDim2.fromScale(0.5, 0.5))
	Unparented("held_offset_p4c", UDim2.new(0.37, 3, 0.3, 1), UDim2.new(1.7, 11, 2.3, 7), 7, UDim2.fromOffset(200, 100))
	Unparented("big_bar", UDim2.new(0.2, 50, 0.2, 40), UDim2.new(3, 0, 4, 0), 30)
	Unparented("bar_wider", UDim2.fromOffset(20, 15), UDim2.new(2, 0, 3, 0), 25)
	Unparented("storage_scale_canvas", UDim2.fromOffset(100, 80), UDim2.new(2, 10, 3, 20), 12, "storage")
	Unparented("shown_negative_window", UDim2.fromOffset(3, 1), UDim2.fromOffset(11, 7), 7, "shown")
	Unparented("shown_scale_canvas", UDim2.fromOffset(30, 10), UDim2.fromScale(2, 3), 4, "shown")
	Unparented("scale_canvas_small", UDim2.new(0.5, 100, 0.5, 80), UDim2.new(0.5, 50, 0.5, 300), 12)
	local Over, Mid, Under = Vector2.new(13.37, 1000), Vector2.new(13.37, 21.71), Vector2.new(-20, -30)
	Case("nil_mid", false, nil, Mid)
	Case("nil_over", false, nil, Over)
	Case("nil_under", false, nil, Under)
	Case("nil_scaled_mid", true, nil, Mid)
	Case("nil_scaled_over", true, nil, Over)
	Case("frame_mid", false, "frame", Mid)
	Case("frame_over", false, "frame", Over)
	Case("storage_mid", false, "storage", Mid)
	Case("storage_over", false, "storage", Over)
	Case("cell_mid", false, "cell", Mid)
	Case("cell_over", false, "cell", Over)
	Case("cell_under", false, "cell", Under)
	Case("cell_scaled_over", true, "cell", Over)
	Case("hidden_over", false, "hidden", Over)
	Case("hidden_mid", false, "hidden", Mid)
	task.wait()
	for _, Step in Cases do
		Step(1)
	end
	task.wait()
	for _, Step in Cases do
		Step(2)
	end
end
