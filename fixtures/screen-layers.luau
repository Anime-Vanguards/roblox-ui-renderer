-- --all-layers: ScreenGuis a fixture parents under the PlayerGui and CoreGui, drawn in Roblox's order
-- (the PlayerGui pass under CoreGui's, then DisplayOrder, then the order they were last parented or
-- turned Enabled; measured in Studio). Each draws a 120x80 Frame, offset 20 px from the one before,
-- so the overlaps show the order:
--   Low      PlayerGui, DisplayOrder 0, parented first, re-enabled last  -> top of the DisplayOrder-0 tie
--   Mid      PlayerGui, DisplayOrder 0, parented second
--   Nested   PlayerGui/Folder, DisplayOrder 0, parented third (Folders are looked through)
--   High     PlayerGui, DisplayOrder 5                                    -> above the tie
--   Core     CoreGui, DisplayOrder -10                                    -> above every PlayerGui layer
--   Off      PlayerGui, Enabled false                                     -> not drawn
--   World    PlayerGui SurfaceGui                                         -> not drawn (3D)
-- Draw order: Mid, Nested, Low, High, then RURStory and Core (CoreGui, DisplayOrder 0 and -10 tie
-- by pass order: Core -10 first, then RURStory 0).
return function(_Target)
	local Players = game:GetService("Players")
	local PlayerGui = Players.LocalPlayer.PlayerGui
	local CoreGui = game:GetService("CoreGui")
	local Colors = {
		Low = Color3.fromRGB(220, 40, 40),
		Mid = Color3.fromRGB(40, 160, 60),
		Nested = Color3.fromRGB(40, 90, 220),
		High = Color3.fromRGB(230, 200, 40),
		Core = Color3.fromRGB(200, 60, 200),
		Off = Color3.fromRGB(255, 255, 255),
		World = Color3.fromRGB(255, 255, 255),
	}
	local Index = 0
	local function Layer(Class, Name, DisplayOrder)
		local Gui = Instance.new(Class)
		Gui.Name = Name
		if Class == "ScreenGui" then
			Gui.DisplayOrder = DisplayOrder
			Gui.IgnoreGuiInset = true
		end
		local Box = Instance.new("Frame")
		Box.Name = "Box"
		Box.BorderSizePixel = 0
		Box.BackgroundColor3 = Colors[Name]
		Box.Position = UDim2.fromOffset(20 + Index * 20, 20 + Index * 20)
		Box.Size = UDim2.fromOffset(120, 80)
		Box.Parent = Gui
		Index += 1
		return Gui
	end

	local Low = Layer("ScreenGui", "Low", 0)
	Low.Parent = PlayerGui
	Layer("ScreenGui", "Mid", 0).Parent = PlayerGui
	local Folder = Instance.new("Folder")
	Folder.Name = "Layers"
	Folder.Parent = PlayerGui
	Layer("ScreenGui", "Nested", 0).Parent = Folder
	Layer("ScreenGui", "High", 5).Parent = PlayerGui
	Layer("ScreenGui", "Core", -10).Parent = CoreGui
	local Off = Layer("ScreenGui", "Off", 0)
	Off.Enabled = false
	Off.Parent = PlayerGui
	Layer("SurfaceGui", "World", 0).Parent = PlayerGui
	-- Low moves to the top of its tie
	Low.Enabled = false
	Low.Enabled = true
end
