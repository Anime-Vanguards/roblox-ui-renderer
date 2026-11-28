-- Rich lines whose tallest run is whitespace only: where a client puts the line's baseline. Follows the B*_richsize
-- cells of fixtures/text-probe-tabs.luau ("Hello<font size="40">\t</font>|", drawn lower than its 24 px glyphs alone
-- place it): spaces instead of a tab, leading and trailing whitespace, two whitespace runs, a larger drawn run beside
-- a still larger space, a wrapped line ending in the larger spaces, a second paragraph, another face, centred and
-- bottom-aligned labels, TextScaled, and smaller whitespace as a control. SourceSansPro 24 on black, cell outlines grey.
-- compare: --viewport 1200x700 --time 0.2 --background 0,0,0
return function(Target)
	local Face = Font.new("rbxasset://fonts/families/SourceSansPro.json")
	local Cells = {
		{ "tab", "Hello<font size=\"40\">\t</font>|" },
		{ "spaces", "Hello<font size=\"40\">   </font>|" },
		{ "trailing", "Hello|<font size=\"40\">  </font>" },
		{ "leading", "<font size=\"40\">  </font>Hello|" },
		{ "tworuns", "Ab<font size=\"36\"> </font>cd<font size=\"48\"> </font>ef" },
		{ "drawnbigger", "Hi<font size=\"30\">x</font><font size=\"40\"> </font>|" },
		{ "drawnbiggest", "Hi<font size=\"44\">x</font><font size=\"40\"> </font>|" },
		{ "smaller", "Hello<font size=\"12\">  </font>|" },
		{ "face", "Hello<font face=\"FredokaOne\" size=\"40\">  </font>|" },
		{ "bold", "Hello<b><font size=\"40\">  </font></b>|" },
		{ "paragraphs", "Hello<font size=\"40\"> </font>|\nWorld|" },
		{ "secondline", "Hello|\nWorld<font size=\"40\"> </font>|" },
		{ "wrapend", "Hello there<font size=\"40\">   </font>world", { Wrapped = true, W = 150, H = 100 } },
		{ "wrapmid", "Hi<font size=\"40\">  </font>there more words", { Wrapped = true, W = 150, H = 100 } },
		{ "centred", "Hello<font size=\"40\">  </font>|", { X = Enum.TextXAlignment.Center, Y = Enum.TextYAlignment.Center } },
		{ "bottom", "Hello<font size=\"40\">  </font>|", { Y = Enum.TextYAlignment.Bottom } },
		{ "scaled", "Hello<font size=\"40\">  </font>|", { Scaled = true } },
		{ "plainbig", "Hello<font size=\"40\">x</font>|" },
	}
	for Index, Cell in Cells do
		local Options = Cell[3] or {}
		local Column, Row = (Index - 1) % 4, (Index - 1) // 4
		local Label = Instance.new("TextLabel")
		Label.Name = Cell[1]
		Label.BackgroundTransparency = 1
		Label.FontFace = Face
		Label.TextSize = 24
		Label.RichText = true
		Label.Text = Cell[2]
		Label.TextColor3 = Color3.new(1, 1, 1)
		Label.TextWrapped = Options.Wrapped == true
		Label.TextScaled = Options.Scaled == true
		Label.TextXAlignment = Options.X or Enum.TextXAlignment.Left
		Label.TextYAlignment = Options.Y or Enum.TextYAlignment.Top
		Label.Position = UDim2.fromOffset(20 + Column * 290, 20 + Row * 130)
		Label.Size = UDim2.fromOffset(Options.W or 260, Options.H or 110)
		local Stroke = Instance.new("UIStroke")
		Stroke.ApplyStrokeMode = Enum.ApplyStrokeMode.Border
		Stroke.Color = Color3.fromRGB(70, 70, 70)
		Stroke.Thickness = 1
		Stroke.Parent = Label
		Label.Parent = Target
	end
end
