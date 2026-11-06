-- UIShadow probe P (2026-09-27): a button grid - 2 x 2 rounded bodies
-- (200 x 65, UICorner 5) with coloured Transparency 0.82 BlurRadius 24 shadows behind them, gaps narrower than
-- the shadows (the story has UIScale 0.885; the same differences show at 1). Top right: the same grid with
-- transparent bodies (shadows only); below: the clear grid's shadows one per cell.
-- compare: --viewport 1200x900 --time 0.2 --background 11,12,17
return function(Target)
	local Green = Color3.fromRGB(14, 235, 88)
	local Purple = Color3.fromRGB(150, 47, 235)

	local function Body(Parent, X, Y, Color, Visible)
		local F = Instance.new("Frame")
		F.Name = "Body"
		F.Position = UDim2.fromOffset(X, Y)
		F.Size = UDim2.fromOffset(200, 65)
		F.BackgroundColor3 = Color
		F.BackgroundTransparency = Visible and 0 or 1
		F.BorderSizePixel = 0
		local Corner = Instance.new("UICorner")
		Corner.CornerRadius = UDim.new(0, 5)
		Corner.Parent = F
		local Shadow = Instance.new("UIShadow")
		Shadow.Color = Color
		Shadow.Transparency = 0.82
		Shadow.BlurRadius = UDim.new(0, 24)
		Shadow.Parent = F
		F.Parent = Parent
	end

	local function Grid(Name, X, Y, Scale, Visible, Single, Only)
		local Holder = Instance.new("Frame")
		Holder.Name = Name
		Holder.BackgroundTransparency = 1
		Holder.Position = UDim2.fromOffset(X, Y)
		Holder.Size = UDim2.fromOffset(480, 200)
		Holder.Parent = Target
		local UIScale = Instance.new("UIScale")
		UIScale.Scale = Scale
		UIScale.Parent = Holder
		if Single then
			Body(Holder, 40, 40, Green, Visible)
			return
		end
		local Bodies = { { 40, 40, Green }, { 255, 40, Purple }, { 40, 117, Purple }, { 255, 117, Purple } }
		for Index, Entry in Bodies do
			if not Only or Only == Index then
				Body(Holder, Entry[1], Entry[2], Entry[3], Visible)
			end
		end
	end

	Grid("Plain", 20, 20, 1, true)
	Grid("PlainClear", 600, 20, 1, false)
	-- The clear grid's four shadows one per cell
	for Index = 1, 4 do
		Grid(`PlainOnly{Index}`, 20 + ((Index - 1) % 2) * 580, 300 + math.floor((Index - 1) / 2) * 280, 1, false, false, Index)
	end
end
