-- Pixelated slices whose middle or corners run past their texture (degenerate SliceCenter, large SliceScale)
-- compare: --viewport 600x300 --time 0.5 --background 20,20,26
return function(Target)
	local Cases = {
		{ Rect.new(4, 8, 12, 8), 2, 60, 40 },
		{ Rect.new(2, 3, 8, 3), 2, 40, 30 },
		{ Rect.new(6, 6, 6, 6), 3, 70, 50 },
		{ Rect.new(4, 4, 12, 12), 3, 50, 30 },
		{ Rect.new(8, 2, 8, 14), 2.5, 45, 64 },
	}
	for Index, Case in Cases do
		local L = Instance.new("ImageLabel")
		L.Name = `Past{Index}`
		L.Position = UDim2.fromOffset(10 + (Index - 1) * 100 + 0.25, 20.5)
		L.Size = UDim2.fromOffset(Case[3], Case[4])
		L.BackgroundTransparency = 1
		L.BorderSizePixel = 0
		L.Image = "rbxasset://textures/ui/Motor.png"
		L.ResampleMode = Enum.ResamplerMode.Pixelated
		L.ScaleType = Enum.ScaleType.Slice
		L.SliceCenter = Case[1]
		L.SliceScale = Case[2]
		L.Parent = Target
	end
end
