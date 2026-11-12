-- Images of an EditableImage, turned and not: the texels encode their own position (red a triangle wave
-- along x, green along y, 0 128 255 128 a period of four texels; blue 255), so every pixel shows where it was
-- sampled to a few hundredths of a pixel, and blue alone is the coverage. Row 1 and 2: 3 px a texel, the whole
-- image and an ImageRect crop of its middle, at six angles. Row 3: fractional places and sizes. Rows 4 to 7:
-- the image at 2, 1.5, 1.2, 1, 0.8 and 0.6 px a texel at four angles: an EditableImage has no mip levels, so
-- the minified ones alias where an asset's texture would blur.
-- compare: --viewport 1200x900 --time 1 --background 0,0,0
return function(Target)
	local AssetService = game:GetService("AssetService")
	local Wave = { 0, 128, 255, 128 }
	local function Make(N)
		local Ok, Editable = pcall(function()
			return AssetService:CreateEditableImage({ Size = Vector2.new(N, N) })
		end)
		if not Ok or not Editable then
			return nil
		end
		local Pixels = buffer.create(N * N * 4)
		for Y = 0, N - 1 do
			for X = 0, N - 1 do
				local Offset = (Y * N + X) * 4
				buffer.writeu8(Pixels, Offset, Wave[X % 4 + 1])
				buffer.writeu8(Pixels, Offset + 1, Wave[Y % 4 + 1])
				buffer.writeu8(Pixels, Offset + 2, 255)
				buffer.writeu8(Pixels, Offset + 3, 255)
			end
		end
		Editable:WritePixelsBuffer(Vector2.zero, Vector2.new(N, N), Pixels)
		return Editable
	end
	local function Image(Name, Editable, X, Y, W, H, Rotation, Props)
		local Label = Instance.new("ImageLabel")
		Label.Name = Name
		Label.Position = UDim2.fromOffset(X, Y)
		Label.Size = UDim2.fromOffset(W, H)
		Label.Rotation = Rotation
		Label.BackgroundTransparency = 1
		Label.BorderSizePixel = 0
		Label.ImageContent = Content.fromObject(Editable)
		for Key, Value in Props or {} do
			(Label :: any)[Key] = Value
		end
		Label.Parent = Target
	end

	local Base = Make(64)
	if not Base then
		return
	end
	local Angles = { 13, 30, 45, -65, 160, 0 }
	local Crop = { ImageRectOffset = Vector2.new(16, 16), ImageRectSize = Vector2.new(32, 32) }
	for Column, Rotation in Angles do
		local X = 30 + (Column - 1) * 150
		Image(`Whole{Rotation}`, Base, X, 30, 96, 96, Rotation)
		Image(`Crop{Rotation}`, Base, X, 180, 96, 96, Rotation, Crop)
	end
	Image("Fraction13", Base, 30.5, 330.25, 100.5, 77.25, 13)
	Image("Fraction30", Base, 180.25, 330.5, 77.75, 96.5, 30)
	Image("Wide7", Base, 330, 345, 150, 60, 7)
	Image("Turn1", Base, 500, 330, 96, 96, 1)
	Image("Turn89", Base, 630, 330, 96, 96, 89)
	Image("Turn90", Base, 760, 330, 96, 96, 90)
	Image("Small13", Base, 900, 330, 32, 32, 13)

	for Column, N in { 48, 64, 80, 96, 120, 160 } do
		local Editable = Make(N)
		if not Editable then
			return
		end
		for Row, Rotation in { 13, 30, -65, 0 } do
			Image(`Texels{N}Turn{Rotation}`, Editable, 30 + (Column - 1) * 105, 470 + (Row - 1) * 105, 64, 64, Rotation)
		end
	end
end
