-- AutomaticSize X TextScaled labels as list items (a mail icon: a 100 x 33 frame with a centred
-- horizontal UIListLayout (padding 5), a 30 x 30 icon and a FredokaOne "Mail" label (AutomaticSize X,
-- Size (0, 0, 1, 0), TextScaled, UITextSizeConstraint 12-30, UIStroke 2): Studio 43 px text, 50 wide;
-- the same label outside a list is 45 px, layout-probe-scaledstroke). Variants: no icon, no gap,
-- wider frame, left alignment, no constraint, another text, vertical list.
return function(Target)
	local Results = {}
	local Nodes = {}
	local Count = 0
	local Face = Font.new("rbxasset://fonts/families/FredokaOne.json")
	local function Case(Name, Options)
		Count += 1
		local Holder = Instance.new("Frame")
		Holder.BackgroundTransparency = 1
		Holder.Position = UDim2.fromOffset(((Count - 1) % 4) * 300 + 10, ((Count - 1) // 4) * 80 + 10)
		Holder.Size = UDim2.fromOffset(Options.Width or 100, Options.Height or 33)
		Holder.Parent = Target
		if Options.List ~= false then
			local List = Instance.new("UIListLayout")
			List.FillDirection = if Options.Vertical then Enum.FillDirection.Vertical else Enum.FillDirection.Horizontal
			List.HorizontalAlignment = Options.Align or Enum.HorizontalAlignment.Center
			List.Padding = UDim.new(0, Options.Gap or 5)
			List.SortOrder = Enum.SortOrder.LayoutOrder
			List.VerticalAlignment = Enum.VerticalAlignment.Center
			List.Parent = Holder
		end
		if Options.Icon ~= false then
			local Icon = Instance.new("ImageLabel")
			Icon.AnchorPoint = Vector2.new(0.5, 0.5)
			Icon.LayoutOrder = -1
			Icon.Position = UDim2.fromScale(0.5, 0.5)
			Icon.Size = UDim2.fromOffset(30, 30)
			Icon.Parent = Holder
			Nodes[`{Name}_icon`] = Icon
		end
		local Label = Instance.new("TextLabel")
		Label.AnchorPoint = Vector2.new(0.5, 0.5)
		Label.AutomaticSize = Enum.AutomaticSize.X
		Label.BackgroundTransparency = 1
		Label.FontFace = Face
		Label.Position = UDim2.fromScale(0.5, 0.8)
		Label.Size = UDim2.new(0, 0, 1, 0)
		Label.Text = Options.Text or "Mail"
		Label.TextScaled = true
		Label.TextSize = 30
		Label.TextWrapped = true
		Label.TextXAlignment = Enum.TextXAlignment.Left
		Label.Parent = Holder
		if Options.Constraint ~= false then
			local Constraint = Instance.new("UITextSizeConstraint")
			Constraint.MaxTextSize = 30
			Constraint.MinTextSize = 12
			Constraint.Parent = Label
		end
		local Stroke = Instance.new("UIStroke")
		Stroke.Thickness = 2
		Stroke.Parent = Label
		Nodes[`{Name}_label`] = Label
	end
	Case("story", {})
	Case("noIcon", { Icon = false })
	Case("noGap", { Gap = 0 })
	Case("wide", { Width = 200 })
	Case("left", { Align = Enum.HorizontalAlignment.Left })
	Case("free", { List = false })
	Case("freeIcon", { List = false, Icon = true })
	Case("noConstraint", { Constraint = false })
	Case("longText", { Text = "Mailbox" })
	Case("vertical", { Vertical = true })
	Case("tall", { Height = 50 })
	Case("narrow", { Width = 60 })
	task.wait()
	for Key, Node in Nodes do
		Results[Key] = { Node.AbsolutePosition.X - Node.Parent.AbsolutePosition.X, Node.AbsoluteSize.X, Node.AbsoluteSize.Y }
		if Node:IsA("TextLabel") then
			table.insert(Results[Key], Node.TextBounds.X)
			table.insert(Results[Key], Node.TextBounds.Y)
		end
	end
	return Results
end
