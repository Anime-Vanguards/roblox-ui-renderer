-- DPI variant probe: rbxasset textures that ship @2x/@3x files, to see which file Studio samples at
-- 1.5 DPI.
return function(Target)
	local function Image(Name, Uri, X, Y, Size, Props)
		local Label = Instance.new("ImageLabel")
		Label.Name = Name
		Label.Position = UDim2.fromOffset(X, Y)
		Label.Size = UDim2.fromOffset(Size, Size)
		Label.BackgroundColor3 = Color3.fromRGB(40, 90, 200)
		Label.BorderSizePixel = 0
		Label.Image = Uri
		for Key, Value in Props or {} do
			(Label :: any)[Key] = Value
		end
		Label.Parent = Target
	end

	local Robux = "rbxasset://textures/ui/common/robux_small.png" -- 16, @2x 32, @3x 48
	local Button = "rbxasset://textures/ui/Settings/Help/XButtonLight.png" -- 55, @2x 110
	Image("Robux16", Robux, 10, 10, 16)
	Image("Robux32", Robux, 40, 10, 32)
	Image("Robux64", Robux, 90, 10, 64)
	Image("Button55", Button, 170, 10, 55)
	Image("Button110", Button, 240, 10, 110)

	-- ImageRect and SliceCenter units with a substituted @2x file
	Image("RobuxRect", Robux, 10, 140, 60, { ImageRectOffset = Vector2.new(4, 4), ImageRectSize = Vector2.new(8, 8) })
	Image("ButtonRect", Button, 80, 140, 60, { ImageRectOffset = Vector2.new(10, 20), ImageRectSize = Vector2.new(30, 20) })
	Image("ButtonSlice", Button, 150, 140, 150, {
		ScaleType = Enum.ScaleType.Slice,
		SliceCenter = Rect.new(20, 20, 35, 35),
	})
	Image("ButtonFit", Button, 310, 140, 90, { ScaleType = Enum.ScaleType.Fit })
	Image("ButtonTile", Button, 410, 140, 100, { ScaleType = Enum.ScaleType.Tile, TileSize = UDim2.fromOffset(30, 30) })
end
