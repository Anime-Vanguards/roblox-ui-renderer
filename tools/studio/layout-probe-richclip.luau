-- Rich vs plain wrapped text in boxes shorter than the text: does TextBounds.Y count the lines cut
-- off by the box height (layout-probe-scaledcap: rich TextScaled text at MinTextSize reports every
-- line), and with TextTruncate?
return function(Target)
	local Results = {}
	local Labels = {}
	local Face = Font.new("rbxasset://fonts/families/SourceSansPro.json")
	local Count = 0
	for _, Rich in { false, true } do
		for _, Height in { 10, 20, 30, 45, 70 } do
			for _, Truncate in { "None", "AtEnd" } do
				Count += 1
				local Label = Instance.new("TextLabel")
				Label.Position = UDim2.fromOffset(((Count - 1) % 10) * 110, ((Count - 1) // 10) * 90)
				Label.Size = UDim2.fromOffset(100, Height)
				Label.FontFace = Face
				Label.TextSize = 16
				Label.RichText = Rich
				Label.TextWrapped = true
				Label.TextTruncate = Enum.TextTruncate[Truncate]
				Label.Text = "one two three four five six seven eight"
				Label.Parent = Target
				table.insert(Labels, { Key = `{if Rich then "rich" else "plain"}_h{Height}_{Truncate}`, Label = Label })
			end
		end
		-- a word wider than the box
		Count += 1
		local Label = Instance.new("TextLabel")
		Label.Position = UDim2.fromOffset(((Count - 1) % 10) * 110, ((Count - 1) // 10) * 90)
		Label.Size = UDim2.fromOffset(30, 20)
		Label.FontFace = Face
		Label.TextSize = 16
		Label.RichText = Rich
		Label.TextWrapped = true
		Label.Text = "extraordinary"
		Label.Parent = Target
		table.insert(Labels, { Key = `{if Rich then "rich" else "plain"}_longword`, Label = Label })
	end
	task.wait()
	for _, Entry in Labels do
		Results[Entry.Key] = { Entry.Label.TextBounds.X * 1.5, Entry.Label.TextBounds.Y * 1.5, if Entry.Label.TextFits then 1 else 0 }
	end
	return Results
end
