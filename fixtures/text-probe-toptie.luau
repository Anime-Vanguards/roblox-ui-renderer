-- Text line tops just below a half device pixel: top-aligned labels ("Hg", SourceSans 20) whose top lies d = 0 to 0.004
-- device px below k + 0.5, through UDim scales of a 1000 px holder. Row set A is placed for 1.5x, set B for 3x and set C
-- for 1x (at another scale a set's tops are not near a half pixel). Which row the client starts the text on shows how a
-- line top rounds; frame edges round up from 4/15 x scale / 256 px below (render/geometry.rs snap_device).
-- rur compare --fixture fixtures/text-probe-toptie.luau --viewport 1200x600 --time 0.2 --background 0,0,0
return function(Target)
	local Holder = Instance.new("Frame")
	Holder.Name = "Holder"
	Holder.BackgroundTransparency = 1
	Holder.Size = UDim2.fromOffset(1000, 1000)
	Holder.Parent = Target
	local Face = Font.new("rbxasset://fonts/families/SourceSansPro.json")
	for Set, Scale in { 1.5, 3, 1 } do
		for Row = 0, 2 do
			for Step = 0, 20 do
				local D = Step * 0.0002
				local K = 40 + (Set - 1) * 180 + Row * 60
				local Device = K * Scale + 0.5 - D
				local L = Instance.new("TextLabel")
				L.Name = string.format("S%dR%dD%02d", Set, Row, Step)
				L.BackgroundTransparency = 1
				L.TextColor3 = Color3.fromRGB(255, 255, 255)
				L.FontFace = Face
				L.TextSize = 20 + Row * 3
				L.Text = "Hg"
				L.TextXAlignment = Enum.TextXAlignment.Left
				L.TextYAlignment = Enum.TextYAlignment.Top
				L.Position = UDim2.new(0, 10 + Step * 52, Device / Scale / 1000, 0)
				L.Size = UDim2.fromOffset(46, 40)
				L.Parent = Holder
			end
		end
	end
end
