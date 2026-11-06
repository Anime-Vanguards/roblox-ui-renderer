-- Wrapping / TextScaled / LineHeight probes (TextBounds + TextFits) for SourceSansPro and FredokaOne.
local HttpService = game:GetService("HttpService")
local CoreGui = game:GetService("CoreGui")
local TextService = game:GetService("TextService")

local Families = { "SourceSansPro", "FredokaOne" }
for _, Family in Families do
	local Params = Instance.new("GetTextBoundsParams")
	Params.Text = "Warm up"
	Params.Font = Font.new(`rbxasset://fonts/families/{Family}.json`)
	Params.Size = 20
	pcall(TextService.GetTextBoundsAsync, TextService, Params)
end
task.wait(0.3)

local Gui = Instance.new("ScreenGui")
Gui.Name = "__RUR_Measure"
Gui.Parent = CoreGui
local Label = Instance.new("TextLabel")
Label.Parent = Gui

local Texts = {
	"The quick brown fox jumps over the lazy dog",
	"Supercalifragilisticexpialidocious word",
	"a  b   c    d",
	"   leading spaces here",
	"trailing spaces here   ",
	"line one\nline two is longer\n\nfour",
	"日本語のテキストを折り返す",
	"one-two-three four-five",
	"x",
	"",
}
local Widths = { 10, 30, 50, 80, 100, 120, 150, 200, 400 }
local Sizes = { 14, 20, 32 }

local Wrap = {}
for _, Family in Families do
	Label.FontFace = Font.new(`rbxasset://fonts/families/{Family}.json`)
	Label.TextWrapped = true
	Label.TextScaled = false
	for _, Size in Sizes do
		Label.TextSize = Size
		for TextIndex, Text in Texts do
			for _, Width in Widths do
				Label.Size = UDim2.fromOffset(Width, 500)
				Label.Text = Text
				local _ = Label.AbsoluteSize
				Label.Text = Text .. " "
				Label.Text = Text
				task.wait()
				table.insert(Wrap, { f = Family, s = Size, t = TextIndex, w = Width, bx = Label.TextBounds.X, by = Label.TextBounds.Y, fits = Label.TextFits })
			end
		end
	end
end

local Scaled = {}
local Boxes = { { 50, 20 }, { 100, 30 }, { 200, 50 }, { 300, 100 }, { 80, 80 }, { 400, 40 }, { 30, 200 } }
for _, Family in Families do
	Label.FontFace = Font.new(`rbxasset://fonts/families/{Family}.json`)
	Label.TextScaled = true
	for _, Wrapped in { false, true } do
		Label.TextWrapped = Wrapped
		for TextIndex, Text in Texts do
			for _, Box in Boxes do
				Label.Size = UDim2.fromOffset(Box[1], Box[2])
				Label.Text = Text
				local _ = Label.AbsoluteSize
				task.wait()
				table.insert(Scaled, { f = Family, wr = Wrapped, t = TextIndex, bw = Box[1], bh = Box[2], bx = Label.TextBounds.X, by = Label.TextBounds.Y, fits = Label.TextFits })
			end
		end
	end
end

local LineHeights = {}
Label.TextScaled = false
Label.TextWrapped = false
Label.FontFace = Font.new("rbxasset://fonts/families/SourceSansPro.json")
for _, Size in Sizes do
	Label.TextSize = Size
	for _, LineHeight in { 0.5, 0.8, 1, 1.2, 1.5, 2, 3 } do
		Label.LineHeight = LineHeight
		for _, Text in { "one", "one\ntwo", "one\ntwo\nthree" } do
			Label.Text = Text
			table.insert(LineHeights, { s = Size, lh = LineHeight, n = select(2, string.gsub(Text, "\n", "")) + 1, bx = Label.TextBounds.X, by = Label.TextBounds.Y })
		end
	end
end
Label.LineHeight = 1

Gui:Destroy()
return HttpService:JSONEncode({ texts = Texts, wrap = Wrap, scaled = Scaled, lineheights = LineHeights })
