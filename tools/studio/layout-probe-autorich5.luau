-- Rounding of an AutomaticSize text label's width under UIScale (layout-probe-autorich: AbsoluteSize.X
-- is float32(float32(TextBounds.X / UIScale) x UIScale), except 10 labels whose TextBounds.X is a whole
-- number and whose width then comes out one ulp above it instead of one ulp below). Many text widths
-- (runs of "x", "m" and "l" of 1 to 40 characters) at five scales, rich and plain.
return function(Target)
	local Results = {}
	local Labels = {}
	local Face = Font.new("rbxasset://fonts/families/SourceSansPro.json")
	for ScaleIndex, Scale in { 1.3, 0.9, 1.1, 0.7, 0.8391608595848083 } do
		local Holder = Instance.new("Frame")
		Holder.Name = `S{ScaleIndex}`
		Holder.BackgroundTransparency = 1
		Holder.Position = UDim2.fromOffset(0, (ScaleIndex - 1) * 20)
		Holder.Size = UDim2.fromOffset(100, 100)
		Holder.Parent = Target
		local UIScale = Instance.new("UIScale")
		UIScale.Scale = Scale
		UIScale.Parent = Holder
		for _, Character in { "x", "m", "l" } do
			for Count = 1, 40 do
				for _, Rich in { true, false } do
					local L = Instance.new("TextLabel")
					L.BackgroundTransparency = 1
					L.AutomaticSize = Enum.AutomaticSize.XY
					L.Size = UDim2.new()
					L.FontFace = Face
					L.TextSize = 16
					L.RichText = Rich
					L.Text = string.rep(Character, Count)
					L.Parent = Holder
					table.insert(Labels, { Key = `{ScaleIndex}_{Character}_{Count}_{Rich}`, Label = L })
				end
			end
		end
	end
	task.wait()
	for _, Entry in Labels do
		local L = Entry.Label
		Results[Entry.Key] = { L.AbsoluteSize.X, L.TextBounds.X }
	end
	return Results
end
