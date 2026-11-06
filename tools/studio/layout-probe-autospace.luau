-- AutomaticSize XY labels whose text ends in whitespace, under UIScale (layout-probe-fredwidth: a lone
-- " " in FredokaOne at 18 px under UIScale 0.6 reads TextBounds 4 px in Studio; offline the automatic
-- width came out a hair narrower than the text, so the space no longer fit and was dropped: 0).
-- AbsoluteSize and TextBounds for several scales, sizes and texts, plain and rich.
return function(Target)
	local Results = {}
	local Labels = {}
	local Faces = {
		{ "Fre", Font.new("rbxasset://fonts/families/FredokaOne.json") },
		{ "SSP", Font.new("rbxasset://fonts/families/SourceSansPro.json") },
	}
	for ScaleIndex, Scale in { 0.6, 0.9, 0.7, 1.3, 0.8391608595848083 } do
		local Holder = Instance.new("Frame")
		Holder.BackgroundTransparency = 1
		Holder.Position = UDim2.fromOffset((ScaleIndex - 1) * 230, 0)
		Holder.Size = UDim2.fromOffset(230, 900)
		Holder.Parent = Target
		local UIScale = Instance.new("UIScale")
		UIScale.Scale = Scale
		UIScale.Parent = Holder
		local Row = 0
		for _, Pair in Faces do
			for _, Size in { 12, 14, 16, 20 } do
				for TextIndex, Text in { " ", "  ", "a ", "ab  ", "x y " } do
					for _, Rich in { false, true } do
						Row += 1
						local L = Instance.new("TextLabel")
						L.BackgroundTransparency = 1
						L.Position = UDim2.fromOffset(0, Row * 6)
						L.AutomaticSize = Enum.AutomaticSize.XY
						L.Size = UDim2.new()
						L.FontFace = Pair[2]
						L.TextSize = Size
						L.RichText = Rich
						L.Text = Text
						L.Parent = Holder
						table.insert(Labels, { Key = `{ScaleIndex}_{Pair[1]}_{Size}_{TextIndex}_{Rich}`, Label = L })
					end
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
