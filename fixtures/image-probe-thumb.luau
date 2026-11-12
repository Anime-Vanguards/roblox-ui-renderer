-- Thumbnail-sized textures drawn as the stories draw avatar headshots: Pixelated and large (every texel a
-- visible block, to count the texture's size and compare texels), and at the stories' sizes with the default
-- filter. The textures are Studio's own, 60 x 60 and 128 x 128: a live rbxthumb:// headshot is not compared,
-- because the thumbnail service re-renders avatars and hands out renditions of different ages by size (Studio
-- drew a 150 x 150 headshot 6.5 levels brighter than the 150 x 150 file the service returned on the same day:
-- notes/images.md, "image-probe-thumb"), so its pixels are no measure of the renderer.
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

	local Small = "rbxasset://textures/ui/Settings/Help/BButtonDark.png" -- 60 x 60
	local Large = "rbxasset://textures/explosion.png" -- 128 x 128
	Image("Pix60", Small, 10, 10, 420, Enum.ResamplerMode.Pixelated)
	Image("Pix128", Large, 440, 10, 448, Enum.ResamplerMode.Pixelated)
	-- The stories' sizes, default filter: a bracket list's Fit 32 x 32 of its 60 px headshot, a row's 31 x 31
	Image("Story60", Small, 920, 20, 32, nil, Enum.ScaleType.Fit)
	Image("Story128", Large, 980, 20, 31)
	Image("Big60", Small, 920, 80, 240)
end
