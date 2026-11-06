-- TextScaled with a UIStroke (a mail icon's label: FredokaOne "Mail", TextScaled, AutomaticSize X,
-- Size (0, 0, 1, 0) in a 33 px row, UITextSizeConstraint 12-30, UIStroke 2: Studio picks 43 px, the
-- offline layout 45). Stroke thickness 0 / 1 / 2 / 4 (and a Border-mode stroke), AutoX or fixed
-- width, with or without the constraint, rich and plain, box heights 33 / 50.
return function(Target)
	local Results = {}
	local Nodes = {}
	local Count = 0
	local Face = Font.new("rbxasset://fonts/families/FredokaOne.json")
	for _, Height in { 33, 50 } do
		for _, Stroke in { 0, 1, 2, 4, "border" } do
			for _, Auto in { true, false } do
				for _, Capped in { true, false } do
					for _, Rich in { false, true } do
						Count += 1
						local Row = Instance.new("Frame")
						Row.BackgroundTransparency = 1
						Row.Position = UDim2.fromOffset(((Count - 1) % 8) * 150, ((Count - 1) // 8) * 60)
						Row.Size = UDim2.fromOffset(140, Height)
						Row.Parent = Target
						local Label = Instance.new("TextLabel")
						Label.AutomaticSize = if Auto then Enum.AutomaticSize.X else Enum.AutomaticSize.None
						Label.Size = if Auto then UDim2.new(0, 0, 1, 0) else UDim2.new(0, 120, 1, 0)
						Label.FontFace = Face
						Label.RichText = Rich
						Label.TextScaled = true
						Label.TextSize = 30
						Label.TextWrapped = true
						Label.TextXAlignment = Enum.TextXAlignment.Left
						Label.Text = "Mail"
						Label.Parent = Row
						if Capped then
							local Constraint = Instance.new("UITextSizeConstraint")
							Constraint.MaxTextSize = 30
							Constraint.MinTextSize = 12
							Constraint.Parent = Label
						end
						if Stroke == "border" then
							local S = Instance.new("UIStroke")
							S.ApplyStrokeMode = Enum.ApplyStrokeMode.Border
							S.Thickness = 2
							S.Parent = Label
						elseif Stroke > 0 then
							local S = Instance.new("UIStroke")
							S.Thickness = Stroke
							S.Parent = Label
						end
						Nodes[`{Height}_{Stroke}_{Auto}_{Capped}_{Rich}`] = Label
					end
				end
			end
		end
	end
	task.wait()
	for Key, Node in Nodes do
		Results[Key] = { Node.AbsoluteSize.X, Node.AbsoluteSize.Y, Node.TextBounds.X, Node.TextBounds.Y }
	end
	return Results
end
