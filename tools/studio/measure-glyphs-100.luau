-- Per-glyph advance probe: TextBounds of c, cc, ccccc for printable ASCII across fonts/sizes.
local HttpService = game:GetService("HttpService")
local CoreGui = game:GetService("CoreGui")

local Fonts = { { "SourceSansPro", "Regular" }, { "BuilderSans", "Regular" }, { "Montserrat", "Bold" }, { "FredokaOne", "Regular" } }
local Sizes = { 100 }

local Gui = Instance.new("ScreenGui")
Gui.Name = "__RUR_Measure"
Gui.Parent = CoreGui
local Label = Instance.new("TextLabel")
Label.Size = UDim2.fromOffset(4000, 400)
Label.Parent = Gui

local Rows = {}
for _, FontSpec in Fonts do
	Label.FontFace = Font.new(`rbxasset://fonts/families/{FontSpec[1]}.json`, Enum.FontWeight[FontSpec[2]])
	for _, Size in Sizes do
		Label.TextSize = Size
		local Glyphs = {}
		for Code = 33, 126 do
			local Character = string.char(Code)
			Label.Text = Character
			local One = Label.TextBounds.X
			Label.Text = string.rep(Character, 2)
			local Two = Label.TextBounds.X
			Label.Text = string.rep(Character, 5)
			local Five = Label.TextBounds.X
			Glyphs[Character] = { One, Two, Five }
		end
		Label.Text = " "
		local Space1 = Label.TextBounds.X
		Label.Text = "a a"
		local SpaceMid = Label.TextBounds.X
		table.insert(Rows, { font = FontSpec[1], weight = FontSpec[2], size = Size, glyphs = Glyphs, space = { Space1, SpaceMid } })
	end
end
Gui:Destroy()
return HttpService:JSONEncode({ rows = Rows })
