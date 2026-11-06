-- An options prompt's Holder: Size (540, 0) AutomaticSize Y + UIAspectRatioConstraint 4 around a scale-width
-- AutoY panel whose wrapping button row is 490 wide: Studio makes it 518 x 200 (stable 518 x 129.5,
-- the panel's natural width), not 540. Variations of the item width, the holder's size / anchor, the
-- panel's size and the aspect.
return function(Target)
	local Results = {}
	local Nodes = {}
	local Count = 0
	local function Case(Name, Options)
		Count += 1
		local Holder = Instance.new("Frame")
		Holder.Position = UDim2.fromOffset(((Count - 1) % 2) * 600, ((Count - 1) // 2) * 300)
		Holder.Size = UDim2.fromOffset(1200, 900)
		Holder.BackgroundTransparency = 1
		Holder.Parent = Target
		local H = Instance.new("Frame")
		H.AutomaticSize = Enum.AutomaticSize[Options.auto or "Y"]
		H.Size = Options.size or UDim2.fromOffset(540, 0)
		if not Options.corner then
			H.AnchorPoint = Vector2.new(0.5, 0.5)
			H.Position = UDim2.fromScale(0.5, 0.5)
		end
		H.BackgroundTransparency = 1
		H.Parent = Holder
		if Options.aspect ~= false then
			local C = Instance.new("UIAspectRatioConstraint")
			C.AspectRatio = Options.ratio or 4
			C.Parent = H
		end
		local Main = Instance.new("Frame")
		Main.Size = Options.mainSize or UDim2.new(1, 0, 0, 0)
		Main.AutomaticSize = Enum.AutomaticSize.Y
		Main.BackgroundTransparency = 1
		Main.Parent = H
		local P = Instance.new("UIPadding")
		P.PaddingLeft, P.PaddingRight = UDim.new(0, 14), UDim.new(0, 14)
		P.PaddingTop, P.PaddingBottom = UDim.new(0, 16), UDim.new(0, 16)
		P.Parent = Main
		local L = Instance.new("UIListLayout")
		L.Padding = UDim.new(0, 12)
		L.HorizontalAlignment = Enum.HorizontalAlignment.Center
		L.Parent = Main
		local Buttons = Instance.new("Frame")
		Buttons.Size = UDim2.new(1, 0, 0, 0)
		Buttons.AutomaticSize = Enum.AutomaticSize.Y
		Buttons.BackgroundTransparency = 1
		Buttons.Parent = Main
		local BL = Instance.new("UIListLayout")
		BL.FillDirection = Enum.FillDirection.Horizontal
		BL.Wraps = Options.wraps ~= false
		BL.HorizontalAlignment = Enum.HorizontalAlignment.Center
		BL.Padding = UDim.new(0, 10)
		BL.Parent = Buttons
		for Index = 1, Options.items or 3 do
			local B = Instance.new("Frame")
			B.Size = UDim2.fromOffset(Options.itemWidth or 240, 56)
			B.Parent = Buttons
		end
		Nodes[Name] = H
		Nodes[Name .. "_main"] = Main
		Nodes[Name .. "_buttons"] = Buttons
	end
	Case("base", {})
	Case("items100", { itemWidth = 100 })
	Case("items200x2", { itemWidth = 200, items = 2 })
	Case("small", { size = UDim2.fromOffset(300, 0) })
	Case("noaspect", { aspect = false })
	Case("corner", { corner = true })
	Case("offsetmain", { mainSize = UDim2.new(0, 540, 0, 0) })
	Case("nowrap", { wraps = false })
	Case("ratio2", { ratio = 2 })
	Case("autoXY", { auto = "XY", size = UDim2.fromOffset(0, 0) })
	for _ = 1, 3 do
		task.wait()
	end
	for Key, Node in Nodes do
		Results[Key] = { Node.AbsoluteSize.X, Node.AbsoluteSize.Y }
	end
	return Results
end
