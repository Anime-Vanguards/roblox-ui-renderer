-- A UI Labs generic story: `render` builds the UI once, then follows control changes through
-- `props.subscribe`. `rur render --controls '{"Label": "Sale", "Style.Wide": true}'` and the desktop
-- storybook change them.
return {
	summary = "A badge that follows its controls",
	controls = {
		Label = "New",
		Style = {
			EntryType = "ControlGroup",
			Controls = {
				Wide = false,
			},
		},
	},
	render = function(Props)
		local Badge = Instance.new("TextLabel")
		Badge.Name = "Badge"
		Badge.AnchorPoint = Vector2.new(0.5, 0.5)
		Badge.Position = UDim2.fromScale(0.5, 0.5)
		Badge.BackgroundColor3 = Color3.fromRGB(220, 72, 96)
		Badge.FontFace = Font.new("rbxasset://fonts/families/SourceSansPro.json", Enum.FontWeight.Bold)
		Badge.TextColor3 = Color3.new(1, 1, 1)
		Badge.TextSize = 22

		local Corner = Instance.new("UICorner")
		Corner.CornerRadius = UDim.new(0.5, 0)
		Corner.Parent = Badge

		local function Update(Values)
			Badge.Text = Values.Label
			Badge.Size = UDim2.fromOffset(if Values.Style.Wide then 240 else 120, 40)
		end
		Update(Props.controls)
		local Unsubscribe = Props.subscribe(Update)
		Badge.Parent = Props.target

		return function()
			Unsubscribe()
			Badge:Destroy()
		end
	end,
}
