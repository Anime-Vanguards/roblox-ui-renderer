-- Roblox's own GUI in a playing client (compare with --mode play): the top bar's menu button and Unibar, the player
-- list with two leaderstats, and the health bar of a character at 30 % health, over two panels of the game's at the top
-- corners, which Roblox's GUI covers. In edit mode (Studio's edit viewport) only the game's panels. Measured rules:
-- docs/render.md "Roblox's own GUI".
-- compare: --viewport 900x500 --time 1.5 --background 20,20,26 --mode play
return function(Target)
	local Player = game:GetService("Players").LocalPlayer

	local function Panel(Name, Position, Size, Color)
		local Frame = Instance.new("Frame")
		Frame.Name = Name
		Frame.Position = Position
		Frame.Size = Size
		Frame.BackgroundColor3 = Color
		Frame.BorderSizePixel = 0
		Frame.Parent = Target
	end
	Panel("Left", UDim2.fromOffset(8, 8), UDim2.fromOffset(240, 120), Color3.fromRGB(70, 130, 200))
	Panel("Right", UDim2.new(1, -360, 0, 40), UDim2.fromOffset(300, 70), Color3.fromRGB(220, 160, 40))

	local Stats = Instance.new("Folder")
	Stats.Name = "leaderstats"
	local Coins = Instance.new("IntValue")
	Coins.Name = "Coins"
	Coins.Value = 12345
	Coins.Parent = Stats
	local Rank = Instance.new("StringValue")
	Rank.Name = "Rank"
	Rank.Value = "Gold"
	Rank.Parent = Stats
	Stats.Parent = Player

	local Model = Instance.new("Model")
	Model.Name = "Character"
	local Humanoid = Instance.new("Humanoid")
	Humanoid.Parent = Model
	Model.Parent = workspace
	Player.Character = Model
	Humanoid.Health = 30

	return function()
		Stats:Destroy()
		Player.Character = nil
		Model:Destroy()
	end
end
