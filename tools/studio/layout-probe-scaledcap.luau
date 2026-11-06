-- TextScaled + UITextSizeConstraint under UIScale, plain and rich text: is MaxTextSize / MinTextSize
-- scaled by the UIScale? (a buff list: the button caps rich text at TextSize x ancestor scale.)
return function(Target)
	local Results = {}
	local Labels = {}
	local Face = Font.new("rbxasset://fonts/families/FredokaOne.json")
	for _, Scale in { 1, 0.885, 1.3 } do
		local Holder = Instance.new("Frame")
		Holder.Size = UDim2.fromOffset(1000, 1000)
		Holder.BackgroundTransparency = 1
		Holder.Parent = Target
		if Scale ~= 1 then
			local UIScale = Instance.new("UIScale")
			UIScale.Scale = Scale
			UIScale.Parent = Holder
		end
		for _, Rich in { false, true } do
			for _, Max in { 12, 20, 24.78, 30.5, 45 } do
				local Label = Instance.new("TextLabel")
				Label.Size = UDim2.fromOffset(400, 200)
				Label.FontFace = Face
				Label.RichText = Rich
				Label.Text = "Cancel"
				Label.TextScaled = true
				Label.Parent = Holder
				local Constraint = Instance.new("UITextSizeConstraint")
				Constraint.MaxTextSize = Max
				Constraint.Parent = Label
				table.insert(Labels, { Key = `max{Scale}_{Rich}_{Max}`, Label = Label })
			end
			for _, Min in { 10, 30, 50 } do
				local Label = Instance.new("TextLabel")
				Label.Size = UDim2.fromOffset(60, 12)
				Label.FontFace = Face
				Label.RichText = Rich
				Label.Text = "Cancel"
				Label.TextScaled = true
				Label.Parent = Holder
				local Constraint = Instance.new("UITextSizeConstraint")
				Constraint.MinTextSize = Min
				Constraint.Parent = Label
				table.insert(Labels, { Key = `min{Scale}_{Rich}_{Min}`, Label = Label })
			end
			-- constraint without TextScaled
			local Label = Instance.new("TextLabel")
			Label.Size = UDim2.fromOffset(400, 200)
			Label.FontFace = Face
			Label.RichText = Rich
			Label.Text = "Cancel"
			Label.TextSize = 40
			Label.Parent = Holder
			local Constraint = Instance.new("UITextSizeConstraint")
			Constraint.MaxTextSize = 24.78
			Constraint.Parent = Label
			table.insert(Labels, { Key = `fixed{Scale}_{Rich}`, Label = Label })
		end
	end
	task.wait()
	for _, Entry in Labels do
		Results[Entry.Key] = { Entry.Label.TextBounds.X * 1.5, Entry.Label.TextBounds.Y * 1.5 }
	end
	return Results
end
