-- Rich TextTruncate (AtEnd, left and top aligned, SourceSans) of "<letter>ow" in boxes a quarter of a device px apart (at
-- 1.5x) around the width of the first letter and the ellipsis, for eight letters and three ways to the same drawn size:
-- TextSize 14 under UIScale 2, TextSize 28 unscaled, TextSize 26 under UIScale 1.3. Where the box only reaches that width
-- through its next whole pixel, the client draws the letter alone in some of them (fixtures/text-probe-richtrunc-edge.luau).
-- rur compare --fixture fixtures/text-probe-richtrunc-band.luau --viewport 1000x840 --time 0.2 --background 0,0,0
return function(Target)
	local TextService = game:GetService("TextService")
	local Face = Font.new("rbxasset://fonts/families/SourceSansPro.json")
	local Configs = { { 14, 2 }, { 28, 1 }, { 26, 1.3 } }
	local Y = 4
	for ConfigIndex, Config in Configs do
		local Size, Scale = Config[1], Config[2]
		local E = math.floor(Size * Scale)
		for _, Letter in { "A", "B", "E", "H", "M", "N", "O", "W" } do
			local Huge = Vector2.new(10000, 10000)
			-- the fit in device px at 1.5x, roughly (TextService rounds up to logical px)
			local Fit = (TextService:GetTextSize(Letter, E, Enum.Font.SourceSans, Huge).X
				+ TextService:GetTextSize("\u{2026}", E, Enum.Font.SourceSans, Huge).X) * 1.5
			local Holder = Instance.new("Frame")
			Holder.Name = string.format("c%d_%s", ConfigIndex, Letter)
			Holder.BackgroundTransparency = 1
			Holder.Position = UDim2.fromOffset(4, Y)
			Holder.Size = UDim2.fromOffset(1000, 30)
			Holder.Parent = Target
			local UIScale = Instance.new("UIScale")
			UIScale.Scale = Scale
			UIScale.Parent = Holder
			local X = 0
			for Step = -12, 6 do
				local Device = math.round(Fit) + Step / 4
				local Width = Device / (1.5 * Scale)
				local L = Instance.new("TextLabel")
				L.Name = string.format("d%05d", math.round(Device * 100))
				L.BackgroundColor3 = Color3.fromRGB(60 + 30 * (ConfigIndex - 1), 50, 90)
				L.BorderSizePixel = 0
				L.TextColor3 = Color3.fromRGB(255, 255, 255)
				L.FontFace = Face
				L.TextSize = Size
				L.RichText = true
				L.Text = Letter .. "ow"
				L.TextTruncate = Enum.TextTruncate.AtEnd
				L.TextXAlignment = Enum.TextXAlignment.Left
				L.TextYAlignment = Enum.TextYAlignment.Top
				L.Position = UDim2.fromScale(X / 1000, 0)
				L.Size = UDim2.new(Width / 1000, 0, 0, math.ceil(30 / Scale))
				L.Parent = Holder
				X += Width + 12 / Scale
			end
			Y += 34
		end
	end
end
