-- Text children with scale widths in AutomaticSize containers (tools/studio/layout-probe-autotext2.luau as a
-- fixture, every holder named after its case): container plain or vertical list, AutomaticSize X / Y / XY, child
-- Size (1, 0) / (0.5, 0) / (1, -20) / (0.5, 30) wide and 25 tall, child AutomaticSize None / X / Y / XY, wrapped or
-- not, short or long text. Holders are 290 wide and overlap (rows 8.5 px apart); only the layout is compared.
--   node cli/rur.mjs compare --fixture fixtures/layout-autotext-matrix.luau --viewport 1200x900 --time 0.2 --background 0,0,0
return function(Target)
	local Face = Font.new("rbxasset://fonts/families/SourceSansPro.json")
	local Texts = { { "short", "Hi there" }, { "long", "Which units would you like to unequip" } }
	local Widths = { { "s1", UDim.new(1, 0) }, { "s05", UDim.new(0.5, 0) }, { "s1m", UDim.new(1, -20) }, { "s05p", UDim.new(0.5, 30) } }
	local Count = 0
	local function Holder(Name)
		Count += 1
		local H = Instance.new("Frame")
		H.Name = Name
		H.Position = UDim2.fromOffset(((Count - 1) % 4) * 300, ((Count - 1) // 4) * 8.5)
		H.Size = UDim2.fromOffset(290, 100)
		H.BackgroundTransparency = 1
		H.Parent = Target
		return H
	end
	for _, Container in { "plain", "list" } do
		for _, Auto in { "X", "Y", "XY" } do
			for _, WidthPair in Widths do
				local WidthName, Width = WidthPair[1], WidthPair[2]
				for _, ChildAuto in { "None", "X", "Y", "XY" } do
					for _, Wrap in { false, true } do
						for _, TextPair in Texts do
							local A = Instance.new("Frame")
							A.Name = "A"
							A.Size = UDim2.fromOffset(0, 20)
							A.AutomaticSize = Enum.AutomaticSize[Auto]
							A.BackgroundTransparency = 1
							A.Parent = Holder(`{Container}_{Auto}_{WidthName}_{ChildAuto}_{Wrap and "w" or "n"}_{TextPair[1]}`)
							if Container == "list" then
								Instance.new("UIListLayout").Parent = A
							end
							local L = Instance.new("TextLabel")
							L.Name = "L"
							L.FontFace = Face
							L.TextSize = 20
							L.TextColor3 = Color3.new(1, 1, 1)
							L.BackgroundTransparency = 1
							L.Size = UDim2.new(Width.Scale, Width.Offset, 0, 25)
							L.AutomaticSize = Enum.AutomaticSize[ChildAuto]
							L.TextWrapped = Wrap
							L.Text = TextPair[2]
							L.Parent = A
						end
					end
				end
			end
		end
	end
	-- AutomaticSize XY wrapped labels whose base width is wider than the space they have: A is W px wide (AutomaticSize
	-- Y), the label (0.5, 30) or (0, 30) wide; also unwrapped, AutomaticSize Y only and a 2-line text.
	for _, W in { 0, 10, 20, 40 } do
		for _, Entry in {
			{ "k05", UDim.new(0.5, 30), "XY", true, "Hi there" },
			{ "o30", UDim.new(0, 30), "XY", true, "Hi there" },
			{ "nowrap", UDim.new(0.5, 30), "XY", false, "Hi there" },
			{ "yonly", UDim.new(0.5, 30), "Y", true, "Hi there" },
			{ "short", UDim.new(0.5, 30), "XY", true, "Hi" },
		} do
			local A = Instance.new("Frame")
			A.Name = "A"
			A.Size = UDim2.fromOffset(W, 20)
			A.AutomaticSize = Enum.AutomaticSize.Y
			A.BackgroundTransparency = 1
			A.Parent = Holder(`narrow_w{W}_{Entry[1]}`)
			local L = Instance.new("TextLabel")
			L.Name = "L"
			L.FontFace = Face
			L.TextSize = 20
			L.TextColor3 = Color3.new(1, 1, 1)
			L.BackgroundTransparency = 1
			L.Size = UDim2.new(Entry[2].Scale, Entry[2].Offset, 0, 25)
			L.AutomaticSize = Enum.AutomaticSize[Entry[3]]
			L.TextWrapped = Entry[4]
			L.Text = Entry[5]
			L.Parent = A
		end
	end
end
