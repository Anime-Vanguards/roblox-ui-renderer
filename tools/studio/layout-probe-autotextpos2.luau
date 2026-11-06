-- Bisecting layout-probe-invisible's label offset: there an AutoXY TextLabel in a frame with 8 + 8
-- padding (AutomaticSize None or Y) sits at parent x - 8 in Studio, while layout-probe-autotextpos
-- (same label, padding, text) puts it at parent x + 8. One difference per case.
return function(Target)
	local Results = {}
	local Nodes = {}
	local Count = 0
	local function Make(Name, Options)
		Count += 1
		local H = Instance.new("Frame")
		H.BackgroundTransparency = 1
		H.Position = UDim2.fromOffset(((Count - 1) % 4) * 300, ((Count - 1) // 4) * 120)
		H.Size = UDim2.fromOffset(280, 110)
		H.Parent = Target
		local Object = Instance.new(Options.Class or "Frame")
		if Options.SetAutomatic ~= false then
			Object.AutomaticSize = Enum.AutomaticSize[Options.Automatic or "None"]
		end
		Object.Visible = true
		Object.Position = UDim2.fromOffset(10, 10)
		if Options.Size then
			Object.Size = Options.Size
		end
		if Object:IsA("TextButton") then
			Object.Text = ""
		end
		local Pad = Instance.new("UIPadding")
		Pad.PaddingLeft = UDim.new(0, Options.Left or 8)
		Pad.PaddingRight = UDim.new(0, Options.Right or 8)
		Pad.Parent = Object
		local Label = Instance.new("TextLabel")
		Label.AutomaticSize = Enum.AutomaticSize[Options.LabelAutomatic or "XY"]
		Label.Size = Options.LabelSize or UDim2.new()
		Label.Text = Options.Text or "Traits Enabled"
		Label.TextSize = 16
		Label.Parent = Object
		Object.Parent = if Options.ParentFirst then H else H
		Nodes[`{Name}_object`] = Object
		Nodes[`{Name}_label`] = Label
	end
	Make("replica", {})
	Make("explicitSize", { Size = UDim2.fromOffset(100, 100) })
	Make("noAutomaticSet", { SetAutomatic = false })
	Make("short", { Size = UDim2.fromOffset(100, 20) })
	Make("textAb", { Text = "ab" })
	Make("autoY", { Automatic = "Y" })
	Make("autoX", { Automatic = "X" })
	Make("labelAutoX", { LabelAutomatic = "X", LabelSize = UDim2.fromOffset(0, 20) })
	Make("labelAutoY", { LabelAutomatic = "Y", LabelSize = UDim2.fromOffset(50, 0) })
	Make("labelNone", { LabelAutomatic = "None", LabelSize = UDim2.fromOffset(50, 20) })
	Make("padLeftOnly", { Right = 0 })
	Make("padRightOnly", { Left = 0 })
	Make("padOther", { Left = 20, Right = 5 })
	Make("button", { Class = "TextButton" })
	Make("image", { Class = "ImageLabel" })
	Make("wide", { Size = UDim2.fromOffset(300, 100) })
	task.wait()
	for Key, Node in Nodes do
		Results[Key] = { Node.AbsolutePosition.X - Node.Parent.AbsolutePosition.X, Node.AbsolutePosition.Y - Node.Parent.AbsolutePosition.Y, Node.AbsoluteSize.X, Node.AbsoluteSize.Y }
	end
	return Results
end
