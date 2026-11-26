-- MaxVisibleGraphemes on an empty TextBox's placeholder: whether N cuts the PlaceholderText the way it cuts Text.
-- Empty TextBoxes with a placeholder at N -1, 0, 1, 4, 9, 40; the same with TextTruncate AtEnd in a narrow box,
-- wrapped, rich (tags in the placeholder) and centred; TextBoxes with text at the same N beside them as a reference.
--   node cli/rur.mjs compare --fixture fixtures/text-probe-placeholder.luau --viewport 900x720 --time 0.2 --background 30,30,40
return function(Target)
	local Placeholder = "Search for items here"
	local function Box(Name, Column, Row, Props)
		local Box = Instance.new("TextBox")
		Box.Name = Name
		Box.BackgroundColor3 = Color3.fromRGB(45, 45, 60)
		Box.BorderSizePixel = 0
		Box.Position = UDim2.fromOffset(20 + (Column - 1) * 440, 15 + (Row - 1) * 58)
		Box.Size = UDim2.fromOffset(420, 50)
		Box.Font = Enum.Font.SourceSans
		Box.Text = ""
		Box.PlaceholderText = Placeholder
		Box.PlaceholderColor3 = Color3.fromRGB(190, 190, 205)
		Box.TextSize = 24
		Box.TextColor3 = Color3.new(1, 1, 1)
		Box.TextXAlignment = Enum.TextXAlignment.Left
		Box.ClearTextOnFocus = false
		Box.TextEditable = false
		for Key, Value in Props do
			Box[Key] = Value
		end
		Box.Parent = Target
		return Box
	end

	for Row, N in { -1, 0, 1, 4, 9, 40 } do
		Box(`EmptyN{N}`, 1, Row, { MaxVisibleGraphemes = N })
		Box(`TextN{N}`, 2, Row, { Text = Placeholder, MaxVisibleGraphemes = N })
	end
	Box("TruncN5", 1, 7, { Size = UDim2.fromOffset(120, 50), TextTruncate = Enum.TextTruncate.AtEnd, MaxVisibleGraphemes = 5 })
	Box("TruncN12", 1, 8, { Size = UDim2.fromOffset(120, 50), TextTruncate = Enum.TextTruncate.AtEnd, MaxVisibleGraphemes = 12 })
	Box("WrapN14", 2, 7, { Size = UDim2.fromOffset(160, 50), TextWrapped = true, TextSize = 18, MaxVisibleGraphemes = 14 })
	Box("RichN8", 2, 8, { RichText = true, PlaceholderText = "<b>Search</b> for <i>items</i>", MaxVisibleGraphemes = 8 })
	Box("CenterN6", 1, 9, { TextXAlignment = Enum.TextXAlignment.Center, MaxVisibleGraphemes = 6 })
	Box("ScaledN6", 2, 9, { TextScaled = true, MaxVisibleGraphemes = 6 })
	-- the same box after its text was set and cleared again
	local Cleared = Box("ClearedN3", 1, 10, { Text = "Typed", MaxVisibleGraphemes = 3 })
	Cleared.Text = ""
end
