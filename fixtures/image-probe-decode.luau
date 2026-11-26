-- Image probe: how the client decodes image files, on textures Studio ships. Pixelated at whole device pixels per
-- texel (3 at 1.5x, 2 at 1x for the 2x cells), so every decoded texel shows as it is: a progressive 4:4:4 JPEG
-- (sky/moon.jpg) and a baseline 4:2:2 JPEG with an ICC profile (sky/sun.jpg), a 16-bit greyscale PNG
-- (particles/explosion01_core_alpha.png), 2-bit and 4-bit palette PNGs with and without tRNS, an 8-bit greyscale,
-- a greyscale + alpha and a 1-bit greyscale PNG, each also once with the default filter. The last row slices a
-- 2048 px sprite sheet (ImageRect and SliceCenter units of a texture over 1024 px): a SliceCenter inside the 1024
-- capped rect, one past it, and one inside an ImageRect.
-- compare: --viewport 1200x900 --time 0.5 --background 20,20,26
return function(Target)
	local function Image(Name, Uri, X, Y, W, H, Props)
		local Label = Instance.new("ImageLabel")
		Label.Name = Name
		Label.Position = UDim2.fromOffset(X, Y)
		Label.Size = UDim2.fromOffset(W, H)
		Label.BackgroundColor3 = Color3.fromRGB(40, 90, 200)
		Label.BorderSizePixel = 0
		Label.Image = Uri
		for Key, Value in Props or {} do
			(Label :: any)[Key] = Value
		end
		Label.Parent = Target
		return Label
	end
	local Pixelated = { ResampleMode = Enum.ResamplerMode.Pixelated }

	local Moon = "rbxasset://sky/moon.jpg" -- 128 x 128, progressive, 4:4:4
	local Sun = "rbxasset://sky/sun.jpg" -- 128 x 128, baseline, 4:2:2, ICC profile
	Image("MoonPix", Moon, 10, 10, 256, 256, Pixelated)
	Image("SunPix", Sun, 276, 10, 256, 256, Pixelated)
	Image("Moon", Moon, 542, 10, 128, 128)
	Image("Sun", Sun, 680, 10, 128, 128)
	Image("MoonSmall", Moon, 542, 148, 50, 50)
	Image("SunSmall", Sun, 602, 148, 50, 50)

	local Core16 = "rbxasset://textures/particles/explosion01_core_alpha.png" -- 128 x 32, 16-bit greyscale
	Image("Core16Pix", Core16, 10, 276, 512, 128, Pixelated)
	Image("Core16", Core16, 532, 276, 256, 64)

	local Arrow = "rbxasset://textures/ArrowCursor.png" -- 64 x 64, 2-bit palette with tRNS
	local Grid = "rbxasset://textures/grid16.png" -- 512 x 512, 4-bit palette
	local Loading = "rbxasset://textures/loading/loadingTexture.png" -- 512 x 512, 8-bit greyscale
	local Move = "rbxasset://textures/advancedMove.png" -- 157 x 98, greyscale + alpha
	local White = "rbxasset://textures/ui/InGameMenu/WhiteSquare.png" -- 64 x 64, 1-bit greyscale
	Image("ArrowPix", Arrow, 10, 414, 128, 128, Pixelated)
	Image("GridPix", Grid, 148, 414, 128, 128, {
		ResampleMode = Enum.ResamplerMode.Pixelated,
		ImageRectOffset = Vector2.new(0, 0),
		ImageRectSize = Vector2.new(64, 64),
	})
	Image("LoadingPix", Loading, 286, 414, 128, 128, {
		ResampleMode = Enum.ResamplerMode.Pixelated,
		ImageRectOffset = Vector2.new(200, 200),
		ImageRectSize = Vector2.new(64, 64),
	})
	Image("MovePix", Move, 424, 414, 314, 196, Pixelated)
	Image("WhitePix", White, 748, 414, 64, 64, Pixelated)
	Image("Arrow", Arrow, 822, 414, 64, 64)
	Image("Grid", Grid, 896, 414, 128, 128)
	Image("Move", Move, 822, 552, 157, 98)

	local Sheet = "rbxasset://textures/AvatarEditorImages/Sheet.png" -- 2048 x 2048
	local function Slice(Name, X, Center, Props)
		local P = { ScaleType = Enum.ScaleType.Slice, SliceCenter = Center, SliceScale = 0.25 }
		for Key, Value in Props or {} do
			P[Key] = Value
		end
		Image(Name, Sheet, X, 620, 280, 220, P)
	end
	Slice("SliceInside", 10, Rect.new(200, 200, 800, 800))
	Slice("SlicePast", 300, Rect.new(700, 700, 1300, 1300))
	Slice("SliceRect", 590, Rect.new(100, 100, 400, 400), {
		ImageRectOffset = Vector2.new(512, 512),
		ImageRectSize = Vector2.new(512, 512),
		SliceScale = 0.5,
	})
	Image("SheetRect", Sheet, 880, 620, 256, 256, {
		ResampleMode = Enum.ResamplerMode.Pixelated,
		ImageRectOffset = Vector2.new(256, 256),
		ImageRectSize = Vector2.new(256, 256),
	})
end
