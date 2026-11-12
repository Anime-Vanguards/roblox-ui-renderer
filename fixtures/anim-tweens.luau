-- Continuous TweenService animation of every tweenable property type: an easing strip (all 11
-- styles x 3 directions, reversing forever) and a grid of cells, each tweening one or two
-- properties (Color3, UDim2 size/position, AnchorPoint, Rotation, transparency, UICorner radius,
-- UIStroke, UIGradient, TextSize (integer steps), text colour, ZIndex, Visible, CanvasPosition,
-- UIListLayout padding, UIScale, delayed repeats). Every tween repeats forever, so any --time shows
-- motion; it is also a steady load for the live preview.
--   node cli/rur.mjs render --fixture fixtures/anim-tweens.luau --viewport 800x520 --time 1.25
-- Studio captures are only approximate mid-animation (its clock is not controllable); values are
-- verified by the probes in tools/studio/anim-probe-*.luau instead (notes/animation.md).
return function(Target)
	local TweenService = game:GetService("TweenService")
	local RunService = game:GetService("RunService")
	local Tweens = {}

	local function New(ClassName: string, Properties: { [string]: any }, Parent: Instance?): any
		local Object = Instance.new(ClassName)
		for Key, Value in Properties do
			Object[Key] = Value
		end
		Object.Parent = Parent
		return Object
	end

	local function Loop(Object: Instance, Time: number, Goals: { [string]: any }, Style: Enum.EasingStyle?, Direction: Enum.EasingDirection?, Reverses: boolean?, Delay: number?)
		local Info = TweenInfo.new(Time, Style or Enum.EasingStyle.Sine, Direction or Enum.EasingDirection.InOut, -1, Reverses ~= false, Delay or 0)
		local Tween = TweenService:Create(Object, Info, Goals)
		Tween:Play()
		table.insert(Tweens, Tween)
	end

	local Root = New("Frame", {
		Name = "AnimTweens",
		Size = UDim2.fromOffset(800, 520),
		BackgroundColor3 = Color3.fromRGB(24, 26, 32),
		BorderSizePixel = 0,
	}, Target)

	-- Easing strip: one bar per style and direction, width tweened 0 -> 1 and back
	local Strip = New("Frame", {
		Name = "Easing",
		Position = UDim2.fromOffset(12, 12),
		Size = UDim2.fromOffset(776, 200),
		BackgroundTransparency = 1,
	}, Root)
	-- The styles a tween takes: Studio 0.747 lists Spring and Bezier too, and TweenInfo.new raises for them
	local Styles = {}
	for _, Style in Enum.EasingStyle:GetEnumItems() do
		if pcall(TweenInfo.new, 1, Style) then
			table.insert(Styles, Style)
		end
	end
	table.sort(Styles, function(A, B)
		return A.Value < B.Value
	end)
	local Directions = { Enum.EasingDirection.In, Enum.EasingDirection.Out, Enum.EasingDirection.InOut }
	for Column, Direction in Directions do
		for Row, Style in Styles do
			local Track = New("Frame", {
				Name = `{Style.Name}{Direction.Name}`,
				Position = UDim2.new((Column - 1) / 3, 0, 0, (Row - 1) * 18),
				Size = UDim2.new(1 / 3, -8, 0, 14),
				BackgroundColor3 = Color3.fromRGB(44, 48, 58),
				BorderSizePixel = 0,
			}, Strip)
			local Bar = New("Frame", {
				Name = "Bar",
				Size = UDim2.fromScale(0, 1),
				BackgroundColor3 = Color3.fromHSV((Row - 1) / #Styles, 0.65, 0.95),
				BorderSizePixel = 0,
			}, Track)
			Loop(Bar, 1.1, { Size = UDim2.fromScale(1, 1) }, Style, Direction)
		end
	end

	-- Property cells
	local Grid = New("Frame", {
		Name = "Cells",
		Position = UDim2.fromOffset(12, 220),
		Size = UDim2.fromOffset(776, 288),
		BackgroundTransparency = 1,
	}, Root)
	New("UIGridLayout", {
		CellSize = UDim2.fromOffset(88, 88),
		CellPadding = UDim2.fromOffset(10, 10),
		SortOrder = Enum.SortOrder.LayoutOrder,
	}, Grid)

	local Order = 0
	local function Cell(Name: string): Frame
		Order += 1
		return New("Frame", {
			Name = Name,
			LayoutOrder = Order,
			BackgroundColor3 = Color3.fromRGB(36, 40, 50),
			BorderSizePixel = 0,
			ClipsDescendants = true,
		}, Grid)
	end

	local function Box(Parent: Instance, Properties: { [string]: any }): Frame
		local Defaults = {
			Size = UDim2.fromOffset(40, 40),
			Position = UDim2.fromScale(0.5, 0.5),
			AnchorPoint = Vector2.new(0.5, 0.5),
			BackgroundColor3 = Color3.fromRGB(90, 170, 255),
			BorderSizePixel = 0,
		}
		for Key, Value in Properties do
			Defaults[Key] = Value
		end
		return New("Frame", Defaults, Parent)
	end

	-- Color3 (Sine InOut)
	Loop(Box(Cell("Color"), {}), 1.4, { BackgroundColor3 = Color3.fromRGB(255, 90, 120) })

	-- UDim2 size with scale and offset (Back Out)
	Loop(Box(Cell("Size"), { Size = UDim2.new(0.2, 4, 0.2, 4) }), 0.9, { Size = UDim2.new(0.6, 10, 0.5, 20) }, Enum.EasingStyle.Back, Enum.EasingDirection.Out)

	-- UDim2 position (Bounce Out)
	Loop(Box(Cell("Position"), { Size = UDim2.fromOffset(24, 24), Position = UDim2.new(0.5, 0, 0, 14) }), 1.2, { Position = UDim2.new(0.5, 0, 1, -14) }, Enum.EasingStyle.Bounce, Enum.EasingDirection.Out)

	-- AnchorPoint (Quad)
	Loop(Box(Cell("AnchorPoint"), { Size = UDim2.fromOffset(28, 28), AnchorPoint = Vector2.new(0, 0) }), 1, { AnchorPoint = Vector2.new(1, 1) }, Enum.EasingStyle.Quad)

	-- Rotation, linear, no reverse (wraps every 2 s)
	Loop(Box(Cell("Rotation"), { Size = UDim2.fromOffset(46, 18) }), 2, { Rotation = 360 }, Enum.EasingStyle.Linear, Enum.EasingDirection.In, false)

	-- Transparency (Exponential)
	Loop(Box(Cell("Transparency"), {}), 0.8, { BackgroundTransparency = 0.9 }, Enum.EasingStyle.Exponential)

	-- UICorner radius (UDim)
	local Rounded = Box(Cell("CornerRadius"), { Size = UDim2.fromOffset(56, 56) })
	Loop(New("UICorner", { CornerRadius = UDim.new(0, 0) }, Rounded), 1.3, { CornerRadius = UDim.new(0.5, 0) }, Enum.EasingStyle.Cubic)

	-- UIStroke thickness and colour (Elastic)
	local Stroked = Box(Cell("Stroke"), { Size = UDim2.fromOffset(44, 44) })
	Loop(New("UIStroke", { Thickness = 1, Color = Color3.fromRGB(255, 255, 255) }, Stroked), 1.5, { Thickness = 8, Color = Color3.fromRGB(255, 200, 60) }, Enum.EasingStyle.Elastic, Enum.EasingDirection.Out)

	-- UIGradient offset and rotation (Circular)
	local Graded = Box(Cell("Gradient"), { Size = UDim2.fromOffset(70, 70), BackgroundColor3 = Color3.new(1, 1, 1) })
	local Gradient = New("UIGradient", {
		Color = ColorSequence.new(Color3.fromRGB(255, 80, 80), Color3.fromRGB(80, 120, 255)),
		Offset = Vector2.new(-0.5, 0),
	}, Graded)
	Loop(Gradient, 1.6, { Offset = Vector2.new(0.5, 0), Rotation = 180 }, Enum.EasingStyle.Circular)

	-- TextSize steps in whole points; text colour and transparency (Quart)
	local Label = New("TextLabel", {
		Size = UDim2.fromScale(1, 1),
		BackgroundTransparency = 1,
		Text = "Aa",
		TextSize = 10,
		TextColor3 = Color3.fromRGB(255, 255, 255),
		FontFace = Font.fromEnum(Enum.Font.GothamBold),
	}, Cell("TextSize"))
	Loop(Label, 1.2, { TextSize = 40, TextColor3 = Color3.fromRGB(120, 255, 170) }, Enum.EasingStyle.Quart)

	-- ZIndex (integer rounding) swaps which box is on top
	local Stack = Cell("ZIndex")
	local Front = Box(Stack, { Position = UDim2.fromScale(0.4, 0.45), BackgroundColor3 = Color3.fromRGB(255, 170, 60), ZIndex = 1 })
	Box(Stack, { Position = UDim2.fromScale(0.6, 0.55), ZIndex = 5 })
	Loop(Front, 1, { ZIndex = 9 }, Enum.EasingStyle.Linear)

	-- Visible: true -> false flips only at the end of each pass (boolean rule)
	Loop(Box(Cell("Visible"), { BackgroundColor3 = Color3.fromRGB(200, 90, 255) }), 0.5, { Visible = false }, Enum.EasingStyle.Linear)

	-- ScrollingFrame.CanvasPosition
	local Scroller = New("ScrollingFrame", {
		Size = UDim2.fromScale(1, 1),
		CanvasSize = UDim2.fromOffset(0, 300),
		BackgroundTransparency = 1,
		ScrollBarThickness = 4,
		BorderSizePixel = 0,
	}, Cell("CanvasPosition"))
	for Index = 1, 6 do
		Box(Scroller, { Position = UDim2.fromOffset(44, Index * 48 - 20), BackgroundColor3 = Color3.fromHSV(Index / 6, 0.6, 0.9) })
	end
	Loop(Scroller, 1.4, { CanvasPosition = Vector2.new(0, 212) }, Enum.EasingStyle.Sine)

	-- UIListLayout padding
	local Listed = Cell("ListPadding")
	local List = New("UIListLayout", { Padding = UDim.new(0, 0), SortOrder = Enum.SortOrder.LayoutOrder }, Listed)
	for Index = 1, 4 do
		New("Frame", { Size = UDim2.new(1, 0, 0, 12), LayoutOrder = Index, BackgroundColor3 = Color3.fromHSV(Index / 5, 0.5, 0.9), BorderSizePixel = 0 }, Listed)
	end
	Loop(List, 1, { Padding = UDim.new(0, 10) }, Enum.EasingStyle.Quint)

	-- UIScale
	local Scaled = Box(Cell("UIScale"), { Size = UDim2.fromOffset(30, 30) })
	Loop(New("UIScale", { Scale = 0.5 }, Scaled), 0.9, { Scale = 2 }, Enum.EasingStyle.Back, Enum.EasingDirection.InOut)

	-- Delayed repeats (reported Delayed between passes)
	Loop(Box(Cell("DelayRepeat"), { Size = UDim2.fromOffset(20, 20), Position = UDim2.new(0, 14, 0.5, 0) }), 0.5, { Position = UDim2.new(1, -14, 0.5, 0) }, Enum.EasingStyle.Quad, Enum.EasingDirection.Out, false, 0.3)

	-- Image colour and transparency
	local Image = New("ImageLabel", {
		Size = UDim2.fromOffset(60, 60),
		Position = UDim2.fromScale(0.5, 0.5),
		AnchorPoint = Vector2.new(0.5, 0.5),
		BackgroundTransparency = 1,
		Image = "rbxasset://textures/ui/GuiImagePlaceholder.png",
		ImageColor3 = Color3.fromRGB(255, 255, 255),
	}, Cell("ImageColor"))
	Loop(Image, 1.1, { ImageColor3 = Color3.fromRGB(60, 200, 255), ImageTransparency = 0.6 }, Enum.EasingStyle.Sine)

	-- Tweens outlive their destroyed targets (measured), so cancel them once the fixture is unmounted
	local Watcher
	Watcher = RunService.Heartbeat:Connect(function()
		if Root:IsDescendantOf(game) then
			return
		end
		Watcher:Disconnect()
		for _, Tween in Tweens do
			Tween:Cancel()
		end
	end)
end
