-- The border of a CanvasGroup: where it is drawn, on which pixels, and what of the group applies to it.
-- Row 1: BorderSizePixel 1, 2, 3 and 5 at whole places, then 1 and 3 at fractional places and sizes.
-- Row 2: BorderMode Outline, Middle and Inset at 4 px; GroupTransparency 0.5; GroupColor3; a child that
-- overflows the group; a UIGradient with a transparent half; a UICorner (no border is drawn with one).
-- Row 3: the same border on groups turned 2, 20, 45 and 90 degrees, plain and with a banded UIGradient.
-- Row 4: a plain Frame with the same borders at the same fractional places, for the snapping to compare, and
-- two turned plain Frames under a banded UIGradient (their edges are not anti-aliased).
-- Row 5: other sizes. Row 6: an Inset or Middle border with a child over it, tinted, faded and under a gradient.
-- compare: --viewport 1200x900 --time 0.2 --background 20,20,26
return function(Target)
	local function Group(Name, X, Y, W, H, Border, Props, Class)
		local G = Instance.new(Class or "CanvasGroup")
		G.Name = Name
		G.Position = UDim2.fromOffset(X, Y)
		G.Size = UDim2.fromOffset(W, H)
		G.BackgroundColor3 = Color3.fromRGB(0, 200, 90)
		G.BorderColor3 = Color3.fromRGB(255, 60, 200)
		G.BorderSizePixel = Border
		for Key, Value in Props or {} do
			(G :: any)[Key] = Value
		end
		G.Parent = Target
		return G
	end
	local function Child(Parent, X, Y, W, H)
		local F = Instance.new("Frame")
		F.Name = "Child"
		F.Position = UDim2.fromOffset(X, Y)
		F.Size = UDim2.fromOffset(W, H)
		F.BackgroundColor3 = Color3.fromRGB(60, 90, 255)
		F.BorderSizePixel = 0
		F.Parent = Parent
		return F
	end
	local function Bands(Parent, Rotation)
		local U = Instance.new("UIGradient")
		U.Rotation = Rotation
		U.Transparency = NumberSequence.new({
			NumberSequenceKeypoint.new(0, 0),
			NumberSequenceKeypoint.new(0.499, 0),
			NumberSequenceKeypoint.new(0.5, 1),
			NumberSequenceKeypoint.new(1, 1),
		})
		U.Parent = Parent
	end

	-- Row 1
	for Column, Border in { 1, 2, 3, 5 } do
		Child(Group(`Whole{Border}`, 30 + (Column - 1) * 120, 30, 80, 60, Border), 10, 10, 30, 20)
	end
	Child(Group("Frac1a", 510.25, 30.25, 80, 60, 1), 10, 10, 30, 20)
	Child(Group("Frac1b", 630.5, 30.5, 80.5, 60.5, 1), 10, 10, 30, 20)
	Child(Group("Frac1c", 750.75, 30.75, 81, 61, 1), 10, 10, 30, 20)
	Child(Group("Frac3a", 870.333, 30.667, 80.333, 60.667, 3), 10, 10, 30, 20)
	Child(Group("Frac3b", 990.5, 30.5, 81, 61, 3), 10, 10, 30, 20)
	Child(Group("Frac2", 1100.25, 30.75, 60.5, 60.25, 2), 10, 10, 30, 20)

	-- Row 2
	Child(Group("Outline4", 30, 140, 80, 60, 4, { BorderMode = Enum.BorderMode.Outline }), 10, 10, 30, 20)
	Child(Group("Middle4", 150, 140, 80, 60, 4, { BorderMode = Enum.BorderMode.Middle }), 10, 10, 30, 20)
	Child(Group("Inset4", 270, 140, 80, 60, 4, { BorderMode = Enum.BorderMode.Inset }), 10, 10, 30, 20)
	Child(Group("Faded3", 390, 140, 80, 60, 3, { GroupTransparency = 0.5 }), 10, 10, 30, 20)
	Child(Group("Tinted3", 510, 140, 80, 60, 3, { GroupColor3 = Color3.fromRGB(255, 128, 64) }), 10, 10, 30, 20)
	Child(Group("Overflow3", 630, 140, 80, 60, 3), -20, 20, 120, 20)
	local Half = Group("Half3", 750, 140, 80, 60, 3)
	Child(Half, 10, 10, 30, 20)
	Bands(Half, 0)
	local Round = Group("Round3", 870, 140, 80, 60, 3)
	Child(Round, 10, 10, 30, 20)
	local Corner = Instance.new("UICorner")
	Corner.CornerRadius = UDim.new(0, 12)
	Corner.Parent = Round
	Child(Group("Clear3", 990, 140, 80, 60, 3, { BackgroundTransparency = 1 }), 10, 10, 30, 20)
	Child(Group("HalfBack3", 1100, 140, 60, 60, 3, { BackgroundTransparency = 0.5 }), 10, 10, 30, 20)

	-- Row 3
	for Column, Rotation in { 2, 20, 45, 90 } do
		local X = 40 + (Column - 1) * 140
		Child(Group(`Turn{Rotation}`, X, 270, 90, 60, 1, { Rotation = Rotation }), 10, 10, 30, 20)
		Child(Group(`Turn{Rotation}Wide`, X, 400, 90, 60, 3, { Rotation = Rotation }), 10, 10, 30, 20)
		local Banded = Group(`Turn{Rotation}Banded`, X + 580, 270, 90, 60, 1, { Rotation = Rotation })
		Child(Banded, 10, 10, 30, 20)
		Bands(Banded, 90)
		local Wide = Group(`Turn{Rotation}BandedWide`, X + 580, 400, 90, 60, 3, { Rotation = Rotation })
		Child(Wide, 10, 10, 30, 20)
		Bands(Wide, 0)
	end

	-- Row 4
	Group("Plain1a", 30.25, 530.25, 80, 60, 1, nil, "Frame")
	Group("Plain1b", 150.5, 530.5, 80.5, 60.5, 1, nil, "Frame")
	Group("Plain1c", 270.75, 530.75, 81, 61, 1, nil, "Frame")
	Group("Plain3a", 390.333, 530.667, 80.333, 60.667, 3, nil, "Frame")
	Group("Plain3b", 510.5, 530.5, 81, 61, 3, nil, "Frame")
	Group("Plain2", 630.25, 530.75, 60.5, 60.25, 2, nil, "Frame")
	Group("PlainWhole1", 750, 530, 80, 60, 1, nil, "Frame")
	Group("PlainWhole3", 870, 530, 80, 60, 3, nil, "Frame")
	Bands(Group("PlainTurnBanded1", 985, 535, 70, 45, 1, { Rotation = 20 }, "Frame"), 90)
	Bands(Group("PlainTurnBanded0", 1085, 535, 70, 45, 0, { Rotation = 20 }, "Frame"), 90)

	-- Row 6: what lies over an Inset or Middle border and what tints it; a gradient's colours on the border;
	-- the same gradients on plain Frames
	local Sweep = ColorSequence.new(Color3.fromRGB(255, 255, 255), Color3.fromRGB(40, 40, 40))
	local function Colours(Parent)
		local U = Instance.new("UIGradient")
		U.Color = Sweep
		U.Parent = Parent
	end
	Child(Group("InsetTint", 30, 835, 80, 50, 4, { BorderMode = Enum.BorderMode.Inset, GroupColor3 = Color3.fromRGB(255, 128, 64) }), 10, 10, 30, 20)
	Child(Group("InsetChild", 150, 835, 80, 50, 4, { BorderMode = Enum.BorderMode.Inset }), -5, -5, 40, 30)
	Child(Group("MiddleChild", 270, 835, 80, 50, 4, { BorderMode = Enum.BorderMode.Middle }), -5, -5, 40, 30)
	Child(Group("InsetFaded", 390, 835, 80, 50, 4, { BorderMode = Enum.BorderMode.Inset, GroupTransparency = 0.5 }), -5, -5, 40, 30)
	local Swept = Group("Swept3", 510, 835, 80, 50, 3)
	Child(Swept, 10, 10, 30, 20)
	Colours(Swept)
	Colours(Group("PlainSwept3", 630, 835, 80, 50, 3, nil, "Frame"))
	Bands(Group("PlainHalf3", 750, 835, 80, 50, 3, nil, "Frame"), 0)
	Child(Group("PlainInsetChild", 870, 835, 80, 50, 4, { BorderMode = Enum.BorderMode.Inset }, "Frame"), -5, -5, 40, 30)
	local SweptInset = Group("SweptInset", 990, 835, 80, 50, 4, { BorderMode = Enum.BorderMode.Inset })
	Child(SweptInset, -5, -5, 40, 30)
	Colours(SweptInset)
	Child(Group("OutlineFadedChild", 1100, 835, 60, 50, 3, { GroupTransparency = 0.5 }), -5, -5, 40, 30)

	-- Row 5: groups whose places put the border on other device pixel phases
	for Column = 1, 9 do
		local Shift = (Column - 1) / 6
		Child(Group(`Phase{Column}`, 30 + (Column - 1) * 120 + Shift, 640 + Shift, 80, 60, 1), 10, 10, 30, 20)
		Child(Group(`Phase{Column}Wide`, 30 + (Column - 1) * 120 + Shift, 750 + Shift, 80 + Shift, 60 + Shift, 2), 10, 10, 30, 20)
	end
end
