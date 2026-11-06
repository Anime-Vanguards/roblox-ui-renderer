-- Thumbnail probe: which texture Studio draws for an rbxthumb:// headshot. a bracket list's and a row list's avatar
-- headshots (rbxthumb AvatarHeadShot w=60 / w=150) differ from our thumbnails.roblox.com PNGs all over the face.
-- Here the same thumbnails are drawn Pixelated and large (every texel a visible block, to count the texture's
-- size and compare texels), and at the stories' sizes with the default filter.
-- compare: --viewport 1200x900 --time 2 --background 30,30,30
-- analysis: python tools/analysis/image_thumb.py image-probe-thumb
return function(Target)
	local function Image(Name, Uri, X, Y, Size, Resample, ScaleType)
		local Label = Instance.new("ImageLabel")
		Label.Name = Name
		Label.Position = UDim2.fromOffset(X, Y)
		Label.Size = UDim2.fromOffset(Size, Size)
		Label.BackgroundTransparency = 1
		Label.BorderSizePixel = 0
		Label.Image = Uri
		Label.ResampleMode = Resample or Enum.ResamplerMode.Default
		Label.ScaleType = ScaleType or Enum.ScaleType.Stretch
		Label.Parent = Target
		return Label
	end

	local Head60 = "rbxthumb://type=AvatarHeadShot&id=156&w=60&h=60"
	local Head150 = "rbxthumb://type=AvatarHeadShot&id=1&w=150&h=150"
	Image("Pix60", Head60, 10, 10, 420, Enum.ResamplerMode.Pixelated)
	Image("Pix150", Head150, 440, 10, 450, Enum.ResamplerMode.Pixelated)
	-- The stories' sizes, default filter: the bracket list's Fit 32 x 32 of the w=60 thumbnail, the rows' 31 x 31 of w=150
	Image("Story60", Head60, 920, 20, 32, nil, Enum.ScaleType.Fit)
	Image("Story150", Head150, 980, 20, 31)
	Image("Big60", Head60, 920, 80, 240)
end
