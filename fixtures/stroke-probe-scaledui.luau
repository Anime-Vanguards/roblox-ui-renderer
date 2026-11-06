-- Text UIStroke with StrokeSizingMode.ScaledSize under a UIScale: plain TextSize, TextScaled, and TextScaled
-- clamped by a UITextSizeConstraint (an emote card name: MaxTextSize 30, UIScale 1.03,
-- Thickness 0.05724). White stroke on transparent text over black so the stroker radius shows; thicknesses are
-- picked so that "Thickness x TextSize" and "... x UIScale" land on different 0.1 px radii.
-- compare: --viewport 1200x700 --time 0.3 --background 0,0,0
return function(Target)
	local function Label(Parent, Name, Text, X, Y, W, H, Thickness, Props)
		local Label = Instance.new("TextLabel")
		Label.Name = Name
		Label.BackgroundTransparency = 1
		Label.FontFace = Font.new("rbxasset://fonts/families/FredokaOne.json")
		Label.TextSize = 40
		Label.Text = Text
		Label.TextColor3 = Color3.new(1, 1, 1)
		Label.TextTransparency = 1
		Label.TextXAlignment = Enum.TextXAlignment.Left
		Label.TextYAlignment = Enum.TextYAlignment.Top
		Label.Position = UDim2.fromOffset(X, Y)
		Label.Size = UDim2.fromOffset(W, H)
		for Key, Value in Props or {} do
			(Label :: any)[Key] = Value
		end
		Label.Parent = Parent

		local Stroke = Instance.new("UIStroke")
		Stroke.Thickness = Thickness
		Stroke.Color = Color3.new(1, 1, 1)
		Stroke.StrokeSizingMode = Enum.StrokeSizingMode.ScaledSize
		Stroke.LineJoinMode = Enum.LineJoinMode.Miter
		Stroke.Parent = Label
		return Label, Stroke
	end

	for Row, Scale in { 1, 1.03, 0.885, 1.25 } do
		local Holder = Instance.new("Frame")
		Holder.Name = `Scale{Row}`
		Holder.BackgroundTransparency = 1
		Holder.Position = UDim2.fromOffset(10, 10 + (Row - 1) * 170)
		Holder.Size = UDim2.fromOffset(900, 140)
		Holder.Parent = Target
		local UIScale = Instance.new("UIScale")
		UIScale.Scale = Scale
		UIScale.Parent = Holder
		local X = 0
		for Index, Thickness in { 0.04, 0.05724, 0.07 } do
			Label(Holder, `Plain{Index}`, "Hi", X, 0, 80, 60, Thickness)
			Label(Holder, `Scaled{Index}`, "Hid", X, 60, 90, 40, Thickness, { TextScaled = true })
			local Capped = Label(Holder, `Capped{Index}`, "Hid", X + 100, 60, 140, 70, Thickness, { TextScaled = true })
			local Constraint = Instance.new("UITextSizeConstraint")
			Constraint.MaxTextSize = 30
			Constraint.Parent = Capped
			X += 260
		end
	end
end
