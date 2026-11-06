-- UIShadow probe Q (2026-09-27): how overlapping shadows of sibling frames combine.
-- Transparent frames (shadows only), BlurRadius 40, pure colours on black. Row 1: red frame then blue frame
-- (child order), Transparency 0 / 0.5 / 0.82; row 2: the same with the frames' order swapped (blue first);
-- row 3: red / green / blue triple, Transparency 0 and 0.82, and a red pair (same colour).
-- compare: --viewport 1200x900 --time 0.2 --background 0,0,0
return function(Target)
	local Red, Green, Blue = Color3.new(1, 0, 0), Color3.new(0, 1, 0), Color3.new(0, 0, 1)

	local function Shadowed(Parent, Name, X, Y, W, H, Color, Transparency)
		local F = Instance.new("Frame")
		F.Name = Name
		F.Position = UDim2.fromOffset(X, Y)
		F.Size = UDim2.fromOffset(W, H)
		F.BackgroundTransparency = 1
		F.BorderSizePixel = 0
		local Shadow = Instance.new("UIShadow")
		Shadow.Color = Color
		Shadow.Transparency = Transparency
		Shadow.BlurRadius = UDim.new(0, 40)
		Shadow.Parent = F
		F.Parent = Parent
	end

	local function Cell(Name, X, Y, List, Transparency)
		local Holder = Instance.new("Frame")
		Holder.Name = Name
		Holder.BackgroundTransparency = 1
		Holder.Position = UDim2.fromOffset(X, Y)
		Holder.Size = UDim2.fromOffset(360, 260)
		Holder.Parent = Target
		for Index, Entry in List do
			Shadowed(Holder, `S{Index}`, Entry[1], Entry[2], 100, 100, Entry[3], Transparency)
		end
	end

	local Transparencies = { 0, 0.5, 0.82 }
	for Index, Transparency in Transparencies do
		local X = 20 + (Index - 1) * 390
		Cell(`RB{Index}`, X, 20, { { 70, 70, Red }, { 170, 90, Blue } }, Transparency)
		Cell(`BR{Index}`, X, 300, { { 170, 90, Blue }, { 70, 70, Red } }, Transparency)
	end
	Cell("RGB0", 20, 580, { { 70, 70, Red }, { 170, 70, Green }, { 120, 130, Blue } }, 0)
	Cell("RGB82", 410, 580, { { 70, 70, Red }, { 170, 70, Green }, { 120, 130, Blue } }, 0.82)
	Cell("RR", 800, 580, { { 70, 70, Red }, { 170, 90, Red } }, 0)
end
