-- ScreenGuis inside GuiObjects: each is laid out and drawn as a ScreenGui of its own, over the whole screen, whatever
-- holds it (clipping, rotated, scaled, scrolled, a CanvasGroup, invisible, a list, a button, a Folder in a frame, two
-- levels deep). Each cell holds a grey holder H at its left and the nested ScreenGui N, whose objects sit at screen
-- offsets beside it (N's DisplayOrder is above the compare harness's own ScreenGui; the harness's backdrop is in that
-- one). A disabled nested ScreenGui draws nothing; one with Global ZIndexBehavior sorts its own descendants.
--   node cli/rur.mjs compare --fixture fixtures/layout-nested-screengui.luau --viewport 1600x1000 --time 0.2 --background 20,20,26
return function(Target)
	local Count = 0
	local function Frame(Name, Parent, X, Y, W, H, Colour, Class)
		local F = Instance.new(Class or "Frame")
		F.Name = Name
		F.Position = UDim2.fromOffset(X, Y)
		F.Size = UDim2.fromOffset(W, H)
		F.BackgroundColor3 = Colour or Color3.fromRGB(90, 90, 104)
		F.BorderSizePixel = 0
		if F:IsA("GuiButton") then
			F.AutoButtonColor = false
		end
		if F:IsA("TextButton") or F:IsA("TextLabel") then
			F.Text = ""
		end
		F.Parent = Parent
		return F
	end
	local function Nested(Name, Parent)
		local N = Instance.new("ScreenGui")
		N.Name = Name
		-- (above the harness's ScreenGui: rur's story gui has DisplayOrder 0, the client's compare gui 2000000000)
		N.DisplayOrder = 2000000000 + Count
		N.ZIndexBehavior = Enum.ZIndexBehavior.Sibling
		N.ResetOnSpawn = false
		N.Parent = Parent
		return N
	end
	-- Case(name, build(cell, x0, y0)): a cell of 316 x 246 at (x0, y0)
	local function Case(Name, Build)
		local X0, Y0 = (Count % 5) * 320, (Count // 5) * 250
		local Cell = Frame(Name, Target, X0, Y0, 316, 246, Color3.fromRGB(30, 30, 38))
		Count += 1
		Build(Cell, X0, Y0)
	end
	local ORANGE, BLUE, GREEN, RED = Color3.fromRGB(255, 140, 0), Color3.fromRGB(40, 120, 255), Color3.fromRGB(40, 220, 90), Color3.fromRGB(220, 40, 60)
	-- its objects: A (orange) at (x0 + 160, y0 + 20) holding a dark A2
	local function Objects(N, X0, Y0)
		local A = Frame("A", N, X0 + 160, Y0 + 20, 100, 80, ORANGE)
		Frame("A2", A, 20, 20, 30, 30, Color3.fromRGB(40, 40, 40))
		return A
	end
	Case("plain", function(Cell, X0, Y0)
		local H = Frame("H", Cell, 10, 10, 140, 100)
		Objects(Nested("N", H), X0, Y0)
	end)
	Case("clip", function(Cell, X0, Y0)
		local H = Frame("H", Cell, 10, 10, 140, 100)
		H.ClipsDescendants = true
		local N = Nested("N", H)
		Frame("A", N, X0 + 60, Y0 + 60, 160, 100, ORANGE)
	end)
	Case("rotated", function(Cell, X0, Y0)
		local H = Frame("H", Cell, 10, 10, 140, 100)
		H.Rotation = 30
		Objects(Nested("N", H), X0, Y0)
	end)
	Case("uiscale", function(Cell, X0, Y0)
		local H = Frame("H", Cell, 10, 10, 140, 100)
		local Scale = Instance.new("UIScale")
		Scale.Scale = 0.5
		Scale.Parent = H
		Objects(Nested("N", H), X0, Y0)
	end)
	Case("scrolled", function(Cell, X0, Y0)
		local H = Frame("H", Cell, 10, 10, 140, 100, nil, "ScrollingFrame")
		H.CanvasSize = UDim2.fromOffset(0, 400)
		H.ScrollBarThickness = 6
		Frame("I", H, 10, 70, 60, 40, Color3.fromRGB(120, 120, 140))
		Objects(Nested("N", H), X0, Y0)
		H.CanvasPosition = Vector2.new(0, 60)
	end)
	Case("canvasgroup", function(Cell, X0, Y0)
		local H = Frame("H", Cell, 10, 10, 140, 100, nil, "CanvasGroup")
		H.GroupColor3 = Color3.fromRGB(255, 80, 80)
		local N = Nested("N", H)
		Frame("A", N, X0 + 60, Y0 + 60, 160, 100, ORANGE)
	end)
	Case("invisible", function(Cell, X0, Y0)
		local H = Frame("H", Cell, 10, 10, 140, 100)
		Objects(Nested("N", H), X0, Y0)
		H.Visible = false
	end)
	Case("disabled", function(Cell, X0, Y0)
		local H = Frame("H", Cell, 10, 10, 140, 100)
		local N = Nested("N", H)
		Objects(N, X0, Y0)
		N.Enabled = false
	end)
	Case("two_levels", function(Cell, X0, Y0)
		local H = Frame("H", Cell, 10, 10, 140, 100)
		local N = Nested("N", H)
		local A = Frame("A", N, X0 + 160, Y0 + 20, 140, 100, ORANGE)
		local N2 = Nested("N2", A)
		Frame("B", N2, X0 + 10, Y0 + 130, 100, 80, BLUE)
	end)
	Case("folder", function(Cell, X0, Y0)
		local H = Frame("H", Cell, 10, 10, 140, 100)
		local Folder = Instance.new("Folder")
		Folder.Name = "Group"
		Folder.Parent = H
		Objects(Nested("N", Folder), X0, Y0)
	end)
	Case("scale_sizes", function(Cell, X0, Y0)
		local H = Frame("H", Cell, 10, 10, 140, 100)
		local N = Nested("N", H)
		local A = Frame("A", N, X0 + 160, Y0 + 20, 0, 0, ORANGE)
		-- (5 % x 6 % of the screen)
		A.Size = UDim2.fromScale(0.05, 0.06)
		local B = Frame("B", N, 0, 0, 0, 0, BLUE)
		B.Position = UDim2.new(X0 / 1600, 160, Y0 / 1000, 120)
		B.Size = UDim2.new(0.0625, 0, 0.08, 0)
	end)
	Case("global", function(Cell, X0, Y0)
		local H = Frame("H", Cell, 10, 10, 140, 100)
		local N = Nested("N", H)
		N.ZIndexBehavior = Enum.ZIndexBehavior.Global
		local P = Frame("P", N, X0 + 160, Y0 + 20, 100, 80, BLUE)
		P.ZIndex = 3
		local Q = Frame("Q", P, 40, 40, 100, 80, GREEN)
		Q.ZIndex = 1
		local S = Frame("S", N, X0 + 180, Y0 + 80, 60, 100, RED)
		S.ZIndex = 2
	end)
	Case("sibling", function(Cell, X0, Y0)
		-- the same objects in a Sibling ScreenGui
		local H = Frame("H", Cell, 10, 10, 140, 100)
		local N = Nested("N", H)
		local P = Frame("P", N, X0 + 160, Y0 + 20, 100, 80, BLUE)
		P.ZIndex = 3
		local Q = Frame("Q", P, 40, 40, 100, 80, GREEN)
		Q.ZIndex = 1
		local S = Frame("S", N, X0 + 180, Y0 + 80, 60, 100, RED)
		S.ZIndex = 2
	end)
	Case("text", function(Cell, X0, Y0)
		local H = Frame("H", Cell, 10, 10, 140, 100)
		local N = Nested("N", H)
		local L = Frame("L", N, X0 + 160, Y0 + 20, 140, 40, Color3.fromRGB(40, 40, 60), "TextLabel")
		L.Text = "Nested"
		L.TextSize = 24
		L.TextColor3 = Color3.new(1, 1, 1)
	end)
	Case("list", function(Cell, X0, Y0)
		-- a list's container holding the nested ScreenGui between two items: it takes no slot
		local H = Frame("H", Cell, 10, 10, 140, 200)
		local List = Instance.new("UIListLayout")
		List.SortOrder = Enum.SortOrder.LayoutOrder
		List.Padding = UDim.new(0, 6)
		List.Parent = H
		local I1 = Frame("I1", H, 0, 0, 120, 40, Color3.fromRGB(120, 120, 140))
		I1.LayoutOrder = 1
		local N = Nested("N", H)
		Objects(N, X0, Y0)
		local I2 = Frame("I2", H, 0, 0, 120, 40, Color3.fromRGB(150, 150, 170))
		I2.LayoutOrder = 3
	end)
	Case("button", function(Cell, X0, Y0)
		local H = Frame("H", Cell, 10, 10, 140, 100, nil, "TextButton")
		Objects(Nested("N", H), X0, Y0)
	end)
	Case("anchored", function(Cell, X0, Y0)
		-- objects anchored at their centre in the nested ScreenGui, against its own size
		local H = Frame("H", Cell, 10, 10, 140, 100)
		local N = Nested("N", H)
		local A = Frame("A", N, 0, 0, 100, 80, ORANGE)
		A.AnchorPoint = Vector2.new(0.5, 0.5)
		A.Position = UDim2.new(0, X0 + 210, 0, Y0 + 60)
		local B = Frame("B", N, 0, 0, 80, 60, BLUE)
		B.AnchorPoint = Vector2.new(1, 1)
		B.Position = UDim2.new(0, X0 + 300, 0, Y0 + 230)
	end)
end
