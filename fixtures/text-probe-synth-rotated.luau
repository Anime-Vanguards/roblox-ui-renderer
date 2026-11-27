-- Synthetic italic under a rotation: faces without an italic file (FredokaOne, Bangers, LuckiestGuy) set Italic through
-- FontFace and through rich <i>, at TextSize 21 (a fractional 31.5 px at 1.5x) and 36, turned -13, 13, 30 and 45 degrees,
-- white on the holder's flat background (the labels are transparent). A row of the same labels unturned is the control.
-- rur compare --fixture fixtures/text-probe-synth-rotated.luau --viewport 1600x920 --time 0.2 --background 0,0,0
return function(Target)
	local Holder = Instance.new("Frame")
	Holder.Name = "Holder"
	Holder.BorderSizePixel = 0
	Holder.BackgroundColor3 = Color3.fromRGB(24, 26, 40)
	Holder.Size = UDim2.fromOffset(1600, 920)
	Holder.Parent = Target
	local Families = { "FredokaOne", "Bangers", "LuckiestGuy" }
	local Angles = { 0, -13, 13, 30, 45 }
	for A, Angle in Angles do
		for F, Family in Families do
			for V, Size in { 21, 36 } do
				local Rich = V == 2
				local L = Instance.new("TextLabel")
				L.Name = string.format("A%dF%dS%d", A, F, Size)
				L.BackgroundTransparency = 1
				L.TextColor3 = Color3.fromRGB(250, 250, 250)
				L.TextSize = Size
				L.Size = UDim2.fromOffset(200, 60)
				L.AnchorPoint = Vector2.new(0.5, 0.5)
				L.Position = UDim2.fromOffset(120 + ((F - 1) * 2 + V - 1) * 225, 92 + (A - 1) * 180)
				if Rich then
					L.FontFace = Font.new("rbxasset://fonts/families/" .. Family .. ".json")
					L.RichText = true
					L.Text = "<i>Wolf gjp</i> 42"
				else
					L.FontFace = Font.new("rbxasset://fonts/families/" .. Family .. ".json", Enum.FontWeight.Regular, Enum.FontStyle.Italic)
					L.Text = "Italic Wolf"
				end
				L.Rotation = Angle
				L.Parent = Holder
			end
		end
		-- the same at a fractional position, FredokaOne only
		local L = Instance.new("TextLabel")
		L.Name = string.format("A%dFrac", A)
		L.BackgroundTransparency = 1
		L.TextColor3 = Color3.fromRGB(250, 250, 250)
		L.TextSize = 30
		L.Size = UDim2.new(0, 220, 0, 60)
		L.AnchorPoint = Vector2.new(0.5, 0.5)
		L.Position = UDim2.new(0.0003, 1470, 0, 92.37 + (A - 1) * 180)
		L.FontFace = Font.new("rbxasset://fonts/families/FredokaOne.json", Enum.FontWeight.Bold, Enum.FontStyle.Italic)
		L.Text = "Bold Italic 7"
		L.Rotation = Angle
		L.Parent = Holder
	end
end
