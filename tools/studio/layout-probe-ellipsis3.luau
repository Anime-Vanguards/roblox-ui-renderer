-- Width the truncation ellipsis adds to TextBounds after a kept word (AtEnd "Ab…"), per size: the
-- lone-ellipsis probes (-ellipsis, -ellipsis2) mix in other effects; layout-probe-autoedge has "Max…"
-- at 27 px = 37 + 21 where ceil(21.11) = 22 was assumed. Boxes 3 px wider than "Ab" + "…" (both
-- measured first), sizes 8-60, three fonts.
return function(Target)
	local Results = {}
	local Faces = {
		SSP = Font.new("rbxasset://fonts/families/SourceSansPro.json"),
		MB = Font.new("rbxasset://fonts/families/Montserrat.json", Enum.FontWeight.Bold),
		Fre = Font.new("rbxasset://fonts/families/FredokaOne.json"),
	}
	local Pending = {}
	for Key, Face in Faces do
		for Size = 8, 60 do
			local Pieces = {}
			for _, Text in { "Ab", "\u{2026}", "Ab\u{2026}" } do
				local L = Instance.new("TextLabel")
				L.Size = UDim2.fromOffset(600, 100)
				L.FontFace = Face
				L.TextSize = Size
				L.Text = Text
				L.Parent = Target
				table.insert(Pieces, L)
			end
			table.insert(Pending, { Key = `{Key}_{Size}`, Pieces = Pieces, Face = Face, Size = Size })
		end
	end
	task.wait()
	local Labels = {}
	for _, Entry in Pending do
		local Word, Dots, Both = Entry.Pieces[1].TextBounds.X, Entry.Pieces[2].TextBounds.X, Entry.Pieces[3].TextBounds.X
		for _, Piece in Entry.Pieces do
			Piece:Destroy()
		end
		local L = Instance.new("TextLabel")
		L.Size = UDim2.fromOffset(math.ceil(Word + Dots + 2), 100)
		L.FontFace = Entry.Face
		L.TextSize = Entry.Size
		L.Text = "Ab Cdefghijklmnopqrstuvw"
		L.TextTruncate = Enum.TextTruncate.AtEnd
		L.Parent = Target
		table.insert(Labels, { Key = Entry.Key, Label = L, Word = Word, Dots = Dots, Both = Both })
	end
	task.wait()
	for _, Entry in Labels do
		Results[Entry.Key] = { Entry.Word * 1.5, Entry.Dots * 1.5, Entry.Both * 1.5, Entry.Label.TextBounds.X * 1.5, Entry.Label.AbsoluteSize.X * 1.5 }
	end
	return Results
end
