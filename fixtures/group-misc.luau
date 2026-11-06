-- CanvasGroup odds and ends. Row 1 (groups on whole logical pixels): UIGradient on a group with an opaque
-- background, the same on a transparent group, a plain frame with that gradient; GroupColor3 over
-- semi-transparent children. Row 2 (odd logical sizes and positions, i.e. resampled textures): a rounded
-- opaque group, a nested resampled group, text, a UIStroke and a rounded child, a UIGradient.
-- compare: --viewport 1200x700 --time 0.2 --background 20,20,26
return function(Target)
	local function Gradient(Parent)
		local G = Instance.new("UIGradient")
		G.Color = ColorSequence.new(Color3.fromRGB(255, 60, 60), Color3.fromRGB(60, 60, 255))
		G.Transparency = NumberSequence.new({ NumberSequenceKeypoint.new(0, 0), NumberSequenceKeypoint.new(0.7, 0.35), NumberSequenceKeypoint.new(1, 0.6) })
		G.Parent = Parent
		return G
	end

	local function Frame(Parent, Name, X, Y, W, H, Color, Transparency)
		local F = Instance.new("Frame")
		F.Name = Name
		F.Position = UDim2.fromOffset(X, Y)
		F.Size = UDim2.fromOffset(W, H)
		F.BackgroundColor3 = Color
		F.BackgroundTransparency = Transparency or 0
		F.BorderSizePixel = 0
		F.Parent = Parent
		return F
	end

	local function Group(Name, X, Y, W, H, Props)
		local G = Instance.new("CanvasGroup")
		G.Name = Name
		G.Position = UDim2.fromOffset(X, Y)
		G.Size = UDim2.fromOffset(W, H)
		G.BackgroundColor3 = Color3.fromRGB(40, 70, 130)
		G.BorderSizePixel = 0
		for Key, Value in Props or {} do
			(G :: any)[Key] = Value
		end
		G.Parent = Target
		return G
	end

	-- Row 1: gradients
	local A = Group("GradientOpaque", 20, 20, 200, 80)
	Gradient(A)
	Frame(A, "White", 20, 10, 160, 40, Color3.new(1, 1, 1))
	local B = Group("GradientClear", 240, 20, 200, 80, { BackgroundTransparency = 1 })
	Gradient(B)
	Frame(B, "White", 20, 10, 160, 40, Color3.new(1, 1, 1))
	local C = Frame(Target, "GradientFrame", 460, 20, 200, 80, Color3.new(1, 1, 1))
	Gradient(C)

	-- Row 1: GroupColor3 over semi-transparent children
	local D = Group("Tinted", 680, 20, 256, 80, { BackgroundTransparency = 1, GroupColor3 = Color3.fromRGB(255, 120, 120) })
	for K = 0, 31 do
		Frame(D, `S{K}`, K * 8, 0, 8, 40, Color3.fromRGB(K * 8, 255 - K * 8, (K * 56) % 256), 0.5)
		Frame(D, `T{K}`, K * 8, 40, 8, 40, Color3.fromRGB(255 - K * 8, K * 8, 200), 0)
	end

	-- Row 2: resampled textures (odd sizes and positions)
	local E = Group("RoundedOpaque", 21, 151, 181, 101)
	local Corner = Instance.new("UICorner")
	Corner.CornerRadius = UDim.new(0, 21)
	Corner.Parent = E
	Frame(E, "Bar", 41, 11, 21, 71, Color3.fromRGB(250, 220, 60))
	Frame(E, "Wide", 81, 41, 81, 21, Color3.fromRGB(90, 230, 140))

	local F = Group("Outer", 223, 151, 181, 101, { BackgroundColor3 = Color3.fromRGB(70, 40, 90) })
	local Inner = Instance.new("CanvasGroup")
	Inner.Name = "Inner"
	Inner.Position = UDim2.fromOffset(11, 11)
	Inner.Size = UDim2.fromOffset(121, 61)
	Inner.BackgroundColor3 = Color3.fromRGB(30, 110, 90)
	Inner.BorderSizePixel = 0
	Inner.GroupTransparency = 0.2
	Inner.Parent = F
	Frame(Inner, "Bar", 21, 11, 13, 41, Color3.fromRGB(250, 220, 60))
	Frame(Inner, "Bar2", 71, 21, 31, 11, Color3.new(1, 1, 1))

	local G = Group("Text", 425, 151, 181, 101)
	local Label = Instance.new("TextLabel")
	Label.Name = "Label"
	Label.Position = UDim2.fromOffset(9, 9)
	Label.Size = UDim2.fromOffset(161, 41)
	Label.BackgroundTransparency = 1
	Label.Text = "Resampled"
	Label.TextColor3 = Color3.new(1, 1, 1)
	Label.FontFace = Font.new("rbxasset://fonts/families/FredokaOne.json")
	Label.TextSize = 28
	Label.Parent = G
	local Small = Label:Clone()
	Small.Name = "Small"
	Small.Position = UDim2.fromOffset(9, 55)
	Small.Text = "tiny text 123"
	Small.TextSize = 14
	Small.FontFace = Font.new("rbxasset://fonts/families/GothamSSm.json")
	Small.Parent = G

	local H = Group("Stroke", 627, 151, 181, 101)
	local Boxed = Frame(H, "Boxed", 21, 21, 61, 41, Color3.fromRGB(200, 90, 60))
	local Stroke = Instance.new("UIStroke")
	Stroke.Thickness = 3
	Stroke.Color = Color3.new(1, 1, 1)
	Stroke.Parent = Boxed
	local Round = Frame(H, "Round", 101, 21, 61, 61, Color3.fromRGB(240, 240, 120))
	local RoundCorner = Instance.new("UICorner")
	RoundCorner.CornerRadius = UDim.new(0, 17)
	RoundCorner.Parent = Round

	local I = Group("GradientResampled", 829, 151, 181, 101)
	Gradient(I)
	Frame(I, "White", 21, 11, 141, 41, Color3.new(1, 1, 1))
	Frame(I, "Dark", 21, 61, 141, 21, Color3.fromRGB(20, 20, 20))

	-- Row 3: a resampled transparent group holding semi-transparent children and an image (a UI kit's button style)
	local J = Group("Button", 21, 301, 239, 63, { BackgroundTransparency = 1 })
	local JCorner = Instance.new("UICorner")
	JCorner.CornerRadius = UDim.new(0, 31)
	JCorner.Parent = J
	Frame(J, "Glass", 0, 0, 239, 63, Color3.new(1, 1, 1), 0.85)
	local Slash = Instance.new("ImageLabel")
	Slash.Name = "Slash"
	Slash.Size = UDim2.fromScale(1, 1)
	Slash.BackgroundTransparency = 1
	Slash.Image = "rbxasset://textures/ui/Motor.png"
	Slash.ImageTransparency = 0.6
	Slash.ScaleType = Enum.ScaleType.Crop
	Slash.Parent = J
end
