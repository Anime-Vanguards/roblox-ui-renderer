-- Trailing whitespace of a paragraph: TextBounds.X of a paragraph that ends in spaces before a newline, TextScaled with
-- trailing spaces, and rich text whose paragraph's spaces overflow the box before a newline. "one two three four" is
-- exactly 200 px in SourceSans 32 at 1x (a space 5).
--   node cli/rur.mjs compare --fixture fixtures/text-probe-trailspace.luau --viewport 1760x560 --dpi 1 --time 0.2 --background 0,0,0
return function(Target)
	local TextService = game:GetService("TextService")
	local function Width(Text)
		return TextService:GetTextSize(Text, 32, Enum.Font.SourceSans, Vector2.new(100000, 100000)).X
	end
	local A = "one two three four"
	local W1 = Width(A)

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
		L.Position = UDim2.fromOffset(10 + (Index % 6) * 290, 10 + math.floor(Index / 6) * 128)
		L.Size = UDim2.fromOffset(BoxWidth, Height or 120)
		for Key, Value in Props or {} do
			L[Key] = Value
		end
		L.Parent = Target
		return L
	end

	-- TextScaled with trailing spaces (a row 60 px tall holds one 32 px line, not two)
	Cell("TextLabel", A .. " ", W1, { TextScaled = true }, 60)
	Cell("TextLabel", A .. "   ", W1, { TextScaled = true }, 60)
	Cell("TextLabel", A .. " ", W1, { TextScaled = true, TextWrapped = false }, 60)
	Cell("TextLabel", A .. " ", W1, { TextScaled = true, RichText = true }, 60)
	Cell("TextLabel", A .. " \nfive", W1, { TextScaled = true }, 100)
	Cell("TextLabel", "  " .. A, W1, { TextScaled = true }, 60)
	-- TextBounds.X of paragraphs ending in spaces
	Cell("TextLabel", A .. "   \nfive", W1)
	Cell("TextLabel", A .. " \nfive", W1, { RichText = true, TextWrapped = false })
	Cell("TextLabel", A .. " \nfive", W1, { AutomaticSize = Enum.AutomaticSize.X, TextWrapped = false })
	Cell("TextLabel", "two \nfive", W1)
	Cell("TextLabel", A .. " \nfive\nsix", W1)
	Cell("TextLabel", A .. " \n", W1, { TextWrapped = false })
	-- rich text: a paragraph whose spaces overflow before a newline
	Cell("TextLabel", A .. " ", W1, { RichText = true })
	Cell("TextLabel", A .. "  \nfive", W1 + 5, { RichText = true })
	Cell("TextLabel", A .. " \nfive", W1 + 4, { RichText = true })
	Cell("TextLabel", A .. " \nfive", W1 + 5, { RichText = true })
	Cell("TextLabel", "one two three four five \nsix", W1, { RichText = true })
	Cell("TextLabel", A .. "<b> </b>\nfive", W1, { RichText = true })
	Cell("TextLabel", A .. " \n\nfive", W1, { RichText = true })
	Cell("TextLabel", A .. " \nfive", W1, { RichText = true, TextXAlignment = Enum.TextXAlignment.Right })
	Cell("TextLabel", A .. " \nfive", W1, { RichText = true, TextScaled = true }, 100)
	Cell("TextLabel", "abcdefghijklmnopqrstuvwxyz \nfive", W1, { RichText = true })
end
