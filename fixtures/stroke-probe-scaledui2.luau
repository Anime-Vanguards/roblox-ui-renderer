-- Text UIStroke with StrokeSizingMode.ScaledSize under a UIScale whose TextSize x UIScale is fractional (e.g.
-- 40 x 0.885 = 35.4, 40 x 0.8875 = 35.5): which reference size Studio multiplies Thickness by (unrounded,
-- floored or rounded). Plain labels only; follows fixtures/stroke-probe-scaledui.luau.
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

	local Row = 0
	for _, Scale in { 0.885, 0.8875, 0.9125, 1.1125, 0.7625 } do
		for _, Size in { 40, 37, 23 } do
			local Holder = Instance.new("Frame")
			Holder.Name = `S{Scale}_T{Size}`
			Holder.BackgroundTransparency = 1
			Holder.Position = UDim2.fromOffset(10 + (Row % 3) * 390, 10 + (Row // 3) * 80)
			Holder.Size = UDim2.fromOffset(380, 70)
			Holder.Parent = Target
			local UIScale = Instance.new("UIScale")
			UIScale.Scale = Scale
			UIScale.Parent = Holder
			for Index, Thickness in { 0.04, 0.05724, 0.07, 0.09 } do
				local Label = Label(Holder, `P{Index}`, "Hi", (Index - 1) * 90, 0, 85, 60, Thickness)
				Label.TextSize = Size
			end
			Row += 1
		end
	end
end
