-- An item view's frame (the shape behind two item view stories): a (0.999, 0, 0, 800) AutomaticSize Y frame of ratio
-- 2.1362 centred in a 1200 x 900 cell, with a UIScale, holding a 500 x 292 AutomaticSize Y holder at a scale position (its
-- (1, 0) AutomaticSize Y panel holding a 400 x 150 box) and a scale-sized notice label.
--   node cli/rur.mjs compare --fixture fixtures/layout-autosize-aspect-view.luau --viewport 1200x900 --time 0.2 --background 20,20,26
return function(Target)
	local Cell = Instance.new("Frame")
	Cell.Name = "Cell"
	Cell.BackgroundTransparency = 1
	Cell.Size = UDim2.fromOffset(1200, 900)
	Cell.Parent = Target
	local View = Instance.new("Frame")
	View.Name = "ViewFrame"
	View.AnchorPoint = Vector2.new(0.5, 0.5)
	View.AutomaticSize = Enum.AutomaticSize.Y
	View.BackgroundColor3 = Color3.fromRGB(200, 40, 50)
	View.BackgroundTransparency = 0.6
	View.Position = UDim2.fromScale(0.5, 0.5)
	View.Size = UDim2.new(0.999, 0, 0, 800)
	View.Parent = Cell
	local Aspect = Instance.new("UIAspectRatioConstraint")
	Aspect.AspectRatio = 2.1362
	Aspect.Parent = View
	Instance.new("UIScale").Parent = View

	local Holder = Instance.new("Frame")
	Holder.Name = "Holder"
	Holder.AnchorPoint = Vector2.new(0.5, 0.5)
	Holder.AutomaticSize = Enum.AutomaticSize.Y
	Holder.BackgroundColor3 = Color3.fromRGB(40, 90, 200)
	Holder.BackgroundTransparency = 0.4
	Holder.Position = UDim2.fromScale(0.585, 0.37)
	Holder.Size = UDim2.fromOffset(500, 292)
	Holder.Parent = View
	local Main = Instance.new("Frame")
	Main.Name = "Main"
	Main.AutomaticSize = Enum.AutomaticSize.Y
	Main.BackgroundColor3 = Color3.fromRGB(30, 30, 38)
	Main.Size = UDim2.fromScale(1, 0)
	Main.Parent = Holder
	local Box = Instance.new("Frame")
	Box.Name = "Box"
	Box.BackgroundColor3 = Color3.fromRGB(40, 160, 60)
	Box.Size = UDim2.fromOffset(400, 150)
	Box.Parent = Main

	local Notice = Instance.new("TextLabel")
	Notice.Name = "Notice"
	Notice.AnchorPoint = Vector2.new(0.5, 0.5)
	Notice.BackgroundColor3 = Color3.fromRGB(230, 200, 40)
	Notice.BackgroundTransparency = 0.5
	Notice.FontFace = Font.fromEnum(Enum.Font.FredokaOne)
	Notice.Position = UDim2.fromScale(0.5, 0.8396)
	Notice.Size = UDim2.fromScale(0.2257, 0.0301)
	Notice.Text = "Press to view"
	Notice.TextColor3 = Color3.new(1, 1, 1)
	Notice.TextScaled = true
	Notice.Parent = View
end
