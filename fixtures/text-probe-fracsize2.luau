-- Fractional pixel sizes, part 2 (text-probe-fracsize.luau: base text quads scale by unfloored / floored
-- size): rich <font size> runs of odd sizes (76.5 px, 37.5 px), runs under UIScale 0.885 / 0.625, and a
-- UITextSizeConstraint that sets an odd size, in GothamBlack where the quad scale shows.
-- rur compare --fixture fixtures/text-probe-fracsize2.luau --viewport 1200x900 --time 0.2 --background 20,20,26
return function(Target)
	local Gotham = Font.fromEnum(Enum.Font.GothamBlack)
	local function Label(Name, Props, Parent)
		local L = Instance.new("TextLabel")
		L.Name = Name
		L.BackgroundTransparency = 1
		L.TextColor3 = Color3.new(1, 1, 1)
		L.TextXAlignment = Enum.TextXAlignment.Left
		L.FontFace = Gotham
		L.Size = UDim2.fromOffset(560, 80)
		for Key, Value in Props do
			L[Key] = Value
		end
		L.Parent = Parent or Target
		return L
	end

	Label("Run51", { TextSize = 30, RichText = true, Text = `Ab <font size="51">WAVE</font> Cd`, Position = UDim2.fromOffset(6, 6) })
	Label("Run25", { TextSize = 30, RichText = true, Text = `WA <font size="25">WAVE gy</font> VE`, Position = UDim2.fromOffset(600, 6) })
	Label("Run50", { TextSize = 31, RichText = true, Text = `Ab <font size="50">WAVE</font> Cd`, Position = UDim2.fromOffset(6, 96) })

	local Constrained = Label("Constraint", { TextScaled = true, Text = "WAVE gy", Size = UDim2.fromOffset(560, 120), Position = UDim2.fromOffset(600, 96) })
	local Constraint = Instance.new("UITextSizeConstraint")
	Constraint.MaxTextSize = 37
	Constraint.Parent = Constrained

	for Index, Value in { 0.885, 0.625 } do
		local Scaled = Instance.new("Frame")
		Scaled.Name = `UIScale{Index}`
		Scaled.BackgroundTransparency = 1
		Scaled.Size = UDim2.fromOffset(1900, 200)
		Scaled.Position = UDim2.fromOffset(6, 230 + (Index - 1) * 150)
		Scaled.Parent = Target
		local Scale = Instance.new("UIScale")
		Scale.Scale = Value
		Scale.Parent = Scaled
		Label(`S{Index}Run`, { TextSize = 30, RichText = true, Text = `Ab <font size="51">WAVE</font> Cd`, Size = UDim2.fromOffset(800, 110) }, Scaled)
		Label(`S{Index}Plain`, { TextSize = 51, Text = "WAVE gy", Size = UDim2.fromOffset(800, 110), Position = UDim2.fromOffset(820, 0) }, Scaled)
	end
end
