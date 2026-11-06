-- A/B: TextBounds with TextWrapped false vs true (wide box) for the same strings.
local HttpService = game:GetService("HttpService")
local CoreGui = game:GetService("CoreGui")
local Gui = Instance.new("ScreenGui")
Gui.Name = "__RUR_Measure"
Gui.Parent = CoreGui
local Label = Instance.new("TextLabel")
Label.Size = UDim2.fromOffset(3000, 500)
Label.Parent = Gui
local Strings = {
	"Supercalifragilisticexpialidocious word", "Supercalifragilisticexpialidocious", "word", " word", "s w", "line two is longer",
	"line", "two", "is", "longer", "one-two-three four-five", "four-five", "one-two-three", "Hello World", "a b", "ab", "d", "r", "e ", " e",
}
local Rows = {}
for _, Family in { "SourceSansPro", "FredokaOne" } do
	Label.FontFace = Font.new(`rbxasset://fonts/families/{Family}.json`)
	for _, Size in { 14, 20, 32 } do
		Label.TextSize = Size
		for _, Text in Strings do
			Label.TextWrapped = false
			Label.Text = Text
			local _ = Label.AbsoluteSize
			task.wait()
			local Unwrapped = Label.TextBounds.X
			Label.TextWrapped = true
			local _ = Label.AbsoluteSize
			task.wait()
			local Wrapped = Label.TextBounds.X
			table.insert(Rows, { f = Family, s = Size, t = Text, u = math.round(Unwrapped * 1.5), w = math.round(Wrapped * 1.5) })
		end
	end
end
Gui:Destroy()
return HttpService:JSONEncode(Rows)
