-- RichText measurement: TextBounds (and ContentText) for styled runs, sizes, faces, line breaks,
-- escapes and malformed markup.
return function(Target)
	local Results = {}
	local Labels = {}
	local Cases = {
		{ "plain", "Hello World" },
		{ "bold", "Hello <b>World</b>" },
		{ "italic", "<i>Italic</i> text" },
		{ "sizeBig", "<font size=\"30\">Big</font> small" },
		{ "sizeSmall", "<font size=\"10\">tiny</font> normal" },
		{ "sizeMixed", "<font size=\"10\">tiny</font><font size=\"40\">HUGE</font>" },
		{ "face", "<font face=\"GothamSSm\">Gotham</font> Source" },
		{ "family", "<font family=\"rbxasset://fonts/families/FredokaOne.json\">Fredoka</font> x" },
		{ "weight", "<font weight=\"bold\">Heavy</font> x" },
		{ "weight900", "<font weight=\"900\">Heavy</font> x" },
		{ "br", "Line1<br/>Line2" },
		{ "brSize", "<font size=\"30\">Big</font><br/>small" },
		{ "newline", "Line1\nLine2" },
		{ "stroke", "<stroke color=\"#ff0000\" thickness=\"2\">Stroked</stroke>" },
		{ "strokeThick", "<stroke thickness=\"6\">Stroked</stroke>" },
		{ "escapes", "a &lt; b &amp; c &gt; d &quot;e&quot; &apos;f&apos;" },
		{ "bareAmp", "Review & craft" },
		{ "bareLt", "a < b" },
		{ "nested", "<b><i>nested</i></b> x" },
		{ "uc", "<uc>upper</uc> x" },
		{ "sc", "<sc>SmallCaps</sc> x" },
		{ "underline", "<u>under</u> <s>strike</s>" },
		{ "unknown", "<foo>x</foo> y" },
		{ "unclosed", "<b>bold" },
		{ "comment", "<!--comment-->visible" },
		{ "mark", "<mark color=\"#ffff00\">marked</mark> x" },
		{ "sizeLines", "<font size=\"30\">A</font>\nB\n<font size=\"12\">C</font>" },
		{ "transparency", "<font transparency=\"0.5\">faded</font> x" },
		{ "color", "<font color=\"#ff0000\">red</font> x" },
		{ "boldSpace", "<b>a</b> <b>b</b>" },
		{ "kernSplit", "A<b>V</b>A" },
	}
	local Face = Font.new("rbxasset://fonts/families/SourceSansPro.json")
	for Index, Case in Cases do
		for _, Rich in { true, false } do
			local Label = Instance.new("TextLabel")
			Label.Position = UDim2.fromOffset(0, Index * 10)
			Label.Size = UDim2.fromOffset(600, 200)
			Label.FontFace = Face
			Label.TextSize = 20
			Label.RichText = Rich
			Label.Text = Case[2]
			Label.Parent = Target
			table.insert(Labels, { Key = `{Case[1]}_{if Rich then "r" else "p"}`, Label = Label })
		end
	end
	task.wait()
	for _, Entry in Labels do
		Results[Entry.Key] = { Entry.Label.TextBounds.X * 1.5, Entry.Label.TextBounds.Y * 1.5 }
		Results[Entry.Key .. "_content"] = Entry.Label.ContentText
	end
	return Results
end
