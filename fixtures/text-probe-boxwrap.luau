-- Wrap whitespace in a TextBox against a TextLabel: a client TextBox keeps the whitespace at a wrap on the line only
-- while it fits and starts the next line with the rest. "one two three four" is exactly 200 px in SourceSans 32 at 1x
-- (a space 5); box widths are taken from TextService so the cells hold at any scale. Names: g<group>_<kind>_w<box>.
--   node cli/rur.mjs compare --fixture fixtures/text-probe-boxwrap.luau --viewport 1760x1200 --dpi 1 --time 0.2 --background 0,0,0
return function(Target)
	local TextService = game:GetService("TextService")
	local function Width(Text)
		return TextService:GetTextSize(Text, 32, Enum.Font.SourceSans, Vector2.new(100000, 100000)).X
	end
	local A = "one two three four"
	local W1 = Width(A)
	local Six = "one two three four five six"
	local Three = "one two three four   five six"
	-- a word of about Target px (narrower letters fill the rest)
	local function Word(Target)
		local S = ""
		for _, C in { "n", "e", "r", "i", "l" } do
			while Width(S .. C) <= Target do
				S ..= C
			end
		end
		return S
	end

	local Count = 0
	local function Cell(Group, Kind, Text, BoxWidth, Props, Height)
		Count += 1
		local Index = Count - 1
		local L = Instance.new(Kind)
		L.Name = string.format("c%02d_g%d_%s_w%d", Count, Group, Kind, BoxWidth)
		L.BackgroundColor3 = Color3.fromRGB(40, 40, 60)
		L.BorderSizePixel = 0
		L.TextColor3 = Color3.fromRGB(255, 255, 255)
		L.Font = Enum.Font.SourceSans
		L.TextSize = 32
		L.Text = Text
		L.TextWrapped = true
		L.TextXAlignment = Enum.TextXAlignment.Left
		L.TextYAlignment = Enum.TextYAlignment.Top
		if Kind == "TextBox" then
			L.ClearTextOnFocus = false
			L.MultiLine = true
		end
		for Key, Value in Props or {} do
			L[Key] = Value
		end
		L.Position = UDim2.fromOffset(10 + (Index % 6) * 290, 10 + math.floor(Index / 6) * 108)
		L.Size = UDim2.fromOffset(BoxWidth, Height or 100)
		L.Parent = Target
		return L
	end

	-- 1: one space at the wrap, the box from 1 px short of the line to 1 px past line + space
	for _, D in { -1, 0, 4, 5, 6 } do
		Cell(1, "TextBox", Six, W1 + D)
	end
	Cell(1, "TextLabel", Six, W1)
	Cell(1, "TextButton", Six, W1)
	-- 2: three spaces at the wrap
	for _, D in { 0, 4, 5, 9, 10, 14, 15, 16 } do
		Cell(2, "TextBox", Three, W1 + D)
	end
	-- 3: right and centre alignment, spaces partly kept on line 1
	for _, Align in { Enum.TextXAlignment.Right, Enum.TextXAlignment.Center } do
		for _, D in { 0, 5, 6 } do
			Cell(3, "TextBox", Six, W1 + D, { TextXAlignment = Align })
		end
		for _, D in { 5, 10, 15 } do
			Cell(3, "TextBox", Three, W1 + D, { TextXAlignment = Align })
		end
	end
	-- 4: line 2 opens with the moved space: does the next word still fit beside it?
	for _, D in { -7, -6, -5, -4, -3, 1, 5 } do
		local B = Word(W1 + D)
		local Box = Cell(4, "TextBox", A .. " " .. B .. " end", W1)
		Box.Name ..= string.format("_b%d", Width(B))
	end
	Cell(4, "TextLabel", A .. " " .. Word(W1 - 5) .. " end", W1)
	Cell(4, "TextLabel", A .. " " .. Word(W1 - 3) .. " end", W1)
	-- 6: whitespace at a paragraph's end and before a newline
	Cell(6, "TextBox", A .. " ", W1)
	Cell(6, "TextLabel", A .. " ", W1)
	Cell(6, "TextBox", A .. "   ", W1 + 5)
	Cell(6, "TextBox", A .. " \nfive", W1)
	Cell(6, "TextLabel", A .. " \nfive", W1)
	Cell(6, "TextBox", A .. "  \nfive", W1 + 5)
	-- 7: other TextBox states
	Cell(7, "TextBox", Six, W1, { MultiLine = false })
	Cell(7, "TextBox", Six, W1, { RichText = true })
	Cell(7, "TextBox", "", W1, { PlaceholderText = Six, PlaceholderColor3 = Color3.fromRGB(255, 255, 255) })
	Cell(7, "TextBox", Six, W1, { TextEditable = false })
	Cell(7, "TextBox", "<b>one</b> two three four five six", W1, { RichText = true })
	Cell(7, "TextBox", Six, W1, { TextXAlignment = Enum.TextXAlignment.Right, RichText = true })
	-- 5: whitespace runs wider than the box (tall cells on the last rows)
	while Count % 6 ~= 0 do
		Count += 1
	end
	Cell(5, "TextBox", "abc" .. string.rep(" ", 50) .. "def", 100, nil, 300)
	Cell(5, "TextLabel", "abc" .. string.rep(" ", 50) .. "def", 100, nil, 300)
	Cell(5, "TextBox", "abc" .. string.rep(" ", 25) .. "def ghi", 100, nil, 300)
	Cell(5, "TextBox", string.rep(" ", 30) .. "abc", 100, nil, 300)
	Cell(5, "TextBox", "abcdefghijklmnop qrs", 100, nil, 300)
	Cell(5, "TextLabel", "abcdefghijklmnop qrs", 100, nil, 300)
end
