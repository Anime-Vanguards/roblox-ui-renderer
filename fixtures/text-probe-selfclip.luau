-- Does a text object's own ClipsDescendants cut its own text? Zero-size labels and narrow labels whose
-- text overflows, each with ClipsDescendants off and on. Every label sits over a dim marker frame (its
-- sibling, drawn first) so hidden text is visible as a missing overlay.
--   Rows 1-2 (Zero*): width 0, height 40, unwrapped, Left / Center / Right; row 1 clip off, row 2 on.
--   Row 3 (ZeroWrap*): width 0, TextWrapped, clip off / on; (ZeroH*): height 0, clip off / on.
--   Row 4 (ZeroSize*): 0 x 0, clip off / on; (TallClip*): 120 x 8 box with 24 px text, clip off / on.
--   Rows 5-6 (Narrow*): 60 x 30 box, overflowing text, Left / Center / Right; row 5 clip off, row 6 on.
--   Row 7 (Child*): a narrow clipping label with an overflowing child frame, and a clipped parent
--   frame holding a zero-width label (the parent's clip applies to the label).
--   Row 8 (Stroke*): a box that just fits its text, UIStroke 4 px, clip off / on; (Legacy*): legacy
--   TextStroke in a narrow clipping box; (Rot*): a narrow clipping label rotated 20 degrees.
--   node cli/rur.mjs compare --fixture fixtures/text-probe-selfclip.luau --viewport 900x700 --time 0.2 --background 30,30,40
return function(Target)
	local Text = "the/whole/path Why?"
	local function Marker(X, Y, W, H)
		local Frame = Instance.new("Frame")
		Frame.Name = "Marker"
		Frame.BackgroundColor3 = Color3.fromRGB(60, 60, 80)
		Frame.BorderSizePixel = 0
		Frame.Position = UDim2.fromOffset(X, Y)
		Frame.Size = UDim2.fromOffset(W, H)
		Frame.Parent = Target
	end
	local function Label(Name, X, Y, W, H, Props, Parent)
		local Label = Instance.new("TextLabel")
		Label.Name = Name
		Label.BackgroundTransparency = 1
		Label.BorderSizePixel = 0
		Label.Position = UDim2.fromOffset(X, Y)
		Label.Size = UDim2.fromOffset(W, H)
		Label.Font = Enum.Font.SourceSans
		Label.Text = Text
		Label.TextSize = 24
		Label.TextColor3 = Color3.new(1, 1, 1)
		for Key, Value in Props do
			Label[Key] = Value
		end
		Label.Parent = Parent or Target
		return Label
	end
	local Aligns = { "Left", "Center", "Right" }
	for Row, Clip in { false, true } do
		for Column, Align in Aligns do
			local X = 150 + (Column - 1) * 290
			local Y = 20 + (Row - 1) * 70
			Marker(X - 140, Y - 5, 280, 50)
			Label(`Zero{Align}{Clip and "Clip" or ""}`, X, Y, 0, 40, {
				ClipsDescendants = Clip,
				TextXAlignment = Enum.TextXAlignment[Align],
			})
		end
	end
	for Column, Clip in { false, true } do
		local X = 80 + (Column - 1) * 200
		Marker(X - 60, 160, 120, 140)
		Label(`ZeroWrap{Clip and "Clip" or ""}`, X, 165, 0, 130, { ClipsDescendants = Clip, TextWrapped = true })
	end
	for Column, Clip in { false, true } do
		local X = 440 + (Column - 1) * 230
		Marker(X - 10, 175, 220, 50)
		Label(`ZeroH{Clip and "Clip" or ""}`, X, 200, 200, 0, { ClipsDescendants = Clip })
	end
	for Column, Clip in { false, true } do
		local X = 440 + (Column - 1) * 230
		Marker(X - 110, 235, 220, 50)
		Label(`ZeroSize{Clip and "Clip" or ""}`, X, 260, 0, 0, { ClipsDescendants = Clip })
	end
	for Column, Clip in { false, true } do
		local X = 20 + (Column - 1) * 200
		Marker(X - 10, 315, 180, 45)
		Label(`TallClip{Clip and "Clip" or ""}`, X, 334, 120, 8, {
			ClipsDescendants = Clip,
			Text = "Tall Why?",
			BackgroundTransparency = 0,
			BackgroundColor3 = Color3.fromRGB(120, 40, 40),
		})
	end
	for Row, Clip in { false, true } do
		for Column, Align in Aligns do
			local X = 150 + (Column - 1) * 290
			local Y = 380 + (Row - 1) * 60
			Marker(X - 120, Y - 5, 300, 40)
			Label(`Narrow{Align}{Clip and "Clip" or ""}`, X, Y, 60, 30, {
				ClipsDescendants = Clip,
				TextXAlignment = Enum.TextXAlignment[Align],
				BackgroundTransparency = 0,
				BackgroundColor3 = Color3.fromRGB(120, 40, 40),
			})
		end
	end
	do
		Marker(20, 505, 300, 60)
		local Parent = Label("ChildClip", 100, 515, 60, 40, {
			ClipsDescendants = true,
			Text = "",
			BackgroundTransparency = 0,
			BackgroundColor3 = Color3.fromRGB(120, 40, 40),
		})
		local Child = Instance.new("Frame")
		Child.Name = "Overflow"
		Child.BackgroundColor3 = Color3.fromRGB(80, 200, 120)
		Child.BorderSizePixel = 0
		Child.Position = UDim2.fromOffset(-30, 10)
		Child.Size = UDim2.fromOffset(120, 20)
		Child.Parent = Parent
	end
	do
		Marker(380, 505, 300, 60)
		local Holder = Instance.new("Frame")
		Holder.Name = "ChildHolder"
		Holder.BackgroundTransparency = 1
		Holder.ClipsDescendants = true
		Holder.Position = UDim2.fromOffset(480, 510)
		Holder.Size = UDim2.fromOffset(100, 50)
		Holder.Parent = Target
		Label("ZeroInClip", 50, 5, 0, 40, { TextXAlignment = Enum.TextXAlignment.Center }, Holder)
	end
	for Column, Clip in { false, true } do
		local X = 30 + (Column - 1) * 150
		Marker(X - 15, 585, 130, 60)
		local Fit = Label(`Stroke{Clip and "Clip" or ""}`, X, 600, 100, 30, { ClipsDescendants = Clip, Text = "Why?", TextSize = 30 })
		local Stroke = Instance.new("UIStroke")
		Stroke.Thickness = 4
		Stroke.Color = Color3.fromRGB(230, 120, 40)
		Stroke.Parent = Fit
	end
	Marker(330, 585, 170, 60)
	Label("LegacyClip", 380, 600, 70, 30, {
		ClipsDescendants = true,
		TextStrokeTransparency = 0,
		TextStrokeColor3 = Color3.fromRGB(230, 120, 40),
		TextXAlignment = Enum.TextXAlignment.Center,
	})
	Marker(560, 580, 300, 70)
	Label("RotClip", 680, 600, 60, 30, {
		ClipsDescendants = true,
		Rotation = 20,
		TextXAlignment = Enum.TextXAlignment.Center,
		BackgroundTransparency = 0,
		BackgroundColor3 = Color3.fromRGB(120, 40, 40),
	})
end
