-- Contextual UIStroke on text under a UIScale (a UI kit's pill labels: Thickness 2.5, UIScale 0.885):
-- does the stroke radius follow the UIScale? Left column scaled, right column unscaled reference.
--   node cli/rur.mjs compare --fixture fixtures/stroke-text-uiscale.luau --viewport 800x480 --time 0.2 --background 60,110,200
return function(Target)
	local function Column(Name, X, Scale)
		local Holder = Instance.new("Frame")
		Holder.Name = Name
		Holder.BackgroundTransparency = 1
		Holder.Position = UDim2.fromOffset(X, 20)
		Holder.Size = UDim2.fromOffset(360, 440)
		Holder.Parent = Target
		if Scale then
			local UIScale = Instance.new("UIScale")
			UIScale.Scale = Scale
			UIScale.Parent = Holder
		end
		for Index, Thickness in { 1, 2.5, 4 } do
			local Label = Instance.new("TextLabel")
			Label.Name = "Label" .. Index
			Label.BackgroundTransparency = 1
			Label.Position = UDim2.fromOffset(10, 10 + (Index - 1) * 130)
			Label.Size = UDim2.fromOffset(340, 110)
			Label.Font = Enum.Font.FredokaOne
			Label.Text = "Chat Mystery"
			Label.TextSize = 40
			Label.TextColor3 = Color3.new(1, 1, 1)
			Label.Parent = Holder
			local Stroke = Instance.new("UIStroke")
			Stroke.Thickness = Thickness
			Stroke.Color = Color3.new(0, 0, 0)
			Stroke.Parent = Label
		end
	end
	Column("Scaled", 10, 0.885)
	Column("Plain", 420, nil)
end
