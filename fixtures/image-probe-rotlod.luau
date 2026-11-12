-- The mip level of a turned image, inside it and in its anti-aliased fringe. The texture is 16 x 16 with one
-- opaque white row at the top and bottom over clear rows (Studio's own RoundedCenterBorder.png), so the first
-- half texel inside the top edge reads the level directly: the level's edge texel is 1, 1/2, 1/4 ... opaque and
-- as bright, and the pixel is their product. Rows: shapes from square to 1:19 (the image minified across only,
-- along only, or both); columns: 14 angles. Analysis: python tools/analysis/image_rotlod.py <compare dir>.
-- Pixel centres that lie exactly on an edge or a corner's diagonal (30, 45 and 60 degrees about a centre on a
-- half pixel) fall to the frame or the fringe by the rasterizer's tie rule.
-- compare: --viewport 1200x900 --time 1 --background 0,0,0
return function(Target)
	local Tex = "rbxasset://textures/StudioSharedUI/RoundedCenterBorder.png"
	local Shapes = { { 8, 8 }, { 6, 6 }, { 16, 8 }, { 12, 6 }, { 32, 8 }, { 24, 6 }, { 64, 8 }, { 150, 8 }, { 150, 6 }, { 8, 16 }, { 8, 32 }, { 5, 5 }, { 10, 5 }, { 4, 4 } }
	local Angles = { 0.5, 5, 10, 15, 20, 22.5, 25, 30, 35, 40, 45, 52, 60, 75 }
	local Y = 30
	for Row, Shape in Shapes do
		local Pitch = math.max(Shape[1], Shape[2]) + 14
		local PerRow = math.min(#Angles, math.floor(1180 / Pitch))
		for Index, Rot in Angles do
			if Index > PerRow then break end
			local L = Instance.new("ImageLabel")
			L.Name = `V_{Shape[1]}_{Shape[2]}_{Rot}`
			L.AnchorPoint = Vector2.new(0.5, 0.5)
			L.Position = UDim2.fromOffset(10 + (Index - 0.5) * Pitch, Y + Pitch / 2)
			L.Size = UDim2.fromOffset(Shape[1], Shape[2])
			L.Rotation = Rot
			L.BackgroundTransparency = 1
			L.BorderSizePixel = 0
			L.Image = Tex
			L.Parent = Target
		end
		Y += Pitch + 4
	end
end
