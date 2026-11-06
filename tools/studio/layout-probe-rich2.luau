-- RichText details: face names, small caps, LineHeight with mixed sizes, wrapping across runs,
-- sizes under UIScale, empty lines, kerning across runs.
return function(Target)
	local Results = {}
	local Labels = {}
	local Face = Font.new("rbxasset://fonts/families/SourceSansPro.json")
	local function Add(Key, Text, Props, Parent)
		local Label = Instance.new("TextLabel")
		Label.Size = UDim2.fromOffset(600, 300)
		Label.FontFace = Face
		Label.TextSize = 20
		Label.RichText = true
		Label.Text = Text
		for K, V in Props or {} do
			(Label :: any)[K] = V
		end
		Label.Parent = Parent or Target
		table.insert(Labels, { Key = Key, Label = Label })
	end
	Add("faceGotham", "<font face=\"Gotham\">Gotham</font> x")
	Add("faceFredoka", "<font face=\"FredokaOne\">Fredoka</font> x")
	Add("faceMontserrat", "<font face=\"Montserrat\">Montserrat</font> x")
	Add("faceArial", "<font face=\"Arial\">Arial</font> x")
	Add("faceGothamBold", "<font face=\"GothamBold\">GB</font> x")
	Add("faceBoldTag", "<b><font face=\"Gotham\">GB</font></b> x")
	Add("scLower", "<sc>abc</sc>")
	Add("scUpper", "<sc>ABC</sc>")
	Add("scMixed", "<sc>Hello World</sc>")
	Add("ucPlain", "ABC")
	Add("plainSmall16", "ABC", { TextSize = 16 })
	Add("plainSmall14", "ABC", { TextSize = 14 })
	Add("lhMixed", "<font size=\"30\">A</font>\nB\n<font size=\"12\">C</font>", { LineHeight = 1.5 })
	Add("lhPlain", "A\nB\nC", { LineHeight = 1.5 })
	Add("wrapRuns", "Hello <b>bold world</b> and <font size=\"30\">big words here</font> end", { TextWrapped = true, Size = UDim2.fromOffset(150, 300) })
	Add("wrapRunsNarrow", "aa<b>bb</b>cc dd<font size=\"30\">ee</font>ff", { TextWrapped = true, Size = UDim2.fromOffset(40, 300) })
	Add("emptyLines", "a<br/><br/><font size=\"30\">b</font>")
	Add("emptyBig", "a<br/><font size=\"30\"><br/></font>b")
	Add("trailingBr", "a<br/>")
	Add("kernBold", "A<b>V</b>A")
	Add("kernColor", "A<font color=\"#ff0000\">V</font>A")
	Add("kernPlain", "AVA")
	Add("sizeInBold", "<b>x<font size=\"30\">Y</font>z</b>")
	Add("spaceRuns", "a <b> </b> b")
	do
		local Holder = Instance.new("Frame")
		Holder.Size = UDim2.fromOffset(700, 400)
		Holder.BackgroundTransparency = 1
		Holder.Parent = Target
		local S = Instance.new("UIScale")
		S.Scale = 0.885
		S.Parent = Holder
		Add("scaledSize30", "<font size=\"30\">Big</font> small", nil, Holder)
		Add("scaledSize35", "<font size=\"35\">Big</font>", nil, Holder)
	end
	task.wait()
	for _, Entry in Labels do
		Results[Entry.Key] = { Entry.Label.TextBounds.X * 1.5, Entry.Label.TextBounds.Y * 1.5 }
		Results[Entry.Key .. "_content"] = Entry.Label.ContentText
	end
	return Results
end
