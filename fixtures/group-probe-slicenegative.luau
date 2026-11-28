-- compare: --viewport 1200x720 --time 0.3 --background 20,20,26
-- Sliced images at a negative SliceScale inside CanvasGroups: the far corner a negative scale draws past the image's
-- top and left, within the group and past the group's edge, in a group whose texture is drawn 1:1 and in one under a
-- UIScale (a resampled texture), beside the same images in plain Frames.
return function(Target)
	local Stone = "rbxasset://textures/ui/LegacyRbxGui/StoneBlockSide.png" -- 32x32, opaque
	local Motor = "rbxasset://textures/ui/Motor.png" -- 16x16, opaque

	local function Slice(Parent, Name, Uri, X, Y, W, H, Center, Scale, Pixelated)
		local Label = Instance.new("ImageLabel")
		Label.Name = Name
		Label.Position = UDim2.fromOffset(X, Y)
		Label.Size = UDim2.fromOffset(W, H)
		Label.BackgroundTransparency = 1
		Label.BorderSizePixel = 0
		Label.Image = Uri
		Label.ScaleType = Enum.ScaleType.Slice
		Label.SliceCenter = Center
		Label.SliceScale = Scale
		if Pixelated then
			Label.ResampleMode = Enum.ResamplerMode.Pixelated
		end
		Label.Parent = Parent
	end

	local Cases = {
		{ Stone, Rect.new(8, 8, 24, 24), -1 },
		{ Stone, Rect.new(8, 8, 24, 24), -0.25 },
		{ Stone, Rect.new(4, 4, 28, 28), -0.5 },
		{ Motor, Rect.new(2, 2, 14, 14), -1, true },
	}

	local function Row(Name, Y, Kind)
		local Holder = Instance.new("Frame")
		Holder.Name = Name
		Holder.Position = UDim2.fromOffset(0, Y)
		Holder.Size = UDim2.fromOffset(1200, 230)
		Holder.BackgroundTransparency = 1
		Holder.Parent = Target
		if Kind == "Scaled" then
			local Scale = Instance.new("UIScale")
			Scale.Scale = 0.87
			Scale.Parent = Holder
		end
		for Index, Case in Cases do
			local Box = Instance.new(if Kind == "Plain" then "Frame" else "CanvasGroup")
			Box.Name = `Box{Index}`
			Box.Position = UDim2.fromOffset(10 + (Index - 1) * 290, 10)
			Box.Size = UDim2.fromOffset(261, 201)
			Box.BackgroundColor3 = Color3.fromRGB(50, 50, 62)
			Box.BorderSizePixel = 0
			Box.ClipsDescendants = true
			Box.Parent = Holder
			-- Its far corner stays inside the box
			Slice(Box, "Inside", Case[1], 140, 100, 64, 64, Case[2], Case[3], Case[4])
			-- Its far corner reaches past the box's top and left
			Slice(Box, "Past", Case[1], 21, 17, 64, 48, Case[2], Case[3], Case[4])
		end
	end
	Row("Plain", 0, "Plain")
	Row("Groups", 235, "Groups")
	Row("Scaled", 470, "Scaled")
end
