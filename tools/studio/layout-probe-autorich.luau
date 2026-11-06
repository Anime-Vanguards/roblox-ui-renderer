-- AutomaticSize XY text under UIScale (a stage list's `FriendsOnly/Content/Label`: rich Poppins
-- ExtraBold 16 "Friends Only" under UIScale 0.83916: Studio TextBounds 61.333 = its AbsoluteSize,
-- offline 59.333 = the text without its space). Rich and plain, several scales, fonts, sizes and texts
-- with inner / trailing spaces; AbsoluteSize and TextBounds of each label.
return function(Target)
	local Results = {}
	local Labels = {}
	-- (an array: labels sit at positions that depend on the order, and the available space matters)
	local Faces = {
		{ "Poppins", Font.new("rbxassetid://11702779409", Enum.FontWeight.ExtraBold) },
		{ "Source", Font.new("rbxasset://fonts/families/SourceSansPro.json") },
		{ "Fredoka", Font.new("rbxasset://fonts/families/FredokaOne.json") },
	}
	local Texts = { "Friends Only", "Max Players", "Stage 1", "Hello", "a b c d", "End " }
	for ScaleIndex, Scale in { 0.8391608595848083, 0.75, 0.9, 1.1, 1.3, 0.6 } do
		local Holder = Instance.new("Frame")
		Holder.Name = `S{ScaleIndex}`
		Holder.BackgroundTransparency = 1
		Holder.Position = UDim2.fromOffset(((ScaleIndex - 1) % 3) * 400, ((ScaleIndex - 1) // 3) * 450)
		Holder.Size = UDim2.fromOffset(400, 450)
		Holder.Parent = Target
		local UIScale = Instance.new("UIScale")
		UIScale.Scale = Scale
		UIScale.Parent = Holder
		local Count = 0
		for _, Pair in Faces do
			local FaceName, Face = Pair[1], Pair[2]
			for _, Size in { 16, 20 } do
				for TextIndex, Text in Texts do
					for _, Rich in { true, false } do
						Count += 1
						local L = Instance.new("TextLabel")
						L.BackgroundTransparency = 1
						L.Position = UDim2.fromOffset(((Count - 1) % 4) * 100, ((Count - 1) // 4) * 12)
						L.AutomaticSize = Enum.AutomaticSize.XY
						L.Size = UDim2.new()
						L.FontFace = Face
						L.TextSize = Size
						L.RichText = Rich
						L.Text = Text
						L.Parent = Holder
						table.insert(Labels, { Key = `{ScaleIndex}_{FaceName}_{Size}_{TextIndex}_{Rich}`, Label = L })
					end
				end
			end
		end
	end
	task.wait()
	for _, Entry in Labels do
		local L = Entry.Label
		Results[Entry.Key] = { L.AbsoluteSize.X, L.AbsoluteSize.Y, L.TextBounds.X, L.TextBounds.Y }
	end
	return Results
end
