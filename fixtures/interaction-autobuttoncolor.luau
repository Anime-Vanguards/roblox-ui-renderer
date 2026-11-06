-- AutoButtonColor tint: the hover-round colours as concentric stacks of TextButtons
-- and ImageButtons (every button under the cursor hovers), plus a pressed-looking set via --actions.
-- Studio reference values: out/interaction/hover-round, press-tint (tools/interaction/experiments).
--   node cli/rur.mjs render --fixture fixtures/interaction-autobuttoncolor.luau --viewport 900x500 --time 0.5
--     --background 20,20,26 --actions '[{"t":0.1,"type":"move","x":250,"y":250}]'
return function(Target)
	local Colors = { { 255, 245, 51 }, { 25, 15, 5 }, { 145, 85, 115 }, { 175, 205, 235 }, { 1, 2, 3 }, { 128, 127, 129 }, { 211, 221, 241 }, { 65, 75, 95 } }
	local Count = #Colors
	for _, Stack in { { Name = "T", Class = "TextButton", X = 250, Y = 250 }, { Name = "I", Class = "ImageButton", X = 650, Y = 250 } } do
		for Index, C in Colors do
			local B = Instance.new(Stack.Class)
			B.Name = Stack.Name .. Index
			if Stack.Class == "TextButton" then
				B.Text = ""
			end
			B.BorderSizePixel = 0
			B.AnchorPoint = Vector2.new(0.5, 0.5)
			B.Position = UDim2.fromOffset(Stack.X, Stack.Y)
			B.Size = UDim2.fromOffset(20 + (Count - Index) * 24, 20 + (Count - Index) * 24)
			B.ZIndex = Index
			B.BackgroundColor3 = Color3.fromRGB(C[1], C[2], C[3])
			B.Parent = Target
		end
	end
end
