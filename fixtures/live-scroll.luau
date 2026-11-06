-- Live preview cost check: 300 rows (frame + corner + stroke + label + icon) in one ScrollingFrame; scrolling
-- re-exports only the rows the clip shows (the retained export, native/src/engine/export/retained.rs)
return function(Target)
	local S = Instance.new("ScrollingFrame")
	S.Name = "List"
	S.Position = UDim2.fromOffset(50, 50)
	S.Size = UDim2.fromOffset(500, 700)
	S.CanvasSize = UDim2.fromOffset(500, 300 * 60)
	S.Parent = Target
	for Index = 0, 299 do
		local Row = Instance.new("Frame")
		Row.Name = "Row" .. Index
		Row.Position = UDim2.fromOffset(10, Index * 60)
		Row.Size = UDim2.new(1, -30, 0, 52)
		Row.BackgroundColor3 = Color3.fromHSV((Index % 12) / 12, 0.5, 0.6)
		Row.Parent = S
		Instance.new("UICorner").Parent = Row
		local Stroke = Instance.new("UIStroke")
		Stroke.Thickness = 2
		Stroke.Parent = Row
		local Label = Instance.new("TextLabel")
		Label.Name = "Label"
		Label.BackgroundTransparency = 1
		Label.Position = UDim2.fromOffset(60, 0)
		Label.Size = UDim2.new(1, -70, 1, 0)
		Label.Text = "Row number " .. Index
		Label.TextSize = 22
		Label.TextXAlignment = Enum.TextXAlignment.Left
		Label.Parent = Row
		local Icon = Instance.new("Frame")
		Icon.Name = "Icon"
		Icon.Position = UDim2.fromOffset(8, 8)
		Icon.Size = UDim2.fromOffset(36, 36)
		Icon.BackgroundColor3 = Color3.new(1, 1, 1)
		Icon.Parent = Row
		Instance.new("UICorner").Parent = Icon
	end
end
