-- ViewportFrame probe: which built-in materials a ViewportFrame draws. A place saved without
-- MaterialService.Use2022Materials (a bare place, as compare uses) shows the pre-2022 materials in its frames as
-- in its world. One slab per material facing the camera, lit at a slant so relief shows; two frame sizes.
-- compare: --viewport 980x560 --time 0.2 --background 46,46,46
-- compare-unstable 328,16,159,159 483,16,159,159 18,171,159,159 173,171,159,159 483,171,159,159 18,326,159,159 173,326,159,159 328,326,159,159 483,326,159,159 804,8,85,85 877,8,85,85 659,80,85,85 731,80,85,85 877,80,85,85 659,153,85,85 731,153,85,85 804,153,85,85 877,153,85,85: the textured slabs: Studio shifts these materials' textures by a new random offset per part on every mount (two captures of this scene, mounted twice: 456,560 px apart over 2 levels)
local Materials = {
	Enum.Material.Plastic, Enum.Material.SmoothPlastic, Enum.Material.Brick, Enum.Material.WoodPlanks,
	Enum.Material.Slate, Enum.Material.Foil, Enum.Material.Metal, Enum.Material.Concrete,
	Enum.Material.Fabric, Enum.Material.DiamondPlate, Enum.Material.Granite, Enum.Material.Cobblestone,
}

local function Build(Target, Name, Position, Size, Distance)
	local Frame = Instance.new("ViewportFrame")
	Frame.Name = Name
	Frame.Position = Position
	Frame.Size = Size
	Frame.BackgroundColor3 = Color3.fromRGB(30, 36, 50)
	Frame.BorderSizePixel = 0
	Frame.Ambient = Color3.fromRGB(110, 110, 110)
	Frame.LightColor = Color3.new(1, 1, 1)
	Frame.LightDirection = Vector3.new(-1, -0.6, -1)
	Frame.Parent = Target
	local Camera = Instance.new("Camera")
	Camera.CFrame = CFrame.new(0, 0, Distance)
	Camera.FieldOfView = 40
	Camera.Parent = Frame
	Frame.CurrentCamera = Camera
	for Index, Material in Materials do
		local Column = (Index - 1) % 4
		local Row = (Index - 1) // 4
		local Part = Instance.new("Part")
		Part.Anchored = true
		Part.Material = Material
		Part.Size = Vector3.new(3.6, 3.6, 0.5)
		Part.CFrame = CFrame.new((Column - 1.5) * 4, (1 - Row) * 4, 0)
		Part.Color = Color3.fromRGB(190, 175, 160)
		Part.TopSurface = Enum.SurfaceType.Smooth
		Part.BottomSurface = Enum.SurfaceType.Smooth
		Part.Parent = Frame
	end
end

return function(Target)
	Build(Target, "Near", UDim2.fromOffset(10, 10), UDim2.fromOffset(640, 480), 17)
	Build(Target, "Small", UDim2.fromOffset(660, 10), UDim2.fromOffset(300, 225), 17)
end
