-- How much narrower than its text a box can be before the text no longer fits it (wraps / drops a
-- trailing space from TextBounds): layout-probe-autorich2 found plain text compares
-- float32(AbsoluteSize.X x 1.5) >= its px width exactly, while rich text still fits boxes a few ulps
-- narrower. Boxes of (W - E) physical px for text W px wide, E from 1e-5 to 1.5 px, under UIScale 1,
-- 0.83916 and 1.3, rich and plain, wrapped and not, Poppins and SourceSans.
return function(Target)
	local Results = {}
	local Faces = {
		Poppins = Font.new("rbxassetid://11702779409", Enum.FontWeight.ExtraBold),
		Source = Font.new("rbxasset://fonts/families/SourceSansPro.json"),
	}
	local Texts = { "Friends Only", "End " }
	local Gaps = { 1e-5, 1e-4, 1e-3, 0.01, 0.05, 0.1, 0.2, 0.3, 0.4, 0.45, 0.49, 0.5, 0.51, 0.6, 0.75, 0.9, 0.99, 1.01, 1.5 }
	local Pending = {}
	for ScaleIndex, Scale in { 1, 0.8391608595848083, 1.3 } do
		local Holder = Instance.new("Frame")
		Holder.Name = `S{ScaleIndex}`
		Holder.BackgroundTransparency = 1
		Holder.Position = UDim2.fromOffset((ScaleIndex - 1) * 400, 0)
		Holder.Size = UDim2.fromOffset(100, 100)
		Holder.Parent = Target
		local UIScale = Instance.new("UIScale")
		UIScale.Scale = Scale
		UIScale.Parent = Holder
		for FaceName, Face in Faces do
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
					table.insert(Pending, { Key = `{ScaleIndex}_{FaceName}_{TextIndex}_{Rich}`, Label = Reference, Scale = Scale, Holder = Holder, Text = Text, Rich = Rich, Face = Face })
				end
			end
		end
	end
	task.wait()
	local Labels = {}
	for _, Entry in Pending do
		local Bounds = Entry.Label.TextBounds.X
		Results[`{Entry.Key}_ref`] = { Entry.Label.AbsoluteSize.X, Bounds, Entry.Label.TextBounds.Y }
		local Pixels = math.floor(Bounds * 1.5 + 0.5)
		for _, Wrapped in { false, true } do
			for GapIndex, Gap in Gaps do
				local L = Instance.new("TextLabel")
				L.BackgroundTransparency = 1
				L.Size = UDim2.new((Pixels - Gap) / 1.5 / Entry.Scale / 100, 0, 0, 40)
				L.FontFace = Entry.Face
				L.TextSize = 16
				L.RichText = Entry.Rich
				L.TextWrapped = Wrapped
				L.Text = Entry.Text
				L.Parent = Entry.Holder
				table.insert(Labels, { Key = `{Entry.Key}_{Wrapped}_{GapIndex}`, Label = L })
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
