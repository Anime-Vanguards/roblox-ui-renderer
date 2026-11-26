-- Path2Ds under clipping frames, for StarterGui.ClipsDescendantsSupportsRotation on (the default, compare's place)
-- and off (a place that opts out: captured in a client of such a place and rendered with
-- --rotation-clipping disabled; notes/path2d.md "Clipping"). Lines and a curve leave a clipping Frame on each side,
-- one Path2D sits in a turned clipping Frame, one in a nested clip, a turned Frame crosses the clip's edge as the
-- control, and one Path2D has no clipping ancestor.
-- compare: --viewport 640x360 --time 0.2 --background 20,20,26
return function(Target)
	local Point = Path2DControlPoint.new
	local Offset = UDim2.fromOffset
	local function Path(Parent, Thickness, Color, Points)
		local P = Instance.new("Path2D")
		P.Color3 = Color
		P.Thickness = Thickness
		local List = {}
		for Index, Entry in Points do
			List[Index] = Point(Offset(Entry[1], Entry[2]), Offset(Entry[3] or 0, Entry[4] or 0), Offset(Entry[5] or 0, Entry[6] or 0))
		end
		P:SetControlPoints(List)
		P.Parent = Parent
		return P
	end
	local function Box(Parent, Name, X, Y, W, H, Rotation, Clips)
		local F = Instance.new("Frame")
		F.Name = Name
		F.BorderSizePixel = 0
		F.BackgroundColor3 = Color3.fromRGB(60, 64, 80)
		F.Position = Offset(X, Y)
		F.Size = Offset(W, H)
		F.Rotation = Rotation or 0
		F.ClipsDescendants = Clips == true
		F.Parent = Parent
		return F
	end
	local Red = Color3.fromRGB(240, 70, 70)
	local Yellow = Color3.fromRGB(240, 220, 80)
	local Green = Color3.fromRGB(90, 220, 120)
	-- a clipping Frame: a line leaving it on the right, a curve leaving it on the left and the bottom
	local Clip = Box(Target, "Clip", 40, 40, 200, 150, 0, true)
	Path(Clip, 8, Red, { { 20, 30 }, { 320, 30 } })
	Path(Clip, 6, Yellow, { { -60, 60, 0, 0, 120, 0 }, { 140, 220, 0, -120, 0, 0 } })
	-- the control: a turned Frame across the clip's right edge
	local Turned = Box(Clip, "TurnedChild", 120, 100, 160, 10, 15)
	Turned.BackgroundColor3 = Color3.fromRGB(80, 150, 240)
	-- a Path2D in a nested clip (both clips cut it)
	local Inner = Box(Clip, "Inner", 100, 80, 80, 50, 0, true)
	Inner.BackgroundColor3 = Color3.fromRGB(90, 95, 115)
	Path(Inner, 5, Green, { { -40, 10 }, { 160, 40 } })
	-- a turned clipping Frame: a Path2D across its edge
	local TurnedClip = Box(Target, "TurnedClip", 320, 60, 160, 110, 10, true)
	Path(TurnedClip, 8, Red, { { 20, 50 }, { 260, 50 } })
	-- no clipping ancestor
	Path(Target, 6, Yellow, { { 330, 260 }, { 600, 300 } })
end
