-- Rich text runs next to each other with the same formatting (each character in a tag of its own) against the untagged
-- text: a client lays them out as one run (kerned across the tags), and keeps runs of other formatting apart. Four faces,
-- three texts, eleven tag patterns.
-- compare: --viewport 1000x1300 --time 0.2 --background 0,0,0
return function(Target)
	local Faces = { "SourceSansPro", "FredokaOne", "Bangers", "Arimo" }
	local Texts = { "AVAWATo", "Wave LTyard", "hello world" }
	local Patterns = {
		{ "plain", function(c) return c end, false },
		{ "rich", function(c) return c end, true },
		{ "same0", function(c) return `<font transparency="0">{c}</font>` end, true },
		{ "samecol", function(c) return `<font color="#F0F0F0">{c}</font>` end, true },
		{ "red", function(c) return `<font color="#FF2020">{c}</font>` end, true },
		{ "alt", function(c, i) return if i % 2 == 0 then `<font color="#FF2020">{c}</font>` else `<font color="#20FF20">{c}</font>` end, true },
		{ "altsame", function(c, i) return if i % 2 == 0 then `<font color="#F0F0F0">{c}</font>` else `<font color="rgb(240,240,240)">{c}</font>` end, true },
		{ "bold", function(c) return `<b>{c}</b>` end, true },
		{ "size", function(c) return `<font size="22">{c}</font>` end, true },
		{ "mixcol", function(c, i) return if i % 2 == 0 then `<font color="#F0F0F0">{c}</font>` else c end, true },
		{ "nest", function(c, i) return if i % 2 == 0 then `<b><i>{c}</i></b>` else `<i><b>{c}</b></i>` end, true },
	}
	local Y = 4
	for F, FaceName in Faces do
		for T, Text in Texts do
			for P, Pattern in Patterns do
				local Parts = {}
				local I = 0
				for _, Code in utf8.codes(Text) do
					I += 1
					table.insert(Parts, Pattern[2](utf8.char(Code), I))
				end
				local L = Instance.new("TextLabel")
				L.Name = `R{F}_{T}_{Pattern[1]}`
				L.Position = UDim2.fromOffset(4 + ((P - 1) % 3) * 330, Y + ((P - 1) // 3) * 26)
				L.Size = UDim2.fromOffset(320, 24)
				L.BackgroundColor3 = Color3.fromRGB(30, 32, 44)
				L.BorderSizePixel = 0
				L.TextColor3 = Color3.fromRGB(240, 240, 240)
				L.FontFace = Font.new(`rbxasset://fonts/families/{FaceName}.json`)
				L.TextSize = 22
				L.TextXAlignment = Enum.TextXAlignment.Left
				L.RichText = Pattern[3]
				L.Text = table.concat(Parts)
				L.Parent = Target
			end
			Y += 108
		end
	end
end
