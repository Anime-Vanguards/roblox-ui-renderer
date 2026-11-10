-- Measures TextBounds of the faces whose hhea table has a line gap (Arimo, Ubuntu, Permanent Marker,
-- AccanthisADFStd, Guru and the cloud family Poppins): the grid of measure-text.luau. Its results are
-- data/studio/text-bounds-linegap-1.5x.json and -1x.json, one per DPI scale of the Studio session that ran
-- it (the bounds are device pixels over that scale: thirds of a pixel at 1.5, whole pixels at 1).
local HttpService = game:GetService("HttpService")
local CoreGui = game:GetService("CoreGui")

local Fonts = {
	{ "PermanentMarker", "Regular" }, { "AccanthisADFStd", "Regular" }, { "Guru", "Regular" },
	{ "Ubuntu", "Regular" }, { "Ubuntu", "Medium" }, { "Ubuntu", "Bold" },
	{ "Arimo", "Regular" }, { "Arimo", "Bold" },
	{ "Poppins", "Regular", "rbxassetid://11702779409" }, { "Poppins", "Medium", "rbxassetid://11702779409" },
	{ "Poppins", "SemiBold", "rbxassetid://11702779409" }, { "Poppins", "Bold", "rbxassetid://11702779409" },
}
local Sizes = { 8, 10, 11, 12, 13, 14, 16, 18, 20, 24, 28, 32, 36, 40, 48, 64, 100 }
local Strings = { "a", "ab", "VA", "To", "Te", "Yo", "fi", "Hello World", "The quick brown fox jumps over the lazy dog", "1234567890", "WWWWW", "iiiii", "ygjpq", "Hello  ", "  x" }

local Gui = Instance.new("ScreenGui")
Gui.Name = "__RUR_Measure"
Gui.Parent = CoreGui
local Label = Instance.new("TextLabel")
Label.Size = UDim2.fromOffset(4000, 400)
Label.TextWrapped = false
Label.Parent = Gui

local function Face(FontSpec)
	return Font.new(FontSpec[3] or `rbxasset://fonts/families/{FontSpec[1]}.json`, Enum.FontWeight[FontSpec[2]])
end

-- Warm every face first: TextBounds read before a font finishes loading uses the fallback face.
local TextService = game:GetService("TextService")
for _ = 1, 2 do
	for _, FontSpec in Fonts do
		local Params = Instance.new("GetTextBoundsParams")
		Params.Text = "Warm up"
		Params.Font = Face(FontSpec)
		Params.Size = 20
		pcall(TextService.GetTextBoundsAsync, TextService, Params)
	end
	task.wait(1)
end

local Rows = {}
for _, FontSpec in Fonts do
	Label.FontFace = Face(FontSpec)
	task.wait(0.1)
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
		table.insert(Rows, { font = FontSpec[1], weight = FontSpec[2], family = FontSpec[3], size = Size, widths = Widths, height = Height })
	end
end
Gui:Destroy()

return HttpService:JSONEncode({ strings = Strings, rows = Rows, version = version() })
