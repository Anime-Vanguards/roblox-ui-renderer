-- Does Instance:Clone() keep the child order? GetChildren order is the draw order of equal-ZIndex siblings
-- and breaks list-layout ties, and `rur compare` dumps and captures a frozen clone of the story target.
--   node tools/studio-lock.mjs node tools/studio-mcp.cjs luau tools/sweep/probe-clone-order.luau
-- Mounts a temporary ScreenGui under CoreGui and destroys it before returning.

local CoreGui = game:GetService("CoreGui")
local HttpService = game:GetService("HttpService")

local Gui = Instance.new("ScreenGui")
Gui.Name = "__RUR_CloneOrderProbe"

local function Names(Node)
	local Result = {}
	for _, Child in Node:GetChildren() do
		table.insert(Result, Child.Name)
	end
	return table.concat(Result, ",")
end

local function DescendantNames(Node)
	local Result = {}
	for _, Child in Node:GetDescendants() do
		table.insert(Result, Child.Name)
	end
	return table.concat(Result, ",")
end

-- Created and parented in order
local Plain = Instance.new("Frame")
Plain.Name = "Plain"
for Index = 1, 8 do
	local Child = Instance.new(if Index % 2 == 0 then "TextLabel" else "Frame")
	Child.Name = "C" .. Index
	Child.Parent = Plain
end
for Index = 1, 4 do
	local Child = Instance.new("ImageLabel")
	Child.Name = "T" .. Index
	Child.Parent = Plain:FindFirstChild("C3")
end
Plain.Parent = Gui

-- Created in order, parented in another order
local Shuffled = Instance.new("Frame")
Shuffled.Name = "Shuffled"
local Made = {}
for Index = 1, 6 do
	local Child = Instance.new("Frame")
	Child.Name = "S" .. Index
	Made[Index] = Child
end
for _, Index in { 4, 2, 6, 1, 5, 3 } do
	Made[Index].Parent = Shuffled
end
Shuffled.Parent = Gui

-- Children built before their parent is parented (vide-style: create the leaves, then the container)
local Built = Instance.new("Frame")
Built.Name = "Built"
for Index = 1, 5 do
	local Child = Instance.new("Frame")
	Child.Name = "B" .. Index
	local Leaf = Instance.new("Frame")
	Leaf.Name = "Leaf"
	Leaf.Parent = Child
	Child.Parent = Built
end
Built.Parent = Gui

-- Moved: reparenting a child moves it to the end of the list
local Moved = Instance.new("Frame")
Moved.Name = "Moved"
for Index = 1, 5 do
	local Child = Instance.new("Frame")
	Child.Name = "M" .. Index
	Child.Parent = Moved
end
local Second = Moved:FindFirstChild("M2")
Second.Parent = nil
Second.Parent = Moved
Moved.Parent = Gui

Gui.Parent = CoreGui

local Result = {}
for _, Case in { Plain, Shuffled, Built, Moved } do
	local Copy = Case:Clone()
	Copy.Parent = Gui
	Result[Case.Name] = {
		original = Names(Case),
		clone = Names(Copy),
		originalDescendants = DescendantNames(Case),
		cloneDescendants = DescendantNames(Copy),
	}
end
local PlainCopy = Plain:Clone()
Result.PlainInner = { original = Names(Plain:FindFirstChild("C3")), clone = Names(PlainCopy:FindFirstChild("C3")) }

-- Whole-GUI clone (what __RUR_Freeze does with the story target)
local GuiCopy = Gui:Clone()
Result.Gui = { original = Names(Gui), clone = Names(GuiCopy) }
GuiCopy:Destroy()
PlainCopy:Destroy()

Gui:Destroy()
return HttpService:JSONEncode(Result)
