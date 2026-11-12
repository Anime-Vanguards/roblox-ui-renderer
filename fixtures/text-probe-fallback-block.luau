-- A glyph from a fallback face at large sizes (U+2588 FULL BLOCK: SourceSans has none, Arimo draws it), drawn,
-- and under a UIShadow of Mode = Text with the text clear: where the fallback glyph sits and how large it is,
-- against the same glyph with Arimo as the label's own face. Rows: SourceSans (fallback), Arimo (own face).
-- Columns: drawn text; clear text with a hard red shadow (BlurRadius 0); the same with BlurRadius 8.
-- rur compare --fixture fixtures/text-probe-fallback-block.luau --viewport 1200x900 --time 0.3 --background 0,0,0
return function(Target)
	local Sizes = { 100, 60, 45, 30, 14 }
	local Faces = { "rbxasset://fonts/families/SourceSansPro.json", "rbxasset://fonts/families/Arimo.json" }
	for F, Face in Faces do
		local X = 12
		for I, Size in Sizes do
			for V = 1, 3 do
				local L = Instance.new("TextLabel")
				L.Name = string.format("F%dS%dV%d", F, Size, V)
				L.Position = UDim2.fromOffset(X + (V - 1) * 400, 12 + (F - 1) * 200)
				L.Size = UDim2.fromOffset(math.ceil(Size * 1.3), math.ceil(Size * 1.3))
				L.BackgroundTransparency = 1
				L.BorderSizePixel = 0
				L.Text = "\u{2588}x"
				L.TextColor3 = Color3.new(1, 1, 1)
				L.FontFace = Font.new(Face)
				L.TextSize = Size
				L.TextXAlignment = Enum.TextXAlignment.Left
				L.TextYAlignment = Enum.TextYAlignment.Top
				if V > 1 then
					L.TextTransparency = 1
					local S = Instance.new("UIShadow")
					S.Mode = Enum.ApplyShadowMode.Text
					S.Color = Color3.new(1, 0, 0)
					S.BlurRadius = UDim.new(0, if V == 2 then 0 else 8)
					S.Parent = L
				end
				L.Parent = Target
			end
			X += math.ceil(Size * 1.3) + 14
		end
	end
end
