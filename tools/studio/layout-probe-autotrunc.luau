-- AutomaticSize X text with TextTruncate (a bracket list's switch label "TRAITS": AutoX, Size
-- (0, 0, 0, 16), AtEnd, UISizeConstraint MaxSize): Studio 38 wide, offline 0 (truncated to nothing
-- while measuring). With and without a MaxSize cap, wrapped or not, both truncate modes.
return function(Target)
	local Results = {}
	local Labels = {}
	local Face = Font.new("rbxasset://fonts/families/SourceSansPro.json")
	local Count = 0
	for _, Mode in { "AtEnd", "SplitWord", "None" } do
		for _, Text in { "TRAITS", "A somewhat longer label text" } do
			for _, Cap in { false, 60, 500 } do
				for _, Wrap in { false, true } do
					Count += 1
					local L = Instance.new("TextLabel")
					L.Position = UDim2.fromOffset(((Count - 1) % 4) * 300, ((Count - 1) // 4) * 30)
					L.AutomaticSize = Enum.AutomaticSize.X
					L.Size = UDim2.fromOffset(0, 16)
					L.FontFace = Face
					L.TextSize = 16
					L.Text = Text
					L.TextWrapped = Wrap
					L.TextTruncate = Enum.TextTruncate[Mode]
					L.Parent = Target
					if Cap then
						local C = Instance.new("UISizeConstraint")
						C.MaxSize = Vector2.new(Cap, math.huge)
						C.Parent = L
					end
					table.insert(Labels, { Key = `{Mode}_{#Text}_{Cap}_{Wrap}`, Label = L })
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
