-- CanvasGroup compositing arithmetic: 256 colour steps (frames 2 logical = 3 physical px wide) inside
-- groups on whole pixels (no resampling). Rows: GroupTransparency 0.1 / 0.3 / 0.5 / 0.7 / 0.9 / 0.25 over
-- the dark backdrop and over an orange panel; transparent groups holding semi-transparent frames
-- (BackgroundTransparency 0.9 / 0.5 / 0.25 / 0.75) over both backdrops; the same frames without a group.
-- Colours per step k: (k, 255 - k, (k * 7) % 256).
-- compare: --viewport 1200x900 --time 0.2 --background 20,20,26
return function(Target)
	local Panel = Instance.new("Frame")
	Panel.Name = "Panel"
	Panel.Position = UDim2.fromOffset(600, 0)
	Panel.Size = UDim2.fromOffset(600, 900)
	Panel.BackgroundColor3 = Color3.fromRGB(200, 120, 60)
	Panel.BorderSizePixel = 0
	Panel.Parent = Target

	local function Steps(Parent, Transparency)
		for K = 0, 255 do
			local F = Instance.new("Frame")
			F.Name = `S{K}`
			F.Position = UDim2.fromOffset((K % 128) * 2, K < 128 and 0 or 8)
			F.Size = UDim2.fromOffset(2, 8)
			F.BackgroundColor3 = Color3.fromRGB(K, 255 - K, (K * 7) % 256)
			F.BackgroundTransparency = Transparency
			F.BorderSizePixel = 0
			F.Parent = Parent
		end
	end

	local function Group(Name, X, Y, Props, Content)
		local G = Instance.new("CanvasGroup")
		G.Name = Name
		G.Position = UDim2.fromOffset(X, Y)
		G.Size = UDim2.fromOffset(256, 16)
		G.BackgroundTransparency = 1
		G.BorderSizePixel = 0
		for Key, Value in Props do
			(G :: any)[Key] = Value
		end
		G.Parent = Target
		Steps(G, Content)
		return G
	end

	local Row = 0
	for _, T in { 0.1, 0.3, 0.5, 0.7, 0.9, 0.25 } do
		Group(`Faded{T}`, 20, 20 + Row * 24, { GroupTransparency = T }, 0)
		Group(`FadedPanel{T}`, 620, 20 + Row * 24, { GroupTransparency = T }, 0)
		Row += 1
	end
	for _, T in { 0.9, 0.5, 0.25, 0.75 } do
		Group(`Clear{T}`, 20, 20 + Row * 24, {}, T)
		Group(`ClearPanel{T}`, 620, 20 + Row * 24, {}, T)
		Row += 1
	end
	for _, T in { 0.9, 0.5, 0.25, 0.75 } do
		local Plain = Instance.new("Frame")
		Plain.Name = `Plain{T}`
		Plain.Position = UDim2.fromOffset(20, 20 + Row * 24)
		Plain.Size = UDim2.fromOffset(256, 16)
		Plain.BackgroundTransparency = 1
		Plain.Parent = Target
		Steps(Plain, T)
		local PlainPanel = Plain:Clone()
		PlainPanel.Name = `PlainPanel{T}`
		PlainPanel.Position = UDim2.fromOffset(620, 20 + Row * 24)
		PlainPanel.Parent = Target
		Row += 1
	end
	-- Faded groups holding semi-transparent frames
	for _, T in { 0.5, 0.3 } do
		Group(`FadedClear{T}`, 20, 20 + Row * 24, { GroupTransparency = T }, 0.5)
		Group(`FadedClearPanel{T}`, 620, 20 + Row * 24, { GroupTransparency = T }, 0.5)
		Row += 1
	end
end
