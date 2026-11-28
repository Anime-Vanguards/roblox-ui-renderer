-- Rich text markup a client shows as its raw source: unknown tags, tag-name case, <br> forms, a "<" that starts no tag,
-- quoted ">", closing-tag forms. Each case is a rich label and, beside it, a plain label holding the rich label's
-- ContentText. Every case in SourceSansPro; the first column again in FredokaOne.
-- compare: --viewport 1480x1110 --time 0.2 --background 0,0,0
return function(Target)
	local Cases = {
		-- unknown tags
		{ "unk_open2", "A<foo>W<foo>A" },
		{ "unk_closed", "A<foo>W</foo>A" },
		{ "unk_self", "A<foo/>W" },
		{ "unk_stray", "A</foo>W" },
		{ "unk_mismatch", "<b>A<foo>W</b>A</foo>" },
		{ "unk_attr", "<foo x=\"1\">AW</foo>" },
		{ "unk_case", "<FOO>AW</foo>" },
		{ "unk_in_known", "<b>A<foo>W</foo></b>" },
		{ "unk_nested", "<foo>A<bar>W</bar></foo>" },
		{ "unk_open1", "A<foo>W" },
		{ "unk_only", "<foo>" },
		{ "unk_open_in_known", "A<b>W<foo></b>" },
		{ "unk_other_close", "A<foo>W</bar>" },
		{ "unk_dash", "A<foo-bar>W</foo-bar>" },
		{ "unk_ws_close", "A<foo>W</foo >" },
		{ "unk_long", "A<foo>W<foo>A<foo>W<foo>A<foo>T<foo>o<foo>" },
		{ "unk_self_attr", "A<foo x=\"1\"/>W" },
		{ "unk_nested_same", "<foo><foo>A</foo></foo>" },
		{ "unk_stray_end", "<b>A</b></foo>" },
		{ "unk_quote_gt", "<foo x=\">\">A</foo>" },
		{ "self_space", "A<foo />W" },
		-- known tags
		{ "known_mismatch", "<b>A<i>W</b></i>" },
		{ "known_open", "<b>AW" },
		{ "known_stray", "A</b>W" },
		{ "known_case", "<B>AW</b>" },
		{ "known_close_case", "A<b>W</B>" },
		{ "known_self", "A<b/>W" },
		{ "known_self_close", "A<i/>W</i>" },
		{ "extra_close", "A<b>W</b></b>" },
		{ "B_upper", "<B>AW</B>" },
		{ "FONT_upper", "<FONT size=\"30\">AW</FONT>" },
		{ "UC_upper", "<UC>av</UC>" },
		{ "uc", "<uc>av</uc>" },
		{ "sc_case", "<sc>Av</SC>" },
		{ "font_bare", "<font>AW</font>" },
		{ "trail_space", "A<b >W</b >" },
		{ "font_close_space", "<font size=\"30\">A</font >" },
		{ "known_unclosed_attr", "<font size=\"30\">A" },
		{ "b_ref", "<b>AW</b>" },
		{ "plain_ref", "AW" },
		-- <br>
		{ "br", "A<br/>W" },
		{ "br_noslash", "A<br>W" },
		{ "br_close", "A<br>W</br>" },
		{ "br_space", "A<br />W" },
		{ "br_slash_space", "A<br/ >W" },
		{ "br_attr", "A<br x=\"1\"/>W" },
		{ "br_in_b", "<b>A<br>W</b>" },
		-- "<" that starts no tag
		{ "lt_alone", "A < W" },
		{ "lit_digit", "<b>A</b><1>" },
		{ "lit_digit_closed", "<b>A</b><1>W</1>" },
		{ "lit_space", "<b>A</b>< b>" },
		{ "lit_empty", "<b>A</b><>" },
		{ "lit_lt_space", "<b>A</b> < W" },
		{ "lit_lt_end", "<b>A</b><" },
		{ "lit_gt", "<b>A</b>>" },
		{ "lit_unterminated", "<b>A</b><foo" },
		{ "lit_close_empty", "<b>A</b></>" },
		{ "lit_lt_digit_mid", "<b>A</b>1<2" },
		{ "empty_tag", "A<>W" },
		{ "digit_tag", "A<1>W</1>" },
		{ "space_name", "A< b>W</ b>" },
		-- entities, comments
		{ "lit_amp", "<b>A</b>&foo;" },
		{ "unk_entity", "&foo;AW" },
		{ "comment", "A<!--x-->W" },
		{ "comment_open", "A<!--x W" },
		{ "comment_open_b", "<b>A</b><!--x" },
		{ "comment_in_unk", "<foo>A<!--x-->W</foo>" },
		-- names
		{ "dash_vs_name", "<foo-bar>W</foo>" },
		{ "dot_name", "<foo.bar>W</foo.bar>" },
		{ "under_name", "<foo_bar>W</foo_bar>" },
		{ "digit_in_name", "<f1>W</f1>" },
		{ "under_first", "<b>A</b><_x>W</_x>" },
		{ "colon_name", "<b>A</b><a:b>W</a:b>" },
		{ "dollar_name", "<b>A</b><foo$>W</foo$>" },
		{ "dash_end", "<b>A</b><x->W</x->" },
		{ "tab_after_name", "<b\t>A</b>W" },
		{ "newline_after_name", "<b\n>A</b>W" },
		{ "space_slash_close", "<b>A< /b>W" },
		{ "close_space_name", "<b>A</ b>W" },
		-- closing tags, attributes, quotes
		{ "close_attr", "<b>A</b x=\"1\">W" },
		{ "close_self", "<b>A</b/>W" },
		{ "unquoted", "<font size=30>A</font>W" },
		{ "single_quoted", "<font size='30'>A</font>W" },
		{ "single_quote_gt", "<b>A</b><foo x='>'>W</foo>" },
		{ "open_quote", "<font size=\"30>A</font>W" },
		{ "unk_bare_attr", "<b>A</b><foo bar>W</foo>" },
		{ "known_bare_attr", "<b x>A</b>W" },
		{ "known_bad_attr", "<font size=\"x\">A</font>W" },
		{ "no_space_attrs", "<font size=\"30\"color=\"#FF0000\">A</font>W" },
		{ "eq_spaces", "<font size = \"30\">A</font>W" },
		{ "unk_dash_attr", "<b>A</b><foo a-b=\"1\">W</foo>" },
		{ "attr_case", "<font SIZE=\"30\">A</font>W" },
		{ "font_unknown_attr", "<font foo=\"1\">A</font>W" },
		{ "b_attr", "<b x=\"1\">A</b>W" },
		-- known attribute values
		{ "color_bad", "<font color=\"zzz\">A</font>W" },
		{ "color_empty", "<font color=\"\">A</font>W" },
		{ "color_rgb2", "<font color=\"rgb(1,2)\">A</font>W" },
		{ "color_rgb_sp", "<font color=\"rgb(255, 0, 0)\">A</font>W" },
		{ "color_nohash", "<font color=\"FF0000\">A</font>W" },
		{ "color_short", "<font color=\"#F80\">A</font>W" },
		{ "color_name", "<font color=\"red\">A</font>W" },
		{ "color_prefix", "<font color=\"x rgb(1,2,3)\">A</font>W" },
		{ "transp_bad", "<font transparency=\"x\">A</font>W" },
		{ "transp_dot", "<font transparency=\".5\">A</font>W" },
		{ "transp_trail", "<font transparency=\"0.5 \">A</font>W" },
		{ "weight_bad", "<font weight=\"zzz\">A</font>W" },
		{ "weight_kw", "<font weight=\"bold\">A</font>W" },
		{ "weight_case", "<font weight=\"Bold\">A</font>W" },
		{ "weight_num", "<font weight=\"700\">A</font>W" },
		{ "face_bad", "<font face=\"Nope\">A</font>W" },
		{ "family_bad", "<font family=\"rbxasset://fonts/families/Nope.json\">A</font>W" },
		{ "size_px", "<font size=\"30px\">A</font>W" },
		{ "size_space", "<font size=\" 30\">A</font>W" },
		{ "size_trail", "<font size=\"30 \">A</font>W" },
		{ "size_empty", "<font size=\"\">A</font>W" },
		{ "size_exp", "<font size=\"1e1\">A</font>W" },
		{ "size_frac", "<font size=\"22.5\">A</font>W" },
		{ "size_plus", "<font size=\"+30\">A</font>W" },
		{ "size_hex", "<font size=\"0x1E\">A</font>W" },
		{ "size_dup", "<font size=\"30\" size=\"20\">A</font>W" },
		{ "stroke_thick_bad", "<stroke thickness=\"x\">A</stroke>W" },
		{ "stroke_color_bad", "<stroke color=\"zzz\">A</stroke>W" },
		{ "stroke_transp_bad", "<stroke transparency=\"x\">A</stroke>W" },
		{ "stroke_joins_bad", "<stroke joins=\"zzz\">A</stroke>W" },
		{ "mark_color_bad", "<mark color=\"zzz\">A</mark>W" },
		{ "mark_transp_bad", "<mark transparency=\"x\">A</mark>W" },
		-- more names and entities
		{ "uni_name", "<b>A</b><\u{E9}>W</\u{E9}>" },
		{ "colon_first", "<b>A</b><:x>W</:x>" },
		{ "vt_after_name", "<b\v>A</b>W" },
		{ "close_tab", "<b>A</b\t>W" },
		{ "amp_alone", "<b>A</b> & W" },
		{ "amp_amp", "<b>A</b>&amp;" },
		{ "amp_dec", "<b>A</b>&#66;" },
		{ "amp_hex", "<b>A</b>&#x42;" },
		{ "amp_big", "<b>A</b>&#9731;" },
		{ "amp_bad_num", "<b>A</b>&#xZZ;" },
		{ "amp_nosemi", "<b>A</b>&lt" },
		{ "amp_case", "<b>A</b>&LT;" },
	}
	local function Pair(Name, Text, Face, X, Y)
		local L = Instance.new("TextLabel")
		L.Name = Name
		L.Position = UDim2.fromOffset(X, Y)
		L.Size = UDim2.fromOffset(280, 20)
		L.BackgroundTransparency = 1
		L.TextColor3 = Color3.fromRGB(240, 240, 240)
		L.FontFace = Font.new(`rbxasset://fonts/families/{Face}.json`)
		L.TextSize = 16
		L.TextXAlignment = Enum.TextXAlignment.Left
		L.TextYAlignment = Enum.TextYAlignment.Top
		L.RichText = true
		L.Text = Text
		L.Parent = Target
		local C = Instance.new("TextLabel")
		C.Name = `{Name}_content`
		C.Position = UDim2.fromOffset(X + 284, Y)
		C.Size = UDim2.fromOffset(200, 20)
		C.BackgroundTransparency = 1
		C.TextColor3 = Color3.fromRGB(160, 200, 255)
		C.FontFace = Font.new("rbxasset://fonts/families/SourceSansPro.json")
		C.TextSize = 12
		C.TextXAlignment = Enum.TextXAlignment.Left
		C.TextYAlignment = Enum.TextYAlignment.Top
		C.Text = L.ContentText
		C.Parent = Target
	end
	for I, Case in Cases do
		local Column = (I - 1) // 50
		Pair(Case[1], Case[2], "SourceSansPro", 4 + Column * 492, 4 + ((I - 1) % 50) * 22)
	end
	for I = 1, 50 - (#Cases - 100) do
		local Case = Cases[I]
		if not Case then
			break
		end
		Pair(`{Case[1]}_fredoka`, Case[2], "FredokaOne", 4 + 2 * 492, 4 + ((#Cases - 100) + I - 1) * 22)
	end
end
