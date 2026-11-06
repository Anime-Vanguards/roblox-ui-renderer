-- Engine oracle for tools/tests/rotation_projection.mjs (run by tools/tests/engine-oracle.mjs with
-- `rur-native engine --test-hooks 1` in each rotation-clipping mode): retained presentation and
-- canonical exports around a scroll, for the browser's scroll projection to reproduce.
return function()
local I, D = Instance, { UDim2 = UDim2, Color3 = Color3, Vector2 = Vector2 }
local Root = I.new("ScreenGui", game:GetService("CoreGui"))
Root.IgnoreGuiInset = true
local function Box(Parent, Name, X, Y, W, H, Rotation, Class)
	local F = I.new(Class or "Frame", Parent)
	F.Name = Name
	F.Position = D.UDim2.fromOffset(X, Y)
	F.Size = D.UDim2.fromOffset(W, H)
	F.Rotation = Rotation or 0
	F.BorderSizePixel = 0
	F.BackgroundColor3 = D.Color3.fromRGB(60 + #Name * 11 % 170, 40 + #Name * 19 % 160, 50 + #Name * 7 % 180)
	return F
end
local Outer = Box(Root, "Outer", 100, 100, 300, 250, 0, "ScrollingFrame")
Outer.CanvasSize = D.UDim2.fromOffset(300, 800)
Outer.ScrollBarThickness = 0
local Arm = Box(Outer, "Arm", 80, 160, 140, 100, 30)
local Cancel = Box(Arm, "Cancel", -25, 15, 160, 120, -30)
Cancel.ClipsDescendants = true
local Inner = Box(Cancel, "Inner", 10, 10, 120, 80, 0, "ScrollingFrame")
Inner.CanvasSize = D.UDim2.fromOffset(180, 400)
Inner.ScrollBarThickness = 12
Inner.VerticalScrollBarPosition = Enum.VerticalScrollBarPosition.Left
Inner.VerticalScrollBarInset = Enum.ScrollBarInset.Always
Inner.HorizontalScrollBarInset = Enum.ScrollBarInset.Always
local Content = Box(Inner, "Content", -20, 45, 190, 130)
local Nested = Box(Cancel, "NestedClip", -10, 35, 150, 65)
Nested.ClipsDescendants = true
Box(Nested, "NestedContent", -30, -10, 200, 90)
Box(Outer, "UpperEdge", 30, 40, 180, 8, 45)
Box(Outer, "LowerEdge", 30, 320, 180, 8, -30)
local OutsideGroup = Box(Outer, "OutsideGroup", 30, 25, 230, 40, 45, "CanvasGroup")
OutsideGroup.GroupTransparency = 0.2
Box(OutsideGroup, "GroupChild", -30, -10, 270, 70)
local Image = Box(OutsideGroup, "GroupImage", 10, 0, 120, 40, 0, "ImageLabel")
Image.BackgroundTransparency = 1
Image.Image = "rbxassetid://123"
local OutsideScroll = Box(Outer, "OutsideScroll", 80, 25, 180, 20, -45, "ScrollingFrame")
OutsideScroll.CanvasSize = D.UDim2.fromOffset(180, 300)
OutsideScroll.ScrollBarThickness = 10
OutsideScroll.ScrollBarImageColor3 = D.Color3.new(1, 0, 1)
OutsideScroll.BackgroundTransparency = 0.6
local function Capture(Retained)
	rur.layout()
	return rur.scene({ Root }, Retained)
end
local Results = {}
for _, Target in { 0, 180, 0, 180 } do
	Outer.CanvasPosition = D.Vector2.new(0, 100)
	Inner.CanvasPosition = D.Vector2.new(0, 50)
	local Initial = Capture(true)
	local InitialCanonical = Capture(false)
	Outer.CanvasPosition = D.Vector2.new(0, Target)
	Inner.CanvasPosition = D.Vector2.new(0, 50)
	local Authoritative = Capture(true)
	local Canonical = Capture(false)
	table.insert(Results, {
		target = Target,
		outer = rur.id(Outer),
		inner = rur.id(Inner),
		initial = Initial,
		initialCanonical = InitialCanonical,
		authoritative = Authoritative,
		canonical = Canonical,
	})
end
rur.emit("@@ROTATION", Results)
end
