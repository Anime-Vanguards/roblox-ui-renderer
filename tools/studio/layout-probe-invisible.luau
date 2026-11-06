-- AbsoluteSize / AbsolutePosition of invisible objects (a mode window's chip: an AutoXY TextButton
-- created with Visible = false in a wrapping list reads 0 x 0 in Studio, its automatic size offline).
-- Plain and AutomaticSize frames / text buttons, in a list and free, created invisible, hidden after a
-- frame, their children, and objects under an invisible parent.
return function(Target)
	local Results = {}
	local Nodes = {}
	local Late = {}
	local Count = 0
	local function Holder(List)
		Count += 1
		local H = Instance.new("Frame")
		H.BackgroundTransparency = 1
		H.Position = UDim2.fromOffset(((Count - 1) % 4) * 300, ((Count - 1) // 4) * 120)
		H.Size = UDim2.fromOffset(280, 110)
		H.Parent = Target
		if List then
			local Layout = Instance.new("UIListLayout")
			Layout.FillDirection = Enum.FillDirection.Horizontal
			Layout.Padding = UDim.new(0, 6)
			Layout.Parent = H
		end
		return H
	end
	local function Make(Parent, Key, Class, Automatic, Visible)
		local Object = Instance.new(Class)
		Object.AutomaticSize = Enum.AutomaticSize[Automatic]
		Object.Visible = Visible
		Object.Position = UDim2.fromOffset(10, 10)
		if Class == "TextButton" then
			Object.Text = ""
		end
		local Pad = Instance.new("UIPadding")
		Pad.PaddingLeft = UDim.new(0, 8)
		Pad.PaddingRight = UDim.new(0, 8)
		Pad.Parent = Object
		local Label = Instance.new("TextLabel")
		Label.AutomaticSize = Enum.AutomaticSize.XY
		Label.Size = UDim2.new()
		Label.Text = "Traits Enabled"
		Label.TextSize = 16
		Label.Parent = Object
		Object.Parent = Parent
		Nodes[Key] = Object
		Nodes[`{Key}_label`] = Label
		return Object
	end
	for _, List in { false, true } do
		for _, Class in { "Frame", "TextButton" } do
			for _, Automatic in { "None", "XY", "X", "Y" } do
				local Key = `{if List then "list" else "free"}_{Class}_{Automatic}`
				local H = Holder(List)
				local Before = Make(H, `{Key}_visible`, Class, Automatic, true)
				Before.LayoutOrder = 1
				local Hidden = Make(H, `{Key}_hidden`, Class, Automatic, false)
				Hidden.LayoutOrder = 2
				local After = Make(H, `{Key}_later`, Class, Automatic, true)
				After.LayoutOrder = 3
				table.insert(Late, After)
			end
		end
	end
	-- under an invisible parent
	local Parent = Holder(false)
	Parent.Visible = false
	Make(Parent, "underHidden_XY", "Frame", "XY", true)
	Make(Parent, "underHidden_None", "Frame", "None", true)

	task.wait()
	for _, Object in Late do
		Object.Visible = false
	end
	task.wait()
	for Key, Node in Nodes do
		Results[Key] = { Node.AbsolutePosition.X, Node.AbsolutePosition.Y, Node.AbsoluteSize.X, Node.AbsoluteSize.Y }
	end
	return Results
end
