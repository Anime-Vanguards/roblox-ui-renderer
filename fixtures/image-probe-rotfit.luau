-- Turned images as stories use them: ScaleType Fit of square and wide textures at small sizes (an icon tilted
-- 17 or -11 degrees, minified), large and tinted (a banner at -13 degrees), stretched far past the screen, and
-- faint under a UIGradient at -45 degrees. Each rule of the turned-image sampler (mip level, fringe, corners on
-- the 1/256 px grid) is measured on its own in image-probe-rotlod, -rotstrip and -rotfringe; this scene holds
-- the combinations the saved story scenes draw, so a change to the sampler is checked on them in Studio.
-- compare: --viewport 1200x1140 --time 0.5 --background 24,24,30
return function(Target)
	local Square = "rbxasset://textures/explosion.png" -- 128 x 128
	local Wide = "rbxasset://textures/ui/Settings/Help/XboxController.png" -- 473 x 287
	local Strip = "rbxasset://textures/ui/PlayerList/NewAvatarBackground.png" -- 300 x 72

	local function Image(Name, Uri, X, Y, W, H, Rotation, Props)
		local Label = Instance.new("ImageLabel")
		Label.Name = Name
		Label.Position = UDim2.fromOffset(X, Y)
		Label.Size = UDim2.fromOffset(W, H)
		Label.BackgroundTransparency = 1
		Label.BorderSizePixel = 0
		Label.Image = Uri
		Label.ScaleType = Enum.ScaleType.Fit
		Label.Rotation = Rotation
		for Key, Value in Props or {} do
			(Label :: any)[Key] = Value
		end
		Label.Parent = Target
		return Label
	end

	-- Row 1: small tilted icons, square texture in a square rect (minified 3.4 to 5.2 times at 1.5x)
	for Index, Size in { 24.82, 19.21, 21.2, 16.4, 18.9, 32, 48.5 } do
		local X = 20 + (Index - 1) * 70
		Image(`Icon17_{Index}`, Square, X, 20, Size, Size, 17)
		Image(`IconM11_{Index}`, Square, X + 0.37, 80.61, Size, Size, -11)
	end
	-- the same with a wide texture: the fitted quad is shorter than the rect
	for Index, Size in { 24.82, 19.21, 21.2, 16.4, 18.9, 32, 48.5 } do
		local X = 520 + (Index - 1) * 70
		Image(`Wide17_{Index}`, Wide, X, 20, Size, Size, 17)
		Image(`WideM11_{Index}`, Strip, X + 0.37, 80.61, Size, Size, -11)
	end

	-- Row 2: a large tinted banner at -13 degrees, fitted by height and by width
	local Tint = Color3.new(1, 0.9333333373069763, 0.1921568661928177)
	Image("Banner", Wide, 30, 190, 548.33, 214.17, -13, { ImageColor3 = Tint })
	Image("BannerTall", Strip, 640, 170, 214.17, 300, -13, { ImageColor3 = Tint })
	Image("BannerHalf", Wide, 880, 200, 290.5, 190.25, -13, { ImageColor3 = Tint, ImageTransparency = 0.5 })

	-- Row 3: stretched far past both sides of the screen at -13 degrees, half clear
	Image("Beam", Strip, -1000, 600, 3125, 78.125, -13, {
		ScaleType = Enum.ScaleType.Stretch,
		ImageColor3 = Tint,
		ImageTransparency = 0.5,
	})

	-- Row 4: at -45 degrees under a colour UIGradient, faint (ImageTransparency 0.9) and plain; then the same
	-- gradient on an unturned image and on a turned Frame, which tell the gradient's part from the image's
	local function Shine(Parent)
		local Gradient = Instance.new("UIGradient")
		Gradient.Rotation = 90
		Gradient.Color = ColorSequence.new({
			ColorSequenceKeypoint.new(0, Color3.new(0, 0.8667, 1)),
			ColorSequenceKeypoint.new(0.3149, Color3.new(0.2667, 0.8627, 0.9569)),
			ColorSequenceKeypoint.new(0.6349, Color3.new(0.9451, 0.4471, 0.8314)),
			ColorSequenceKeypoint.new(1, Color3.new(1, 0.2039, 0.8157)),
		})
		Gradient.Parent = Parent
	end
	for Index, Transparency in { 0.9, 0.5, 0 } do
		Shine(Image(`Shine{Index}`, Strip, 200 + (Index - 1) * 260, 882, 297, 95, -45, {
			ImageTransparency = Transparency,
		}))
	end
	Image("ShinePlain", Strip, 960, 850, 150, 95, -45)
	Shine(Image("ShineFlat", Strip, 20, 1085, 148.5, 47.5, 0))
	Shine(Image("ShineFlatWide", Wide, 20, 1020, 90, 54.6, 0))
	local Frame = Instance.new("Frame")
	Frame.Name = "FrameShine"
	Frame.Position = UDim2.fromOffset(1040, 1000)
	Frame.Size = UDim2.fromOffset(120, 50)
	Frame.BackgroundColor3 = Color3.new(1, 1, 1)
	Frame.BorderSizePixel = 0
	Frame.Rotation = -45
	Frame.Parent = Target
	Shine(Frame)
end
