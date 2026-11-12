-- How a turned CanvasGroup's texture is sampled. Every group is clear, without a border, and holds one white
-- child over its left half: the edge down its middle shows, pixel by pixel, how wide the texture's texels are
-- on the screen and where they lie (python tools/analysis/group_turned.py reads the ramp across that edge).
-- Row 1 and 2: a 60 x 60 group at 30 angles. Row 3: 120 x 60 and 60 x 120 groups. Row 4: sizes that are not
-- whole device pixels at 1.5x (61, 33, 47), whose texture is resampled when unturned. Row 5: the edge at a
-- third of the group, and groups at fractional places. Row 6: the same child in a turned Frame, for the
-- unfiltered edge.
-- compare: --viewport 1200x900 --time 0.2 --background 0,0,0
return function(Target)
	local function Holder(Class, Name, X, Y, W, H, Rotation, Edge)
		local G = Instance.new(Class)
		G.Name = Name
		G.Position = UDim2.fromOffset(X, Y)
		G.Size = UDim2.fromOffset(W, H)
		G.Rotation = Rotation
		G.BackgroundTransparency = 1
		G.BorderSizePixel = 0
		G.Parent = Target
		local Child = Instance.new("Frame")
		Child.Name = "Half"
		Child.Size = UDim2.fromOffset(Edge or W / 2, H)
		Child.BackgroundColor3 = Color3.new(1, 1, 1)
		Child.BorderSizePixel = 0
		Child.Parent = G
		return G
	end
	local Angles = { 0.25, 0.5, 1, 2, 3, 5, 7, 10, 15, 20, 25, 30, 35, 40, 45, 50, 60, 70, 80, 85, 88, 89, 89.5, 90, 91, 135, 180, 181, 270, -20 }
	for Index, Rotation in Angles do
		local Column, Row = (Index - 1) % 15, (Index - 1) // 15
		Holder("CanvasGroup", `Turn{Index}`, 15 + Column * 79, 15 + Row * 95, 60, 60, Rotation)
	end
	for Index, Rotation in { 2, 20, 45, 70, 90 } do
		Holder("CanvasGroup", `Wide{Index}`, 30 + (Index - 1) * 150, 225, 120, 60, Rotation)
		Holder("CanvasGroup", `Tall{Index}`, 800 + (Index - 1) * 80, 200, 40, 120, Rotation)
	end
	for Index, Rotation in { 2, 20, 45, 90 } do
		Holder("CanvasGroup", `Odd61x{Index}`, 30 + (Index - 1) * 95, 350, 61, 61, Rotation, 30)
		Holder("CanvasGroup", `Odd33x{Index}`, 420 + (Index - 1) * 75, 360, 33, 47, Rotation, 16)
		Holder("CanvasGroup", `Odd47x{Index}`, 740 + (Index - 1) * 110, 350, 47, 33, Rotation, 24)
	end
	for Index, Rotation in { 2, 20, 45, 90 } do
		Holder("CanvasGroup", `Third{Index}`, 30 + (Index - 1) * 95, 460, 60, 60, Rotation, 20)
		Holder("CanvasGroup", `Place{Index}`, 420.4 + (Index - 1) * 95.3, 460.7, 60, 60, Rotation)
		Holder("CanvasGroup", `Big{Index}`, 30 + (Index - 1) * 290, 560, 200, 150, Rotation)
	end
	for Index, Rotation in { 2, 20, 45, 90 } do
		Holder("Frame", `Plain{Index}`, 30 + (Index - 1) * 95, 780, 60, 60, Rotation)
	end
end
