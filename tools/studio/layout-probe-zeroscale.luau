-- Text and sizes under UIScale 0 (a UI that animates cards in from UIScale 0; two stories like it
-- hung the text layout on NaN boxes at scale 0). Labels of
-- every text mode (plain, rich, TextScaled plain / rich, wrapped, constrained, AutomaticSize) inside
-- a frame with UIScale 0, and the same labels at UIScale 1e-3 and 1 for reference.
return function(Target)
	local Results = {}
	local Labels = {}
	local Face = Font.new("rbxasset://fonts/families/GothamSSm.json", Enum.FontWeight.Bold)
	for ScaleIndex, Scale in { 0, 0.001, 1 } do
		local Holder = Instance.new("Frame")
		Holder.Name = `S{ScaleIndex}`
		Holder.BackgroundTransparency = 1
		Holder.Position = UDim2.fromOffset(20, 20 + (ScaleIndex - 1) * 300)
		Holder.Size = UDim2.fromOffset(240, 120)
		Holder.Parent = Target
		local UIScale = Instance.new("UIScale")
		UIScale.Scale = Scale
		UIScale.Parent = Holder
		local List = Instance.new("Frame")
		List.Name = "Auto"
		List.BackgroundTransparency = 1
		List.AutomaticSize = Enum.AutomaticSize.XY
		List.Size = UDim2.fromOffset(10, 10)
		List.Parent = Holder
		local Layout = Instance.new("UIListLayout")
		Layout.Parent = List
		Results[`{ScaleIndex}_holder`] = Holder
		Results[`{ScaleIndex}_list`] = List
		for Index, Spec in {
			{ "plain", false, false, false, false, Enum.AutomaticSize.None },
			{ "rich", true, false, false, false, Enum.AutomaticSize.None },
			{ "scaled", false, true, false, false, Enum.AutomaticSize.None },
			{ "richscaled", true, true, false, false, Enum.AutomaticSize.None },
			{ "richscaledc", true, true, false, true, Enum.AutomaticSize.None },
			{ "scaledc", false, true, false, true, Enum.AutomaticSize.None },
			{ "wrapped", false, false, true, false, Enum.AutomaticSize.None },
			{ "richwrapped", true, false, true, false, Enum.AutomaticSize.None },
			{ "autox", false, false, false, false, Enum.AutomaticSize.X },
			{ "autoxy", true, false, false, false, Enum.AutomaticSize.XY },
			{ "autoy", false, false, true, false, Enum.AutomaticSize.Y },
			{ "scaledscale", false, true, false, false, "scale" },
			{ "richscaledscale", true, true, false, false, "scale" },
		} do
			for _, Parent in { Holder, List } do
				local L = Instance.new("TextButton")
				L.Name = Spec[1]
				L.BackgroundTransparency = 1
				L.FontFace = Face
				L.TextSize = 20
				L.RichText = Spec[2]
				L.TextScaled = Spec[3]
				L.TextWrapped = Spec[4]
				if Spec[5] then
					local C = Instance.new("UITextSizeConstraint")
					C.MinTextSize = 8
					C.MaxTextSize = 30
					C.Parent = L
				end
				if Spec[6] == "scale" then
					L.Size = UDim2.fromScale(0.5, 0.3)
				else
					L.AutomaticSize = Spec[6]
					L.Size = UDim2.fromOffset(if Spec[6] == Enum.AutomaticSize.None then 120 else 0, 30)
				end
				L.Position = UDim2.fromOffset(0, Index * 8)
				L.Text = if Spec[2] then "Claim <b>reward</b> now" else "Claim reward now"
				L.Parent = Parent
				table.insert(Labels, { Key = `{ScaleIndex}_{if Parent == List then "l" else "h"}_{Spec[1]}`, Label = L })
			end
		end
	end
	task.wait()
	for Key, Frame in Results do
		Results[Key] = { Frame.AbsolutePosition.X, Frame.AbsolutePosition.Y, Frame.AbsoluteSize.X, Frame.AbsoluteSize.Y }
	end
	for _, Entry in Labels do
		local L = Entry.Label
		Results[Entry.Key] = { L.AbsolutePosition.X, L.AbsolutePosition.Y, L.AbsoluteSize.X, L.AbsoluteSize.Y, L.TextBounds.X, L.TextBounds.Y, L.TextFits }
	end
	return Results
end
