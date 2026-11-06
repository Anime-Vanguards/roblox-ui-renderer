-- Text in a box a few float32 ulps narrower / wider than the text (layout-probe-autorich: an
-- AutomaticSize label under UIScale is a hair narrower than its own TextBounds and Studio still
-- reports the full width, where the trailing-whitespace rule trimmed the last space offline).
-- For each case an AutomaticSize XY reference label gives TextBounds.X (TB); fixed labels then get
-- widths TB x (1 + k x 3e-8) (scale sizes in a 100 px parent), k = -8..8, wrapped and not.
return function(Target)
	local Results = {}
	local Face = Font.new("rbxassetid://11702779409", Enum.FontWeight.ExtraBold)
	local Texts = { "Friends Only", "End ", "Hello" }
	local Pending = {}
	for ScaleIndex, Scale in { 0.8391608595848083, 1.3, 1, 0.75 } do
		local Holder = Instance.new("Frame")
		Holder.Name = `S{ScaleIndex}`
		Holder.BackgroundTransparency = 1
		Holder.Position = UDim2.fromOffset(((ScaleIndex - 1) % 2) * 600, ((ScaleIndex - 1) // 2) * 450)
		Holder.Size = UDim2.fromOffset(100, 100)
		Holder.Parent = Target
		local UIScale = Instance.new("UIScale")
		UIScale.Scale = Scale
		UIScale.Parent = Holder
		for TextIndex, Text in Texts do
			for _, Rich in { true, false } do
				local Reference = Instance.new("TextLabel")
				Reference.BackgroundTransparency = 1
				Reference.AutomaticSize = Enum.AutomaticSize.XY
				Reference.Size = UDim2.new()
				Reference.FontFace = Face
				Reference.TextSize = 16
				Reference.RichText = Rich
				Reference.Text = Text
				Reference.Parent = Holder
				table.insert(Pending, { Key = `{ScaleIndex}_{TextIndex}_{Rich}`, Label = Reference, Scale = Scale, Holder = Holder, Text = Text, Rich = Rich })
			end
		end
	end
	task.wait()
	local Labels = {}
	for _, Entry in Pending do
		local Bounds = Entry.Label.TextBounds.X
		Results[`{Entry.Key}_ref`] = { Entry.Label.AbsoluteSize.X, Bounds, Entry.Label.TextBounds.Y }
		for _, Wrapped in { false, true } do
			for K = -8, 8 do
				local L = Instance.new("TextLabel")
				L.BackgroundTransparency = 1
				L.Size = UDim2.new(Bounds / Entry.Scale * (1 + K * 3e-8) / 100, 0, 0, 40)
				L.FontFace = Face
				L.TextSize = 16
				L.RichText = Entry.Rich
				L.TextWrapped = Wrapped
				L.Text = Entry.Text
				L.Parent = Entry.Holder
				table.insert(Labels, { Key = `{Entry.Key}_{Wrapped}_{K}`, Label = L })
			end
		end
	end
	task.wait()
	for _, Entry in Labels do
		local L = Entry.Label
		Results[Entry.Key] = { L.AbsoluteSize.X, L.TextBounds.X, L.TextBounds.Y }
	end
	return Results
end
