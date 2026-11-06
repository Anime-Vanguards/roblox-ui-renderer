-- Rich TextScaled (round 4): fine box-width sweeps (0.04 px) of height-limited labels, to pin the dip
-- edges exactly (each edge is where a bisection grid point crosses a whole pixel size), plus the
-- unscaled TextBounds of each text at its start TextSize (the reference layout of the size estimate).
return function(Target)
	local Results = {}
	local Labels = {}
	local Face = Font.new("rbxasset://fonts/families/FredokaOne.json")
	local Holder = Instance.new("Frame")
	Holder.Size = UDim2.fromOffset(1000, 1000)
	Holder.BackgroundTransparency = 1
	Holder.Parent = Target
	-- { text, box height px, first width px, steps, TextSize }
	local Cases = {
		{ "Hi", 60.5, 330, 7000, 8 },
		{ "Cyan", 60.5, 225, 4700, 8 },
	}
	for Index, Case in Cases do
		for Step = 0, Case[4] - 1 do
			local Px = Case[3] + Step * 0.04
			local Label = Instance.new("TextLabel")
			Label.Size = UDim2.new(Px / 1.5 / 1000, 0, Case[2] / 1.5 / 1000, 0)
			Label.FontFace = Face
			Label.RichText = true
			Label.Text = Case[1]
			Label.TextSize = Case[5]
			Label.TextScaled = true
			Label.Parent = Holder
			table.insert(Labels, { Key = `{Index}_{Step}`, Label = Label })
		end
	end
	for _, Text in { "Hi", "Cyan", "iiiiii", "WWWW", "Secondary" } do
		for TextSize = 1, 40 do
			local Label = Instance.new("TextLabel")
			Label.Size = UDim2.fromOffset(900, 300)
			Label.FontFace = Face
			Label.RichText = true
			Label.Text = Text
			Label.TextSize = TextSize
			Label.Parent = Holder
			table.insert(Labels, { Key = `ref_{Text}_{TextSize}`, Label = Label })
		end
	end
	task.wait()
	for _, Entry in Labels do
		Results[Entry.Key] = { Entry.Label.TextBounds.X * 1.5, Entry.Label.TextBounds.Y * 1.5, Entry.Label.AbsoluteSize.X * 1.5, Entry.Label.AbsoluteSize.Y * 1.5 }
	end
	return Results
end
