-- AutomaticSize objects with a scale Size inside a parent with UIPadding: which parent size does the
-- scale resolve against (a missions window's Progress/Value: 0.121 of a 630 px parent padded to 622 is
-- 76.23 in Studio, not 75.26)?
return function(Target)
	local Results = {}
	local Items = {}
	local Face = Font.new("rbxasset://fonts/families/FredokaOne.json")
	local Cases = {
		{ "none", nil },
		{ "offset", { UDim.new(0, 8), UDim.new(0, 12), UDim.new(0, 5), UDim.new(0, 3) } },
		{ "scale", { UDim.new(0.05, 0), UDim.new(0.1, 0), UDim.new(0.07, 0), UDim.new(0.02, 0) } },
		{ "mixed", { UDim.new(0.03, 4), UDim.new(0, 6), UDim.new(0.05, 2), UDim.new(0, 0) } },
	}
	for Index, Case in Cases do
		local Parent = Instance.new("Frame")
		Parent.Position = UDim2.fromOffset(20, 20 + (Index - 1) * 200)
		Parent.Size = UDim2.fromOffset(630, 180)
		Parent.BackgroundTransparency = 1
		Parent.Parent = Target
		if Case[2] then
			local Padding = Instance.new("UIPadding")
			Padding.PaddingLeft, Padding.PaddingRight, Padding.PaddingTop, Padding.PaddingBottom = table.unpack(Case[2])
			Padding.Parent = Parent
		end
		local function Add(Key, Object)
			Object.Parent = Parent
			table.insert(Items, { Key = `{Case[1]}_{Key}`, Object = Object })
		end
		for _, Auto in { "X", "Y", "XY", "None" } do
			local Frame = Instance.new("Frame")
			Frame.AutomaticSize = Enum.AutomaticSize[Auto]
			Frame.Position = UDim2.new(0.1, 0, 0.05, 0)
			Frame.Size = UDim2.new(0.121, 3, 0.17, 2)
			Frame.BackgroundTransparency = 1
			local Child = Instance.new("Frame")
			Child.Size = UDim2.fromOffset(10, 10)
			Child.Parent = Frame
			Add(`frame{Auto}`, Frame)
			local Label = Instance.new("TextLabel")
			Label.AutomaticSize = Enum.AutomaticSize[Auto]
			Label.Position = UDim2.new(0.5, 0, 0.3, 0)
			Label.Size = UDim2.new(0.121, 0, 0.1, 13)
			Label.FontFace = Face
			Label.TextSize = 20
			Label.Text = "2/3"
			Add(`label{Auto}`, Label)
			local Wide = Instance.new("TextLabel")
			Wide.AutomaticSize = Enum.AutomaticSize[Auto]
			Wide.Position = UDim2.new(0.2, 0, 0.6, 0)
			Wide.Size = UDim2.new(0.05, 0, 0.05, 0)
			Wide.FontFace = Face
			Wide.TextSize = 20
			Wide.Text = "Wider than the box"
			Add(`wide{Auto}`, Wide)
		end
	end
	task.wait()
	for _, Entry in Items do
		local Object = Entry.Object
		Results[Entry.Key] = { Object.AbsolutePosition.X, Object.AbsolutePosition.Y, Object.AbsoluteSize.X, Object.AbsoluteSize.Y }
	end
	return Results
end
