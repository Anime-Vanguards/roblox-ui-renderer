-- Rich TextScaled clamped by a UITextSizeConstraint (Round 8, notes/text.md): a list row's button label
-- (FredokaOne, RichText, TextScaled, UITextSizeConstraint MaxTextSize 17, UIStroke 0.75) draws 25 px glyphs
-- with the "W" band a pixel wider, as if its quads were scaled by 25.5 / 25. Labels capped by odd / even
-- MaxTextSize (roomy boxes) and raised by MinTextSize (tiny boxes), rich and plain, at UIScale 1 / 0.885 / 1.25.
-- rur compare --fixture fixtures/text-probe-richcap.luau --viewport 1900x900 --time 0.2 --background 0,0,0
return function(Target)
	local Texts = { "VIEW", "WAVE W", "Review & craft" }
	local Caps = {
		{ Max = 17, Box = { 80, 30 } },
		{ Max = 13, Box = { 110, 40 } },
		{ Max = 21, Box = { 140, 50 } },
		{ Max = 16, Box = { 110, 40 } },
		{ Min = 13, Box = { 30, 10 } },
		{ Min = 17, Box = { 40, 12 } },
	}
	local Scales = { 1, 0.885, 1.25 }
	local Index = 0
	for _, Rich in { true, false } do
		for S, Scale in Scales do
			for C, Cap in Caps do
				for T, Text in Texts do
					local Column = Index % 9
					local Row = Index // 9
					Index += 1
					local Holder = Instance.new("Frame")
					Holder.Name = string.format("%sS%dC%dT%d", if Rich then "R" else "P", S, C, T)
					Holder.BorderSizePixel = 0
					Holder.BackgroundColor3 = Color3.fromRGB(19, 19, 19)
					Holder.Size = UDim2.fromOffset(Cap.Box[1], Cap.Box[2])
					Holder.Position = UDim2.fromOffset(6 + Column * 205, 6 + Row * 66)
					Holder.Parent = Target
					local L = Instance.new("TextLabel")
					L.Name = "Label"
					L.BackgroundTransparency = 1
					L.TextColor3 = Color3.fromRGB(255, 255, 255)
					L.FontFace = Font.new("rbxasset://fonts/families/FredokaOne.json")
					L.RichText = Rich
					L.TextScaled = true
					L.TextWrapped = true
					L.TextSize = 30
					L.Text = Text
					L.Size = UDim2.fromScale(1, 1)
					L.Parent = Holder
					local Constraint = Instance.new("UITextSizeConstraint")
					if Cap.Max then
						Constraint.MaxTextSize = Cap.Max
					end
					if Cap.Min then
						Constraint.MinTextSize = Cap.Min
					end
					Constraint.Parent = L
					local Stroke = Instance.new("UIStroke")
					Stroke.Thickness = 0.75
					Stroke.Color = Color3.fromRGB(0, 0, 0)
					Stroke.Parent = L
					if Scale ~= 1 then
						local U = Instance.new("UIScale")
						U.Scale = Scale
						U.Parent = Holder
					end
				end
			end
		end
	end
end
