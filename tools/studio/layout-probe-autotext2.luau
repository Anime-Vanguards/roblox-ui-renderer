-- Text children with scale widths in AutomaticSize containers (plain and vertical list), a matrix:
-- container AutomaticSize X / Y / XY, child Size (1, 0) / (0.5, 0) / (1, -20) / (0.5, 30) width,
-- child AutomaticSize None / X / Y / XY, wrapped or not, short or long text. Holders are 290 wide.
return function(Target)
	local Results = {}
	local Nodes = {}
	local Face = Font.new("rbxasset://fonts/families/SourceSansPro.json")
	local Texts = { short = "Hi there", long = "Which units would you like to unequip" }
	local Count = 0
	local function Holder()
		Count += 1
		local H = Instance.new("Frame")
		H.Position = UDim2.fromOffset(((Count - 1) % 4) * 300, ((Count - 1) // 4) * 12)
		H.Size = UDim2.fromOffset(290, 100)
		H.BackgroundTransparency = 1
		H.Parent = Target
		return H
	end
	local Widths = { s1 = UDim.new(1, 0), s05 = UDim.new(0.5, 0), s1m = UDim.new(1, -20), s05p = UDim.new(0.5, 30) }
	for _, Container in { "plain", "list" } do
		for _, Auto in { "X", "Y", "XY" } do
			for WidthName, Width in Widths do
				for _, ChildAuto in { "None", "X", "Y", "XY" } do
					for _, Wrap in { false, true } do
						for TextName, Text in Texts do
							local A = Instance.new("Frame")
							A.Size = UDim2.fromOffset(0, 20)
							A.AutomaticSize = Enum.AutomaticSize[Auto]
							A.BackgroundTransparency = 1
							A.Parent = Holder()
							if Container == "list" then
								Instance.new("UIListLayout").Parent = A
							end
							local L = Instance.new("TextLabel")
							L.FontFace = Face
							L.TextSize = 20
							L.Size = UDim2.new(Width.Scale, Width.Offset, 0, 25)
							L.AutomaticSize = Enum.AutomaticSize[ChildAuto]
							L.TextWrapped = Wrap
							L.Text = Text
							L.Parent = A
							Nodes[`{Container}_{Auto}_{WidthName}_{ChildAuto}_{Wrap and "w" or "n"}_{TextName}`] = { A, L }
						end
					end
				end
			end
		end
	end
	for _ = 1, 3 do
		task.wait()
	end
	for Key, Pair in Nodes do
		local A, L = Pair[1], Pair[2]
		Results[Key] = { A.AbsoluteSize.X, A.AbsoluteSize.Y, L.AbsoluteSize.X, L.AbsoluteSize.Y, L.TextBounds.X, L.TextBounds.Y }
	end
	return Results
end
