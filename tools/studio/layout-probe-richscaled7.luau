-- Rich TextScaled (round 4): fine box-width sweeps (0.04 px) at other box heights (30.5 and 90.5 px),
-- to tell a relative error in the size estimate from a fixed tolerance in the size test.
return function(Target)
	local Results = {}
	local Labels = {}
	local Face = Font.new("rbxasset://fonts/families/FredokaOne.json")
	local Holder = Instance.new("Frame")
	Holder.Size = UDim2.fromOffset(1000, 1000)
	Holder.BackgroundTransparency = 1
	Holder.Parent = Target
	-- { text, box height px, first width px, steps }
	local Cases = {
		{ "Hi", 30.5, 170, 4500 },
		{ "Hi", 90.5, 198, 5600 },
	}
	for Index, Case in Cases do
		for Step = 0, Case[4] - 1 do
			local Px = Case[3] + Step * 0.04
			local Label = Instance.new("TextLabel")
			Label.Size = UDim2.new(Px / 1.5 / 1000, 0, Case[2] / 1.5 / 1000, 0)
			Label.FontFace = Face
			Label.RichText = true
			Label.Text = Case[1]
			Label.TextScaled = true
			Label.Parent = Holder
			table.insert(Labels, { Key = `{Index}_{Step}`, Label = Label })
		end
	end
	task.wait()
	for _, Entry in Labels do
		Results[Entry.Key] = { Entry.Label.TextBounds.X * 1.5, Entry.Label.TextBounds.Y * 1.5, Entry.Label.AbsoluteSize.X * 1.5, Entry.Label.AbsoluteSize.Y * 1.5 }
	end
	return Results
end
