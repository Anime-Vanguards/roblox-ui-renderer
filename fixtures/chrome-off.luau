-- Roblox's own GUI as a game turns it off (compare with --mode play): StarterGui:SetCoreGuiEnabled Chat and
-- PlayerList false before the first frame leave the menu button and a Unibar of one button, and GuiService's inset
-- stays 58 (the label). Measured rules: docs/render.md "Roblox's own GUI".
-- compare: --viewport 900x500 --time 1 --background 20,20,26 --mode play
return function(Target)
	local StarterGui = game:GetService("StarterGui")
	StarterGui:SetCoreGuiEnabled(Enum.CoreGuiType.Chat, false)
	StarterGui:SetCoreGuiEnabled(Enum.CoreGuiType.PlayerList, false)

	local Bar = Instance.new("Frame")
	Bar.Name = "Bar"
	Bar.Size = UDim2.new(1, 0, 0, 70)
	Bar.BackgroundColor3 = Color3.fromRGB(60, 170, 110)
	Bar.BorderSizePixel = 0
	Bar.Parent = Target

	local Label = Instance.new("TextLabel")
	Label.Name = "Inset"
	Label.Position = UDim2.fromOffset(300, 140)
	Label.Size = UDim2.fromOffset(300, 30)
	Label.BackgroundTransparency = 1
	Label.TextColor3 = Color3.new(1, 1, 1)
	Label.TextSize = 20
	Label.Text = "inset " .. tostring(game:GetService("GuiService"):GetGuiInset().Y)
	Label.Parent = Target

	return function()
		StarterGui:SetCoreGuiEnabled(Enum.CoreGuiType.All, true)
	end
end
