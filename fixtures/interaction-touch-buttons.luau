-- ContextActionService touch buttons on a touch device (compare with --device touch): eight actions bound with a
-- button (the eighth gets none), titled (one title wraps), with an icon (SetImage), one moved by SetPosition, as a
-- 0.747 client's ContextActionTouch core script draws them (notes/interaction.md "Touch buttons"), and the images of a
-- button at rest and held, drawn by the story on the left. The buttons' ScreenGui is raised above the comparison's own
-- so a client shows it over the backdrop; rur draws it in the PlayerGui pass under the story gui, where nothing of the
-- story covers it. On other devices only the left part. Scores: notes/interaction.md "Touch buttons".
-- compare: --viewport 900x620 --time 0.2 --background 20,20,26 --device touch
return function(Target)
	local Actions = game:GetService("ContextActionService")
	local function Pass()
		return Enum.ContextActionResult.Pass
	end
	Actions:BindAction("FixtureJump", Pass, true, Enum.KeyCode.Space)
	Actions:SetTitle("FixtureJump", "Jump")
	Actions:BindAction("FixtureItem", Pass, true, Enum.KeyCode.E)
	Actions:SetImage("FixtureItem", "rbxasset://textures/ui/GuiImagePlaceholder.png")
	Actions:BindAction("FixtureLong", Pass, true, Enum.KeyCode.R)
	Actions:SetTitle("FixtureLong", "Reload all")
	Actions:BindAction("FixtureBoth", Pass, true, Enum.KeyCode.F)
	Actions:SetTitle("FixtureBoth", "Use")
	Actions:SetImage("FixtureBoth", "rbxasset://textures/ui/GuiImagePlaceholder.png")
	Actions:BindAction("FixtureMoved", Pass, true, Enum.KeyCode.G)
	Actions:SetTitle("FixtureMoved", "Moved")
	Actions:SetPosition("FixtureMoved", UDim2.new(-0.5, 0, 0.1, 0))
	Actions:BindAction("FixtureNone", Pass, false, Enum.KeyCode.H)
	for Index = 6, 8 do
		Actions:BindAction("Fixture" .. Index, Pass, true, Enum.KeyCode.One)
		Actions:SetTitle("Fixture" .. Index, tostring(Index))
	end

	local Player = game:GetService("Players").LocalPlayer
	local PlayerGui = Player and Player:FindFirstChildOfClass("PlayerGui")
	local Buttons = PlayerGui and PlayerGui:FindFirstChild("ContextActionGui")
	if Buttons then
		Buttons.DisplayOrder = 2147483647
	end

	for Index, Image in { "https://www.roblox.com/asset/?id=97166444", "https://www.roblox.com/asset/?id=97166756" } do
		local Sample = Instance.new("ImageLabel")
		Sample.Name = "Sample" .. Index
		Sample.BackgroundTransparency = 1
		Sample.Image = Image
		Sample.Position = UDim2.fromOffset(20 + (Index - 1) * 70, 20)
		Sample.Size = UDim2.fromOffset(45, 45)
		Sample.Parent = Target
		local Big = Sample:Clone()
		Big.Name = "Big" .. Index
		Big.Position = UDim2.fromOffset(20 + (Index - 1) * 130, 90)
		Big.Size = UDim2.fromOffset(120, 120)
		Big.Parent = Target
	end
end
