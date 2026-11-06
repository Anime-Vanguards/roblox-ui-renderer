-- Width of selected strings at every integer physical text size S (TextSize = S / DPI scale).
local HttpService = game:GetService("HttpService")
local CoreGui = game:GetService("CoreGui")

local Scale = 1.5
local Fonts = { { "SourceSansPro", "Regular" }, { "BuilderSans", "Regular" }, { "FredokaOne", "Regular" } }
local Strings = { "a", "A", "m", "W", "i", " ", "aa", "AV", "To", "mm", "Hello" }

local Gui = Instance.new("ScreenGui")
Gui.Name = "__RUR_Measure"
Gui.Parent = CoreGui
local Label = Instance.new("TextLabel")
Label.Size = UDim2.fromOffset(4000, 400)
Label.Parent = Gui

local Rows = {}
for _, FontSpec in Fonts do
	Label.FontFace = Font.new(`rbxasset://fonts/families/{FontSpec[1]}.json`, Enum.FontWeight[FontSpec[2]])
	for S = 6, 150 do
		Label.TextSize = (S + 0.25) / Scale
		local Widths = {}
		for _, Text in Strings do
			Label.Text = Text
			table.insert(Widths, math.round(Label.TextBounds.X * Scale))
		end
		table.insert(Rows, { font = FontSpec[1], S = S, h = math.round(Label.TextBounds.Y * Scale), widths = Widths })
	end
end
Gui:Destroy()
return HttpService:JSONEncode({ strings = Strings, rows = Rows })
