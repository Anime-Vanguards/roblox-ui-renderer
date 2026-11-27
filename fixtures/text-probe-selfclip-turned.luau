-- A label's own ClipsDescendants under a turned ancestor: narrow labels whose text overflows their box (Left, Center,
-- Right; one wrapped, one with a UIStroke), ClipsDescendants off and on, inside frames turned 0, 15, -30 and 90 degrees,
-- and a turned label inside a turned frame. Each label has a dim background so its rect shows; the frames are flat.
-- rur compare --fixture fixtures/text-probe-selfclip-turned.luau --viewport 1600x900 --time 0.2 --background 0,0,0
return function(Target)
	local Angles = { 0, 15, -30, 90 }
	for A, Angle in Angles do
		local Frame = Instance.new("Frame")
		Frame.Name = string.format("Turn%d", A)
		Frame.BackgroundColor3 = Color3.fromRGB(24, 28, 44)
		Frame.BorderSizePixel = 0
		Frame.AnchorPoint = Vector2.new(0.5, 0.5)
		Frame.Position = UDim2.fromOffset(({ 200, 560, 940, 1385 })[A], 450)
		Frame.Size = UDim2.fromOffset(300, 420)
		Frame.Rotation = Angle
		Frame.Parent = Target
		local Cases = {
			{ "Left", Enum.TextXAlignment.Left, false, nil, 0 },
			{ "Center", Enum.TextXAlignment.Center, false, nil, 0 },
			{ "Right", Enum.TextXAlignment.Right, false, nil, 0 },
			{ "Wrap", Enum.TextXAlignment.Left, true, nil, 0 },
			{ "Stroke", Enum.TextXAlignment.Center, false, 3, 0 },
			{ "Turned", Enum.TextXAlignment.Center, false, nil, 20 },
		}
		for C, Case in Cases do
			for Clip = 0, 1 do
				local L = Instance.new("TextLabel")
				L.Name = string.format("%s%d", Case[1], Clip)
				L.BackgroundColor3 = Color3.fromRGB(70, 60, 100)
				L.BorderSizePixel = 0
				L.TextColor3 = Color3.fromRGB(255, 255, 255)
				L.FontFace = Font.new("rbxasset://fonts/families/SourceSansPro.json", Enum.FontWeight.Bold)
				L.TextSize = 24
				L.Text = "Overflowing text"
				L.TextXAlignment = Case[2]
				L.TextWrapped = Case[3]
				L.ClipsDescendants = Clip == 1
				L.Position = UDim2.fromOffset(30 + Clip * 140, 16 + (C - 1) * 66)
				L.Size = UDim2.fromOffset(100, Case[3] and 40 or 30)
				L.Rotation = Case[5]
				if Case[4] then
					local Stroke = Instance.new("UIStroke")
					Stroke.Thickness = Case[4]
					Stroke.Color = Color3.fromRGB(240, 120, 40)
					Stroke.Parent = L
				end
				L.Parent = Frame
			end
		end
	end
end
