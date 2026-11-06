-- Thumbnail probe: what Studio draws for an rbxthumb:// headshot the thumbnail service reports as not
-- "Completed" (user 88: state "Blocked", imageUrl = the "UnapprovedImage" icon; Rows' fourth conversation
-- avatar). Drawn Pixelated at 3 device px per texel (150 x 150 texels) to read the texture back, next to a
-- Completed one, and a w=60 and a w=420 variant of the blocked one (size-specific placeholder?).
-- compare: --viewport 1200x900 --time 2 --background 30,30,30
return function(Target)
	local function Image(Name, Uri, X, Y, Size, Resample)
		local Label = Instance.new("ImageLabel")
		Label.Name = Name
		Label.Position = UDim2.fromOffset(X, Y)
		Label.Size = UDim2.fromOffset(Size, Size)
		Label.BackgroundTransparency = 1
		Label.BorderSizePixel = 0
		Label.Image = Uri
		Label.ResampleMode = Resample or Enum.ResamplerMode.Default
		Label.Parent = Target
		return Label
	end
	Image("Blocked150", "rbxthumb://type=AvatarHeadShot&id=88&w=150&h=150", 10, 10, 300, Enum.ResamplerMode.Pixelated)
	Image("Blocked60", "rbxthumb://type=AvatarHeadShot&id=88&w=60&h=60", 320, 10, 240, Enum.ResamplerMode.Pixelated)
	Image("Blocked420", "rbxthumb://type=AvatarHeadShot&id=88&w=420&h=420", 570, 10, 420, Enum.ResamplerMode.Pixelated)
	Image("BlockedAvatar", "rbxthumb://type=Avatar&id=88&w=150&h=150", 10, 320, 300, Enum.ResamplerMode.Pixelated)
	Image("Missing", "rbxthumb://type=AvatarHeadShot&id=9&w=150&h=150", 320, 320, 300, Enum.ResamplerMode.Pixelated)
	Image("Story", "rbxthumb://type=AvatarHeadShot&id=88&w=150&h=150", 640, 450, 34)
end
