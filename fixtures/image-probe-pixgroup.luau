-- Pixelated images whose pixel centres fall exactly on texel boundaries (16 texels over 36, 24 or 54 device
-- px at 1.5x), where the rasterizer's interpolation decides the texel: on the screen, in CanvasGroups at whole
-- device sizes (the texture is the screen's grid) and at odd sizes (the texture is resampled), tiled, sliced
-- and cropped by ImageRect, clipped by a parent, under a UICorner, and at places off the screen's origin.
-- compare: --viewport 1200x900 --time 0.5 --background 20,20,26
return function(Target)
	local Motor = "rbxasset://textures/ui/Motor.png"
	local function Image(Parent, Name, X, Y, W, H, Props)
		local Label = Instance.new("ImageLabel")
		Label.Name = Name
		Label.Position = UDim2.fromOffset(X, Y)
		Label.Size = UDim2.fromOffset(W, H)
		Label.BackgroundTransparency = 1
		Label.BorderSizePixel = 0
		Label.Image = Motor
		Label.ResampleMode = Enum.ResamplerMode.Pixelated
		for Key, Value in Props or {} do
			(Label :: any)[Key] = Value
		end
		Label.Parent = Parent
		return Label
	end
	local function Group(Name, X, Y, W, H, Class)
		local G = Instance.new(Class or "CanvasGroup")
		G.Name = Name
		G.Position = UDim2.fromOffset(X, Y)
		G.Size = UDim2.fromOffset(W, H)
		G.BackgroundColor3 = Color3.fromRGB(40, 40, 70)
		G.BorderSizePixel = 0
		G.Parent = Target
		return G
	end
	local Tiled = { ScaleType = Enum.ScaleType.Tile, TileSize = UDim2.fromOffset(24, 24) }
	local Sliced = { ScaleType = Enum.ScaleType.Slice, SliceCenter = Rect.new(4, 4, 12, 12) }
	local Cropped = { ImageRectOffset = Vector2.new(2, 3), ImageRectSize = Vector2.new(8, 8) }

	-- Row 1: on the screen
	Image(Target, "Screen24", 10, 10, 24, 24)
	Image(Target, "Screen36", 50, 10, 36, 36)
	Image(Target, "Screen72x30", 100, 10, 72, 30)
	Image(Target, "ScreenTile", 190, 10, 120, 80, Tiled)
	Image(Target, "ScreenSlice", 330, 10, 96, 60, Sliced)
	Image(Target, "ScreenCrop", 440, 10, 48, 48, Cropped)
	Image(Target, "ScreenFar", 1001, 13, 72, 72)
	Image(Target, "ScreenOdd", 811, 11, 67, 68)

	-- Row 2: in a CanvasGroup at a whole device size and place
	local Whole = Group("Whole", 10, 110, 500, 100)
	Image(Whole, "In24", 8, 8, 24, 24)
	Image(Whole, "In36", 40, 8, 36, 36)
	Image(Whole, "In72x30", 90, 8, 72, 30)
	Image(Whole, "InTile", 180, 8, 120, 80, Tiled)
	Image(Whole, "InSlice", 310, 8, 96, 60, Sliced)
	Image(Whole, "InCrop", 420, 8, 48, 48, Cropped)

	-- Row 3: in a CanvasGroup whose texture is resampled (odd size, fractional place)
	local Odd = Group("Odd", 10.3, 230.7, 501, 101)
	Image(Odd, "In24", 8, 8, 24, 24)
	Image(Odd, "In36", 40, 8, 36, 36)
	Image(Odd, "In72x30", 90, 8, 72, 30)
	Image(Odd, "InTile", 180, 8, 120, 80, Tiled)
	Image(Odd, "InSlice", 310, 8, 96, 60, Sliced)
	Image(Odd, "InCrop", 420, 8, 48, 48, Cropped)

	-- Row 4: clipped by a parent, and a group away from the origin
	local Clip = Group("Clip", 10, 350, 200, 60, "Frame")
	Clip.ClipsDescendants = true
	Image(Clip, "Clipped36", -10, -10, 36, 36)
	Image(Clip, "ClippedTile", 40, 20, 240, 80, Tiled)
	local Far = Group("Far", 700, 350, 300, 100)
	Image(Far, "In36", 9, 7, 36, 36)
	Image(Far, "InTile", 60, 8, 120, 80, Tiled)
	Image(Far, "In72", 200, 10, 72, 72)

	-- Row 6: under a UICorner, at whole and fractional places, three radii and four sizes
	local Rounds = { { 36, 4 }, { 72, 12 }, { 120, 35 }, { 72, 4 }, { 48, 24 }, { 120, 12 }, { 36, 12 }, { 96, 8 } }
	local At = 10
	for Index, Entry in Rounds do
		local Shift = (Index - 1) % 3 / 3
		local Label = Image(Target, `Round{Index}`, At + Shift, 520 + Shift * 2, Entry[1], Entry[1])
		local Corner = Instance.new("UICorner")
		Corner.CornerRadius = UDim.new(0, Entry[2])
		Corner.Parent = Label
		At += Entry[1] + 14
	end
	local Wide = Image(Target, "RoundWide", At, 520, 144, 72)
	local WideCorner = Instance.new("UICorner")
	WideCorner.CornerRadius = UDim.new(0, 20)
	WideCorner.Parent = Wide
	local RoundTile = Image(Target, "RoundTile", At + 160, 520.5, 120, 96, Tiled)
	local TileCorner = Instance.new("UICorner")
	TileCorner.CornerRadius = UDim.new(0, 16)
	TileCorner.Parent = RoundTile

	-- Row 5: minified (two and four texels a pixel) and at 3 px a texel exactly
	Image(Target, "Min8", 10, 440, 5.34, 5.34)
	Image(Target, "Min4", 30, 440, 2.67, 2.67)
	Image(Target, "Max32", 60, 440, 32, 32)
	Image(Target, "Max48x40", 110, 440, 48, 40)
	for Index = 1, 10 do
		Image(Target, `Step{Index}`, 200 + (Index - 1) * 60 + Index / 3, 440 + Index / 3, 36, 36)
	end
end
