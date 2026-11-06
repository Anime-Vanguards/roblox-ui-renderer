-- Measures TextBounds for a grid of fonts × sizes × strings (TextLabel under a CoreGui ScreenGui).
local HttpService = game:GetService("HttpService")
local CoreGui = game:GetService("CoreGui")

local Fonts = {
	{ "SourceSansPro", "Regular" }, { "SourceSansPro", "Bold" },
	{ "FredokaOne", "Regular" },
	{ "Montserrat", "Regular" }, { "Montserrat", "Bold" },
	{ "GothamSSm", "Regular" }, { "GothamSSm", "Bold" }, { "GothamSSm", "Medium" }, { "GothamSSm", "Heavy" },
	{ "BuilderSans", "Regular" }, { "BuilderSans", "Bold" },
	{ "Roboto", "Regular" }, { "Arimo", "Regular" }, { "LegacyArial", "Regular" },
}
local Sizes = { 8, 10, 11, 12, 13, 14, 16, 18, 20, 24, 28, 32, 36, 40, 48, 64, 100 }
local Strings = { "a", "ab", "AV", "To", "Te", "Yo", "fi", "Hello World", "The quick brown fox jumps over the lazy dog", "1234567890", "WWWWW", "iiiii", "ygjpq", "Hello  ", "  x" }

local Gui = Instance.new("ScreenGui")
Gui.Name = "__RUR_Measure"
Gui.Parent = CoreGui
local Label = Instance.new("TextLabel")
Label.Size = UDim2.fromOffset(4000, 400)
Label.TextWrapped = false
Label.Parent = Gui

-- Warm every face first: TextBounds read before a font finishes loading uses the fallback face.
local TextService = game:GetService("TextService")
for _, FontSpec in Fonts do
	local Params = Instance.new("GetTextBoundsParams")
	Params.Text = "Warm up"
	Params.Font = Font.new(`rbxasset://fonts/families/{FontSpec[1]}.json`, Enum.FontWeight[FontSpec[2]])
	Params.Size = 20
	pcall(TextService.GetTextBoundsAsync, TextService, Params)
end
task.wait(0.5)

local Rows = {}
for _, FontSpec in Fonts do
	Label.FontFace = Font.new(`rbxasset://fonts/families/{FontSpec[1]}.json`, Enum.FontWeight[FontSpec[2]])
	for _, Size in Sizes do
		Label.TextSize = Size
		local Widths = {}
		local Height = 0
		for _, Text in Strings do
			Label.Text = Text
			local Bounds = Label.TextBounds
			table.insert(Widths, Bounds.X)
			Height = Bounds.Y
		end
		table.insert(Rows, { font = FontSpec[1], weight = FontSpec[2], size = Size, widths = Widths, height = Height })
	end
end
Gui:Destroy()

return HttpService:JSONEncode({ strings = Strings, rows = Rows, scale = workspace.CurrentCamera.ViewportSize.Y })
