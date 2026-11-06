-- FredokaOne "Max Players" at 27 px (TextSize 20 under UIScale 0.9) and 18 px (0.6): Studio's plain
-- text is 130 / 89 px wide where rich text and the offline layout are 136 / 93 (layout-probe-autorich);
-- other texts and sizes agree. Prefixes and pieces of the text, plain and rich, at UIScale 1 (TextSize
-- 18 and 12) and under the two UIScales.
return function(Target)
	local Results = {}
	local Labels = {}
	local Face = Font.new("rbxasset://fonts/families/FredokaOne.json")
	local Texts = { "Max Players", "Max Player", "Max Playe", "Max Play", "Max Pla", "Max Pl", "Max P", "Max ", "Max", "Players", "x P", "x Pl", " P", "Pl", "M", "a", "x", " ", "P", "l", "y", "e", "r", "s", "Max  Players", "MaxPlayers" }
	local Cases = { { 1, 18 }, { 1, 12 }, { 0.9, 20 }, { 0.6, 20 }, { 1, 17 }, { 1, 19 } }
	for CaseIndex, Case in Cases do
		local Holder = Instance.new("Frame")
		Holder.BackgroundTransparency = 1
		Holder.Position = UDim2.fromOffset((CaseIndex - 1) * 190, 0)
		Holder.Size = UDim2.fromOffset(190, 900)
		Holder.Parent = Target
		local UIScale = Instance.new("UIScale")
		UIScale.Scale = Case[1]
		UIScale.Parent = Holder
		for TextIndex, Text in Texts do
			for _, Rich in { false, true } do
				local L = Instance.new("TextLabel")
				L.BackgroundTransparency = 1
				L.Position = UDim2.fromOffset(0, (TextIndex - 1) * 30 + (if Rich then 15 else 0))
				L.AutomaticSize = Enum.AutomaticSize.XY
				L.Size = UDim2.new()
				L.FontFace = Face
				L.TextSize = Case[2]
				L.RichText = Rich
				L.Text = Text
				L.Parent = Holder
				table.insert(Labels, { Key = `{CaseIndex}_{TextIndex}_{Rich}`, Label = L })
			end
		end
	end
	task.wait()
	for _, Entry in Labels do
		local L = Entry.Label
		Results[Entry.Key] = { L.TextBounds.X * 1.5, L.TextBounds.Y * 1.5 }
	end
	return Results
end
