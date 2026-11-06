-- TextTruncate AtEnd on one line: when does Studio keep whole words and when does it cut inside a
-- word (layout-probe-autotrunc: "A somewhat longer label text" at 60 px is "A some…", not "A…")?
-- Texts with short first words, box widths 20..160, SourceSans 16.
return function(Target)
	local Results = {}
	local Labels = {}
	local Face = Font.new("rbxasset://fonts/families/SourceSansPro.json")
	local Strings = { "A somewhat longer label text", "I am here now", "Go to the store", "AB CD EF GH", "Hello world again", "x yyyyyyyyyyyy", "ii some words", "W some words", "WW some words", "il some words", "i. some words", "MMM some words" }
	local Count = 0
	for Index, Text in Strings do
		for Width = 20, 160, 2 do
			Count += 1
			local Label = Instance.new("TextLabel")
			Label.Position = UDim2.fromOffset(((Count - 1) % 6) * 200, ((Count - 1) // 6) * 12)
			Label.Size = UDim2.fromOffset(Width, 20)
			Label.FontFace = Face
			Label.TextSize = 16
			Label.Text = Text
			Label.TextTruncate = Enum.TextTruncate.AtEnd
			Label.Parent = Target
			table.insert(Labels, { Key = `{Index}_{Width}`, Label = Label })
		end
	end
	task.wait()
	for _, Entry in Labels do
		Results[Entry.Key] = { Entry.Label.TextBounds.X * 1.5 }
	end
	return Results
end
