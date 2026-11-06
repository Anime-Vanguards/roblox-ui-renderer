-- MaxVisibleGraphemes: only the first N graphemes of the text are drawn (-1 all, 0 none), while the layout
-- (line breaks, TextScaled size, TextBounds, alignment) stays the full text's. Two columns of labels:
--   ScaledN5 / FixedN5 / ScaledN0: TextScaled and TextSize 24 at N 5, TextScaled at N 0.
--   AllM1 / BeyondN200: N -1 and N past the end (the whole text).
--   WrapN20 (wrapped, fits one line), RightN8 (right aligned).
--   RichN7: tags do not count, an entity is one grapheme; RichDecorN9 / RichDecorN3: <u> / <mark> / <s>
--   runs cut by N; StrokeN5 (UIStroke), LegacyN5 (TextStroke), RichStrokeN4 (<stroke> run).
--   CombiningN4: "e" + U+0301 is one grapheme; NewlineN11: whether "\n" counts.
--   TruncN8 / TruncN30 / TruncM1: TextTruncate AtEnd, N inside and past the shown prefix, and -1.
--   UnderSpaceN7: an <u> run cut just after a space; WrapCenterN26: wrapped and centred, cut on line 2.
--   TruncFitN5 / TruncFitSplitN5: TextTruncate on text that fits; TruncAtEnd* / TruncSplitWord*: narrow
--   boxes at N 8 / 4 / 0; TruncWrapN*: wrapped AtEnd text whose lines fit, N 5 / 22 / -1; RichTruncN*:
--   rich truncation; TruncCenterN* / RichTruncN0: centred truncation at N 0 / 4; TruncWordN*: wrapped
--   truncation with an ellipsis on line 2, N 3 / 7 / -1. notes/text.md "MaxVisibleGraphemes".
--   node cli/rur.mjs compare --fixture fixtures/text-probe-graphemes.luau --viewport 900x930 --time 0.2 --background 30,30,40
return function(Target)
	local Sentence = "Relax. This is not going to take long."
	local function Label(Name, Column, Row, Props)
		local Label = Instance.new("TextLabel")
		Label.Name = Name
		Label.BackgroundColor3 = Color3.fromRGB(45, 45, 60)
		Label.BorderSizePixel = 0
		Label.Position = UDim2.fromOffset(20 + (Column - 1) * 440, 15 + (Row - 1) * 70)
		Label.Size = UDim2.fromOffset(420, 60)
		Label.Font = Enum.Font.SourceSans
		Label.Text = Sentence
		Label.TextSize = 24
		Label.TextColor3 = Color3.new(1, 1, 1)
		local Children = Props.Children
		Props.Children = nil
		for Key, Value in Props do
			Label[Key] = Value
		end
		for _, Child in Children or {} do
			Child.Parent = Label
		end
		Label.Parent = Target
		return Label
	end
	local function Stroke(Thickness)
		local Stroke = Instance.new("UIStroke")
		Stroke.Color = Color3.fromRGB(255, 120, 0)
		Stroke.Thickness = Thickness
		return Stroke
	end

	Label("ScaledN5", 1, 1, { TextScaled = true, MaxVisibleGraphemes = 5 })
	Label("FixedN5", 2, 1, { MaxVisibleGraphemes = 5 })
	Label("ScaledN0", 1, 2, { TextScaled = true, MaxVisibleGraphemes = 0 })
	Label("AllM1", 2, 2, { MaxVisibleGraphemes = -1 })
	Label("BeyondN200", 1, 3, { MaxVisibleGraphemes = 200 })
	Label("WrapN20", 2, 3, { TextWrapped = true, TextSize = 30, MaxVisibleGraphemes = 20 })
	Label("RightN8", 1, 4, { TextXAlignment = Enum.TextXAlignment.Right, MaxVisibleGraphemes = 8 })
	Label("RichN7", 2, 4, {
		RichText = true,
		Text = `<b>Re</b><font color="#ff4040">lax</font>&amp; <i>This</i> is done`,
		MaxVisibleGraphemes = 7,
	})
	Label("RichDecorN9", 1, 5, {
		RichText = true,
		Text = `<u>Under</u> <mark color="#2050ff">marked</mark> <s>struck</s>`,
		TextSize = 30,
		MaxVisibleGraphemes = 9,
	})
	Label("RichDecorN3", 2, 5, {
		RichText = true,
		Text = `<u>Under</u> <mark color="#2050ff">marked</mark> <s>struck</s>`,
		TextSize = 30,
		MaxVisibleGraphemes = 3,
	})
	Label("StrokeN5", 1, 6, { Font = Enum.Font.FredokaOne, TextSize = 30, MaxVisibleGraphemes = 5, Children = { Stroke(3) } })
	Label("LegacyN5", 2, 6, {
		TextSize = 30,
		TextStrokeTransparency = 0,
		TextStrokeColor3 = Color3.fromRGB(255, 120, 0),
		MaxVisibleGraphemes = 5,
	})
	Label("RichStrokeN4", 1, 7, {
		RichText = true,
		Text = `<stroke color="#00ff00" thickness="2">Stroked</stroke> text`,
		TextSize = 30,
		MaxVisibleGraphemes = 4,
	})
	Label("CombiningN4", 2, 7, { Text = "Cafe\u{301} au lait", TextSize = 30, MaxVisibleGraphemes = 4 })
	Label("NewlineN11", 1, 8, { Text = "Line one\nLine two", TextSize = 20, MaxVisibleGraphemes = 11 })
	Label("TruncN8", 2, 8, {
		Size = UDim2.fromOffset(120, 60),
		TextTruncate = Enum.TextTruncate.AtEnd,
		MaxVisibleGraphemes = 8,
	})
	Label("TruncN30", 2, 8, {
		Position = UDim2.fromOffset(620, 505),
		Size = UDim2.fromOffset(120, 60),
		TextTruncate = Enum.TextTruncate.AtEnd,
		MaxVisibleGraphemes = 30,
	})
	Label("TruncM1", 2, 8, {
		Position = UDim2.fromOffset(760, 505),
		Size = UDim2.fromOffset(120, 60),
		TextTruncate = Enum.TextTruncate.AtEnd,
	})
	Label("UnderSpaceN7", 1, 9, { RichText = true, Text = `<u>Relax. This</u> is long`, TextSize = 30, MaxVisibleGraphemes = 7 })
	Label("WrapCenterN26", 2, 9, { Size = UDim2.fromOffset(200, 60), TextWrapped = true, MaxVisibleGraphemes = 26 })
	Label("TruncFitN5", 1, 10, { Text = "Relax now", TextTruncate = Enum.TextTruncate.AtEnd, MaxVisibleGraphemes = 5 })
	local Narrow = { { "AtEnd", 1 }, { "SplitWord", 2 } }
	for _, Entry in Narrow do
		local Mode, Column = Entry[1], Entry[2]
		for Index, Max in { 8, 4, 0 } do
			Label(`Trunc{Mode}N{Max}`, Column, 11, {
				Position = UDim2.fromOffset(20 + (Column - 1) * 440 + (Index - 1) * 140, 715),
				Size = UDim2.fromOffset(130, 60),
				TextTruncate = Enum.TextTruncate[Mode],
				TextXAlignment = Enum.TextXAlignment.Left,
				MaxVisibleGraphemes = Max,
			})
		end
	end
	for Index, Max in { 5, 22, -1 } do
		Label(`TruncWrapN{Max}`, 1, 12, {
			Position = UDim2.fromOffset(20 + (Index - 1) * 140, 785),
			Size = UDim2.fromOffset(130, 40),
			Text = "Relax. This is not going to take long at all, I promise you.",
			TextSize = 20,
			TextWrapped = true,
			TextTruncate = Enum.TextTruncate.AtEnd,
			MaxVisibleGraphemes = Max,
		})
	end
	for Index, Max in { 8, 13 } do
		Label(`RichTruncN{Max}`, 2, 12, {
			Position = UDim2.fromOffset(460 + (Index - 1) * 140, 785),
			Size = UDim2.fromOffset(130, 60),
			RichText = true,
			Text = `<u>Relax.</u> <font color="#ff4040">This is not going</font>`,
			TextTruncate = Enum.TextTruncate.AtEnd,
			MaxVisibleGraphemes = Max,
		})
	end
	Label("TruncFitSplitN5", 2, 10, { Text = "Relax now", TextTruncate = Enum.TextTruncate.SplitWord, MaxVisibleGraphemes = 5 })
	for Index, Max in { 0, 4 } do
		Label(`TruncCenterN{Max}`, 1, 13, {
			Position = UDim2.fromOffset(20 + (Index - 1) * 140, 855),
			Size = UDim2.fromOffset(130, 60),
			TextTruncate = Enum.TextTruncate.AtEnd,
			MaxVisibleGraphemes = Max,
		})
	end
	Label("RichTruncN0", 1, 13, {
		Position = UDim2.fromOffset(300, 855),
		Size = UDim2.fromOffset(130, 60),
		RichText = true,
		Text = `<font color="#ff4040">Relax. This is not going</font>`,
		TextTruncate = Enum.TextTruncate.AtEnd,
		MaxVisibleGraphemes = 0,
	})
	for Index, Max in { 3, 7, -1 } do
		Label(`TruncWordN{Max}`, 2, 13, {
			Position = UDim2.fromOffset(460 + (Index - 1) * 140, 855),
			Size = UDim2.fromOffset(130, 40),
			Text = "Relax. Thisisaverylongwordthatwillnotfit at all",
			TextSize = 20,
			TextWrapped = true,
			TextTruncate = Enum.TextTruncate.AtEnd,
			MaxVisibleGraphemes = Max,
		})
	end
	return function() end
end
