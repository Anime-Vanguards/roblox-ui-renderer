-- A UI kit's button body (button story "Magenta": pill 177 x 57.525 under UIScale 0.885, a pink Outer stroke at
-- ZIndex -99, a black Outer stroke with a transparency UIGradient, an invisible Inner stroke, a UIShadow),
-- rebuilt with one ingredient removed per copy to find which one Studio draws differently at the rims.
-- compare: --viewport 1200x800 --time 0.2 --background 46,46,46
return function(Target)
	local Pink = Color3.new(1, 0.1568627506494522, 0.5098039507865906)
	local function Body(Name, X, Y, Options)
		local Holder = Instance.new("Frame")
		Holder.Name = Name
		Holder.BackgroundTransparency = 1
		Holder.Position = UDim2.fromOffset(X, Y)
		Holder.Size = UDim2.fromOffset(200, 65)
		if not Options.NoScale then
			local Scale = Instance.new("UIScale")
			Scale.Scale = 0.8849999904632568
			Scale.Parent = Holder
		end
		local S = if Options.NoScale then 0.8849999904632568 else 1
		local F = Instance.new("Frame")
		F.Name = "Body"
		F.BackgroundColor3 = Pink
		F.BackgroundTransparency = if Options.NoFill then 1 else 0
		F.BorderSizePixel = 0
		F.Position = UDim2.fromOffset(0, 0)
		F.Size = UDim2.fromOffset(200 * S, 65 * S)
		local Corner = Instance.new("UICorner")
		Corner.CornerRadius = UDim.new(0.5, 0)
		Corner.Parent = F
		if not Options.NoShadow then
			local Shadow = Instance.new("UIShadow")
			Shadow.BlurRadius = UDim.new(0, 24)
			Shadow.Color = Pink
			Shadow.Transparency = 0.82
			Shadow.ZIndex = -1
			Shadow.Parent = F
		end
		if not Options.NoPink then
			local Stroke = Instance.new("UIStroke")
			Stroke.Color = Pink
			Stroke.Thickness = 2 * S
			Stroke.ZIndex = -99
			Stroke.BorderStrokePosition = Enum.BorderStrokePosition.Outer
			Stroke.Parent = F
		end
		if not Options.NoBlack then
			local Stroke = Instance.new("UIStroke")
			Stroke.Color = Color3.new(0, 0, 0)
			Stroke.Thickness = 2.5 * S
			Stroke.ZIndex = 1
			Stroke.BorderStrokePosition = Enum.BorderStrokePosition.Outer
			if not Options.NoGradient then
				local Gradient = Instance.new("UIGradient")
				Gradient.Rotation = 180
				Gradient.Transparency = NumberSequence.new({ NumberSequenceKeypoint.new(0, 0.3), NumberSequenceKeypoint.new(0.5, 0), NumberSequenceKeypoint.new(1, 0.6) })
				Gradient.Parent = Stroke
			end
			Stroke.Parent = F
		end
		F.Parent = Holder
		Holder.Parent = Target
	end
	Body("Full", 40, 30, {})
	Body("NoShadow", 300, 30, { NoShadow = true })
	Body("NoPink", 560, 30, { NoPink = true })
	Body("NoBlack", 820, 30, { NoBlack = true })
	Body("NoGradient", 40, 150, { NoGradient = true })
	Body("NoScale", 300, 150, { NoScale = true })
	Body("NoFill", 560, 150, { NoFill = true })
	Body("OnlyBlack", 820, 150, { NoShadow = true, NoPink = true, NoFill = true })
	Body("OnlyPink", 40, 270, { NoShadow = true, NoBlack = true, NoFill = true })
	Body("OnlyBlackNoGrad", 300, 270, { NoShadow = true, NoPink = true, NoFill = true, NoGradient = true })
	Body("PinkBlackNoShadowNoFill", 560, 270, { NoShadow = true, NoFill = true })
	Body("FillOnly", 820, 270, { NoShadow = true, NoPink = true, NoBlack = true })
end
