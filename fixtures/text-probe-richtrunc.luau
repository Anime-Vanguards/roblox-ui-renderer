-- Rich text cut by TextTruncate: an unwrapped rich line wider than its box draws an ellipsis in Studio
-- (notes/text.md "Rich truncation"). One line in boxes of 28 widths, with AtEnd and SplitWord, left, centred
-- and right aligned; the same with two runs (an underlined word and a coloured rest), wrapped lines cut by the
-- box height, and plain controls.
-- rur compare --fixture fixtures/text-probe-richtrunc.luau --viewport 1500x1130 --time 0.2 --background 0,0,0
return function(Target)
	local Face = Font.new("rbxasset://fonts/families/SourceSansPro.json")
	local Count = 0
	local function Add(Name, Column, Row, Properties)
		local Holder = Instance.new("Frame")
		Holder.Name = Name
		Holder.BorderSizePixel = 0
		Holder.BackgroundColor3 = Color3.fromRGB(20, 24, 40)
		Holder.Position = UDim2.fromOffset(4 + Column * 212, 4 + Row * 31)
		Holder.Size = Properties.Size
		local Label = Instance.new("TextLabel")
		Label.Name = "Label"
		Label.BackgroundTransparency = 1
		Label.TextColor3 = Color3.new(1, 1, 1)
		Label.FontFace = Face
		Label.TextSize = 18
		Label.Size = UDim2.fromScale(1, 1)
		for Key, Value in Properties do
			if Key ~= "Size" then
				Label[Key] = Value
			end
		end
		Label.Parent = Holder
		Holder.Parent = Target
		Count += 1
		return Holder
	end
	local OneRun = `<font color="#ffffff">Relax. This is not going to take</font>`
	local TwoRuns = `<u>Relax.</u> <font color="#ff4040">This is not going to take</font>`
	local Alignments = { Enum.TextXAlignment.Left, Enum.TextXAlignment.Center, Enum.TextXAlignment.Right }
	-- columns 0-2: one run AtEnd by alignment; column 3: SplitWord left; column 4: two runs AtEnd left;
	-- column 5: two runs SplitWord centred; column 6: plain AtEnd left (control)
	for Row = 0, 27 do
		local Width = 12 + Row * 7
		for Index, Alignment in Alignments do
			Add(`One{Index}W{Width}`, Index - 1, Row, {
				Size = UDim2.fromOffset(Width, 26), RichText = true, Text = OneRun,
				TextTruncate = Enum.TextTruncate.AtEnd, TextXAlignment = Alignment,
			})
		end
		Add(`SplitW{Width}`, 3, Row, {
			Size = UDim2.fromOffset(Width, 26), RichText = true, Text = OneRun,
			TextTruncate = Enum.TextTruncate.SplitWord, TextXAlignment = Enum.TextXAlignment.Left,
		})
		Add(`TwoW{Width}`, 4, Row, {
			Size = UDim2.fromOffset(Width, 26), RichText = true, Text = TwoRuns,
			TextTruncate = Enum.TextTruncate.AtEnd, TextXAlignment = Enum.TextXAlignment.Left,
		})
		Add(`TwoSplitW{Width}`, 5, Row, {
			Size = UDim2.fromOffset(Width, 26), RichText = true, Text = TwoRuns,
			TextTruncate = Enum.TextTruncate.SplitWord, TextXAlignment = Enum.TextXAlignment.Center,
		})
		Add(`PlainW{Width}`, 6, Row, {
			Size = UDim2.fromOffset(Width, 26), Text = "Relax. This is not going to take",
			TextTruncate = Enum.TextTruncate.AtEnd, TextXAlignment = Enum.TextXAlignment.Left,
		})
	end
	-- box widths between whole and half pixels: which way the rich fit rounds the box
	local Fractions = { 0.322, 0.325, 0.328, 0.332, 0.335, 0.338, 0.462, 0.465, 0.468, 0.472, 0.475, 0.478 }
	for Index, Fraction in Fractions do
		for Kind = 0, 2 do
			local Outer = Instance.new("Frame")
			Outer.Name = `Frac{Kind}_{Index}`
			Outer.BackgroundTransparency = 1
			Outer.Position = UDim2.fromOffset(4 + (Index - 1) * 106, 4 + (28 + Kind) * 31)
			Outer.Size = UDim2.fromOffset(100, 26)
			Outer.Parent = Target
			local Holder = Add("Box", 0, 0, {
				Size = UDim2.fromScale(Fraction, 1), RichText = Kind ~= 2, Text = if Kind == 2 then "Relax. This is not going to take" else OneRun,
				TextTruncate = if Kind == 1 then Enum.TextTruncate.SplitWord else Enum.TextTruncate.AtEnd, TextXAlignment = Enum.TextXAlignment.Left,
			})
			Holder.Position = UDim2.fromOffset(0, 0)
			Holder.Parent = Outer
		end
	end
	-- runs of another weight, size and colour at the cut, and wrapped rich text cut by the box height
	local Styled = {
		`Relax. <b>This is not going to take</b>`,
		`<font size="26">Relax. This</font> is not going to take`,
		`<font color="#ff4040">Relax. This is</font> not going to take`,
		`<i>Relax.</i> <s>This is not</s> going to take`,
		`<stroke color="#00a0ff" thickness="2">Relax. This is not</stroke> going to take`,
	}
	for Index, Text in Styled do
		for Step = 0, 6 do
			Add(`Styled{Index}_{Step}`, Step, 31 + Index - 1, {
				Size = UDim2.fromOffset(16 + Step * 27, 26), RichText = true, Text = Text,
				TextTruncate = if Step % 2 == 0 then Enum.TextTruncate.AtEnd else Enum.TextTruncate.SplitWord,
				TextXAlignment = Enum.TextXAlignment.Left,
			})
		end
	end
end
