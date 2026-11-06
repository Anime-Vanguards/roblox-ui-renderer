-- AutomaticSize X text that reaches past its parent's edge (layout-probe-autorich: an AutoXY FredokaOne
-- "Max Players" 1 px wider than the space left in its 400 px holder is measured without its space,
-- 130 px instead of 136, like an overflowing line in a fixed box). Labels placed so that the space
-- to the parent's far edge is the text width minus 0.5 .. 40 px: trailing / inner spaces, no spaces,
-- TextTruncate AtEnd / SplitWord, rich and plain, AnchorPoint 0 / 0.5 / 1, under UIScale 1 and 0.9.
return function(Target)
	local Results = {}
	local Labels = {}
	local Face = Font.new("rbxasset://fonts/families/SourceSansPro.json")
	local Texts = { "Max Players", "End ", "Unbroken", "Two words here" }
	local Pending = {}
	for ScaleIndex, Scale in { 1, 0.9 } do
		for TextIndex, Text in Texts do
			for _, Rich in { false, true } do
				local Reference = Instance.new("TextLabel")
				Reference.BackgroundTransparency = 1
				Reference.AutomaticSize = Enum.AutomaticSize.XY
				Reference.Size = UDim2.new()
				Reference.FontFace = Face
				Reference.TextSize = 20
				Reference.RichText = Rich
				Reference.Text = Text
				Reference.Parent = Target
				table.insert(Pending, { Key = `{ScaleIndex}_{TextIndex}_{Rich}`, Label = Reference, Scale = Scale, Text = Text, Rich = Rich })
			end
		end
	end
	task.wait()
	local Row = 0
	for _, Entry in Pending do
		local Width = Entry.Label.TextBounds.X
		Results[`{Entry.Key}_ref`] = { Width }
		Entry.Label.Visible = false
		for _, Truncate in { "None", "AtEnd", "SplitWord" } do
			for _, Anchor in { 0, 0.5, 1 } do
				for _, Short in { 0.5, 2, 5, 12, 40 } do
					Row += 1
					-- A parent 400 wide (local units) under the UIScale; the label's far side is `Short`
					-- local px short of the text width.
					local Local = Width / Entry.Scale
					local Space = Local - Short
					-- Anchor 0: the space to the right edge of a 400 px parent; 1: the space to its left
					-- edge; 0.5: half the parent (a parent 2 x Space wide).
					local Holder = Instance.new("Frame")
					Holder.BackgroundTransparency = 1
					Holder.Position = UDim2.fromOffset(((Row - 1) % 2) * 600, ((Row - 1) // 2) * 4)
					Holder.Size = UDim2.fromOffset(if Anchor == 0.5 then math.floor(2 * Space) else 400, 20)
					Holder.Parent = Target
					local UIScale = Instance.new("UIScale")
					UIScale.Scale = Entry.Scale
					UIScale.Parent = Holder
					local X = if Anchor == 0 then 400 - Space elseif Anchor == 1 then Space else 0
					local L = Instance.new("TextLabel")
					L.BackgroundTransparency = 1
					L.AnchorPoint = Vector2.new(Anchor, 0)
					L.Position = UDim2.fromOffset(math.floor(X), 0)
					L.AutomaticSize = Enum.AutomaticSize.XY
					L.Size = UDim2.new()
					L.FontFace = Face
					L.TextSize = 20
					L.RichText = Entry.Rich
					L.TextTruncate = Enum.TextTruncate[Truncate]
					L.Text = Entry.Text
					L.Parent = Holder
					table.insert(Labels, { Key = `{Entry.Key}_{Truncate}_{Anchor}_{Short}`, Label = L })
				end
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
