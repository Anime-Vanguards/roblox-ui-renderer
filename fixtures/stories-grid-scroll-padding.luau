-- Repro: UIGridLayout with scale CellPadding / CellSize inside a ScrollingFrame with
-- AutomaticCanvasSize, the structure of an emote list
-- (a grid of cards in a window story).
-- In Studio the second row starts CellPadding.Y.Scale * the frame's window height below the first
-- (0.03 * 201.6 = 6.05 px); the offline layout used a larger basis (15.7 px).
return function(Target)
	local Holder = Instance.new("Frame")
	Holder.Name = "Holder"
	Holder.Position = UDim2.fromOffset(40, 40)
	Holder.Size = UDim2.fromOffset(440, 266)
	Holder.BackgroundColor3 = Color3.fromRGB(40, 40, 40)
	Holder.BorderSizePixel = 0
	Holder.Parent = Target

	local List = Instance.new("ScrollingFrame")
	List.Name = "List"
	List.AutomaticCanvasSize = Enum.AutomaticSize.Y
	List.BackgroundTransparency = 1
	List.Position = UDim2.fromScale(0, 0.222)
	List.ScrollBarImageTransparency = 1
	List.ScrollBarThickness = 0
	List.ScrollingDirection = Enum.ScrollingDirection.Y
	List.Size = UDim2.fromScale(0.6049, 0.7587)
	List.Parent = Holder

	local Padding = Instance.new("UIPadding")
	Padding.PaddingLeft = UDim.new(0.022, 0)
	Padding.PaddingTop = UDim.new(0.017, 0)
	Padding.Parent = List

	local Grid = Instance.new("UIGridLayout")
	Grid.CellPadding = UDim2.fromScale(0.045, 0.03)
	Grid.CellSize = UDim2.fromScale(0.21, 0.5)
	Grid.SortOrder = Enum.SortOrder.LayoutOrder
	Grid.Parent = List

	local Aspect = Instance.new("UIAspectRatioConstraint")
	Aspect.Parent = Grid

	for Index = 1, 6 do
		local Cell = Instance.new("Frame")
		Cell.Name = `Cell{Index}`
		Cell.LayoutOrder = Index
		Cell.BackgroundColor3 = Color3.fromRGB(45 + Index * 20, 45, 55)
		Cell.BorderSizePixel = 0
		Cell.Parent = List
	end
end
