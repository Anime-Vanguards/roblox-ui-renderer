-- Wrap whitespace in a TextBox, part 2: what the whitespace a TextBox carries to a new line changes beyond drawing
-- (TextScaled, AutomaticSize, truncation, LineHeight, a rich box's placeholder) and the TextBounds of a paragraph that
-- ends in spaces before a newline. "one two three four" is exactly 200 px in SourceSans 32 at 1x (a space 5).
--   node cli/rur.mjs compare --fixture fixtures/text-probe-boxwrap2.luau --viewport 1760x560 --dpi 1 --time 0.2 --background 0,0,0
return function(Target)
	local TextService = game:GetService("TextService")
	local function Width(Text)
		return TextService:GetTextSize(Text, 32, Enum.Font.SourceSans, Vector2.new(100000, 100000)).X
	end
	local A = "one two three four"
	local W1 = Width(A)
	local Six = "one two three four five six"

	local Count = 0
	local function Cell(Kind, Text, BoxWidth, Props, Height)
		Count += 1
		local Index = Count - 1
		local L = Instance.new(Kind)
		L.Name = string.format("c%02d_%s_w%d", Count, Kind, BoxWidth)
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
		L.Position = UDim2.fromOffset(10 + (Index % 6) * 290, 10 + math.floor(Index / 6) * 128)
		L.Size = UDim2.fromOffset(BoxWidth, Height or 120)
		for Key, Value in Props or {} do
			L[Key] = Value
		end
		L.Parent = Target
		return L
	end

	-- TextScaled: a carried space is a line of its own
	Cell("TextBox", A .. " ", W1, { TextScaled = true }, 60)
	Cell("TextLabel", A .. " ", W1, { TextScaled = true }, 60)
	Cell("TextBox", Six, W1, { TextScaled = true }, 70)
	Cell("TextLabel", Six, W1, { TextScaled = true }, 70)
	-- AutomaticSize Y
	Cell("TextBox", A .. " ", W1, { AutomaticSize = Enum.AutomaticSize.Y }, 10)
	Cell("TextLabel", A .. " ", W1, { AutomaticSize = Enum.AutomaticSize.Y }, 10)
	-- truncation of the last line that fits the height
	Cell("TextBox", Six, W1, { TextTruncate = Enum.TextTruncate.AtEnd }, 40)
	Cell("TextBox", A .. " five six seven", W1, { TextTruncate = Enum.TextTruncate.AtEnd }, 70)
	Cell("TextBox", A .. " five six seven", W1, { TextTruncate = Enum.TextTruncate.SplitWord }, 70)
	-- LineHeight and a bottom-aligned box
	Cell("TextBox", Six, W1, { LineHeight = 1.5 })
	Cell("TextBox", Six, W1, { TextYAlignment = Enum.TextYAlignment.Bottom })
	-- a rich box's placeholder
	Cell("TextBox", "", W1, { RichText = true, PlaceholderText = Six, PlaceholderColor3 = Color3.fromRGB(255, 255, 255) })
	-- a paragraph ending in spaces before a newline: TextBounds.X
	Cell("TextLabel", A .. " \nfive", W1)
	Cell("TextLabel", A .. "  \nfive", W1)
	Cell("TextLabel", A .. " \nfive", W1 + 3)
	Cell("TextLabel", A .. " \nfive", W1 - 10)
	Cell("TextLabel", A .. " \nfive", W1, { TextWrapped = false })
	Cell("TextButton", A .. " \nfive", W1)
	Cell("TextLabel", "two \n" .. A .. " ", W1)
	Cell("TextLabel", A .. " \n" .. A .. " ", W1)
	Cell("TextLabel", A .. " \n", W1)
	Cell("TextLabel", "two  \nfive", 50)
	Cell("TextLabel", A .. " \nfive", W1, { RichText = true })
	Cell("TextBox", A .. " \nfive", W1, { RichText = true })
end
