-- Text UIStroke joins and glyph overlap: sharp-cornered glyphs (SourceSans Pro Bold, Montserrat) with
-- Round/Bevel/Miter strokes (fill hidden with TextTransparency 1), and thick strokes on opaque text
-- to show whether a glyph's stroke covers its neighbour's fill.
-- compare: --viewport 1500x640 --time 0.3 --background 0,0,0
return function(Target)
	local function Label(Name, Family, Weight, Text, Size, X, Y, W, H, Thickness, Join, TextTransparency, StrokeColor)
		local Label = Instance.new("TextLabel")
		Label.Name = Name
		Label.BackgroundTransparency = 1
		Label.FontFace = Font.new(Family, Weight)
		Label.TextSize = Size
		Label.Text = Text
		Label.TextColor3 = Color3.new(1, 1, 1)
		Label.TextTransparency = TextTransparency
		Label.TextXAlignment = Enum.TextXAlignment.Left
		Label.TextYAlignment = Enum.TextYAlignment.Top
		Label.Position = UDim2.fromOffset(X, Y)
		Label.Size = UDim2.fromOffset(W, H)
		Label.Parent = Target

		local Stroke = Instance.new("UIStroke")
		Stroke.Thickness = Thickness
		Stroke.LineJoinMode = Join
		Stroke.Color = StrokeColor or Color3.new(1, 1, 1)
		Stroke.Parent = Label
	end

	local SourceSans = "rbxasset://fonts/families/SourceSansPro.json"
	local Montserrat = "rbxasset://fonts/families/Montserrat.json"
	local Fredoka = "rbxasset://fonts/families/FredokaOne.json"
	local Joins = { Enum.LineJoinMode.Round, Enum.LineJoinMode.Bevel, Enum.LineJoinMode.Miter }
	for Index, Join in Joins do
		Label(`SS{Index}`, SourceSans, Enum.FontWeight.Bold, "VAMZ4", 64, 20 + (Index - 1) * 480, 20, 460, 100, 5, Join, 1)
		Label(`MS{Index}`, Montserrat, Enum.FontWeight.Bold, "VAMZ4", 48, 20 + (Index - 1) * 480, 140, 460, 80, 3, Join, 1)
		Label(`SW{Index}`, SourceSans, Enum.FontWeight.Regular, "WAVE", 64, 20 + (Index - 1) * 480, 240, 460, 100, 8, Join, 0, Color3.new(1, 0, 0))
	end
	Label("FW", Fredoka, Enum.FontWeight.Regular, "WAVE", 64, 20, 380, 460, 100, 10, Enum.LineJoinMode.Round, 0, Color3.new(1, 0, 0))
	Label("FT", Fredoka, Enum.FontWeight.Regular, "Hill", 32, 500, 400, 200, 60, 6, Enum.LineJoinMode.Round, 0, Color3.new(1, 0, 0))
end
