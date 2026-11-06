-- Where an AutomaticSize X text label sits when it is wider than its parent's padded content box
-- (layout-probe-invisible: an AutoXY "Traits Enabled" label, 139 px, in a 100 px frame with 8 + 8
-- padding sits 16 px left of the content origin in Studio). Paddings, text lengths, positions,
-- anchors and parent widths; label AbsolutePosition.X relative to the parent and AbsoluteSize.X.
-- Built parent first (first half: 100/100 as offline) and built unparented, then parented at the end
-- like layout-probe-invisible (second half, keys ending in _late).
return function(Target)
	local Results = {}
	local Nodes = {}
	local Count = 0
	local Paddings = { { 0, 0 }, { 8, 8 }, { 8, 0 }, { 0, 8 }, { 20, 5 } }
	local Texts = { "ab", "Traits", "Traits Enabled", "A much longer label text here" }
	local Cases = {
		{ "p0", UDim2.new(), Vector2.zero, 100 },
		{ "p5", UDim2.fromOffset(5, 0), Vector2.zero, 100 },
		{ "scale", UDim2.fromScale(0.5, 0), Vector2.zero, 100 },
		{ "anchor1", UDim2.fromScale(1, 0), Vector2.new(1, 0), 100 },
		{ "wide", UDim2.new(), Vector2.zero, 180 },
	}
	local function Build(Late, PadIndex, Pad, TextIndex, Text, Case)
		Count += 1
		local Parent = Instance.new("Frame")
		Parent.BackgroundTransparency = 1
		Parent.Position = UDim2.fromOffset(((Count - 1) % 5) * 240 + 60, ((Count - 1) // 5) * 22)
		Parent.Size = UDim2.fromOffset(Case[4], 20)
		if not Late then
			Parent.Parent = Target
		end
		local Padding = Instance.new("UIPadding")
		Padding.PaddingLeft = UDim.new(0, Pad[1])
		Padding.PaddingRight = UDim.new(0, Pad[2])
		Padding.Parent = Parent
		local Label = Instance.new("TextLabel")
		Label.AutomaticSize = Enum.AutomaticSize.XY
		Label.Size = UDim2.new()
		Label.Position = Case[2]
		Label.AnchorPoint = Case[3]
		Label.TextSize = 16
		Label.Text = Text
		Label.Parent = Parent
		if Late then
			Parent.Parent = Target
		end
		Nodes[`{PadIndex}_{TextIndex}_{Case[1]}{if Late then "_late" else ""}`] = Label
	end
	for _, Late in { false, true } do
		for PadIndex, Pad in Paddings do
			for TextIndex, Text in Texts do
				for _, Case in Cases do
					Build(Late, PadIndex, Pad, TextIndex, Text, Case)
				end
			end
		end
	end
	task.wait()
	for Key, Node in Nodes do
		Results[Key] = { Node.AbsolutePosition.X - Node.Parent.AbsolutePosition.X, Node.AbsoluteSize.X }
	end
	return Results
end
