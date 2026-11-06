-- When is ScrollingFrame.CanvasPosition clamped? Set before parenting, on a laid-out frame, beyond the
-- range, together with a CanvasSize change in the same frame; read back immediately and after a frame.
return function(Target)
	local Results = {}
	local function Make(Name, CanvasSize, Parented)
		local Frame = Instance.new("ScrollingFrame")
		Frame.Name = Name
		Frame.Size = UDim2.fromOffset(200, 100)
		Frame.CanvasSize = CanvasSize
		Frame.ScrollBarThickness = 10
		if Parented then
			Frame.Parent = Target
		end
		return Frame
	end
	local function Read(Frame)
		return { Frame.CanvasPosition.X, Frame.CanvasPosition.Y }
	end
	-- (a) unparented
	local A = Make("A", UDim2.fromOffset(600, 500), false)
	A.CanvasPosition = Vector2.new(50, 60)
	Results.a_immediate = Read(A)
	A.Parent = Target
	Results.a_parented = Read(A)
	-- (b) laid out, in range
	local B = Make("B", UDim2.fromOffset(600, 500), true)
	task.wait()
	B.CanvasPosition = Vector2.new(50, 60)
	Results.b_immediate = Read(B)
	-- (c) laid out, beyond range
	local C = Make("C", UDim2.fromOffset(600, 500), true)
	task.wait()
	C.CanvasPosition = Vector2.new(5000, 6000)
	Results.c_immediate = Read(C)
	-- (d) grow canvas then set position in the same frame
	local D = Make("D", UDim2.fromOffset(200, 100), true)
	task.wait()
	D.CanvasSize = UDim2.fromOffset(800, 700)
	D.CanvasPosition = Vector2.new(300, 250)
	Results.d_immediate = Read(D)
	-- (e) set position then grow canvas in the same frame
	local E = Make("E", UDim2.fromOffset(200, 100), true)
	task.wait()
	E.CanvasPosition = Vector2.new(300, 250)
	Results.e_immediate = Read(E)
	E.CanvasSize = UDim2.fromOffset(800, 700)
	Results.e_after_size = Read(E)
	-- (f) shrink canvas below the position
	local G = Make("G", UDim2.fromOffset(800, 700), true)
	task.wait()
	G.CanvasPosition = Vector2.new(300, 250)
	G.CanvasSize = UDim2.fromOffset(300, 200)
	Results.g_immediate = Read(G)
	-- (h) parented but not laid out yet (same frame as creation)
	local H = Make("H", UDim2.fromOffset(600, 500), true)
	H.CanvasPosition = Vector2.new(50, 60)
	Results.h_immediate = Read(H)
	-- (i) negative
	local I = Make("I", UDim2.fromOffset(600, 500), true)
	task.wait()
	I.CanvasPosition = Vector2.new(-20, -30)
	Results.i_immediate = Read(I)
	task.wait()
	for _, Frame in { A, B, C, D, E, G, H, I } do
		Results[`{Frame.Name}_after`] = Read(Frame)
	end
	return Results
end
