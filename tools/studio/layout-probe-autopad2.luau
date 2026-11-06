-- AutomaticSize text objects resolve the automatic axes' scale Size against the parent's whole size
-- (not its padded content box, layout-probe-autopad). Which classes, and which parent size: with a
-- border inset, a UIScale, a ScrollingFrame parent, and an image/frame for comparison.
return function(Target)
	local Results = {}
	local Items = {}
	local Face = Font.new("rbxasset://fonts/families/FredokaOne.json")
	local function Parent(Index, Class, Setup)
		local Object = Instance.new(Class)
		Object.Position = UDim2.fromOffset(20 + ((Index - 1) % 3) * 380, 20 + ((Index - 1) // 3) * 210)
		Object.Size = UDim2.fromOffset(360, 190)
		Object.BackgroundTransparency = 1
		Object.Parent = Target
		local Padding = Instance.new("UIPadding")
		Padding.PaddingLeft = UDim.new(0, 20)
		Padding.PaddingRight = UDim.new(0, 30)
		Padding.PaddingTop = UDim.new(0, 10)
		Padding.PaddingBottom = UDim.new(0, 14)
		Padding.Parent = Object
		if Setup then
			Setup(Object)
		end
		return Object
	end
	local Parents = {
		{ "plain", "Frame", nil },
		{ "border", "Frame", function(Object)
			Object.BorderSizePixel = 6
			Object.BorderMode = Enum.BorderMode.Inset
		end },
		{ "uiscale", "Frame", function(Object)
			local Scale = Instance.new("UIScale")
			Scale.Scale = 0.8
			Scale.Parent = Object
		end },
		{ "scroll", "ScrollingFrame", function(Object)
			Object.CanvasSize = UDim2.fromOffset(500, 400)
		end },
		{ "nopad", "Frame", function(Object)
			Object:FindFirstChildOfClass("UIPadding"):Destroy()
		end },
	}
	for Index, Spec in Parents do
		local Holder = Parent(Index, Spec[2], Spec[3])
		for _, Class in { "TextLabel", "TextButton", "TextBox", "ImageLabel", "Frame" } do
			for _, Auto in { "X", "Y", "XY" } do
				local Object = Instance.new(Class)
				Object.AutomaticSize = Enum.AutomaticSize[Auto]
				Object.Position = UDim2.new(0.1, 0, 0.1, 0)
				Object.Size = UDim2.new(0.2, 3, 0.15, 2)
				Object.BackgroundTransparency = 1
				if Class == "TextLabel" or Class == "TextButton" or Class == "TextBox" then
					Object.FontFace = Face
					Object.TextSize = 14
					Object.Text = "ab"
				else
					local Child = Instance.new("Frame")
					Child.Size = UDim2.fromOffset(5, 5)
					Child.Parent = Object
				end
				Object.Parent = Holder
				table.insert(Items, { Key = `{Spec[1]}_{Class}_{Auto}`, Object = Object })
			end
		end
	end
	task.wait()
	for _, Entry in Items do
		local Object = Entry.Object
		Results[Entry.Key] = { Object.AbsolutePosition.X, Object.AbsolutePosition.Y, Object.AbsoluteSize.X, Object.AbsoluteSize.Y }
	end
	return Results
end
