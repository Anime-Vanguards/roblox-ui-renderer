-- Repro: ScrollingFrame / CanvasGroup / Frame with ClipsDescendants = false, the
-- structure of a missions list (its window code sets
-- ClipsDescendants = false on its ScrollingFrame, and Studio draws the third mission card below the
-- list's bottom edge where the offline export clipped it).
--   node cli/rur.mjs compare --fixture fixtures/stories-scroll-noclip.luau --viewport 1000x620 --time 0.3 --background 20,20,26
return function(Target)
	local function Box(Name, Parent, X, Y, W, H, Color)
		local Box = Instance.new("Frame")
		Box.Name = Name
		Box.Position = UDim2.fromOffset(X, Y)
		Box.Size = UDim2.fromOffset(W, H)
		Box.BackgroundColor3 = Color
		Box.BorderSizePixel = 0
		Box.Parent = Parent
		return Box
	end

	-- Children overflowing every side of a 180x140 container, plus one inside
	local function Fill(Parent)
		Box("Inside", Parent, 20, 20, 60, 40, Color3.fromRGB(90, 160, 230))
		Box("Below", Parent, 40, 110, 70, 80, Color3.fromRGB(230, 120, 90))
		Box("Right", Parent, 140, 50, 90, 30, Color3.fromRGB(120, 220, 120))
		Box("Above", Parent, 100, -30, 50, 50, Color3.fromRGB(220, 200, 90))
		Box("Left", Parent, -25, 70, 45, 25, Color3.fromRGB(200, 110, 220))
	end

	local function Label(Text, X, Y)
		local Caption = Instance.new("TextLabel")
		Caption.Name = "Caption"
		Caption.Position = UDim2.fromOffset(X, Y)
		Caption.Size = UDim2.fromOffset(180, 16)
		Caption.BackgroundTransparency = 1
		Caption.Font = Enum.Font.SourceSans
		Caption.TextSize = 14
		Caption.TextColor3 = Color3.fromRGB(200, 200, 200)
		Caption.TextXAlignment = Enum.TextXAlignment.Left
		Caption.Text = Text
		Caption.Parent = Target
	end

	local function Scroller(Name, X, Y, Clips, CanvasY, Thickness)
		local Scroll = Instance.new("ScrollingFrame")
		Scroll.Name = Name
		Scroll.Position = UDim2.fromOffset(X, Y)
		Scroll.Size = UDim2.fromOffset(180, 140)
		Scroll.BackgroundColor3 = Color3.fromRGB(50, 50, 60)
		Scroll.BorderSizePixel = 0
		Scroll.CanvasSize = UDim2.fromOffset(0, 260)
		Scroll.ScrollBarThickness = Thickness
		Scroll.ClipsDescendants = Clips
		Scroll.Parent = Target
		Fill(Scroll)
		-- Scrolled once laid out: assigned right away, Studio sometimes clamps it to 0 (two captures of
		-- the same mount disagreed)
		if CanvasY ~= 0 then
			task.delay(0.1, function()
				Scroll.CanvasPosition = Vector2.new(0, CanvasY)
			end)
		end
		return Scroll
	end

	-- Row 1: ScrollingFrames (no clip, default clip, no clip scrolled by 30, no clip without a bar)
	Label("Scroll noclip", 60, 20)
	Scroller("ScrollNoClip", 60, 60, false, 0, 8)
	Label("Scroll clip", 300, 20)
	Scroller("ScrollClip", 300, 60, true, 0, 8)
	Label("Scroll noclip scrolled", 540, 20)
	Scroller("ScrollNoClipScrolled", 540, 60, false, 30, 8)
	Label("Scroll noclip no bar", 780, 20)
	Scroller("ScrollNoClipNoBar", 780, 60, false, 0, 0)

	-- Row 2: CanvasGroup and Frame without clipping, and a non-clipping ScrollingFrame inside a clipping Frame
	Label("CanvasGroup noclip", 60, 330)
	local Group = Instance.new("CanvasGroup")
	Group.Name = "GroupNoClip"
	Group.Position = UDim2.fromOffset(60, 370)
	Group.Size = UDim2.fromOffset(180, 140)
	Group.BackgroundColor3 = Color3.fromRGB(50, 50, 60)
	Group.BorderSizePixel = 0
	Group.ClipsDescendants = false
	Group.Parent = Target
	Fill(Group)

	Label("Frame noclip", 300, 330)
	local Plain = Box("FrameNoClip", Target, 300, 370, 180, 140, Color3.fromRGB(50, 50, 60))
	Plain.ClipsDescendants = false
	Fill(Plain)

	Label("Scroll noclip in clipping Frame", 540, 330)
	local Outer = Box("OuterClip", Target, 540, 370, 220, 170, Color3.fromRGB(35, 35, 45))
	Outer.ClipsDescendants = true
	local Inner = Instance.new("ScrollingFrame")
	Inner.Name = "InnerNoClip"
	Inner.Position = UDim2.fromOffset(20, 15)
	Inner.Size = UDim2.fromOffset(180, 140)
	Inner.BackgroundColor3 = Color3.fromRGB(50, 50, 60)
	Inner.BorderSizePixel = 0
	Inner.CanvasSize = UDim2.fromOffset(0, 260)
	Inner.ScrollBarThickness = 8
	Inner.ClipsDescendants = false
	Inner.Parent = Outer
	Fill(Inner)
end
