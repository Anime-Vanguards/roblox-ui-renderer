-- A choice prompt's shape (the case behind several story goldens): an AutomaticSize Y holder of offset Size with a
-- UIAspectRatioConstraint, holding a (1, 0, 0.8862, 0) AutomaticSize Y panel (a centred vertical list, 15 px top and
-- bottom padding: a header, a wrapped description, a 200 x 65 button row) and a (1, 1) shadow frame holding a
-- (1.5, 1.5) image. Four cells: two holder sizes and aspect ratios, with and without the header. Two popup cells: the
-- same holder at ratio 3 around a list with a 0.1 scale gap and 15 px of padding on every side.
--   node cli/rur.mjs compare --fixture fixtures/layout-autosize-prompt.luau --viewport 1800x900 --time 0.2 --background 20,20,26
return function(Target)
	local Face = Font.fromEnum(Enum.Font.FredokaOne)
	local Count = 0
	local function Prompt(Name, HolderSize, Ratio, Header, Popup)
		local Cell = Instance.new("Frame")
		Cell.Name = Name
		Cell.Position = UDim2.fromOffset((Count % 3) * 600, (Count // 3) * 450)
		Cell.Size = UDim2.fromOffset(600, 450)
		Cell.BackgroundTransparency = 1
		Cell.Parent = Target
		Count += 1
		local Holder = Instance.new("Frame")
		Holder.Name = "Holder"
		Holder.AnchorPoint = Vector2.new(0.5, 0.5)
		Holder.AutomaticSize = Enum.AutomaticSize.Y
		Holder.BackgroundTransparency = 1
		Holder.Position = UDim2.fromScale(0.5, 0.5)
		Holder.Size = HolderSize
		Holder.Parent = Cell
		local Aspect = Instance.new("UIAspectRatioConstraint")
		Aspect.AspectRatio = Ratio
		Aspect.Parent = Holder

		local Main = Instance.new("Frame")
		Main.Name = "Main"
		Main.AutomaticSize = Enum.AutomaticSize.Y
		Main.BackgroundColor3 = Color3.fromRGB(3, 3, 3)
		Main.Position = UDim2.fromScale(0, -0)
		Main.Size = UDim2.fromScale(1, 0.8862)
		Main.Parent = Holder
		local List = Instance.new("UIListLayout")
		List.HorizontalAlignment = Enum.HorizontalAlignment.Center
		List.VerticalAlignment = Enum.VerticalAlignment.Center
		List.Padding = if Popup then UDim.new(0.1, 0) else UDim.new(0, 10)
		List.SortOrder = Enum.SortOrder.LayoutOrder
		List.Parent = Main
		local Padding = Instance.new("UIPadding")
		Padding.PaddingTop = UDim.new(0, 15)
		Padding.PaddingBottom = UDim.new(0, 15)
		if Popup then
			Padding.PaddingLeft = UDim.new(0, 15)
			Padding.PaddingRight = UDim.new(0, 15)
		end
		Padding.Parent = Main

		local Description = Instance.new("TextLabel")
		Description.Name = "Description"
		Description.AnchorPoint = Vector2.new(0.5, 0)
		Description.AutomaticSize = Enum.AutomaticSize.Y
		Description.BackgroundTransparency = 1
		Description.FontFace = Face
		Description.Position = UDim2.fromScale(0.5, if Popup then 0.2307 else 0.3422)
		Description.RichText = Popup == true
		Description.Size = UDim2.fromOffset(375, 0)
		Description.Text = if Popup then "This is a small preview of a popup made for testing." else "Would you like to start a quick round or open a new room?"
		Description.TextColor3 = Color3.new(1, 1, 1)
		Description.TextSize = 27
		Description.TextWrapped = true
		Description.TextYAlignment = Enum.TextYAlignment.Bottom
		Description.Parent = Main

		local Buttons = Instance.new("Frame")
		Buttons.Name = "Buttons"
		Buttons.AnchorPoint = if Popup then Vector2.zero else Vector2.new(0.5, 0.5)
		Buttons.BackgroundTransparency = 1
		Buttons.Position = if Popup then UDim2.fromScale(0.0973, 0.6213) else UDim2.fromScale(0.5, 0.85)
		Buttons.Size = if Popup then UDim2.fromOffset(354, 35) else UDim2.fromOffset(200, 65)
		Buttons.AutomaticSize = if Popup then Enum.AutomaticSize.Y else Enum.AutomaticSize.None
		Buttons.LayoutOrder = if Popup then 0 else 1
		Buttons.Parent = Main
		local Row = Instance.new("UIListLayout")
		Row.FillDirection = Enum.FillDirection.Horizontal
		Row.HorizontalAlignment = Enum.HorizontalAlignment.Center
		Row.Padding = if Popup then UDim.new(0.05, 0) else UDim.new(0, 10)
		Row.SortOrder = Enum.SortOrder.LayoutOrder
		Row.Parent = Buttons
		for Index = 1, if Popup then 1 else 2 do
			local Choice = Instance.new("Frame")
			Choice.Name = `Choice{Index}`
			Choice.Size = UDim2.fromOffset(200, 65)
			Choice.BackgroundColor3 = Color3.fromRGB(40, 160, 60)
			Choice.LayoutOrder = Index
			Choice.Parent = Buttons
			local Scale = Instance.new("UIScale")
			Scale.Scale = 0.885
			Scale.Parent = Choice
		end

		if Header then
			local AlertFrame = Instance.new("Frame")
			AlertFrame.Name = "AlertFrame"
			AlertFrame.BackgroundColor3 = Color3.fromRGB(20, 20, 32)
			AlertFrame.LayoutOrder = -1
			AlertFrame.Size = UDim2.new(if Popup then 1 else 0.95, 0, 0, 40)
			AlertFrame.Parent = Main
			local Label = Instance.new("TextLabel")
			Label.Name = "Label"
			Label.AnchorPoint = Vector2.new(0.5, 0.5)
			Label.AutomaticSize = Enum.AutomaticSize.Y
			Label.BackgroundTransparency = 1
			Label.FontFace = Face
			Label.Position = UDim2.fromScale(0.5, 0.5)
			Label.RichText = true
			Label.Size = UDim2.fromOffset(375, 0)
			Label.Text = if Popup then "ALERT" else "OPTIONS"
			Label.TextColor3 = Color3.new(1, 1, 1)
			Label.TextSize = 26
			Label.TextWrapped = true
			Label.TextYAlignment = Enum.TextYAlignment.Bottom
			Label.Parent = AlertFrame
			for Order, Height in if Popup then {} else { [-1] = 16, [1] = 20 } do
				local Underlay = Instance.new("Frame")
				Underlay.Name = `Underlay{Height}`
				Underlay.AnchorPoint = Vector2.new(0, 1)
				Underlay.BackgroundColor3 = Color3.fromRGB(20, 20, 32)
				Underlay.LayoutOrder = Order
				Underlay.Position = UDim2.fromScale(0, 1)
				Underlay.Size = UDim2.new(1, 0, 0, Height)
				Underlay.Parent = AlertFrame
			end
		end

		local Shadow = Instance.new("Frame")
		Shadow.Name = "Shadow"
		Shadow.BackgroundTransparency = 1
		Shadow.Size = UDim2.fromScale(1, 1)
		Shadow.Parent = Holder
		Instance.new("UIScale").Parent = Shadow
		local Image = Instance.new("Frame")
		Image.Name = "ShadowImage"
		Image.AnchorPoint = Vector2.new(0.5, 0.5)
		Image.BackgroundColor3 = Color3.fromRGB(0, 0, 0)
		Image.BackgroundTransparency = 0.7
		Image.Position = UDim2.fromScale(0.5, 0.5)
		Image.Size = UDim2.fromScale(1.5, 1.5)
		Image.Parent = Shadow
	end
	Prompt("header_500", UDim2.fromOffset(500, 158), 2.87, true)
	Prompt("plain_500", UDim2.fromOffset(500, 158), 2.87, false)
	Prompt("plain_413", UDim2.fromOffset(413, 158), 2.7352, false)
	Prompt("header_413", UDim2.fromOffset(413, 158), 2.7352, true)
	Prompt("popup_alert", UDim2.fromOffset(500, 158), 3, true, true)
	Prompt("popup_plain", UDim2.fromOffset(500, 158), 3, false, true)
end
