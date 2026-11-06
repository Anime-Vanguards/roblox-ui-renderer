-- Cached export must keep rotated descendants that escape an offscreen clipping Frame.
-- (the engine golden corpus runs it; native/src/engine/export/scene_tests.rs checks retained vs fresh exports)
return function(Target)
	local function Box(Name, Parent, X, Y, W, H, Rotation)
		local Frame = Instance.new("Frame")
		Frame.Name = Name
		Frame.Position = UDim2.fromOffset(X, Y)
		Frame.Size = UDim2.fromOffset(W, H)
		Frame.BorderSizePixel = 0
		Frame.BackgroundColor3 = Color3.fromRGB(240, 100, 50)
		Frame.Rotation = Rotation or 0
		Frame.Parent = Parent
		return Frame
	end

	for Index, ClassName in { "Frame", "ScrollingFrame", "CanvasGroup" } do
		local Outer = Box(ClassName .. "Outer", Target, 30 + (Index - 1) * 200, 30, 100, 100)
		Outer.BackgroundColor3 = Color3.fromRGB(30, 40, 50)
		Outer.ClipsDescendants = true
		local Inner = Instance.new(ClassName)
		Inner.Name = "OffscreenClipper"
		Inner.Position = UDim2.fromOffset(150, 0)
		Inner.Size = UDim2.fromOffset(30, 30)
		Inner.ClipsDescendants = true
		Inner.BorderSizePixel = 0
		Inner.Parent = Outer
		local Arm = Box("RotatedChild", Inner, -120, 30, 40, 40, 15)
		Box("UnrotatedGrandchild", Arm, 10, 10, 10, 10)
	end
end
