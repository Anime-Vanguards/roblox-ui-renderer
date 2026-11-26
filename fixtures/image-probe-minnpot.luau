-- Image probe: one opaque non-power-of-two texture (823 x 549, odd sides on several mip levels, no @2x file) minified
-- at many sizes, so the mip chain and the trilinear blend can be read level by level. Stretch, on whole device px at
-- 1.5x and 1x (even logical places and sizes), over a black background; the last row squashes it on one axis.
-- compare: --viewport 1200x900 --time 0.5 --background 0,0,0
return function(Target)
	local Uri = "rbxasset://textures/AvatarEditorImages/Catalog_LightTheme.png"
	local function Image(Name, X, Y, W, H)
		local Label = Instance.new("ImageLabel")
		Label.Name = Name
		Label.Position = UDim2.fromOffset(X, Y)
		Label.Size = UDim2.fromOffset(W, H)
		Label.BackgroundTransparency = 1
		Label.BorderSizePixel = 0
		Label.Image = Uri
		Label.Parent = Target
	end
	local X, Y, RowHeight = 10, 10, 0
	for _, W in { 412, 300, 246, 200, 164, 130, 104, 82, 64, 52, 40, 32, 26, 20 } do
		local H = math.floor(W * 549 / 823 / 2 + 0.5) * 2
		if X + W > 1190 then
			X = 10
			Y += RowHeight + 10
			RowHeight = 0
		end
		Image(`S{W}`, X, Y, W, H)
		X += W + 10
		RowHeight = math.max(RowHeight, H)
	end
	X = 10
	Y += RowHeight + 10
	for _, Size in { { 300, 60 }, { 60, 300 }, { 200, 130 }, { 130, 200 }, { 400, 34 } } do
		Image(`Q{Size[1]}x{Size[2]}`, X, Y, Size[1], Size[2])
		X += Size[1] + 10
	end
end
