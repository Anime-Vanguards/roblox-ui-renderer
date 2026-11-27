-- compare: --viewport 1200x900 --time 0.3 --background 20,20,26
-- Sliced images at SliceScale 0 (the corners and edges have no size, so only the middle cell is left over the whole
-- image) and at small and negative SliceScales beside it: textures with distinct borders and centres, centres larger
-- and smaller than the image, Pixelated cells that show which texels the middle takes, a degenerate centre, a centre
-- on the texture's edge, an ImageRect, a UICorner, and the same under UIScale for fractional device sizes.
return function(Target)
	local Root = Instance.new("Frame")
	Root.Name = "Root"
	Root.Size = UDim2.fromScale(1, 1)
	Root.BackgroundTransparency = 1
	Root.BorderSizePixel = 0
	Root.Parent = Target

	local function Slice(Parent, Name, Uri, X, Y, W, H, Center, Props)
		local Label = Instance.new("ImageLabel")
		Label.Name = Name
		Label.Position = UDim2.fromOffset(X, Y)
		Label.Size = UDim2.fromOffset(W, H)
		Label.BackgroundColor3 = Color3.fromRGB(60, 60, 72)
		Label.BorderSizePixel = 0
		Label.Image = Uri
		Label.ScaleType = Enum.ScaleType.Slice
		Label.SliceCenter = Center
		Label.SliceScale = 0
		for Key, Value in Props or {} do
			if Key ~= "Radius" then
				(Label :: any)[Key] = Value
			end
		end
		if Props and Props.Radius then
			local Corner = Instance.new("UICorner")
			Corner.CornerRadius = UDim.new(0, Props.Radius)
			Corner.Parent = Label
		end
		Label.Parent = Parent
		return Label
	end

	local Stone = "rbxasset://textures/ui/LegacyRbxGui/StoneBlockSide.png" -- 32x32, opaque
	local Motor = "rbxasset://textures/ui/Motor.png" -- 16x16, opaque
	local Glow = "rbxasset://textures/ui/Lobby/Buttons/glow_nine_slice.png" -- 156x156, soft alpha
	local StoneCenter = Rect.new(8, 8, 24, 24)
	local GlowCenter = Rect.new(60, 60, 96, 96)
	local Pixelated = { ResampleMode = Enum.ResamplerMode.Pixelated }

	local function Cases(Parent)
		-- Stone: the middle 16 texels stretched over images larger and smaller than it
		local X = 10
		for _, Dims in { { 96, 96 }, { 120, 40 }, { 40, 120 }, { 24, 24 }, { 16, 16 }, { 10, 10 }, { 7, 13 } } do
			Slice(Parent, `Stone{Dims[1]}x{Dims[2]}`, Stone, X, 10, Dims[1], Dims[2], StoneCenter)
			X += Dims[1] + 10
		end
		-- The same Pixelated: which texels the middle shows
		X = 10
		for _, Dims in { { 96, 96 }, { 48, 24 }, { 24, 24 }, { 12, 12 }, { 9, 9 } } do
			Slice(Parent, `StonePix{Dims[1]}x{Dims[2]}`, Stone, X, 140, Dims[1], Dims[2], StoneCenter, Pixelated)
			X += Dims[1] + 10
		end
		-- Glow: a 36-texel middle over larger and smaller images
		X = 10
		for _, Dims in { { 150, 100 }, { 36, 36 }, { 24, 24 }, { 60, 20 } } do
			Slice(Parent, `Glow{Dims[1]}x{Dims[2]}`, Glow, X, 250, Dims[1], Dims[2], GlowCenter)
			X += Dims[1] + 10
		end
		-- Motor's middle cells: wide, degenerate (one texel), on the texture's edge, off-centre
		X = 320
		for Index, Center in { Rect.new(2, 2, 14, 14), Rect.new(8, 8, 8, 8), Rect.new(0, 0, 16, 16), Rect.new(0, 4, 10, 16), Rect.new(3, 1, 7, 12) } do
			Slice(Parent, `MotorPix{Index}`, Motor, X, 250, 48, 48, Center, Pixelated)
			Slice(Parent, `Motor{Index}`, Motor, X, 308, 48, 48, Center)
			X += 58
		end
		-- An ImageRect, a UICorner, a tinted soft image
		Slice(Parent, "StoneRect", Stone, 10, 370, 80, 60, Rect.new(4, 4, 12, 12), { ImageRectOffset = Vector2.new(8, 8), ImageRectSize = Vector2.new(16, 16) })
		Slice(Parent, "StoneRound", Stone, 100, 370, 80, 60, StoneCenter, { Radius = 14 })
		Slice(Parent, "GlowTint", Glow, 190, 370, 120, 60, GlowCenter, { ImageColor3 = Color3.fromRGB(255, 120, 200), ImageTransparency = 0.25 })
		-- Next to SliceScale 0: small scales and a negative zero
		X = 320
		for Index, Scale in { 0.2, 0.05, 0.01, 0.001, 1e-4, 1e-6, -0 } do
			Slice(Parent, `StoneScale{Index}`, Stone, X, 370, 64, 64, StoneCenter, { SliceScale = Scale })
			X += 74
		end
		-- Negative scales, spaced so that what they draw beyond their rect stays apart
		for Index, Case in {
			{ Stone, 520, 110, 64, 64, StoneCenter, -1 },
			{ Stone, 660, 150, 32, 32, StoneCenter, -1 },
			{ Stone, 790, 110, 64, 64, Rect.new(4, 4, 28, 28), -1 },
			{ Stone, 940, 140, 64, 40, Rect.new(4, 12, 20, 24), -1 },
			{ Stone, 1100, 110, 64, 64, StoneCenter, -0.25 },
			{ Motor, 1130, 245, 48, 48, Rect.new(2, 2, 14, 14), -1 },
		} do
			local Props = { SliceScale = Case[7] }
			if Case[1] == Motor then
				Props.ResampleMode = Enum.ResamplerMode.Pixelated
			end
			Slice(Parent, `Negative{Index}`, Case[1], Case[2], Case[3], Case[4], Case[5], Case[6], Props)
		end
	end

	local Plain = Instance.new("Frame")
	Plain.Name = "Plain"
	Plain.Size = UDim2.fromOffset(1200, 450)
	Plain.BackgroundTransparency = 1
	Plain.Parent = Root
	Cases(Plain)

	-- Fractional device sizes and places
	local Scaled = Instance.new("Frame")
	Scaled.Name = "Scaled"
	Scaled.Position = UDim2.fromOffset(0, 455)
	Scaled.Size = UDim2.fromOffset(1200, 450)
	Scaled.BackgroundTransparency = 1
	Scaled.Parent = Root
	local Scale = Instance.new("UIScale")
	Scale.Scale = 0.87
	Scale.Parent = Scaled
	Cases(Scaled)
end
