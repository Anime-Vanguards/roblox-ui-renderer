-- Rich vs plain text in boxes of whole / half physical px narrower than the text (layout-probe-autorich3:
-- wrapped rich text still fits a box 0.99 px narrower than it, not 1.01 px; unwrapped rich text never
-- dropped its trailing space from TextBounds). Fixed offsets n (box = 1.5 n px) from 16 px below the
-- text width W to 1 px above, for three fonts and texts with inner / trailing spaces, wrapped and not.
return function(Target)
	local Results = {}
	local Faces = {
		Poppins = { Font.new("rbxassetid://11702779409", Enum.FontWeight.ExtraBold), 16 },
		Source = { Font.new("rbxasset://fonts/families/SourceSansPro.json"), 16 },
		Fredoka = { Font.new("rbxasset://fonts/families/FredokaOne.json"), 20 },
	}
	local Texts = { "Friends Only", "End ", "Aaa bbb ccc ddd" }
	local Pending = {}
	local Count = 0
	for FaceName, Face in Faces do
		for TextIndex, Text in Texts do
			for _, Rich in { true, false } do
				Count += 1
				local Reference = Instance.new("TextLabel")
				Reference.BackgroundTransparency = 1
				Reference.Position = UDim2.fromOffset(0, Count * 20)
				Reference.AutomaticSize = Enum.AutomaticSize.XY
				Reference.Size = UDim2.new()
				Reference.FontFace = Face[1]
				Reference.TextSize = Face[2]
				Reference.RichText = Rich
				Reference.Text = Text
				Reference.Parent = Target
				table.insert(Pending, { Key = `{FaceName}_{TextIndex}_{Rich}`, Label = Reference, Text = Text, Rich = Rich, Face = Face })
			end
		end
	end
	task.wait()
	local Labels = {}
	for Index, Entry in Pending do
		local Bounds = Entry.Label.TextBounds.X
		local Pixels = math.floor(Bounds * 1.5 + 0.5)
		Results[`{Entry.Key}_ref`] = { Entry.Label.AbsoluteSize.X, Bounds, Entry.Label.TextBounds.Y }
		for _, Wrapped in { false, true } do
			for Offset = math.ceil((Pixels - 16) / 1.5), math.ceil(Pixels / 1.5) + 1 do
				local L = Instance.new("TextLabel")
				L.BackgroundTransparency = 1
				L.Position = UDim2.fromOffset(300 + (if Wrapped then 450 else 0), Index * 40)
				L.Size = UDim2.fromOffset(Offset, 40)
				L.FontFace = Entry.Face[1]
				L.TextSize = Entry.Face[2]
				L.RichText = Entry.Rich
				L.TextWrapped = Wrapped
				L.Text = Entry.Text
				L.Parent = Target
				table.insert(Labels, { Key = `{Entry.Key}_{Wrapped}_{Offset}`, Label = L })
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
