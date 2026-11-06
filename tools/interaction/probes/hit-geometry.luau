-- Hit-test geometry: CoreGui:GetGuiObjectsAtPosition over a grid of points for
-- rotated, clipped, scrolled, scaled, layered (Sibling / Global ZIndex) and hidden objects. Runs through
-- tools/analysis/layout_probe.py (Studio under CoreGui + offline), results filtered to this probe's GUI.
return function(Target)
	local CoreGui = game:GetService("CoreGui")
	local function Make(Class, Name, Parent, Props)
		local O = Instance.new(Class)
		O.Name = Name
		if O:IsA("GuiObject") then
			O.BorderSizePixel = 0
		end
		for Key, Value in Props do
			O[Key] = Value
		end
		O.Parent = Parent
		return O
	end

	-- Rotation: a 200x100 frame rotated 30 degrees, a child rotated again
	local Rot = Make("Frame", "Rot", Target, { Position = UDim2.fromOffset(100, 100), Size = UDim2.fromOffset(200, 100), Rotation = 30 })
	Make("Frame", "RotChild", Rot, { Position = UDim2.fromOffset(150, 0), Size = UDim2.fromOffset(80, 60), Rotation = -45 })
	-- Clipping + scrolling
	local Clip = Make("Frame", "Clip", Target, { Position = UDim2.fromOffset(400, 100), Size = UDim2.fromOffset(150, 150), ClipsDescendants = true })
	Make("Frame", "Clipped", Clip, { Position = UDim2.fromOffset(100, 100), Size = UDim2.fromOffset(150, 150) })
	local Scroll = Make("ScrollingFrame", "Scroll", Target, { Position = UDim2.fromOffset(600, 100), Size = UDim2.fromOffset(150, 150), CanvasSize = UDim2.fromOffset(150, 600), CanvasPosition = Vector2.new(0, 120) })
	Make("Frame", "Item", Scroll, { Position = UDim2.fromOffset(0, 100), Size = UDim2.fromOffset(150, 80) })
	-- UIScale + AnchorPoint
	local Scaled = Make("Frame", "Scaled", Target, { Position = UDim2.fromOffset(100, 300), Size = UDim2.fromOffset(100, 100), AnchorPoint = Vector2.new(0.5, 0.5) })
	Make("UIScale", "UIScale", Scaled, { Scale = 1.5 })
	Make("Frame", "ScaledChild", Scaled, { Position = UDim2.fromOffset(50, 50), Size = UDim2.fromOffset(40, 40) })
	-- Sibling ZIndex order and hidden / non-interactable objects
	Make("Frame", "Low", Target, { Position = UDim2.fromOffset(400, 300), Size = UDim2.fromOffset(150, 100), ZIndex = 1 })
	Make("Frame", "High", Target, { Position = UDim2.fromOffset(450, 320), Size = UDim2.fromOffset(150, 100), ZIndex = 5 })
	Make("Frame", "Hidden", Target, { Position = UDim2.fromOffset(420, 330), Size = UDim2.fromOffset(60, 60), ZIndex = 9, Visible = false })
	Make("Frame", "NoInput", Target, { Position = UDim2.fromOffset(500, 360), Size = UDim2.fromOffset(60, 60), ZIndex = 9, Interactable = false })
	local Group = Make("CanvasGroup", "Group", Target, { Position = UDim2.fromOffset(700, 300), Size = UDim2.fromOffset(100, 100) })
	Make("Frame", "GroupChild", Group, { Position = UDim2.fromOffset(60, 60), Size = UDim2.fromOffset(100, 100) })
	-- Global ZIndex in a second ScreenGui (above the probe GUI)
	local Global = Make("ScreenGui", "__RUR_HitGlobal", CoreGui, { IgnoreGuiInset = true, DisplayOrder = 2000000001, ZIndexBehavior = Enum.ZIndexBehavior.Global })
	local Outer = Make("Frame", "GOuter", Global, { Position = UDim2.fromOffset(100, 500), Size = UDim2.fromOffset(200, 150), ZIndex = 3 })
	Make("Frame", "GInner", Outer, { Position = UDim2.fromOffset(50, 50), Size = UDim2.fromOffset(100, 100), ZIndex = 1 })
	Make("Frame", "GSibling", Global, { Position = UDim2.fromOffset(180, 560), Size = UDim2.fromOffset(100, 100), ZIndex = 2 })
	task.wait(0.2)

	local Results = {}
	local function Query(X, Y)
		local Names = {}
		for _, Object in CoreGui:GetGuiObjectsAtPosition(X, Y) do
			if Object:IsDescendantOf(Target) or Object:IsDescendantOf(Global) then
				table.insert(Names, Object.Name)
			end
		end
		Results[string.format("%04d,%04d", X, Y)] = table.concat(Names, ",")
	end
	for Y = 60, 700, 10 do
		for X = 60, 820, 10 do
			Query(X, Y)
		end
	end
	-- edges: exact left / right / top / bottom of Low (400..550 x 300..400)
	for _, P in { { 400, 350 }, { 399, 350 }, { 549, 350 }, { 550, 350 }, { 450, 300 }, { 450, 299 }, { 450, 399 }, { 450, 400 } } do
		Query(P[1], P[2])
	end
	Global:Destroy()
	return Results
end
