-- A story that reaches its Wally package and shared modules through ReplicatedStorage, as game code does.
-- It needs the Rojo project mounted: `rur-native render --story <this file> --dir out/SmokeTest` from the project
-- folder finds default.project.json by itself.
local ReplicatedStorage = game:GetService("ReplicatedStorage")
local TinyFusion = require(ReplicatedStorage.Packages.TinyFusion)
local Theme = require(ReplicatedStorage.Shared.Theme)

local New, Children, Value, Computed = TinyFusion.New, TinyFusion.Children, TinyFusion.Value, TinyFusion.Computed

return function(Target: Frame)
	local Count = Value(3)
	New("Frame")({
		Name = "Panel",
		AnchorPoint = Vector2.new(0.5, 0.5),
		Position = UDim2.fromScale(0.5, 0.5),
		Size = UDim2.fromOffset(320, 120),
		BackgroundColor3 = Theme.Background,
		Parent = Target,
		[Children] = {
			New("UICorner")({ CornerRadius = UDim.new(0, 12) }),
			New("TextLabel")({
				Name = "Title",
				BackgroundTransparency = 1,
				Size = UDim2.new(1, 0, 0, 60),
				FontFace = Font.new("rbxasset://fonts/families/GothamSSm.json", Enum.FontWeight.Bold),
				TextSize = 24,
				TextColor3 = Theme.Text,
				Text = "Packages mounted",
			}),
			New("TextLabel")({
				Name = "Count",
				BackgroundTransparency = 1,
				Position = UDim2.fromOffset(0, 60),
				Size = UDim2.new(1, 0, 0, 40),
				FontFace = Font.new("rbxasset://fonts/families/GothamSSm.json"),
				TextSize = 18,
				TextColor3 = Theme.Accent,
				Text = Computed(function()
					return `{Count:get()} items`
				end),
			}),
		},
	})
end
